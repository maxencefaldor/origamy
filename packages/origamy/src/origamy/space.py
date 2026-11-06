# Copyright 2026 Maxence Faldor
"""Space: where paper lies once shaping turns it out of the plane.

Each face of shaped paper lies where a rigid motion of space takes it from where it
lies flat. Motions are floats, a 3 by 3 rotation and a shift: they draw and export the
model, and every check still reads the flat paper.
"""

from collections.abc import Callable, Sequence
from dataclasses import dataclass
from math import atan2, cos, degrees, isclose, pi, sin, sqrt
from typing import Final

from origamy.geometry import Isometry, Point
from origamy.state import Edge, State

type Vector = tuple[float, float, float]

type Motion = tuple[float, ...]
"""A rigid motion of space: a 3 by 3 rotation, row by row, then a shift."""

STILL: Motion = (1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0)
"""The motion that leaves every point where it is."""

_TOLERANCE: Final = 1e-9
"""How far apart two floats of a form may lie and still count as one."""


@dataclass(frozen=True, slots=True)
class Turn:
    """A rotation about an axis: through ``point``, along ``direction``, by radians."""

    point: Vector
    direction: Vector
    angle: float

    def motion(self, share: float = 1.0) -> Motion:
        """The rotation as a motion, or ``share`` of it."""
        x, y, z = self.direction
        c, s = cos(self.angle * share), sin(self.angle * share)
        t = 1 - c
        rotation = (
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
        px, py, pz = self.point
        moved = rotate(rotation, self.point)
        return (*rotation, px - moved[0], py - moved[1], pz - moved[2])


@dataclass(frozen=True, slots=True)
class Form:
    """Where each face lies in space: a rigid motion of where it lies flat."""

    motions: tuple[Motion, ...]

    @classmethod
    def flat(cls, faces: int) -> Form:
        """The form of paper that lies flat."""
        return cls((STILL,) * faces)

    def carried(self, state: State, later: State) -> Form:
        """The form of a later state, whose faces lie where their parents lie."""
        return Form(tuple(self.motions[p] for p in state.parents(later)))

    def point(self, face: int, point: Point) -> Vector:
        """Where a point of a face, as it lies flat, lies in space."""
        return apply(self.motions[face], (float(point.x), float(point.y), 0.0))

    def normal(self, face: int, *, flipped: bool) -> Vector:
        """The direction the front of a face looks toward."""
        return rotate(self.motions[face], (0.0, 0.0, -1.0 if flipped else 1.0))


def apply(motion: Motion, point: Vector) -> Vector:
    """Where a motion takes a point."""
    x, y, z = rotate(motion, point)
    return x + motion[9], y + motion[10], z + motion[11]


def compose(first: Motion, then: Motion) -> Motion:
    """The motion that does ``first``, then ``then``."""
    rows = [then[3 * i : 3 * i + 3] for i in range(3)]
    columns = [first[j::3][:3] for j in range(3)]
    rotation = tuple(
        sum(r * c for r, c in zip(row, column, strict=True))
        for row in rows
        for column in columns
    )
    shift = apply(then, (first[9], first[10], first[11]))
    return (*rotation, *shift)


@dataclass(frozen=True, slots=True)
class Shaped:
    """The paper as it lies flat, and its form in space once shaping turns it."""

    state: State
    form: Form | None = None
    """None while the paper lies flat."""


def invert(motion: Motion) -> Motion:
    """The motion that undoes a motion."""
    rotation = tuple(motion[3 * j + i] for i in range(3) for j in range(3))
    shift = rotate(rotation, (motion[9], motion[10], motion[11]))
    return (*rotation, -shift[0], -shift[1], -shift[2])


def embedding(place: Isometry) -> Motion:
    """A face's place as a rigid motion of space, from the crease pattern.

    A place that turns the face over turns it over in space too, so its front looks
    down: the motion stays a rotation, as rigid paper's must.
    """
    xx, xy, yx, yy, tx, ty = (
        float(n) for n in (place.xx, place.xy, place.yx, place.yy, place.tx, place.ty)
    )
    turned = -1.0 if place.flips else 1.0
    return (xx, xy, 0.0, yx, yy, 0.0, 0.0, 0.0, turned, tx, ty, 0.0)


def placements(state: State, form: Form | None) -> list[Motion]:
    """Where each face lies in space, as a rigid motion from the crease pattern."""
    motions = form.motions if form else (STILL,) * len(state.faces)
    return [
        compose(embedding(face.place), motion)
        for face, motion in zip(state.faces, motions, strict=True)
    ]


def crease_angles(
    state: State,
    placed: Sequence[Motion],
    sense: Callable[[int, int], str] | None = None,
) -> dict[Edge, float]:
    """Each crease's fold angle in radians, valleys positive, from the paper's front.

    The angle is the turn about the crease, on the crease pattern, that takes one of
    its faces to where the other lies. A fold pressed flat reads ``sense``, which
    says how two faces meet; by default, as they meet in ``state``.
    """
    meet = sense or state.sense
    angles = {}
    for edge, (f, g) in state.creases.items():
        _, normal = hinge(state, edge, g)
        relative = compose(placed[g], invert(placed[f]))
        across = rotate(relative, normal)
        angle = atan2(
            across[2], sum(a * b for a, b in zip(across, normal, strict=True))
        )
        if abs(abs(angle) - pi) < _TOLERANCE:
            angle = {"V": pi, "M": -pi}.get(meet(f, g), angle)
        angles[edge] = angle
    return angles


def fold_angles(state: State, form: Form | None) -> dict[Edge, float]:
    """Each crease's fold angle in degrees, as FOLD writes it: valleys positive."""
    angles = crease_angles(state, placements(state, form))
    return {edge: degrees(angle) for edge, angle in angles.items()}


def hinge(state: State, edge: Edge, face: int) -> tuple[Vector, Vector]:
    """A crease's direction with ``face`` on its left, and the way into that face.

    Both lie on the crease pattern: a turn about the direction by a positive angle
    lifts the face toward the paper's front.
    """
    (ax, ay), (bx, by) = (state.vertices[v].floats() for v in edge)
    length = sqrt((bx - ax) ** 2 + (by - ay) ** 2)
    ux, uy = (bx - ax) / length, (by - ay) / length
    # A face's corners run counterclockwise, so it lies left of a side it runs along.
    corners = state.faces[face].corners
    after = corners[(corners.index(edge[0]) + 1) % len(corners)]
    if after != edge[1]:
        ux, uy = -ux, -uy
    return (ux, uy, 0.0), (-uy, ux, 0.0)


def same(first: Motion, second: Motion) -> bool:
    """Whether two motions agree, as far as floats tell."""
    return all(
        isclose(a, b, abs_tol=_TOLERANCE) for a, b in zip(first, second, strict=True)
    )


def rotate(motion: Sequence[float], vector: Vector) -> Vector:
    """Where the rotation of a motion takes a vector."""
    x, y, z = vector
    return (
        motion[0] * x + motion[1] * y + motion[2] * z,
        motion[3] * x + motion[4] * y + motion[5] * z,
        motion[6] * x + motion[7] * y + motion[8] * z,
    )
