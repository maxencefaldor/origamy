# Copyright 2026 Maxence Faldor
"""The bridge to the exact kernel, `origamy_kernel`: numbers, points and places as data.

The kernel reads each number as its numerators and denominator, in the tower that
`origamy.field` holds now, so every call passes the tower's radicands along.
"""

from origamy.field import Number, radicands
from origamy.geometry import Isometry, Point
from origamy.state import State
from origamy_kernel.data import CreaseData, NumberData, PlaceData, PointData

type Sheet = tuple[
    list[NumberData],
    list[PointData],
    list[list[int]],
    list[PlaceData],
    list[CreaseData],
]
"""A state's paper as the kernel reads it: the tower's radicands, the vertices, each
face's corners and place, and each crease's vertices and faces."""


def number(value: Number) -> NumberData:
    """A number as its numerators and denominator."""
    return list(value.numerators), value.denominator


def from_number(data: NumberData) -> Number:
    """The number that data holds."""
    numerators, denominator = data
    return Number(tuple(numerators), denominator)


def point(p: Point) -> PointData:
    """A point as its two coordinates."""
    return number(p.x), number(p.y)


def from_point(data: PointData) -> Point:
    """The point that data holds."""
    return Point(from_number(data[0]), from_number(data[1]))


def place(isometry: Isometry) -> PlaceData:
    """An isometry as its matrix row by row, then its translation."""
    return (
        number(isometry.xx),
        number(isometry.xy),
        number(isometry.yx),
        number(isometry.yy),
        number(isometry.tx),
        number(isometry.ty),
    )


def from_place(data: PlaceData) -> Isometry:
    """The isometry that data holds."""
    return Isometry(*(from_number(entry) for entry in data))


def tower() -> list[NumberData]:
    """The tower's radicands, level 1 first."""
    return [number(radicand) for radicand in radicands()]


def sheet(state: State) -> Sheet:
    """A state's paper as the kernel reads it, its creases in the state's order."""
    return (
        tower(),
        [point(p) for p in state.vertices],
        [list(face.corners) for face in state.faces],
        [place(face.place) for face in state.faces],
        [(edge, faces) for edge, faces in state.creases.items()],
    )
