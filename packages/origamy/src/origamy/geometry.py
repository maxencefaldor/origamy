# Copyright 2026 Maxence Faldor
"""Exact plane geometry: points, lines, isometries and convex polygons.

Every coordinate is a `Number`, so every test here is exact: a point is on a line or it
is not. Each test tries float estimates first and computes exactly only when their
error bounds leave the answer open. Folding the paper along a line reflects it across
that line, and a reflection takes its entries from the line's own coordinates.
"""

from collections.abc import Callable, Iterable, Sequence
from dataclasses import dataclass
from fractions import Fraction
from typing import Final, override

from origamy.field import ONE, ZERO, Exact, Number, Real, remembered

type PointLike = Point | tuple[Exact, Exact]
"""What a script may write for a point: a `Point`, or a pair of exact numbers."""

type Polygon = tuple[Point, ...]
"""A convex polygon, its corners counterclockwise."""

_FEWEST_CORNERS: Final = 3
"""The fewest corners a polygon has."""

_EXACT_STEP: Final = 3
"""The degrees between turns whose cosine and sine the tower holds: 18° less 15°."""

_QUARTER: Final = 90
"""The degrees in a quarter turn."""


@dataclass(frozen=True, slots=True)
class Point:
    """A point of the plane, or the vector to it from the origin.

    Examples:
        >>> Point.of(("1/2", 0)) + Point.of((0, "√2"))
        Point.of(('1/2', '√2'))
    """

    x: Number
    y: Number

    @classmethod
    def of(cls, value: PointLike) -> Point:
        """Convert a pair of exact numbers to a `Point`."""
        if isinstance(value, Point):
            return value
        x, y = value
        return cls(Number.of(x), Number.of(y))

    def __add__(self, other: Point) -> Point:
        """The sum of two vectors."""
        return Point(self.x + other.x, self.y + other.y)

    def __sub__(self, other: Point) -> Point:
        """The vector from ``other`` to this point."""
        return Point(self.x - other.x, self.y - other.y)

    def __mul__(self, factor: Real) -> Point:
        """The vector scaled by a number."""
        return Point(self.x * factor, self.y * factor)

    __rmul__ = __mul__

    def __neg__(self) -> Point:
        """The opposite vector."""
        return Point(-self.x, -self.y)

    @classmethod
    def polar(cls, radius: Exact, degrees: Exact) -> Point:
        """The point ``radius`` from the origin, turned ``degrees`` from the x-axis.

        Examples:
            >>> Point.polar(2, 30) == Point.of(("√3", 1))
            True
        """
        return cls(Number.of(radius), ZERO).turned(degrees)

    def turned(self, degrees: Exact) -> Point:
        """The vector turned counterclockwise by a multiple of 3°.

        The tower holds the cosine and sine of every multiple of 3°, since 3° is 18°
        less 15°, and both of those are constructible.

        Examples:
            >>> Point.of((1, 0)).turned(45) == Point.of(("√2/2", "√2/2"))
            True
        """
        turn = Number.of(degrees).fraction()
        if turn.denominator != 1 or turn.numerator % _EXACT_STEP:
            msg = f"{turn}° is not a multiple of 3°, so no exact turn reaches it"
            raise ValueError(msg)
        k = turn.numerator // _EXACT_STEP
        return _times(self, _times(_unit(18 * k), _unit(-15 * k)))

    def dot(self, other: Point) -> Number:
        """The dot product with another vector."""
        return self.x * other.x + self.y * other.y

    def cross(self, other: Point) -> Number:
        """The z component of the cross product: positive when `other` turns left."""
        return self.x * other.y - self.y * other.x

    def floats(self) -> tuple[float, float]:
        """The coordinates as floats, for pictures."""
        return float(self.x), float(self.y)

    @override
    def __str__(self) -> str:
        """The coordinates, such as ``(1/2, 0)``."""
        return f"({self.x}, {self.y})"

    @override
    def __repr__(self) -> str:
        """Text that reads back as this point, when its radicals are integers."""
        return f"Point.of(({str(self.x)!r}, {str(self.y)!r}))"


def _times(vector: Point, unit: Point) -> Point:
    """A vector turned by the angle of a unit vector: their complex product."""
    return Point(
        vector.x * unit.x - vector.y * unit.y, vector.x * unit.y + vector.y * unit.x
    )


