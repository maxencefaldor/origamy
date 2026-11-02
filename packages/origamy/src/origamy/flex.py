# Copyright 2026 Maxence Faldor
"""Flexing: shaping that turns the creases around each vertex together.

A flex sets the fold angles of some creases, and the paper follows as rigid origami.
Every face stays rigid, and every other crease turns as little as it can while the
paper stays whole: around each vertex inside the paper, the turns about its creases
undo one another. A solver follows the creases from their angles to the new ones in
small steps, so the paper moves along one continuous path.
"""

from collections import deque
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from math import atan2, ceil, cos, pi, radians, sin, sqrt
from typing import Final

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
_FRAMES: Final = 8
"""How many parts the forms a flex passes through divide its path into."""
_TOUCH: Final = 1e-7
"""How far faces may reach through each other and only touch, as floats tell."""
_RANK: Final = 1e-9
"""How small a share of the largest singular value counts as none in a correction.

Paper pressed flat turns its creases about axes that all lie in one plane, so the
corrections there have directions that move nothing to first order."""
_SMALL: Final = 1e-6
"""A turn too small, in radians, to find its axis from, by floats."""


@dataclass(frozen=True, slots=True)
class _Loop:
    """The creases around a vertex inside the paper, counterclockwise.

    Each crease turns about its direction, which has the next face on its left.
    """

    creases: tuple[int, ...]
    directions: tuple[Vector, ...]


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


def path(before: State, after: State, frames: int) -> tuple[Form, ...] | None:
    """The forms flat paper passes through as a step turns its creases together.

    Each crease turns from its angle before the step to its angle after it, and every
    loop of creases stays closed, as rigid origami. The forms are motions from where
    faces lie after the step: where they lay before it, then one per share of the
    way. No forms come back when no face keeps its place, or when no rigid motion
    follows the creases.
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
    forms = [_form(after, start)]
    for frame in range(1, frames):
        share = frame / frames
        angles = [begin[e] + (finish[e] - begin[e]) * share for e in creases]
        if not _close(loops, angles, set()):
            return None
        forms.append(_form(after, _place(after, creases, angles, root, end[root])))
    return tuple(forms)


def _form(state: State, world: Sequence[Motion]) -> Form:
    """The form of paper whose faces lie in space where ``world`` puts them."""
    return Form(
        tuple(
            compose(invert(embedding(face.place)), motion)
            for face, motion in zip(state.faces, world, strict=True)
        )
    )


def _loops(state: State, index: Mapping[Edge, int]) -> list[_Loop]:
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
        loops.append(_Loop(tuple(loop), tuple(directions)))
    return loops


def _middle(state: State, face: int) -> tuple[float, float]:
    points = [p.floats() for p in state.patterns[face]]
    return (
        sum(x for x, _ in points) / len(points),
        sum(y for _, y in points) / len(points),
    )


def _close(loops: Sequence[_Loop], angles: list[float], driven: set[int]) -> bool:
    """Turn the free creases as little as closes every loop; whether it does.

    Each correction is Newton's: the smallest turns of the free creases that close
    every loop to first order, as least squares find them.
    """
    free = sorted({c for loop in loops for c in loop.creases} - driven)
    column = {crease: i for i, crease in enumerate(free)}
    for _ in range(_TRIES):
        residuals, jacobian = _linearize(loops, angles, column)
        if not residuals.size or float(np.max(np.abs(residuals))) < _CLOSED:
            return True
        if not free:
            return False
        change = np.linalg.lstsq(jacobian, residuals, rcond=_RANK)[0]
        for crease, delta in zip(free, change.tolist(), strict=True):
            angles[crease] -= delta
    return False


def _linearize(
    loops: Sequence[_Loop], angles: Sequence[float], column: Mapping[int, int]
) -> tuple[np.ndarray, np.ndarray]:
    """How far each loop stays open, and how each free crease's turn moves it.

    Turning crease i of a loop by a little turns the loop's product by as much about
    the crease's direction, carried by the turns before it.
    """
    residuals = np.zeros(3 * len(loops))
    jacobian = np.zeros((3 * len(loops), len(column)))
    for row, loop in enumerate(loops):
        product = np.eye(3)
        for crease, direction in zip(loop.creases, loop.directions, strict=True):
            axis = np.asarray(direction)
            if crease in column:
                jacobian[3 * row : 3 * row + 3, column[crease]] += product @ axis
            product = product @ _matrix(axis, angles[crease])
        residuals[3 * row : 3 * row + 3] = _log(product)
    return residuals, jacobian


def _matrix(axis: np.ndarray, angle: float) -> np.ndarray:
    """The turn by ``angle`` about a unit axis, as a matrix."""
    x, y, z = axis
    cross = np.array([[0.0, -z, y], [z, 0.0, -x], [-y, x, 0.0]])
    return np.eye(3) + sin(angle) * cross + (1 - cos(angle)) * (cross @ cross)


def _log(rotation: np.ndarray) -> np.ndarray:
    """A rotation as its axis times its angle."""
    skew = np.array(
        [
            rotation[2, 1] - rotation[1, 2],
            rotation[0, 2] - rotation[2, 0],
            rotation[1, 0] - rotation[0, 1],
        ]
    )
    angle = atan2(float(np.linalg.norm(skew)), float(np.trace(rotation)) - 1)
    scale = 0.5 if angle < _SMALL else angle / (2 * max(sin(angle), _SMALL))
    return scale * skew


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
