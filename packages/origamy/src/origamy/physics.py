# Copyright 2026 Maxence Faldor
"""A build folded physically on origamy-sim, step by step, from the flat sheet.

The simulation meshes the last step's crease pattern once, and the sheet starts flat on
a table, under gravity. Each step turns the creases it changes toward the angles its
state gives them, in quasi-static increments. Where the faces can fold as rigid
origami, the creases follow that motion; elsewhere they turn at even rates. The faces
the step leaves in place stay held. Hands carry the weight of the paper that moves, as
a folder's do, then let go, and the paper settles under its weight. A precrease folds
to the paper it passes through, then unfolds. A step that moves all the paper rigidly,
as turning it over does, moves the simulated paper the same way.

Paper on a table folds up from it, never down into it. A step that turns paper down
folds with the sheet turned over, as a folder makes a mountain fold, then turns it back.

Each fold ends with the simulated paper measured against the state it reaches. The
measure gives how far each face's corners lie from their places in that state, and
which overlapping faces stack the other way round.
"""

import itertools
import math
from collections.abc import Sequence
from dataclasses import dataclass
from typing import Annotated

import numpy as np
from numpy.typing import NDArray
from pydantic import BaseModel, ConfigDict, Field

from origamy.errors import OrigamyError
from origamy.flex import path
from origamy.geometry import Isometry, Point, bounds, center, intersection
from origamy.paper import Step
from origamy.space import crease_angles, placements
from origamy.state import Pair, State
from origamy_sim import (
    Crease,
    Material,
    Mesh,
    Refinement,
    Settings,
    Sheet,
    Simulation,
    Table,
)


class PhysicsError(OrigamyError):
    """The simulation cannot fold a step of the build."""


class Physics(BaseModel):
    """How a build folds on the simulator: the paper, its mesh and its creases."""

    model_config = ConfigDict(
        frozen=True, extra="forbid", use_attribute_docstrings=True
    )

    size: Annotated[float, Field(gt=0, allow_inf_nan=False)] = 0.15
    """The paper's longer side, in meters; 15 cm by default, as kami's."""

    refinement: Refinement = Refinement(max_area=4e-4)
    """How finely the crease pattern's faces split into triangles.

    The default makes triangles of at most 4 cm², with angles of at least 20°.
    """

    increments: Annotated[int, Field(gt=0)] = 30
    """The quasi-static increments in which a fold turns its creases.

    The default keeps each increment's turn small enough that a collapse keeps to the
    way it folds.
    """

    margin: Annotated[float, Field(gt=0, lt=math.pi)] = 0.01
    """How far short of a half turn a folded crease rests, in radians.

    A crease cannot rest at a half turn, where its two faces would pass through each
    other.
    """

    drive: Crease = Crease(stiffness=0.9, yield_moment=math.inf)
    """The crease each fold makes, turning toward the angle each step gives it.

    The default is a hundred times as stiff as a crease in kami and never yields. A fold
    then turns its faces about its creases rather than bending them.
    """

    material: Material = Material()
    """The paper's material; kami by default."""

    settings: Settings = Settings(time_step=math.inf, table=Table())
    """The simulation's settings.

    The default steps quasi-statically, under gravity, with the paper on a table.
    """


@dataclass(frozen=True)
class Stage:
    """One fold of the simulation: to a step's state, or a precrease's folded paper."""

    step: int
    """The step the fold belongs to, counted from the start."""
    state: State
    """The paper the fold reaches."""
    frames: tuple[NDArray[np.float64], ...]
    """The mesh's vertices in meters as the fold goes, each of shape (n, 3).

    A frame follows each increment and each turn over, and the last shows the paper
    settled.
    """
    converged: bool
    """Whether every time step's solve converged."""
    iterations: int
    """The Newton iterations of all the fold's time steps."""
    offset: float
    """The farthest a face's corner lies from its place in the state, in meters."""
    misordered: tuple[Pair, ...]
    """The pairs of the state's stacking, upper face first, that the simulation stacks
    the other way round."""


@dataclass(frozen=True)
class Folding:
    """A build folded on the simulator."""

    mesh: Mesh
    """The last step's crease pattern in triangles, in meters, its vertices first."""
    faces: NDArray[np.int64]
    """Each triangle's face in the last step's state."""
    stages: tuple[Stage, ...]
    """Each fold, in order."""


