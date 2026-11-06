# Copyright 2026 Maxence Faldor
"""Flexing: shaping that turns the creases around each vertex together.

A flex sets the fold angles of some creases, and the paper follows as rigid origami.
Every face stays rigid, and every other crease turns as little as it can while the
paper stays whole: around each vertex inside the paper, the turns about its creases
undo one another. A solver follows the creases from their angles to the new ones in
small steps, so the paper moves along one continuous path.
"""

from bisect import bisect_left
from collections import deque
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from itertools import accumulate, pairwise
from math import atan2, ceil, cos, inf, pi, radians, remainder, sin, sqrt, tau
from typing import Final, cast

import numpy as np

from origamy.actions import Outcome
from origamy.geometry import center, intersection
from origamy.rules import RefusalError
from origamy.space import (
    Form,
    Motion,
    Shaped,
    Vector,
    apply,
    compose,
    crease_angles,
    embedding,
    hinge,
    invert,
    placements,
    rotate,
    same,
)
from origamy.state import Edge, State

_STEP: Final = radians(2)
"""The most a named crease turns between two corrections of the solver."""
_CLOSED: Final = 1e-11
"""How far from undone the turns around a vertex may stay, in radians."""
_TRIES: Final = 40
"""How many corrections the solver makes at each step before it gives up."""
_SHRINK: Final = 0.9
"""How much a correction must shrink the loops' largest gap to count as progress."""
_STALLED: Final = 5
"""How many corrections in a row without progress make the solver give up early."""
_FRAMES: Final = 8
"""How many parts the forms a flex passes through divide its path into."""
_TOUCH: Final = 1e-7
"""How far faces may reach through each other and only touch, as floats tell."""
_JUMP: Final = radians(8)
"""The most a crease turns between two small parts of a path that stays continuous."""
_START: Final = 8
"""How many small turns from its start paper may first meet the nearest closed loops."""
_RANK: Final = 1e-9
"""How small a share of the largest singular value counts as none in a correction.

Paper pressed flat turns its creases about axes that all lie in one plane, so the
corrections there have directions that move nothing to first order."""
_SMALL: Final = 1e-6
"""A turn too small, in radians, to find its axis from, by floats."""


@dataclass(frozen=True, slots=True)
class _Loops:
    """The creases around each vertex inside the paper, counterclockwise, as arrays.

    Row i holds the loop round one vertex, padded after its end with creases of no
    turn. Each crease turns about its direction, which has the next face on its left.
    """

    creases: np.ndarray
    """Each loop's creases, of shape (loops, longest), -1 past a loop's end."""
    directions: np.ndarray
    """Each crease's direction, of shape (loops, longest, 3)."""
    cross: np.ndarray
    """Each direction's cross product as a matrix, of shape (loops, longest, 3, 3)."""

    @classmethod
    def of(cls, loops: Sequence[tuple[list[int], list[Vector]]]) -> _Loops:
        """Loops packed from each loop's creases and their directions."""
        longest = max((len(creases) for creases, _ in loops), default=0)
        creases = np.full((len(loops), longest), -1, dtype=np.int64)
        directions = np.zeros((len(loops), longest, 3))
        for row, (loop, ways) in enumerate(loops):
            creases[row, : len(loop)] = loop
            directions[row, : len(loop)] = ways
        x, y, z = (directions[..., i] for i in range(3))
        zero = np.zeros_like(x)
        cross = np.stack(
            [
                np.stack([zero, -z, y], axis=-1),
                np.stack([z, zero, -x], axis=-1),
                np.stack([-y, x, zero], axis=-1),
            ],
            axis=-2,
        )
        return cls(creases, directions, cross)

    @property
    def used(self) -> set[int]:
        """Every crease some loop holds."""
        return set(self.creases[self.creases >= 0].tolist())

    def turns(self, angles: np.ndarray) -> np.ndarray:
        """Each crease's turn about its direction, as a matrix, by Rodrigues' formula.

        The matrices have shape (loops, longest, 3, 3).
        """
        turned = np.where(self.creases >= 0, angles[self.creases], 0.0)
        sine, cosine = np.sin(turned)[..., None, None], np.cos(turned)[..., None, None]
        return np.eye(3) + sine * self.cross + (1 - cosine) * (self.cross @ self.cross)


