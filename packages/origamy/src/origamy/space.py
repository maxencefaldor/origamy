# Copyright 2026 Maxence Faldor
"""Space: where paper lies once shaping turns it out of the plane.

Each face of shaped paper lies where a rigid motion of space takes it from where it
lies flat. Motions are floats, a 3 by 3 rotation and a shift: they draw and export the
model, and every check still reads the flat paper.
"""

from collections.abc import Sequence
from dataclasses import dataclass
from math import cos, isclose, sin
from typing import Final

from origamy.geometry import Point
from origamy.state import State

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