@remembered
def _unit(degrees: int) -> Point:
    """The unit vector at a multiple of 15° or 18°, counterclockwise of the x-axis."""
    quarter, rest = divmod(degrees % 360, _QUARTER)
    match rest:
        case 0:
            c, s = ONE, ZERO
        case 15 | 30 | 45 | 60 | 75:
            cosines = {
                15: "√6/4 + √2/4",
                30: "√3/2",
                45: "√2/2",
                60: "1/2",
                75: "√6/4 - √2/4",
            }
            c, s = Number.of(cosines[rest]), Number.of(cosines[_QUARTER - rest])
        case 18 | 36 | 54 | 72:
            five = Number.of(5).sqrt()
            low, high = (10 - 2 * five).sqrt() / 4, (10 + 2 * five).sqrt() / 4
            golden, small = (five + 1) / 4, (five - 1) / 4
            cosines = {18: high, 36: golden, 54: low, 72: small}
            c, s = cosines[rest], cosines[_QUARTER - rest]
        case _:
            msg = f"{degrees}° is a multiple of neither 15° nor 18°"
            raise ValueError(msg)
    for _ in range(quarter):
        c, s = -s, c
    return Point(c, s)


@dataclass(frozen=True, slots=True)
class Line:
    """The points ``p`` with ``normal · p == offset``.

    The normal is scaled so that its first nonzero coordinate is 1, so two descriptions
    of one line compare equal.

    Examples:
        >>> Line.through((0, 0), (1, 1)) == Line.through((2, 2), (-1, -1))
        True
        >>> print(Line.through((0, 0), (1, 1)))
        x - y = 0
    """

    normal: Point
    offset: Number

    def __post_init__(self) -> None:
        """Refuse a line without a direction."""
        if not (self.normal.x or self.normal.y):
            msg = "a line needs two distinct points"
            raise ValueError(msg)

    @classmethod
    def make(cls, normal: Point, offset: Number) -> Line:
        """The line ``normal · p == offset``, scaled so equal lines compare equal."""
        scale = normal.x or normal.y
        if scale == ONE:
            return cls(normal, offset)
        return cls(normal * (1 / scale), offset / scale)

    @classmethod
    def through(cls, p: PointLike, q: PointLike) -> Line:
        """The line through two distinct points."""
        p, q = Point.of(p), Point.of(q)
        direction = q - p
        if not (direction.x or direction.y):
            msg = f"a line needs two distinct points: got {p} twice"
            raise ValueError(msg)
        normal = Point(-direction.y, direction.x)
        return cls.make(normal, normal.dot(p))

    @classmethod
    def at(cls, point: PointLike, direction: PointLike) -> Line:
        """The line through a point, along a direction."""
        point = Point.of(point)
        return cls.through(point, point + Point.of(direction))

    @property
    def point(self) -> Point:
        """A point of the line: where it meets the x axis, or else the y axis."""
        if self.normal.x:
            return Point(self.offset / self.normal.x, ZERO)
        return Point(ZERO, self.offset / self.normal.y)

    @property
    def direction(self) -> Point:
        """A vector along the line, with the line's normal on its right."""
        return Point(-self.normal.y, self.normal.x)

    def value(self, p: Point) -> Number:
        """``normal · p - offset``: zero on the line, its sign giving the side."""
        return self.normal.x * p.x + self.normal.y * p.y - self.offset

    def side(self, p: Point) -> int:
        """Return 1 or -1 for the side of the line ``p`` is on, or 0 when on it."""
        normal = self.normal
        estimate = (
            normal.x.estimate() * p.x.estimate()
            + normal.y.estimate() * p.y.estimate()
            - self.offset.estimate()
        )
        side = estimate.sign()
        return self.value(p).sign() if side is None else side

    def contains(self, p: Point) -> bool:
        """Whether ``p`` is on the line."""
        return not self.side(p)

    def reflect(self, p: Point) -> Point:
        """The mirror image of ``p`` across the line."""
        return p - self.normal * (2 * self.value(p) / self.normal.dot(self.normal))

    def meet(self, other: Line) -> Point | None:
        """The point where two lines cross, or None when they are parallel."""
        determinant = self.normal.cross(other.normal)
        if not determinant:
            return None
        x = (self.offset * other.normal.y - other.offset * self.normal.y) / determinant
        y = (self.normal.x * other.offset - other.normal.x * self.offset) / determinant
        return Point(x, y)

    def cut(self, p: Point, q: Point) -> Point:
        """The point where the segment from ``p`` to ``q`` crosses the line."""
        vp = self.value(p)
        return p + (q - p) * (vp / (vp - self.value(q)))

    def parallel(self, other: Line) -> bool:
        """Whether the two lines are parallel or the same."""
        return not self.normal.cross(other.normal)

    @override
    def __str__(self) -> str:
        """The equation of the line, such as ``x + y = 1``."""
        terms = [
            (coefficient, name)
            for coefficient, name in ((self.normal.x, "x"), (self.normal.y, "y"))
            if coefficient
        ]
        text = ""
        for coefficient, name in terms:
            magnitude = abs(coefficient)
            if coefficient.rational:
                factor = "" if magnitude == ONE else str(magnitude)
            else:
                factor = f"({magnitude})"
            if not text:
                text = f"{'-' if coefficient < 0 else ''}{factor}{name}"
            else:
                text += f" {'-' if coefficient < 0 else '+'} {factor}{name}"
        return f"{text} = {self.offset}"


