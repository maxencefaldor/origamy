# Copyright 2026 Maxence Faldor
"""A build folded physically on origamy-sim, step by step, from the flat sheet.

The simulation meshes the last step's crease pattern once, and the sheet starts flat on
a table, under gravity. Each step turns the creases it changes toward the angles its
state gives them, in quasi-static increments. Where the faces can fold as rigid
origami, the creases follow that motion; elsewhere they turn at even rates. A hand
holds down the paper the step leaves in place where it touches the table. Hands carry
the weight of the paper that moves, as a folder's do, then let go, and the paper
settles under its weight. A precrease folds
to the paper it passes through, then unfolds. A step that moves all the paper rigidly,
as turning it over or a tilt does, moves the simulated paper the same way, resting on
the table where it rested. A step that shapes the paper in space turns its creases to
the angles the step's form gives them.

Paper on a table folds up from it, never down into it. A step that turns paper down
folds with the sheet turned over, as a folder makes a mountain fold, then turns it back.
A step that turns paper both up and down folds with the model lifted off the table,
then lays it back down.

A crease that wraps other layers bends round them over a crease band, half a circle
round the layers inside. Spans turn the paper beyond one side of the band against the
paper beyond the other, and the band bends as the layers inside let it. Creases a fold
turns together along one straight line share one band, and bands cross where their
creases cross. Hands hold the paper beside a band, never the band itself.

Without contact between layers, paper passes through paper as a sheet without
thickness does, and creases fold sharp, with no bands. Orders then keep the layers in
the stacking. Paper over a layer keeps to its side, as the stacking the fold starts
from has it, until the paper leaves that layer. Paper that comes over a layer keeps to
the side the stacking the fold reaches gives it.

Each fold ends with the simulated paper measured against the state it reaches. The
measure gives how far each face's corners lie from their places in that state, and
which overlapping faces stack the other way round. Every stage also measures how deep
the paper passes through itself where it ends.
"""

import itertools
import math
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from fractions import Fraction
from typing import Annotated, Final

import numpy as np
from numpy.typing import NDArray
from pydantic import BaseModel, ConfigDict, Field

from origamy.actions import stack_at
from origamy.errors import OrigamyError
from origamy.field import Number
from origamy.flex import path
from origamy.geometry import (
    HALF,
    Isometry,
    Point,
    bounds,
    center,
    intersection,
)
from origamy.paper import Step
from origamy.space import (
    Form,
    Motion,
    Shaped,
    Vector,
    apply,
    compose,
    crease_angles,
    embedding,
    invert,
    placements,
    same,
)
from origamy.state import Edge, Pair, State
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


class Bands(BaseModel):
    """How a crease that wraps other layers bends round them: over a crease band.

    The band is as wide as half a circle round the layers inside. Spans turn the paper
    beyond one side of the band against the paper beyond the other, and the band bends
    as it must between them.
    """

    model_config = ConfigDict(
        frozen=True, extra="forbid", use_attribute_docstrings=True
    )

    lines: Annotated[int, Field(gt=0)] = 2
    """The lines of mesh edges on each side of a band's crease."""

    edge: Annotated[float, Field(gt=0, allow_inf_nan=False)] = 3e-3
    """The length of the mesh edges along a band at its ends, in meters.

    Creases cross at a band's ends, so the band's mesh is finest there. Cells much
    smaller than the default beside a sharp crease would stop it closing.
    """

    growth: Annotated[float, Field(gt=1, allow_inf_nan=False)] = 1.5
    """How much longer each mesh edge along a band is than the one before it.

    The edges grow from the band's ends to its middle.
    """

    crease: Crease = Crease()
    """The crease a band's fold leaves, spread over its lines; kami's by default."""


class Physics(BaseModel):
    """How a build folds on the simulator: the paper, its mesh and its creases."""

    model_config = ConfigDict(
        frozen=True, extra="forbid", use_attribute_docstrings=True
    )

    size: Annotated[float, Field(gt=0, allow_inf_nan=False)] = 0.15
    """The paper's longer side, in meters; 15 cm by default, as kami's."""

    refinement: Refinement = Refinement(max_area=4e-4, min_area=6.25e-6)
    """How finely the crease pattern's faces split into triangles.

    The default makes triangles of at most 4 cm², with angles of at least 20°. The
    refinement leaves triangles under 6.25 mm² as they are. A vertex just off a sharp
    crease would stop its fold short of flat, as the layers there meet.
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

    bands: Bands = Bands()
    """How creases that wrap other layers bend round them."""

    material: Material = Material(compression=1e-3)
    """The paper's material: kami, which wrinkles under compression, keeping a
    thousandth of its stiffness there.

    A fold that wraps layers a crease joins makes the outer layer travel farther round
    it, and folded kami takes the difference by wrinkling."""

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
    form: Form | None
    """Where its faces lie in space once shaping turns the paper; None while flat."""
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
    """The farthest a face's corner lies from its place in the state, in meters.

    Shaped paper settles where its weight takes it, so its place in space is matched to
    the simulated paper first, as rigidly as fits best.
    """
    misordered: tuple[Pair, ...]
    """The pairs of the state's stacking, upper face first, that the simulation stacks
    the other way round.

    A pair misorders where its upper face lies below the lower one by more than half the
    thickness contact keeps between layers.
    """
    crossing: float = 0.0
    """The deepest the simulated paper passes through itself where the fold ends, in
    meters.

    An edge of the mesh passes through a triangle where it crosses the triangle's plane
    inside it, the two sharing no corner. The depth is how far the edge's nearer end
    lies from that plane, so layers that touch cross by nothing.
    """
    settled: bool = False
    """Whether the stage settles the paper, bent, away from any exact state."""


@dataclass(frozen=True)
class Folding:
    """A build folded on the simulator."""

    mesh: Mesh
    """The last step's crease pattern in triangles, in meters, its vertices first."""
    faces: NDArray[np.int64]
    """Each triangle's face in the last step's state."""
    creep: float
    """The farthest a face may creep back where its creases wrap layers, in meters."""
    stages: tuple[Stage, ...]
    """Each fold, in order."""