def simulate(steps: Sequence[Step], physics: Physics | None = None) -> Folding:
    """Fold a build's steps on the simulator, from the flat sheet of its first step.

    Raises:
        PhysicsError: A step shapes the paper in space, or turns paper both up from
            the table and down into it. A step that moves all the paper must move it
            rigidly.
    """
    physics = physics or Physics()
    left, bottom, right, top = bounds(steps[0].state.vertices)
    scale = physics.size / float(max(right - left, top - bottom))
    folder = _Folder(steps[-1].state, scale, physics)
    stages: list[Stage] = []
    for index, step in enumerate(steps[1:], 1):
        if step.form is not None:
            msg = (
                f"step {index} shapes the paper in space, and the simulation folds flat"
            )
            raise PhysicsError(msg)
        before = steps[index - 1].state
        for after in (step.through, step.state) if step.through else (step.state,):
            stages.append(folder.fold(index, before, after))
            before = after
    return Folding(folder.mesh, folder.faces, tuple(stages))


class _Folder:
    """The simulated sheet, meshed from the build's last crease pattern."""

    def __init__(self, last: State, scale: float, physics: Physics) -> None:
        self.last, self.scale, self.physics = last, scale, physics
        self.mesh, self.faces = Mesh.of_faces(
            [(float(v.x) * scale, float(v.y) * scale) for v in last.vertices],
            [list(face.corners) for face in last.faces],
            physics.refinement,
        )
        self.simulation = Simulation(
            [Sheet(self.mesh, physics.material)], physics.settings
        )
        table = physics.settings.table
        gap = physics.settings.contact.gap
        lying = (
            0.0
            if table is None
            else table.height + 0.5 * (physics.material.thickness + gap)
        )
        rest = self.mesh.rest
        self.simulation.set_positions(
            0, np.column_stack([rest, np.full(len(rest), lying)])
        )
        # A crease runs along each interior edge between triangles of different faces.
        triangles: dict[tuple[int, int], list[int]] = {}
        for t, corners in enumerate(self.mesh.triangles.tolist()):
            for a, b in itertools.pairwise((*corners, corners[0])):
                triangles.setdefault((min(a, b), max(a, b)), []).append(t)
        sides = [triangles[u, v] for u, v, _, _ in self.mesh.dihedrals.tolist()]
        creases = [
            i for i, (s, t) in enumerate(sides) if self.faces[s] != self.faces[t]
        ]
        self.edges = self.mesh.dihedrals[creases, :2]
        self.sides = np.array([sides[i] for i in creases], dtype=np.int64)
        # Each vertex carries a third of the area of each triangle round it.
        corners = self.mesh.rest[self.mesh.triangles]
        (ax, ay), (bx, by) = ((corners[:, i] - corners[:, 0]).T for i in (1, 2))
        areas = 0.5 * np.abs(ax * by - ay * bx)
        lumped = np.bincount(
            self.mesh.triangles.ravel(),
            weights=np.repeat(areas / 3, 3),
            minlength=len(self.mesh.rest),
        )
        self.weights = np.outer(
            lumped * physics.material.mass_per_area, physics.settings.gravity
        )
        self.angles = np.zeros(len(creases))
        self.made = np.zeros(len(creases), dtype=bool)

    def fold(self, step: int, before: State, after: State) -> Stage:
        """Fold the paper from one state to the next, and measure it there.

        A face keeps still where its place does not change.
        """
        under = before.parents(after)
        held = np.array(
            [
                face.place == before.faces[under[f]].place
                for f, face in enumerate(after.faces)
            ]
        )
        parents = np.array(after.parents(self.last), dtype=np.int64)
        sheet = self.simulation
        sheet.hold(0, np.arange(len(self.mesh.rest)), held=False)
        if not held.any():
            self._move(step, before, after, under)
            frames, converged, iterations = (sheet.positions(0),), True, 0
        else:
            frames, converged, iterations = self._drive(
                step, before, after, parents, held[parents[self.faces]]
            )
        offset, misordered = self._measure(after, parents, frames[-1])
        return Stage(step, after, frames, converged, iterations, offset, misordered)

    def _move(self, step: int, before: State, after: State, under: list[int]) -> None:
        """Move all the paper rigidly, as the step moves every face."""
        motion = after.faces[0].place @ before.faces[under[0]].place.inverse()
        if any(
            face.place != motion @ before.faces[under[f]].place
            for f, face in enumerate(after.faces)
        ):
            msg = f"step {step} moves all the paper, but not rigidly"
            raise PhysicsError(msg)
        positions = self.simulation.positions(0)
        matrix = np.array(
            [[float(motion.xx), float(motion.xy)], [float(motion.yx), float(motion.yy)]]
        )
        shift = np.array([float(motion.tx), float(motion.ty)]) * self.scale
        positions[:, :2] = positions[:, :2] @ matrix.T + shift
        if motion.flips:
            positions[:, 2] = (
                positions[:, 2].min() + positions[:, 2].max() - positions[:, 2]
            )
        self.simulation.set_positions(0, positions)

    def _drive(
        self,
        step: int,
        before: State,
        after: State,
        parents: NDArray[np.int64],
        still: NDArray[np.bool_],
    ) -> tuple[tuple[NDArray[np.float64], ...], bool, int]:
        """Turn the creases toward the state's angles, holding the still triangles."""
        sheet = self.simulation
        pairs = parents[self.faces[self.sides]]
        for i in np.flatnonzero(~self.made & (pairs[:, 0] != pairs[:, 1])):
            u, v = self.edges[i].tolist()
            sheet.set_crease(0, (u, v), self.physics.drive)
            self.made[i] = True
        made = np.flatnonzero(self.made)
        goals = np.array([self._angle(after, f, g) for f, g in pairs[made].tolist()])
        held = np.zeros(len(self.mesh.rest), dtype=bool)
        held[self.mesh.triangles[still]] = True
        sheet.hold(0, np.flatnonzero(held))
        # Hands carry the weight of the paper the fold moves, as a folder's do.
        moving = np.flatnonzero(~held)
        sheet.set_forces(0, moving, -self.weights[moving])
        frames: list[NDArray[np.float64]] = []
        mirror = None
        if self._turned_down(step, made, goals, still):
            mirror = self._turn_over()
            frames.append(sheet.positions(0))
        edges = [(u, v) for u, v in self.edges[made].tolist()]
        converged, iterations = True, 0
        for angles in self._plan(before, after, pairs[made], goals):
            for edge, angle in zip(edges, angles.tolist(), strict=True):
                sheet.set_rest_angle(0, edge, angle)
            report = sheet.step()
            converged &= report.converged
            iterations += report.iterations
            frames.append(sheet.positions(0))
        self.angles[made] = goals
        if mirror is not None:
            self._turn_over(mirror[0])
            frames.append(sheet.positions(0))
        # The hands let go, and the paper settles under its weight.
        sheet.set_forces(0, moving, np.zeros((len(moving), 3)))
        report = sheet.step()
        converged &= report.converged
        iterations += report.iterations
        frames.append(sheet.positions(0))
        return tuple(frames), converged, iterations

    def _plan(
        self,
        before: State,
        after: State,
        pairs: NDArray[np.int64],
        goals: NDArray[np.float64],
    ) -> list[NDArray[np.float64]]:
        """The angles the creases between faces of ``after`` rest at, by increment.

        The creases follow the paper as rigid origami where its faces can fold rigidly
        from one state to the other. Elsewhere they turn together at even rates, and the
        faces bend as they must. Either way, they turn slowly near both ends. Flat paper
        there offers several ways to fold, and small turns keep to the one the step
        takes.
        """
        count = self.physics.increments
        shares = [(1 - math.cos(math.pi * i / count)) / 2 for i in range(1, count)]
        start = self.angles[self.made]
        forms = path(before, after, shares)
        if forms is None:
            return [start + (goals - start) * share for share in (*shares, 1.0)]
        edge = {frozenset(faces): e for e, faces in after.creases.items()}
        creases = [edge[frozenset((f, g))] for f, g in pairs.tolist()]
        closed = math.pi - self.physics.margin
        angles = []
        for form in forms:
            turns = crease_angles(after, placements(after, form))
            angles.append(np.clip([turns[e] for e in creases], -closed, closed))
        return [*angles, goals]

    def _angle(self, state: State, f: int, g: int) -> float:
        """The angle the crease between two faces of a state rests at."""
        closed = math.pi - self.physics.margin
        return {"V": closed, "M": -closed, "F": 0.0}[state.sense(f, g)]

    def _turned_down(
        self,
        step: int,
        made: NDArray[np.int64],
        goals: NDArray[np.float64],
        still: NDArray[np.bool_],
    ) -> bool:
        """Whether the creases turn paper down from the faces held still.

        A crease's angle is positive toward the paper's front. Paper turning about a
        crease from a held face sweeps the side of the face its angles share. The front
        of the held face points up or down, so that side lies above or below the table.
        """
        if self.physics.settings.table is None:
            return False
        positions = self.simulation.positions(0)
        sides = set[int]()
        for i, goal in zip(made.tolist(), goals.tolist(), strict=True):
            s, t = self.sides[i].tolist()
            if still[s] == still[t] or goal == self.angles[i]:
                continue
            a, b, c = positions[self.mesh.triangles[s if still[s] else t]]
            up = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
            sides.add(int(np.sign(up * (goal + self.angles[i]))))
        sides.discard(0)
        if len(sides) > 1:
            msg = f"step {step} turns paper both up from the table and down into it"
            raise PhysicsError(msg)
        return sides == {-1}

    def _turn_over(self, middle: float | None = None) -> tuple[float, float]:
        """Turn the paper over from left to right, about a line across the table.

        The line runs through ``middle``, by default the middle of what the paper
        covers. The paper keeps the heights it covers, so it rests on the table as
        before. The turn is the reflection `_reflect` makes with the middle and level
        it returns.
        """
        sheet = self.simulation
        held = sheet.held(0)
        sheet.hold(0, np.arange(len(held)), held=False)
        positions = sheet.positions(0)
        x, z = positions[:, 0], positions[:, 2]
        mirror = (
            0.5 * (x.min() + x.max()) if middle is None else middle,
            z.min() + z.max(),
        )
        sheet.set_positions(0, _reflect(positions, mirror))
        sheet.hold(0, np.flatnonzero(held))
        return mirror

    def _measure(
        self, state: State, parents: NDArray[np.int64], positions: NDArray[np.float64]
    ) -> tuple[float, tuple[Pair, ...]]:
        """How far paper at positions lies from a state, and which pairs misorder."""
        places: dict[int, Point] = {}
        for f, face in enumerate(self.last.faces):
            place = state.faces[parents[f]].place
            for v in face.corners:
                places.setdefault(v, place(self.last.vertices[v]))
        expected = np.array([(float(p.x), float(p.y)) for p in places.values()])
        offset = float(
            np.linalg.norm(
                positions[list(places), :2] - expected * self.scale, axis=1
            ).max()
        )
        misordered = tuple(
            (upper, lower)
            for upper, lower in sorted(state.above)
            if not self._stacked(state, parents, positions, upper, lower)
        )
        return offset, misordered

    def _stacked(
        self,
        state: State,
        parents: NDArray[np.int64],
        positions: NDArray[np.float64],
        upper: int,
        lower: int,
    ) -> bool:
        """Whether the simulated paper lays a face above another where they overlap."""
        point = center(intersection(state.folded[upper], state.folded[lower]))
        high, low = (
            self._height(state.faces[face].place, parents == face, positions, point)
            for face in (upper, lower)
        )
        return high > low

    def _height(
        self,
        place: Isometry,
        within: NDArray[np.bool_],
        positions: NDArray[np.float64],
        point: Point,
    ) -> float:
        """The simulated height of the paper a face's place puts at a point.

        The paper is the face's, made of the last state's faces ``within`` it. The
        triangle holding the point is the one with the greatest least barycentric
        weight.
        """
        origin = place.inverse()(point)
        target = np.array([float(origin.x), float(origin.y)]) * self.scale
        corners = self.mesh.triangles[within[self.faces]]
        a, b, c = (self.mesh.rest[corners[:, i]] for i in range(3))
        edges = np.stack([b - a, c - a], axis=-1)
        u = np.linalg.solve(edges, (target - a)[..., None])[..., 0]
        weights = np.column_stack([1 - u.sum(axis=1), u])
        best = int(weights.min(axis=1).argmax())
        return float(weights[best] @ positions[corners[best], 2])


def _reflect(
    points: NDArray[np.float64], mirror: tuple[float, float] | None
) -> NDArray[np.float64]:
    """Points reflected left to right about a middle, and up to down about a level.

    The level is twice the height of the reflecting plane; without a mirror, the points
    stay.
    """
    if mirror is None:
        return points
    middle, level = mirror
    return np.column_stack(
        [2 * middle - points[:, 0], points[:, 1], level - points[:, 2]]
    )
