# Copyright 2026 Maxence Faldor
"""Exact constructions of points and fold lines, after the Huzita-Justin axioms.

A person finds a fold line on real paper by lining up points and lines already there.
These functions build the same lines exactly, so a script says how it finds each fold.
`Line.through` is the first axiom; the rest are here.
"""

from origamy.errors import OrigamyError
from origamy.field import Number
from origamy.geometry import HALF, Line, Point, PointLike


def midpoint(p: PointLike, q: PointLike) -> Point:
    """The point halfway between two points.

    Examples:
        >>> print(midpoint((0, 0), (1, "1/2")))
        (1/2, 1/4)
    """
    p, q = Point.of(p), Point.of(q)
    return (p + q) * HALF


def meet(first: Line, second: Line) -> Point:
    """The point where two lines cross.

    Examples:
        >>> print(meet(Line.through((0, 0), (1, 1)), Line.through((0, 1), (1, 0))))
        (1/2, 1/2)
    """
    point = first.meet(second)
    if point is None:
        msg = f"the lines {first} and {second} are parallel, so they do not meet"
        raise OrigamyError(msg)
    return point


def fold_onto(p: PointLike, q: PointLike) -> Line:
    """The fold line that puts point ``p`` onto point ``q`` (axiom 2).

    Examples:
        >>> print(fold_onto((0, 0), (1, 0)))
        x = 1/2
    """
    p, q = Point.of(p), Point.of(q)
    if p == q:
        msg = f"folding a point onto itself has no single line: got {p} twice"
        raise OrigamyError(msg)
    return Line.make(q - p, (q - p).dot(midpoint(p, q)))


def perpendicular(line: Line, through: PointLike) -> Line:
    """The fold line through a point that folds a line onto itself (axiom 4).

    Examples:
        >>> print(perpendicular(Line.through((0, 0), (1, 0)), (1, 1)))
        x = 1
    """
    through = Point.of(through)
    return Line.make(line.direction, line.direction.dot(through))


def bisect(a: PointLike, vertex: PointLike, b: PointLike) -> Line:
    """The fold line that halves the angle from ``a`` through ``vertex`` to ``b``.

    The line puts the ray toward ``a`` onto the ray toward ``b`` (axiom 3). Its
    direction takes the rays' lengths, square roots that stay exact.

    Examples:
        >>> print(bisect((1, 0), (0, 0), (0, 1)))
        x - y = 0
        >>> print(bisect((1, 0), (0, 0), (1, 1)))
        x - (1 + √2)y = 0
    """
    a, vertex, b = Point.of(a), Point.of(vertex), Point.of(b)
    u, v = a - vertex, b - vertex
    if not (u.x or u.y) or not (v.x or v.y):
        msg = f"an angle needs two points apart from its vertex {vertex}"
        raise OrigamyError(msg)
    direction = u * _length(v) + v * _length(u)
    if not (direction.x or direction.y):
        return perpendicular(Line.through(vertex, a), vertex)
    return Line.at(vertex, direction)


def _length(vector: Point) -> Number:
    return vector.dot(vector).sqrt()