def simulate(steps: Sequence[Step], physics: Physics | None = None) -> Folding:
    """Fold a build's steps on the simulator, from the flat sheet of its first step.

    Raises:
        PhysicsError: A step moves all the paper, but not rigidly.
    """
    physics = physics or Physics()
    left, bottom, right, top = bounds(steps[0].state.vertices)
    scale = physics.size / float(max(right - left, top - bottom))
    folds: list[tuple[int, Shaped, Shaped]] = []
    for index, step in enumerate(steps[1:], 1):
        if step.action == "settle":
            continue
        before = Shaped(steps[index - 1].state, steps[index - 1].form)
        reached = Shaped(step.state, step.form)
        for after in (Shaped(step.through), reached) if step.through else (reached,):
            folds.append((index, before, after))
            before = after
    folder = _Folder(
        steps[-1].state, [(b.state, a.state) for _, b, a in folds], scale, physics
    )
    stages = [folder.fold(index, before, after) for index, before, after in folds]
    for index, step in enumerate(steps[1:], 1):
        if step.action == "settle":
            stages.append(folder.settle(index, Shaped(step.state, step.form), step))
    return Folding(folder.mesh, folder.faces, folder.creep, tuple(stages))


_CHUNK: Final = 256
"""The edges a crossing test takes at once, each against every triangle."""