def flex(paper: Shaped, targets: Mapping[Edge, float], fixed: int) -> Outcome:
    """Turn the named creases to their fold angles, in radians, the paper following.

    Face ``fixed`` stays where it lies in space. Each other crease turns as little as
    it can, and every face stays rigid.
    """
    state = paper.state
    placed = placements(state, paper.form)
    start = crease_angles(state, placed)
    creases = sorted(state.creases)
    index = {edge: i for i, edge in enumerate(creases)}
    loops = _loops(state, index)
    angles = [start[edge] for edge in creases]
    driven = {index[edge]: angle for edge, angle in targets.items()}
    reach = max((abs(a - angles[i]) for i, a in driven.items()), default=0.0)
    begin = {i: angles[i] for i in driven}
    count = max(1, ceil(reach / _STEP))
    marks = {round(count * k / _FRAMES) for k in range(1, _FRAMES)} - {0, count}
    frames = []
    for step in range(1, count + 1):
        for i, target in driven.items():
            angles[i] = begin[i] + (target - begin[i]) * step / count
        if not _close(loops, angles, set(driven)):
            msg = "no rigid motion of the faces turns the creases to those angles"
            raise RefusalError(msg, rule="no-flex", state=state)
        through = [creases[i] for i, a in enumerate(angles) if abs(a) > pi + _CLOSED]
        if through:
            faces = tuple(sorted({f for edge in through for f in state.creases[edge]}))
            msg = f"the creases between faces {_ids(faces)} fold past flat"
            raise RefusalError(
                msg, rule="turns-through-paper", faces=faces, state=state
            )
        if step == 1:
            first = _form(state, _place(state, creases, angles, fixed, placed[fixed]))
            _leave_stacks(paper, first)
        if step in marks or step == count:
            frames.append(list(angles))
    origin = placed[fixed]
    worlds = [_place(state, creases, a, fixed, origin) for a in frames]
    for world in worlds:
        crossed = _crossing(state, world)
        if crossed:
            msg = f"faces {_ids(crossed)} pass through each other"
            raise RefusalError(
                msg, rule="turns-through-paper", faces=crossed, state=state
            )
    path = tuple(_form(state, world) for world in worlds[:-1])
    form = _form(state, worlds[-1])
    old = paper.form or Form.flat(len(state.faces))
    moved = frozenset(
        f for f, motion in enumerate(form.motions) if not same(motion, old.motions[f])
    )
    return Outcome(state, moved, form, path=path)


def _leave_stacks(paper: Shaped, first: Form) -> None:
    """Refuse a flex that starts by turning a layer into the layers it lies on.

    Faces stacked in one plane part as the flex begins: wherever two overlap, the
    upper must not move below the lower, measured across their plane.
    """
    state = paper.state
    form = paper.form or Form.flat(len(state.faces))
    sunk = set()
    for upper, lower in state.above:
        if not same(form.motions[upper], form.motions[lower]):
            continue
        common = intersection(state.folded[upper], state.folded[lower])
        x, y = center(common).floats()
        across = rotate(form.motions[upper], (0.0, 0.0, 1.0))
        rise = _dot(
            _minus(
                apply(first.motions[upper], (x, y, 0.0)),
                apply(first.motions[lower], (x, y, 0.0)),
            ),
            across,
        )
        if rise < -_TOUCH:
            sunk.update((upper, lower))
    if sunk:
        faces = tuple(sorted(sunk))
        msg = f"faces {_ids(faces)} turn into the layers they lie on"
        raise RefusalError(msg, rule="turns-through-paper", faces=faces, state=state)


def _crossing(state: State, world: Sequence[Motion]) -> tuple[int, int] | None:
    """Two faces that pass through each other in space, if any do.

    Faces cross when each runs through the other's plane, from one side to the
    other, along a stretch the two share. Faces that touch along an edge, or lie in
    one plane, as stacked layers do, do not cross.
    """
    solids = [
        [apply(world[f], (*state.vertices[v].floats(), 0.0)) for v in face.corners]
        for f, face in enumerate(state.faces)
    ]
    boxes = [
        [(min(p[k] for p in s), max(p[k] for p in s)) for k in range(3)] for s in solids
    ]
    for f in range(len(solids)):
        for g in range(f + 1, len(solids)):
            if all(
                lo < hi_ + _TOUCH and lo_ < hi + _TOUCH
                for (lo, hi), (lo_, hi_) in zip(boxes[f], boxes[g], strict=True)
            ) and _cross(solids[f], solids[g]):
                return f, g
    return None


def _cross(first: Sequence[Vector], second: Sequence[Vector]) -> bool:
    """Whether two convex polygons in space pass through each other."""
    normal_a, normal_b = _normal(first), _normal(second)
    sides_a = [_dot(normal_b, _minus(p, second[0])) for p in first]
    sides_b = [_dot(normal_a, _minus(p, first[0])) for p in second]
    if not _straddles(sides_a) or not _straddles(sides_b):
        return False
    line = _cross3(normal_a, normal_b)
    if _dot(line, line) < _SMALL * _SMALL:
        return False
    low_a, high_a = _span(first, sides_a, line)
    low_b, high_b = _span(second, sides_b, line)
    return min(high_a, high_b) - max(low_a, low_b) > _TOUCH