@dataclass(frozen=True, slots=True)
class Isometry:
    """The map ``p ↦ M p + t`` with ``M`` orthogonal: where folding puts the paper.

    A face of the paper lies where its isometry maps it. Each fold composes a
    reflection, so the isometry turns the face over when it is a reflection.
    """

    xx: Number
    xy: Number
    yx: Number
    yy: Number
    tx: Number
    ty: Number

    @classmethod
    def reflection(cls, line: Line) -> Isometry:
        """The reflection across a line."""
        n = line.normal
        scale = 2 / n.dot(n)
        return cls(
            1 - scale * n.x * n.x,
            -scale * n.x * n.y,
            -scale * n.x * n.y,
            1 - scale * n.y * n.y,
            scale * line.offset * n.x,
            scale * line.offset * n.y,
        )

    def __call__(self, p: Point) -> Point:
        """Where the isometry maps a point."""
        return Point(
            self.xx * p.x + self.xy * p.y + self.tx,
            self.yx * p.x + self.yy * p.y + self.ty,
        )

    def __matmul__(self, other: Isometry) -> Isometry:
        """The composition: ``other`` first, then ``self``."""
        return Isometry(
            self.xx * other.xx + self.xy * other.yx,
            self.xx * other.xy + self.xy * other.yy,
            self.yx * other.xx + self.yy * other.yx,
            self.yx * other.xy + self.yy * other.yy,
            self.xx * other.tx + self.xy * other.ty + self.tx,
            self.yx * other.tx + self.yy * other.ty + self.ty,
        )

    def inverse(self) -> Isometry:
        """The inverse map: an orthogonal matrix inverts by its transpose."""
        return Isometry(
            self.xx,
            self.yx,
            self.xy,
            self.yy,
            -(self.xx * self.tx + self.yx * self.ty),
            -(self.xy * self.tx + self.yy * self.ty),
        )

    @property
    def flips(self) -> bool:
        """Whether the map turns the paper over, showing its back.

        The determinant of an orthogonal matrix is 1 or -1, so the estimates of its
        entries almost always tell its sign without exact arithmetic.
        """
        xx, xy, yx, yy = (n.estimate() for n in (self.xx, self.xy, self.yx, self.yy))
        sign = (xx * yy - xy * yx).sign()
        if sign is None:
            sign = (self.xx * self.yy - self.xy * self.yx).sign()
        return sign < 0

    def line(self, line: Line) -> Line:
        """The image of a line."""
        return Line.through(self(line.point), self(line.point + line.direction))

    @remembered
    def polygon(self, polygon: Polygon) -> Polygon:
        """The image of a polygon, its corners still counterclockwise."""
        image = tuple(self(p) for p in polygon)
        return image[::-1] if self.flips else image


IDENTITY = Isometry(ONE, ZERO, ZERO, ONE, ZERO, ZERO)
HALF = Number.of("1/2")


def center(polygon: Sequence[Point]) -> Point:
    """The mean of a polygon's corners: a point inside it when it is convex."""
    return sum(polygon[1:], polygon[0]) * Number.of(Fraction(1, len(polygon)))


def area(polygon: Sequence[Point]) -> Number:
    """The signed area: positive when the corners run counterclockwise."""
    total = ZERO
    for i, p in enumerate(polygon):
        total += p.cross(polygon[(i + 1) % len(polygon)])
    return total * HALF


def orientation(p: Point, q: Point, r: Point) -> int:
    """Return 1, -1 or 0 as ``r`` lies left of, right of or on the line ``pq``."""
    if r in (p, q) or p == q:
        return 0
    estimate = (q.x.estimate() - p.x.estimate()) * (r.y.estimate() - p.y.estimate()) - (
        q.y.estimate() - p.y.estimate()
    ) * (r.x.estimate() - p.x.estimate())
    side = estimate.sign()
    return (q - p).cross(r - p).sign() if side is None else side


def convex(polygon: Sequence[Point]) -> bool:
    """Whether a polygon turns left at each corner: convex, counterclockwise."""
    return len(polygon) >= _FEWEST_CORNERS and all(
        orientation(p, q, r) > 0
        for (p, q), r in zip(sides(polygon), [*polygon[2:], *polygon[:2]], strict=True)
    )


def sides(polygon: Sequence[Point]) -> list[tuple[Point, Point]]:
    """The sides of a polygon, each from a corner to the next."""
    return list(zip(polygon, [*polygon[1:], polygon[0]], strict=True))