def crossing(points: NDArray[np.float64], triangles: NDArray[np.int64]) -> float:
    """The deepest a mesh passes through itself, in the units of its points.

    An edge passes through a triangle where it crosses the triangle's plane inside it,
    the two sharing no corner. The depth is how far the edge's nearer end lies from that
    plane, so layers that touch cross by nothing. Returns 0 for a mesh that never passes
    through itself.
    """
    edges = np.unique(np.sort(triangles[:, [0, 1, 1, 2, 2, 0]].reshape(-1, 2)), axis=0)
    corners = points[triangles]
    normals = np.cross(corners[:, 1] - corners[:, 0], corners[:, 2] - corners[:, 0])
    normals /= np.linalg.norm(normals, axis=1, keepdims=True)
    low, high = corners.min(axis=1), corners.max(axis=1)
    deepest = 0.0
    for chunk in np.array_split(edges, max(1, len(edges) // _CHUNK)):
        ends = points[chunk]
        # Only an edge and a triangle whose boxes overlap can meet.
        meet = np.all(
            (ends.min(axis=1)[:, None] <= high) & (ends.max(axis=1)[:, None] >= low),
            axis=2,
        )
        e, t = np.nonzero(meet)
        shared = (triangles[t, :, None] == chunk[e, None, :]).any(axis=(1, 2))
        e, t = e[~shared], t[~shared]
        a, b, origin, normal = ends[e, 0], ends[e, 1], corners[t, 0], normals[t]
        heights = np.stack(
            [np.einsum("ij,ij->i", end - origin, normal) for end in (a, b)], axis=1
        )
        across = heights[:, 0] * heights[:, 1] < 0
        a, b, heights, t = a[across], b[across], heights[across], t[across]
        share = heights[:, 0] / (heights[:, 0] - heights[:, 1])
        hit = a + share[:, None] * (b - a)
        inside = np.all(_barycentric(hit, corners[t]) > 0, axis=1)
        if inside.any():
            deepest = max(deepest, float(np.abs(heights[inside]).min(axis=1).max()))
    return deepest


def _barycentric(
    points: NDArray[np.float64], corners: NDArray[np.float64]
) -> NDArray[np.float64]:
    """Each point's barycentric weights in its triangle, in the triangle's plane."""
    u, v = corners[:, 1] - corners[:, 0], corners[:, 2] - corners[:, 0]
    w = points - corners[:, 0]
    uu, uv, vv = (np.einsum("ij,ij->i", p, q) for p, q in ((u, u), (u, v), (v, v)))
    wu, wv = np.einsum("ij,ij->i", w, u), np.einsum("ij,ij->i", w, v)
    area = uu * vv - uv * uv
    second, third = (vv * wu - uv * wv) / area, (uu * wv - uv * wu) / area
    return np.column_stack([1 - second - third, second, third])


class _Folder:
    """The simulated sheet, meshed from the build's last crease pattern."""

    def __init__(
        self,
        last: State,
        folds: Sequence[tuple[State, State]],
        scale: float,
        physics: Physics,
    ) -> None:
        self.last, self.scale, self.physics = last, scale, physics
        contact = physics.settings.contact
        layer = physics.material.thickness * contact.thickness_share + contact.gap
        # Without contact between layers, no layers lie inside a crease to bend round.
        widths = _widths(folds, last, layer) if contact.layers else {}
        self.creep = max(widths.values(), default=0.0)
        self.mesh, self.faces, lines = _mesh(last, scale, widths, physics)
        sheet = Sheet(self.mesh, physics.material)
        drives = _Drives.of(last, self.mesh, self.faces, lines, physics.bands.lines)
        spans = [sheet.add_span(stencil) for stencil in drives.stencils]
        self.simulation = Simulation([sheet], physics.settings)
        self.bands = drives.bands
        self.banded = np.zeros(len(self.mesh.rest), dtype=bool)
        for band, _ in self.bands:
            self.banded[np.array(band, dtype=np.int64).ravel()] = True
        table = physics.settings.table
        self.lying = 0.0 if table is None else table.height + 0.5 * layer
        # Paper below half a layer over the lying sheet touches the table.
        self.ground = -math.inf if table is None else self.lying + 0.5 * layer
        rest = self.mesh.rest
        self.simulation.set_positions(
            0, np.column_stack([rest, np.full(len(rest), self.lying)])
        )
        self.edges = np.array(drives.edges, dtype=np.int64).reshape(-1, 2)
        self.spans = np.array(spans, dtype=np.int64)
        self.ends = np.array(drives.ends, dtype=np.int64).reshape(-1, 2)
        corners = self.mesh.rest[self.mesh.triangles]
        (ax, ay), (bx, by) = ((corners[:, i] - corners[:, 0]).T for i in (1, 2))
        areas = 0.5 * np.abs(ax * by - ay * bx)
        # Each face's largest triangle, which lies flat away from the bands, shows which
        # way the face turns.
        self.sample = np.array(
            [
                int(np.argmax(np.where(self.faces == f, areas, -1.0)))
                for f in range(len(last.faces))
            ]
        )
        # Each vertex carries a third of the area of each triangle round it.
        lumped = np.bincount(
            self.mesh.triangles.ravel(),
            weights=np.repeat(areas / 3, 3),
            minlength=len(self.mesh.rest),
        )
        self.weights = np.outer(
            lumped * physics.material.mass_per_area, physics.settings.gravity
        )
        self.angles = np.zeros(len(self.ends))
        self.made = np.zeros(len(self.ends), dtype=bool)

    def _make(self, drive: int) -> None:
        """Make a drive: the crease along mesh edges, or a span across a band."""
        sheet, sharp = self.simulation, len(self.edges)
        if drive < sharp:
            u, v = self.edges[drive].tolist()
            sheet.set_crease(0, (u, v), self.physics.drive)
        else:
            sheet.set_span_crease(0, int(self.spans[drive - sharp]), self.physics.drive)
        self.made[drive] = True

    def _rest(self, drive: int, angle: float) -> None:
        """Set the angle a drive rests at."""
        sheet, sharp = self.simulation, len(self.edges)
        if drive < sharp:
            u, v = self.edges[drive].tolist()
            sheet.set_rest_angle(0, (u, v), angle)
        else:
            sheet.set_span_rest_angle(0, int(self.spans[drive - sharp]), angle)

    def fold(self, step: int, before: Shaped, after: Shaped) -> Stage:
        """Fold the paper from one state to the next, and measure it there.

        A face keeps still where neither its place nor its motion in space changes.
        """
        under = before.state.parents(after.state)
        start = placements(before.state, before.form)
        end = placements(after.state, after.form)
        held = np.array(
            [
                face.place == before.state.faces[under[f]].place
                and same(start[under[f]], end[f])
                for f, face in enumerate(after.state.faces)
            ]
        )
        parents = np.array(after.state.parents(self.last), dtype=np.int64)
        sheet = self.simulation
        sheet.hold(0, np.arange(len(self.mesh.rest)), held=False)
        if not held.any():
            self._move(step, [start[p] for p in under], end)
            frames, converged, iterations = (sheet.positions(0),), True, 0
        else:
            if not self.physics.settings.contact.layers:
                self._order(before, after)
            frames, converged, iterations = self._drive(
                before, after, parents, held[parents]
            )
        offset, misordered = self._measure(after, parents, frames[-1])
        return Stage(
            step,
            after.state,
            after.form,
            frames,
            converged,
            iterations,
            offset,
            misordered,
            crossing(frames[-1], self.mesh.triangles),
        )

    def settle(self, step: int, shaped: Shaped, settling: Step) -> Stage:
        """Turn the creases a settle step names toward its angles, the paper bending.

        No hand holds the paper: it bends as its stiffness and weight decide, and
        settles on the table. A settle stage leaves the state on purpose, so it
        measures no offset and no misordered pair.
        """
        goals = dict(settling.settle)
        parents = np.array(shaped.state.parents(self.last), dtype=np.int64)
        pairs = parents[self.ends]
        edge = {frozenset(faces): e for e, faces in shaped.state.creases.items()}
        drives: dict[int, float] = {}
        for drive, (f, g) in enumerate(pairs.tolist()):
            crease = edge.get(frozenset((f, g)))
            if crease in goals:
                drives[drive] = goals[crease]
        for drive in drives:
            if not self.made[drive]:
                self._make(drive)
        sheet = self.simulation
        sheet.hold(0, np.arange(len(self.mesh.rest)), held=False)
        closed = math.pi - self.physics.margin
        start = {drive: float(self.angles[drive]) for drive in drives}
        frames: list[NDArray[np.float64]] = []
        converged, iterations = True, 0
        count = self.physics.increments
        for i in range(1, count + 1):
            share = (1 - math.cos(math.pi * i / count)) / 2
            for drive, goal in drives.items():
                aim = float(np.clip(goal, -closed, closed))
                self._rest(drive, start[drive] + (aim - start[drive]) * share)
            report = sheet.step()
            converged &= report.converged
            iterations += report.iterations
            frames.append(sheet.positions(0))
        for drive, goal in drives.items():
            self.angles[drive] = float(np.clip(goal, -closed, closed))
        report = sheet.step()
        frames.append(sheet.positions(0))
        converged &= report.converged
        iterations += report.iterations
        return Stage(
            step,
            shaped.state,
            shaped.form,
            tuple(frames),
            converged,
            iterations,
            0.0,
            (),
            crossing(frames[-1], self.mesh.triangles),
            settled=True,
        )

    def _move(self, step: int, start: Sequence[Motion], end: Sequence[Motion]) -> None:
        """Move all the paper rigidly from where faces lie to where they go.

        The paper keeps the lowest height it reaches, so it rests where it rested.
        """
        motion = compose(invert(start[0]), end[0])
        if not all(
            same(compose(first, motion), last)
            for first, last in zip(start, end, strict=True)
        ):
            msg = f"step {step} moves all the paper, but not rigidly"
            raise PhysicsError(msg)
        positions = self.simulation.positions(0)
        rotation = np.array(motion[:9]).reshape(3, 3)
        moved = positions @ rotation.T + np.array(motion[9:]) * self.scale
        moved[:, 2] += positions[:, 2].min() - moved[:, 2].min()
        self.simulation.set_positions(0, moved)

    def _drive(
        self,
        before: Shaped,
        after: Shaped,
        parents: NDArray[np.int64],
        kept: NDArray[np.bool_],
    ) -> tuple[tuple[NDArray[np.float64], ...], bool, int]:
        """Turn the creases toward the state's angles; the faces ``kept`` stay still."""
        sheet = self.simulation
        pairs = parents[self.ends]
        fresh = ~self.made & (pairs[:, 0] != pairs[:, 1])
        # A band's lines take the crease the fold leaves as its spans first turn.
        crease = self.physics.bands.crease.band(2 * self.physics.bands.lines + 1)
        for band, drives in self.bands:
            if fresh[drives].any():
                for edge in band:
                    sheet.set_crease(0, edge, crease)
        for drive in np.flatnonzero(fresh).tolist():
            self._make(drive)
        made = np.flatnonzero(self.made)
        goals = self._angles(after, pairs[made])
        staying = np.zeros(len(self.mesh.rest), dtype=bool)
        staying[self.mesh.triangles[kept[self.faces]]] = True
        # Hands carry the weight of the paper the fold moves, as a folder's do.
        moving = np.flatnonzero(~staying)
        sheet.set_forces(0, moving, -self.weights[moving])
        frames: list[NDArray[np.float64]] = []
        sides = self._sides(made, goals, kept)
        mirror = self._turn_over() if sides == {-1} else None
        self._hold(staying)
        # Paper that turns both up and down needs room below, so the hands lift the
        # model off the table by as far as paper reaches.
        lift = self.physics.size if len(sides) > 1 else 0.0
        self._raise(lift)
        if mirror is not None or lift:
            frames.append(sheet.positions(0))
        converged, iterations = True, 0
        for angles in self._plan(before, after, pairs[made], goals):
            for drive, angle in zip(made.tolist(), angles.tolist(), strict=True):
                self._rest(drive, angle)
            report = sheet.step()
            converged &= report.converged
            iterations += report.iterations
            frames.append(sheet.positions(0))
        self.angles[made] = goals
        if mirror is not None:
            self._turn_over(mirror[0])
            frames.append(sheet.positions(0))
        if lift:
            # The hands lower the model until it touches the table again.
            positions = sheet.positions(0)
            self._raise(-min(lift, positions[:, 2].min() - self.lying))
            frames.append(sheet.positions(0))
        # The hands let go, and the paper settles under its weight.
        sheet.set_forces(0, moving, np.zeros((len(moving), 3)))
        report = sheet.step()
        converged &= report.converged
        iterations += report.iterations
        frames.append(sheet.positions(0))
        return tuple(frames), converged, iterations

    def _order(self, before: Shaped, after: Shaped) -> None:
        """Keep paper without thickness on its side of the layers it overlaps.

        The stacking the fold starts from orders the paper at once. The stacking it
        reaches orders paper that comes to overlap, once it comes over the other layer
        on its side. Paper of a face the fold turns to the other side of another face
        waits for that side alone. The rigid path may take it there at no motion, a
        crease folded flat one way meeting its state folded flat the other.
        """
        holding = self._orders(before)
        waiting = self._orders(after)
        # Where the fold reaches, each triangle and piece lie in faces of its state.
        reached = np.array(after.state.parents(self.last), dtype=np.int64)
        sides = {
            (reached[self.faces[triangle]], reached[piece], front)
            for triangle, piece, front in waiting
        }
        for key in list(holding):
            triangle, piece, front = key
            if (reached[self.faces[triangle]], reached[piece], not front) in sides:
                del holding[key]
            waiting.pop(key, None)
        orders = [(key, others, True) for key, others in holding.items()]
        orders += [(key, others, False) for key, others in waiting.items()]
        self.simulation.set_orders(
            0,
            np.array([triangle for (triangle, _, _), _, _ in orders], dtype=np.int64),
            [others for _, others, _ in orders],
            [front for (_, _, front), _, _ in orders],
            [holds for _, _, holds in orders],
        )

    def _orders(self, shaped: Shaped) -> dict[tuple[int, int, bool], list[int]]:
        """The orders a stacking gives, each to the triangles of a piece of paper.

        Each order is a triangle, the last pattern's face it keeps to, and the side,
        with that face's triangles. Each triangle of a face keeps to the front or back
        of a face stacked with it, as the stacking has it, where its centroid lies over
        that face. The triangle keeps to the triangles of the last pattern's face below
        its centroid, which move as one. Faces that shaping moves apart take no order.
        """
        state = shaped.state
        motions = shaped.form.motions if shaped.form else None
        owners = np.array(state.parents(self.last), dtype=np.int64)[self.faces]
        members = [np.flatnonzero(owners == f) for f in range(len(state.faces))]
        corners = self.mesh.rest[self.mesh.triangles]

        def placed(face: int, points: NDArray[np.float64]) -> NDArray[np.float64]:
            motion = embedding(state.faces[face].place)
            turn = np.array(motion[:9]).reshape(3, 3)[:2, :2]
            return points @ turn.T + np.array(motion[9:11]) * self.scale

        orders: dict[tuple[int, int, bool], list[int]] = {}
        for upper, lower in state.stacking:
            if motions is not None and not same(motions[upper], motions[lower]):
                continue
            # The upper face lies on the lower face's front where that front faces up.
            for face, other, front in (
                (upper, lower, not state.faces[lower].place.flips),
                (lower, upper, state.faces[upper].place.flips),
            ):
                kept, below = members[face], members[other]
                centroids = placed(face, corners[kept].mean(axis=1))
                over = _depths(centroids, placed(other, corners[below])) > -_NEAR
                for i, j in zip(*np.nonzero(over), strict=True):
                    piece = self.faces[below[j]]
                    orders.setdefault(
                        (int(kept[i]), int(piece), front),
                        below[self.faces[below] == piece].tolist(),
                    )
        return orders

    def _plan(
        self,
        before: Shaped,
        after: Shaped,
        pairs: NDArray[np.int64],
        goals: NDArray[np.float64],
    ) -> list[NDArray[np.float64]]:
        """The angles the creases between faces of ``after`` rest at, by increment.

        The creases follow the paper as rigid origami where its faces can fold rigidly
        from one flat state to the other. Elsewhere, and where shaping turns the paper
        in space, they turn together at even rates, and the faces bend as they must.
        Either way, they turn slowly near both ends. Flat paper there offers several
        ways to fold, and small turns keep to the one the step takes.
        """
        count = self.physics.increments
        shares = [(1 - math.cos(math.pi * i / count)) / 2 for i in range(1, count)]
        start = self.angles[self.made]
        flat = before.form is None and after.form is None
        forms = path(before.state, after.state, shares) if flat else None
        if forms is None:
            return [start + (goals - start) * share for share in (*shares, 1.0)]
        return [
            *(self._angles(Shaped(after.state, form), pairs) for form in forms),
            goals,
        ]

    def _angles(self, shaped: Shaped, pairs: NDArray[np.int64]) -> NDArray[np.float64]:
        """The angles the creases between pairs of faces rest at, as shaped paper lies.

        A crease pressed flat rests a margin short of a half turn.
        """
        state = shaped.state
        turns = crease_angles(state, placements(state, shaped.form))
        edge = {frozenset(faces): e for e, faces in state.creases.items()}
        closed = math.pi - self.physics.margin
        return np.clip(
            [turns[edge[frozenset((f, g))]] for f, g in pairs.tolist()],
            -closed,
            closed,
        )

    def _hold(self, staying: NDArray[np.bool_]) -> None:
        """Hold down the vertices ``staying`` where they lie on the table.

        A hand holds the paper beside the bands, which bend as they must. Where no paper
        that stays touches the table, the hand holds all that stays.
        """
        staying = staying & ~self.banded
        held = staying & (self.simulation.positions(0)[:, 2] < self.ground)
        if not held.any():
            held = staying
        self.simulation.hold(0, np.flatnonzero(held))

    def _sides(
        self,
        made: NDArray[np.int64],
        goals: NDArray[np.float64],
        kept: NDArray[np.bool_],
    ) -> set[int]:
        """The sides of the table the creases turn paper to from the faces ``kept``.

        A side is 1 above the kept paper and -1 below it. A crease's angle is positive
        toward the paper's front. Paper turning about a crease from a kept face sweeps
        the side of the face its angles share. The front of the kept face points up or
        down, so that side lies above or below. Without a table, no side matters.
        """
        if self.physics.settings.table is None:
            return set()
        positions = self.simulation.positions(0)
        sides = set[int]()
        for i, goal in zip(made.tolist(), goals.tolist(), strict=True):
            f, g = self.ends[i].tolist()
            if kept[f] == kept[g] or goal == self.angles[i]:
                continue
            a, b, c = positions[self.mesh.triangles[self.sample[f if kept[f] else g]]]
            up = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
            sides.add(int(np.sign(up * (goal + self.angles[i]))))
        sides.discard(0)
        return sides

    def _raise(self, height: float) -> None:
        """Raise the paper and what holds it by a height, lowering it if negative."""
        sheet = self.simulation
        held = sheet.held(0)
        sheet.hold(0, np.arange(len(held)), held=False)
        positions = sheet.positions(0)
        positions[:, 2] += height
        sheet.set_positions(0, positions)
        sheet.hold(0, np.flatnonzero(held))

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
        self, shaped: Shaped, parents: NDArray[np.int64], positions: NDArray[np.float64]
    ) -> tuple[float, tuple[Pair, ...]]:
        """How far paper at positions lies from a state, and which pairs misorder.

        Flat paper lies where its state puts it on the table. Shaped paper is first
        matched to where its form puts it, by the rigid motion that fits best.
        """
        state = shaped.state
        placed = placements(state, shaped.form)
        places: dict[int, Vector] = {}
        for f, face in enumerate(self.last.faces):
            for v in face.corners:
                x, y = self.last.vertices[v].floats()
                places.setdefault(v, apply(placed[parents[f]], (x, y, 0.0)))
        vertices = list(places)
        expected = np.array(list(places.values())) * self.scale
        found = positions[vertices]
        if shaped.form is None:
            found, expected = found[:, :2], expected[:, :2]
        else:
            expected = _fitted(expected, found)
        offset = float(np.linalg.norm(found - expected, axis=1).max())
        motions = shaped.form.motions if shaped.form else None
        misordered = tuple(
            (upper, lower)
            for upper, lower in sorted(state.above)
            if (motions is None or same(motions[upper], motions[lower]))
            and not self._stacked(state, parents, positions, upper, lower)
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
        """Whether the simulated paper lays a face above another where they overlap.

        Above is along the upper face's front in the simulated paper, or its back
        where the face shows the paper's back. The upper face may lie below the lower
        one by up to half the thickness contact keeps, as layers without it do.
        """
        point = center(intersection(state.folded[upper], state.folded[lower]))
        sample = self.sample[np.flatnonzero(parents == upper)[0]]
        a, b, c = positions[self.mesh.triangles[sample]]
        up = np.cross(b - a, c - a) * (-1 if state.faces[upper].place.flips else 1)
        up /= np.linalg.norm(up)
        high, low = (
            float(
                self._height(state.faces[face].place, parents == face, positions, point)
                @ up
            )
            for face in (upper, lower)
        )
        contact = self.physics.settings.contact
        kept = self.physics.material.thickness * contact.thickness_share
        return high > low - 0.5 * kept

    def _height(
        self,
        place: Isometry,
        within: NDArray[np.bool_],
        positions: NDArray[np.float64],
        point: Point,
    ) -> NDArray[np.float64]:
        """Where the simulated paper a face's place puts at a point lies.

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
        return weights[best] @ positions[corners[best]]


def _widths(
    folds: Sequence[tuple[State, State]], last: State, layer: float
) -> dict[Edge, float]:
    """The width of each crease band of the last pattern, in meters.

    A crease that wraps k layers bends round them in half a circle (k + 1) layers
    across. Creases a fold turns together, end to end along a straight line, share the
    widest band among them. They do so where they run on along the folded paper, and
    where they fold back over each other both wrapping layers, joined at their end.
    """
    wraps = _wraps(folds, last)
    joined = {edge: edge for edge in last.creases}

    def root(edge: Edge) -> Edge:
        while joined[edge] != edge:
            edge = joined[edge]
        return edge

    for before, after in folds:
        for first, second, onward in _lined(before, after, last):
            if onward or (first in wraps and second in wraps):
                joined[root(max(first, second))] = root(min(first, second))
    widest: dict[Edge, float] = {}
    for edge, layers in sorted(wraps.items()):
        width = 0.5 * math.pi * (layers + 1) * layer
        widest[root(edge)] = max(widest.get(root(edge), 0.0), width)
    return {edge: widest[root(edge)] for edge in last.creases if root(edge) in widest}


def _lined(before: State, after: State, last: State) -> list[tuple[Edge, Edge, bool]]:
    """The creases a fold turns that meet end to end along a straight line.

    Each pair comes with whether it runs on along the folded paper; a pair that does not
    folds back over itself.
    """
    parents, earlier = after.parents(last), before.parents(last)
    turned: dict[int, list[Edge]] = {}
    for edge, (f, g) in sorted(last.creases.items()):
        sense = "F" if parents[f] == parents[g] else after.sense(parents[f], parents[g])
        was = "F" if earlier[f] == earlier[g] else before.sense(earlier[f], earlier[g])
        if sense not in ("F", was):
            for vertex in edge:
                turned.setdefault(vertex, []).append(edge)
    pairs = []
    vertices = last.vertices
    for vertex, edges in sorted(turned.items()):
        for first, second in itertools.combinations(edges, 2):
            (a,) = set(first) - {vertex}
            (b,) = set(second) - {vertex}
            out, back = vertices[a] - vertices[vertex], vertices[b] - vertices[vertex]
            if out.cross(back).sign() != 0 or out.dot(back).sign() >= 0:
                continue
            face = after.faces[parents[last.creases[first][0]]].place
            other = after.faces[parents[last.creases[second][0]]].place
            middle = face(vertices[vertex])
            onward = (face(vertices[a]) - middle).dot(other(vertices[b]) - middle)
            pairs.append((first, second, onward.sign() < 0))
    return pairs


def _wraps(folds: Sequence[tuple[State, State]], last: State) -> dict[Edge, int]:
    """The creases of the last pattern that fold round other layers.

    Each takes the most layers it wraps in any fold.
    """
    wraps: dict[Edge, int] = {}
    for _, after in folds:
        parents = after.parents(last)
        for edge, (f, g) in sorted(last.creases.items()):
            pf, pg = parents[f], parents[g]
            if pf == pg or after.sense(pf, pg) == "F":
                continue
            stack = stack_at(after, _beside(after, last, edge, pf))
            if pf in stack and pg in stack:
                layers = abs(stack.index(pf) - stack.index(pg)) - 1
                if layers > wraps.get(edge, 0):
                    wraps[edge] = layers
    return wraps


_NUDGE: Final = Number.of(Fraction(1, 2**20))
"""How far beside a folded crease, as a share of the way to its face's center, the
layers it wraps are counted."""


def _beside(after: State, last: State, edge: Edge, face: int) -> Point:
    """A point just beside a folded crease of the last pattern, over one face."""
    a, b = (last.vertices[v] for v in edge)
    middle = after.faces[face].place((a + b) * HALF)
    return middle + (center(after.folded[face]) - middle) * _NUDGE


@dataclass(frozen=True)
class _Drives:
    """The drives of a mesh's creases: sharp ones along mesh edges, then spans."""

    edges: list[tuple[int, int]]
    """The mesh edge of each sharp drive."""
    stencils: list[tuple[int, int, int, int, int]]
    """The stencil of each span."""
    ends: list[tuple[int, int]]
    """The faces of the last pattern each drive joins, the sharp drives first."""
    bands: list[tuple[list[tuple[int, int]], list[int]]]
    """Each band's mesh edges, and its spans among the drives."""

    @classmethod
    def of(
        cls,
        last: State,
        mesh: Mesh,
        faces: NDArray[np.int64],
        lines: Sequence[_Line],
        count: int,
    ) -> _Drives:
        """The drives of a mesh of the last pattern with the lines of its bands.

        A crease drives the mesh edges along it, or turns its band by spans across it.
        ``count`` is the lines on each side of a band's crease.
        """
        triangles = _beside_edges(mesh)
        between = {frozenset(pair): edge for edge, pair in last.creases.items()}
        outer = {
            (line.edge, line.face): line.chain for line in lines if line.rank == count
        }
        # A band turns where it has an outer line on both sides.
        bands: dict[Edge, list[tuple[int, int]]] = {
            edge: []
            for edge, pair in last.creases.items()
            if all((edge, face) in outer for face in pair)
        }
        edges: list[tuple[int, int]] = []
        ends: list[tuple[int, int]] = []
        for u, v, _, _ in mesh.dihedrals.tolist():
            f, g = (int(faces[t]) for t in triangles[u, v])
            if f == g:
                continue
            edge = between[frozenset((f, g))]
            if edge in bands:
                bands[edge].append((u, v))
            else:
                edges.append((u, v))
                ends.append((f, g))
        for line in lines:
            if line.edge in bands:
                bands[line.edge].extend(
                    (min(u, v), max(u, v)) for u, v in itertools.pairwise(line.chain)
                )
        stencils: list[tuple[int, int, int, int, int]] = []
        found = []
        for edge, band in bands.items():
            f, g = last.creases[edge]
            crease = np.mean([mesh.rest[v] for v in edge], axis=0)
            spans = _spans(mesh, triangles, crease, outer[edge, f], outer[edge, g])
            first = len(edges) + len(stencils)
            found.append((band, list(range(first, first + len(spans)))))
            stencils.extend(spans)
            ends.extend((f, g) for _ in spans)
        return cls(edges, stencils, ends, found)


@dataclass(frozen=True)
class _Line:
    """A line of a crease band, in the mesh."""

    edge: Edge
    """The band's crease, in the last pattern."""
    face: int
    """The face of the last pattern the line runs in."""
    rank: int
    """The line's rank from the crease, from 1 to the lines on each side."""
    chain: list[int]
    """The mesh's vertices along the line, in order."""


def _mesh(
    last: State, scale: float, widths: Mapping[Edge, float], physics: Physics
) -> tuple[Mesh, NDArray[np.int64], list[_Line]]:
    """The last pattern meshed, with a band of ``widths`` along some of its creases.

    Returns:
        The mesh, each triangle's face, and the lines of the bands.
    """
    points = _Points([p.floats() for p in last.vertices], scale)
    faces = [face.corners for face in last.faces]
    longest = math.sqrt(4 * physics.refinement.max_area / math.sqrt(3))
    lines = _Layout(points, last, widths, physics.bands, longest).lines()
    segments = [pair for _, _, _, stops in lines for pair in itertools.pairwise(stops)]
    mesh, owners, chains = Mesh.of_faces(
        points.array(), points.polygons(faces), physics.refinement, segments
    )
    # Each line's vertices join the chains of its segments.
    pieces = iter(chains)
    found = []
    for edge, face, rank, stops in lines:
        chain = [stops[0]]
        for _ in range(len(stops) - 1):
            chain.extend(next(pieces).tolist()[1:])
        found.append(_Line(edge, face, rank, chain))
    return mesh, owners, found


_NEAR: Final = 1e-9
"""The distance under which two points of a mesh are one, in meters."""


class _Points:
    """Points of the plane, in meters, and the points each face's side passes through.

    Points closer than a nanometer are one.
    """

    def __init__(self, points: Sequence[tuple[float, float]], scale: float) -> None:
        self.points = [np.array(p) * scale for p in points]
        self.on: dict[frozenset[int], list[int]] = {}

    def add(
        self, point: NDArray[np.float64], side: frozenset[int] | None = None
    ) -> int:
        """The index of a point, added if new, on a face's side if ``side`` names it."""
        near = np.linalg.norm(np.array(self.points) - point, axis=1) < _NEAR
        if near.any():
            return int(np.argmax(near))
        self.points.append(point)
        if side is not None:
            self.on.setdefault(side, []).append(len(self.points) - 1)
        return len(self.points) - 1

    def array(self) -> NDArray[np.float64]:
        """Every point, of shape (n, 2)."""
        return np.array(self.points)

    def polygons(self, faces: Sequence[Sequence[int]]) -> list[list[int]]:
        """Each face's corners, with the points on its sides between them."""
        polygons = []
        for corners in faces:
            polygon = []
            for c, d in itertools.pairwise((*corners, corners[0])):
                polygon.append(c)
                polygon.extend(
                    sorted(
                        self.on.get(frozenset((c, d)), []),
                        key=lambda v, c=c: float(
                            np.linalg.norm(self.points[v] - self.points[c])
                        ),
                    )
                )
            polygons.append(polygon)
        return polygons


type _Frame = tuple[NDArray[np.float64], NDArray[np.float64], NDArray[np.float64]]
"""A band's crease in one of its faces: the crease's first point, its direction, and
the direction into the face."""

type _Stop = frozenset[int] | None
"""What a band's line ends on: a face's side, by its corners."""


class _Layout:
    """The lines of a band along each crease of ``widths``, added to ``points``.

    Each crease splits at stations, ``bands.edge`` apart at its ends, each piece
    ``bands.growth`` times the one before, up to ``longest``. The crease then gets
    ``bands.lines`` lines in each of its faces, parallel to it and evenly spaced, the
    last half the band's width away. A line runs across its face up to the face's sides,
    and splits at the stations and wherever it crosses another band's line.
    """

    def __init__(
        self,
        points: _Points,
        last: State,
        widths: Mapping[Edge, float],
        bands: Bands,
        longest: float,
    ) -> None:
        self.points, self.last, self.widths, self.bands = points, last, widths, bands
        # The corners of the sharp creases, which keep the cells round them coarse.
        self.sharp = {v for edge in last.creases if edge not in widths for v in edge}
        self.frames: dict[tuple[Edge, int], _Frame] = {}
        self.stations: dict[Edge, list[float]] = {}
        for edge in widths:
            a, b = (points.points[v] for v in edge)
            length = float(np.linalg.norm(b - a))
            along = (b - a) / length
            self.stations[edge] = _stations(length, bands.edge, bands.growth, longest)
            for distance in self.stations[edge]:
                points.add(a + along * distance, frozenset(edge))
            for face in last.creases[edge]:
                corners = last.faces[face].corners
                middle = np.mean([points.points[c] for c in corners], axis=0)
                inward = _left(along) * np.sign((middle - a) @ _left(along))
                self.frames[edge, face] = (a, along, inward)

    def lines(self) -> list[tuple[Edge, int, int, list[int]]]:
        """Each line, with its crease, its face, its rank and its points.

        Lines of two bands in one face share the point where they cross.
        """
        lines = [
            (edge, face, rank, stops)
            for edge, face in self.frames
            for rank in range(1, self.bands.lines + 1)
            if (stops := self._line(edge, face, rank))
        ]
        points = self.points.points
        ends = [(points[stops[0][1]], points[stops[-1][1]]) for *_, stops in lines]
        for i, (edge, face, _, stops) in enumerate(lines):
            for j in range(i + 1, len(lines)):
                other, elsewhere, _, crossed = lines[j]
                if other == edge or elsewhere != face:
                    continue
                if (point := _meet(*ends[i], *ends[j])) is None:
                    continue
                vertex = self.points.add(point)
                for crease, line in ((edge, stops), (other, crossed)):
                    start, along, _ = self.frames[crease, face]
                    line.append((float((point - start) @ along), vertex))
        found = []
        for edge, face, rank, stops in lines:
            ordered = [v for _, v in sorted(stops)]
            unique = [v for k, v in enumerate(ordered) if k == 0 or v != ordered[k - 1]]
            found.append((edge, face, rank, unique))
        return found

    def _line(self, edge: Edge, face: int, rank: int) -> list[tuple[float, int]]:
        """The points of a band's line, by their distance along its crease.

        The list is empty where the line misses its face.
        """
        a, along, inward = self.frames[edge, face]
        origin = a + inward * rank * self.widths[edge] / (2 * self.bands.lines)
        corners = self.last.faces[face].corners
        points = self.points.points
        bounds: list[tuple[NDArray[np.float64], NDArray[np.float64], _Stop]] = [
            (points[c], _left(points[d] - points[c]), frozenset((c, d)))
            for c, d in itertools.pairwise((*corners, corners[0]))
        ]
        (low, start), (high, stop) = _clip(origin, along, bounds)
        if high - low < _NEAR:
            return []
        return [
            (low, self._end(edge, origin + along * low, start)),
            *(
                (t, self.points.add(origin + along * t))
                for t in self.stations[edge]
                if low + _NEAR < t < high - _NEAR
            ),
            (high, self._end(edge, origin + along * high, stop)),
        ]

    def _runs_on(self, edge: Edge, corner: int) -> bool:
        """Whether a band runs on straight through one of its crease's ends.

        The band runs on into the next crease along its line, where that crease has the
        same band and one sharp crease alone crosses there, straight through.
        """
        vertices = self.last.vertices
        (a,) = set(edge) - {corner}
        out = vertices[a] - vertices[corner]
        onward, crossing = False, list[Point]()
        for crease in self.last.creases:
            if corner not in crease or crease == edge:
                continue
            (b,) = set(crease) - {corner}
            back = vertices[b] - vertices[corner]
            if out.cross(back).sign() == 0 and out.dot(back).sign() < 0:
                onward = self.widths.get(crease) == self.widths[edge]
            elif crease in self.widths:
                return False
            else:
                crossing.append(back)
        match crossing:
            case [first, second] if onward:
                straight = first.cross(second).sign() == 0
                return straight and first.dot(second).sign() < 0
            case _:
                return False

    def _end(self, edge: Edge, point: NDArray[np.float64], stop: _Stop) -> int:
        """The point a band's line ends at, on a face's side.

        A band's lines meet at its crease's end where a sharp crease meets it too: a
        sharp crease folds flat only with no points close beside it. The band runs on
        through a sharp crease that alone crosses it, since its lines then put points
        only on that crease. Elsewhere a line ends on the side it meets, and crosses the
        band of a crease there.
        """
        if stop is not None and len(shared := stop & set(edge)) == 1:
            (corner,) = shared
            if corner in self.sharp and not self._runs_on(edge, corner):
                return corner
        # The line's end splits the side as it joins the mesh, after the refinement.
        return self.points.add(point)


def _clip(
    origin: NDArray[np.float64],
    along: NDArray[np.float64],
    bounds: Sequence[tuple[NDArray[np.float64], NDArray[np.float64], _Stop]],
) -> tuple[tuple[float, _Stop], tuple[float, _Stop]]:
    """Where a line through ``origin`` enters and leaves the half-planes of ``bounds``.

    Each bound is a point on a half-plane's edge, its inward normal, and what the line
    ends on there. A line outside a half-plane it runs parallel to enters after it
    leaves.
    """
    low: tuple[float, _Stop] = (-math.inf, None)
    high: tuple[float, _Stop] = (math.inf, None)
    for start, normal, stop in bounds:
        rate = float(along @ normal)
        depth = float((origin - start) @ normal)
        if abs(rate) < 1e-12 * float(np.linalg.norm(normal)):
            if depth < 0:
                return (math.inf, None), (-math.inf, None)
            continue
        t = -depth / rate
        if rate > 0 and t > low[0]:
            low = (t, stop)
        elif rate < 0 and t < high[0]:
            high = (t, stop)
    return low, high


def _meet(
    p: NDArray[np.float64],
    q: NDArray[np.float64],
    a: NDArray[np.float64],
    b: NDArray[np.float64],
) -> NDArray[np.float64] | None:
    """Where segment ``pq`` crosses segment ``ab``, ends included, if it does."""
    u, v = q - p, b - a
    across = _cross(u, v)
    if abs(across) < 1e-12 * float(np.linalg.norm(u) * np.linalg.norm(v)):
        return None
    share, along = _cross(a - p, v) / across, _cross(a - p, u) / across
    if not (0 <= share <= 1 and 0 <= along <= 1):
        return None
    return p + u * share


def _stations(
    length: float, first: float, growth: float, longest: float
) -> list[float]:
    """Distances along a segment at which to split it, closest near its two ends.

    The pieces start ``first`` long at each end, and each grows by ``growth`` up to
    ``longest``. The middle splits evenly into pieces no longer than ``longest``.
    """
    near: list[float] = []
    end, piece = 0.0, min(first, longest)
    while end + piece < length / 2:
        end += piece
        near.append(end)
        piece = min(piece * growth, longest)
    pieces = max(1, math.ceil((length - 2 * end) / longest))
    middle = [end + (length - 2 * end) * k / pieces for k in range(1, pieces)]
    return [*near, *middle, *(length - d for d in reversed(near))]


def _beside_edges(mesh: Mesh) -> dict[tuple[int, int], list[int]]:
    """The triangles beside each mesh edge, its smaller vertex first."""
    triangles: dict[tuple[int, int], list[int]] = {}
    for t, corners in enumerate(mesh.triangles.tolist()):
        for a, b in itertools.pairwise((*corners, corners[0])):
            triangles.setdefault((min(a, b), max(a, b)), []).append(t)
    return triangles


def _spans(
    mesh: Mesh,
    triangles: Mapping[tuple[int, int], Sequence[int]],
    crease: NDArray[np.float64],
    near: Sequence[int],
    far: Sequence[int],
) -> list[tuple[int, int, int, int, int]]:
    """The spans across a band, from the outer line on its near side to the far one's.

    Each edge of the near line, with the triangle beyond it, gives a span. The span's
    far direction runs from the far line into the triangle beyond it, across from the
    edge.
    ``crease`` is a point of the band's crease.
    """
    rest = mesh.rest

    def beyond(chain: Sequence[int]) -> list[tuple[int, int, int]]:
        """Each edge of a line, with the corner of the triangle beyond it.

        The triangle lies on the line's far side from the crease.
        """
        found: list[tuple[int, int, int]] = []
        for a, b in itertools.pairwise(chain):
            inside = _cross(rest[b] - rest[a], crease - rest[a])
            found.extend(
                (a, b, c)
                for t in triangles[min(a, b), max(a, b)]
                for c in mesh.triangles[t].tolist()
                if c not in (a, b)
                and _cross(rest[b] - rest[a], rest[c] - rest[a]) * inside < 0
            )
        return found

    others = beyond(far)
    stencils = []
    for a, b, c in beyond(near):
        start, end = (
            (a, b) if _cross(rest[b] - rest[a], rest[c] - rest[a]) > 0 else (b, a)
        )
        middle = 0.5 * (rest[a] + rest[b])
        p, q, r = min(
            others,
            key=lambda o: float(
                np.linalg.norm(0.5 * (rest[o[0]] + rest[o[1]]) - middle)
            ),
        )
        # The far direction runs as square across the band as the triangle lets it.
        along = (rest[end] - rest[start]) / np.linalg.norm(rest[end] - rest[start])
        foot = min(
            (p, q),
            key=lambda v: (
                abs(float((rest[r] - rest[v]) @ along))
                / float(np.linalg.norm(rest[r] - rest[v]))
            ),
        )
        stencils.append((start, end, c, foot, r))
    return stencils


def _left(u: NDArray[np.float64]) -> NDArray[np.float64]:
    """A vector of the plane turned a quarter turn counterclockwise."""
    return np.array([-u[1], u[0]])


def _depths(
    points: NDArray[np.float64], polygons: NDArray[np.float64]
) -> NDArray[np.float64]:
    """How deep each point lies in each convex polygon.

    A depth is the least distance to the polygon's edges' lines, negative outside. The
    points are of shape (n, 2), the polygons of shape (m, k, 2), and the depths of shape
    (n, m).
    """
    edges = np.roll(polygons, -1, axis=1) - polygons
    inward = np.stack([-edges[..., 1], edges[..., 0]], axis=-1)
    inward /= np.linalg.norm(inward, axis=-1, keepdims=True)
    # A clockwise polygon, of negative area, has its inside to the right of its edges.
    x, y = polygons[..., 0], polygons[..., 1]
    areas = (x * np.roll(y, -1, axis=1) - np.roll(x, -1, axis=1) * y).sum(axis=1)
    inward *= np.sign(areas)[:, None, None]
    offsets = points[:, None, None, :] - polygons[None]
    return np.einsum("nmkd,mkd->nmk", offsets, inward).min(axis=2)


def _cross(u: NDArray[np.float64], v: NDArray[np.float64]) -> float:
    """The cross product of two vectors of the plane.

    The product is positive when ``v`` turns left of ``u``.
    """
    return float(u[0] * v[1] - u[1] * v[0])


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


def _fitted(
    points: NDArray[np.float64], target: NDArray[np.float64]
) -> NDArray[np.float64]:
    """Points moved by the rigid motion that brings them nearest a target, by Kabsch.

    Each point pairs with the target's point of the same row, and the motion keeps the
    points' handedness, so it never mirrors them.
    """
    middle, aim = points.mean(axis=0), target.mean(axis=0)
    u, _, vt = np.linalg.svd((points - middle).T @ (target - aim))
    turn = np.diag([1.0, 1.0, np.sign(np.linalg.det(u @ vt))])
    return (points - middle) @ (u @ turn @ vt) + aim