def _straddles(sides: Sequence[float]) -> bool:
    return min(sides) < -_TOUCH and max(sides) > _TOUCH


def _span(
    polygon: Sequence[Vector], sides: Sequence[float], line: Vector
) -> tuple[float, float]:
    """Where a polygon meets the other's plane, as an interval along ``line``."""
    reach = []
    for (p, a), (q, b) in zip(
        zip(polygon, sides, strict=True),
        zip((*polygon[1:], polygon[0]), (*sides[1:], sides[0]), strict=True),
        strict=True,
    ):
        if (a < 0) != (b < 0):
            share = a / (a - b)
            point = tuple(pk + share * (qk - pk) for pk, qk in zip(p, q, strict=True))
            reach.append(_dot(line, (point[0], point[1], point[2])))
    return min(reach), max(reach)


def _normal(polygon: Sequence[Vector]) -> Vector:
    n = _cross3(_minus(polygon[1], polygon[0]), _minus(polygon[2], polygon[0]))
    length = sqrt(_dot(n, n))
    return n[0] / length, n[1] / length, n[2] / length


def _minus(a: Vector, b: Vector) -> Vector:
    return a[0] - b[0], a[1] - b[1], a[2] - b[2]


def _dot(a: Vector, b: Vector) -> float:
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def _cross3(a: Vector, b: Vector) -> Vector:
    return (
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    )


def _ids(faces: Sequence[int]) -> str:
    return ", ".join(str(f) for f in faces)


def path(
    before: State, after: State, shares: Sequence[float]
) -> tuple[Form, ...] | None:
    """The forms flat paper passes through as a step turns its creases together.

    The creases turn from their angles before the step to their angles after it, and
    every loop of creases stays closed, as rigid origami. At each small part of the
    way, the paper takes the closed loops nearest the angles even rates of turning
    give. Where those jump to another path, the paper follows its own path on until
    it meets them again, so it moves along one continuous path. The creases between
    faces that keep their places stay as they are, so those faces stay still on the
    way. The forms are motions from where faces lie after the step, one at each share
    of the path's length. Share zero is where they lay before the step, and share one
    where they lie after it. No forms come back when no face keeps its place, or when
    no continuous rigid motion reaches the step's end.
    """
    parents = before.parents(after)
    start = [embedding(before.faces[p].place) for p in parents]
    end = [embedding(face.place) for face in after.faces]
    root = next((f for f in range(len(end)) if same(start[f], end[f])), None)
    if root is None:
        return None
    creases = sorted(after.creases)
    begin = crease_angles(
        after, start, lambda f, g: before.sense(parents[f], parents[g])
    )
    finish = crease_angles(after, end)
    loops = _loops(after, {edge: i for i, edge in enumerate(creases)})
    # The paper between faces that keep their places stays still on the way.
    kept = {f for f in range(len(end)) if same(start[f], end[f])}
    still = {i for i, e in enumerate(creases) if set(after.creases[e]) <= kept}
    first = [begin[e] for e in creases]
    turns = [finish[e] - begin[e] for e in creases]
    count = max(1, ceil(max(map(abs, turns), default=0.0) / _STEP))
    nearest = _Nearest(loops, first, turns, count, still)
    walk = _walk(loops, first, nearest, still)
    if walk is None:
        return None
    lengths = list(accumulate(_apart(p, q) for p, q in pairwise(walk)))
    forms = []
    for share in shares:
        if share == 0:
            forms.append(_form(after, start))
            continue
        angles = _along(walk, lengths, share)
        if not _close(loops, angles, still):
            return None
        forms.append(_form(after, _place(after, creases, angles, root, end[root])))
    return tuple(forms)