def clip(polygon: Polygon, line: Line, keep: int) -> Polygon:
    """The part of a convex polygon on one side of a line, on the line included.

    ``keep`` is the side to keep, 1 or -1, as `Line.side` returns it. The result is
    empty, or a polygon with no repeated corner; it may have no area.
    """
    return _clipped(polygon, [line.side(p) * keep for p in polygon], line.value)


@remembered
def intersection(first: Polygon, second: Polygon) -> Polygon:
    """The intersection of two convex polygons, possibly empty or without area."""
    if all(covers(second, p) for p in first):
        return first
    if all(covers(first, p) for p in second):
        return second
    result = first
    for p, q in sides(second):
        if not result:
            break
        result = _clipped(
            result,
            [orientation(p, q, r) for r in result],
            lambda r, p=p, q=q: (q - p).cross(r - p),
        )
    return result


@remembered
def overlaps(first: Polygon, second: Polygon) -> bool:
    """Whether two convex polygons of positive area share some area.

    They do unless a side of one has all of the other on or beyond it.
    """
    return (
        _boxes_meet(first, second)
        and not _apart(first, second)
        and not _apart(second, first)
    )


def inside(polygon: Polygon, point: Point) -> bool:
    """Whether a point lies strictly inside a convex polygon."""
    return all(orientation(p, q, point) > 0 for p, q in sides(polygon))


def covers(polygon: Polygon, point: Point) -> bool:
    """Whether a point lies inside a convex polygon or on its boundary."""
    return all(orientation(p, q, point) >= 0 for p, q in sides(polygon))


def crosses(polygon: Polygon, line: Line) -> bool:
    """Whether a line passes through the inside of a convex polygon."""
    found = {line.side(p) for p in polygon}
    return 1 in found and -1 in found


@remembered
def through_inside(polygon: Polygon, p: Point, q: Point) -> bool:
    """Whether the segment from ``p`` to ``q`` runs through a polygon's inside.

    It does when its line has corners on both sides, and no side of the polygon has
    both of its ends on or beyond it.
    """
    found = {orientation(p, q, corner) for corner in polygon}
    return (
        1 in found
        and -1 in found
        and not any(
            orientation(a, b, p) <= 0 and orientation(a, b, q) <= 0
            for a, b in sides(polygon)
        )
    )


@remembered
def collinear_overlap(first: tuple[Point, Point], second: tuple[Point, Point]) -> bool:
    """Whether two segments on one line share a stretch of positive length."""
    p, q = first
    if orientation(p, q, second[0]) or orientation(p, q, second[1]):
        return False
    direction = q - p
    a = (ZERO, direction.dot(direction))
    b = sorted(direction.dot(r - p) for r in second)
    return max(a[0], b[0]) < min(a[1], b[1])


def bounds(points: Iterable[Point]) -> tuple[Number, Number, Number, Number]:
    """The smallest box around some points: least x, least y, greatest x, greatest y."""
    points = list(points)
    xs = [p.x for p in points]
    ys = [p.y for p in points]
    return min(xs), min(ys), max(xs), max(ys)


@remembered
def box(polygon: Polygon) -> tuple[Number, Number, Number, Number]:
    """The smallest box around a polygon, as `bounds` gives it."""
    return bounds(polygon)


def _boxes_meet(first: Polygon, second: Polygon) -> bool:
    ax0, ay0, ax1, ay1 = box(first)
    bx0, by0, bx1, by1 = box(second)
    return ax0 < bx1 and bx0 < ax1 and ay0 < by1 and by0 < ay1


def _apart(polygon: Polygon, other: Polygon) -> bool:
    """Whether a side of a convex polygon has all of another on or beyond it."""
    return any(all(orientation(p, q, r) <= 0 for r in other) for p, q in sides(polygon))


def _clipped(
    polygon: Polygon, sides_of: list[int], value: Callable[[Point], Number]
) -> Polygon:
    """The corners of a convex polygon on the kept side, and where its sides cross.

    ``sides_of`` gives each corner's side, positive where kept; ``value`` is zero on
    the cutting line and changes sign across it.
    """
    result: list[Point] = []
    for i, p in enumerate(polygon):
        j = (i + 1) % len(polygon)
        if sides_of[i] >= 0:
            result.append(p)
        if sides_of[i] * sides_of[j] < 0:
            vp, vq = value(p), value(polygon[j])
            result.append(p + (polygon[j] - p) * (vp / (vp - vq)))
    return _distinct(result)


def _distinct(points: list[Point]) -> Polygon:
    result = [p for i, p in enumerate(points) if p != points[i - 1]] if points else []
    return tuple(result)