def _walk(
    loops: _Loops,
    first: list[float],
    nearest: _Nearest,
    held: set[int],
) -> list[list[float]] | None:
    """The angles of the closed loops paper passes through, in small turns.

    The walk starts at the first angles and ends at the last of the nearest closed
    loops. At each turn, the paper takes the next of the nearest closed loops a small
    turn away. Where none lies that near, it follows its own path on by a small turn,
    the way it last turned, until it meets them. No walk comes back when the paper
    cannot follow its path, or meets the nearest closed loops nowhere within twice
    their way, or within its first few turns from the start. The creases ``held``
    keep their angles.
    """
    walk: list[list[float]] = [first]
    k, followed = 0, 0
    steps = nearest.steps
    while k < steps:
        here = walk[-1]
        # Paper that has not moved yet meets the nearest closed loops within its first
        # few small turns, or not at all.
        reach = steps if len(walk) > 1 else min(steps, _START)
        meet = next(
            (
                j
                for j in range(k, reach)
                if (near := nearest(j)) is not None and _apart(near, here) <= _JUMP
            ),
            None,
        )
        if meet is not None:
            walk.append(cast("list[float]", nearest(meet)))
            k = meet + 1
            continue
        followed += 1
        last = _apart(here, walk[-2]) if len(walk) > 1 else 0.0
        if not last or followed > 2 * steps:
            return None
        ahead: list[float] = [
            a + remainder(a - b, tau) * _STEP / last
            for a, b in zip(here, walk[-2], strict=True)
        ]
        if not _close(loops, ahead, held) or _apart(ahead, here) > _JUMP:
            return None
        walk.append(ahead)
    return walk


class _Nearest:
    """The closed loops nearest the angles even rates of turning give, step by step.

    Each step's loops are closed the first time the walk asks for them, so a walk that
    stops early closes few.
    """

    def __init__(
        self,
        loops: _Loops,
        first: list[float],
        turns: list[float],
        steps: int,
        held: set[int],
    ) -> None:
        self.loops, self.first, self.turns = loops, first, turns
        self.steps, self.held = steps, held
        self.found: dict[int, list[float] | None] = {}

    def __call__(self, step: int) -> list[float] | None:
        """The closed loops nearest the even turns of a step, counted from 0."""
        if step not in self.found:
            share = (step + 1) / self.steps
            angles = [
                a + t * share for a, t in zip(self.first, self.turns, strict=True)
            ]
            closed = _close(self.loops, angles, self.held)
            self.found[step] = angles if closed else None
        return self.found[step]


def _along(
    walk: Sequence[list[float]], lengths: Sequence[float], share: float
) -> list[float]:
    """The angles a share of the way along a walk, between the angles it passes."""
    if not lengths or lengths[-1] == 0:
        return list(walk[-1] if share else walk[0])
    reach = share * lengths[-1]
    i = min(bisect_left(lengths, reach), len(lengths) - 1)
    low = lengths[i - 1] if i else 0.0
    part = (reach - low) / (lengths[i] - low) if lengths[i] > low else 1.0
    return [
        a + remainder(b - a, tau) * part
        for a, b in zip(walk[i], walk[i + 1], strict=True)
    ]


def _apart(first: Sequence[float], second: Sequence[float]) -> float:
    """How far apart two sets of angles lie: the most one crease turns between them.

    A crease turned a whole turn further lies where it lay, so a crease folded flat
    one way lies where it lies folded flat the other way.
    """
    return max(
        (abs(remainder(a - b, tau)) for a, b in zip(first, second, strict=True)),
        default=0.0,
    )


def _form(state: State, world: Sequence[Motion]) -> Form:
    """The form of paper whose faces lie in space where ``world`` puts them."""
    return Form(
        tuple(
            compose(invert(embedding(face.place)), motion)
            for face, motion in zip(state.faces, world, strict=True)
        )
    )


def _loops(state: State, index: Mapping[Edge, int]) -> _Loops:
    """The loop of creases around each vertex inside the paper."""
    edge_border = {v for edge in state.border for v in edge}
    around: dict[int, list[int]] = {}
    for f, face in enumerate(state.faces):
        for v in face.corners:
            around.setdefault(v, []).append(f)
    loops = []
    for v, faces in sorted(around.items()):
        if v in edge_border:
            continue
        x, y = state.vertices[v].floats()

        def angle(f: int, x: float = x, y: float = y) -> float:
            cx, cy = _middle(state, f)
            return atan2(cy - y, cx - x)

        ordered = sorted(faces, key=angle)
        loop, directions = [], []
        for f, g in zip(ordered, (*ordered[1:], ordered[0]), strict=True):
            edge = next(
                e for e, pair in state.creases.items() if v in e and set(pair) == {f, g}
            )
            direction, _ = hinge(state, edge, g)
            loop.append(index[edge])
            directions.append(direction)
        loops.append((loop, directions))
    return _Loops.of(loops)


def _middle(state: State, face: int) -> tuple[float, float]:
    points = [p.floats() for p in state.patterns[face]]
    return (
        sum(x for x, _ in points) / len(points),
        sum(y for _, y in points) / len(points),
    )


def _close(loops: _Loops, angles: list[float], driven: set[int]) -> bool:
    """Turn the free creases as little as closes every loop; whether it does.

    Each correction is Newton's: the smallest turns of the free creases that close
    every loop to first order, as least squares find them. The solver gives up when
    corrections stop shrinking the largest gap.
    """
    free = np.array(sorted(loops.used - driven), dtype=np.int64)
    column = np.full(len(angles), -1, dtype=np.int64)
    column[free] = np.arange(len(free))
    columns = np.where(loops.creases >= 0, column[loops.creases], -1)
    turned = np.array(angles, dtype=float)
    closed = False
    best, stalled = inf, 0
    for _ in range(_TRIES):
        residuals, jacobian = _linearize(loops, turned, columns, len(free))
        gap = float(np.max(np.abs(residuals))) if residuals.size else 0.0
        if gap < _CLOSED:
            closed = True
            break
        stalled = 0 if gap < _SHRINK * best else stalled + 1
        best = min(best, gap)
        if not len(free) or stalled == _STALLED:
            break
        turned[free] -= np.linalg.lstsq(jacobian, residuals, rcond=_RANK)[0]
    angles[:] = turned.tolist()
    return closed


def _linearize(
    loops: _Loops, angles: np.ndarray, columns: np.ndarray, free: int
) -> tuple[np.ndarray, np.ndarray]:
    """How far each loop stays open, and how each free crease's turn moves it.

    Turning crease i of a loop by a little turns the loop's product by as much about
    the crease's direction, carried by the turns before it. ``columns`` gives each
    crease of each loop its column, or -1 for a crease that stays.
    """
    count, longest = loops.creases.shape
    turns = loops.turns(angles)
    product = np.broadcast_to(np.eye(3), (count, 3, 3))
    jacobian = np.zeros((3 * count, free))
    rows = 3 * np.arange(count)[:, None] + np.arange(3)
    for k in range(longest):
        moved = product @ loops.directions[:, k, :, None]
        taken = columns[:, k] >= 0
        np.add.at(
            jacobian,
            (rows[taken], columns[taken, k][:, None]),
            moved[taken, :, 0],
        )
        product = product @ turns[:, k]
    return _log(product).ravel(), jacobian


def _log(rotations: np.ndarray) -> np.ndarray:
    """Rotations, of shape (n, 3, 3), each as its axis times its angle."""
    skew = np.stack(
        [
            rotations[:, 2, 1] - rotations[:, 1, 2],
            rotations[:, 0, 2] - rotations[:, 2, 0],
            rotations[:, 1, 0] - rotations[:, 0, 1],
        ],
        axis=-1,
    )
    angle = np.arctan2(
        np.linalg.norm(skew, axis=-1), np.trace(rotations, axis1=1, axis2=2) - 1
    )
    scale = np.where(
        angle < _SMALL, 0.5, angle / (2 * np.maximum(np.sin(angle), _SMALL))
    )
    return scale[:, None] * skew


def _place(
    state: State,
    creases: Sequence[Edge],
    angles: Sequence[float],
    root: int,
    origin: Motion,
) -> list[Motion]:
    """Where each face lies once the creases take their angles, from the root's place.

    A face lies where the face it was reached from lies, turned about their crease.
    """
    turned = dict(zip(creases, angles, strict=True))
    world: dict[int, Motion] = {root: origin}
    neighbors: dict[int, list[tuple[int, Edge]]] = {}
    for edge, (f, g) in state.creases.items():
        neighbors.setdefault(f, []).append((g, edge))
        neighbors.setdefault(g, []).append((f, edge))
    queue = deque([root])
    while queue:
        f = queue.popleft()
        for g, edge in neighbors.get(f, []):
            if g not in world:
                direction, _ = hinge(state, edge, g)
                point = state.vertices[edge[0]].floats()
                turn = _about(direction, (point[0], point[1], 0.0), turned[edge])
                world[g] = compose(turn, world[f])
                queue.append(g)
    return [world.get(f, origin) for f in range(len(state.faces))]


def _about(direction: Vector, point: Vector, angle: float) -> Motion:
    """The turn by ``angle`` about the line through ``point`` along ``direction``."""
    rotation = _rotation(direction, angle)
    moved = rotate(rotation, point)
    return (*rotation, point[0] - moved[0], point[1] - moved[1], point[2] - moved[2])


def _rotation(direction: Vector, angle: float) -> tuple[float, ...]:
    x, y, z = direction
    c, s = cos(angle), sin(angle)
    t = 1 - c
    return (
        t * x * x + c,
        t * x * y - s * z,
        t * x * z + s * y,
        t * x * y + s * z,
        t * y * y + c,
        t * y * z - s * x,
        t * x * z - s * y,
        t * y * z + s * x,
        t * z * z + c,
    )
