# Copyright 2026 Maxence Faldor

from math import cos, radians, sin

import pytest
from hypothesis import assume, given
from hypothesis import strategies as st

from origamy.constructions import bisect, fold_onto, meet, midpoint, perpendicular
from origamy.field import Number
from origamy.geometry import (
    Isometry,
    Line,
    Point,
    area,
    clip,
    intersection,
    overlaps,
    through_inside,
)

coordinates = st.fractions(min_value=-4, max_value=4, max_denominator=8)
points = st.builds(lambda x, y: Point.of((x, y)), coordinates, coordinates)
SQUARE = tuple(Point.of(p) for p in ((0, 0), (1, 0), (1, 1), (0, 1)))


@st.composite
def lines(draw: st.DrawFn) -> Line:
    p, q = draw(points), draw(points)
    assume(p != q)
    return Line.through(p, q)


@given(lines(), points)
def test_reflection_is_an_involution_that_keeps_the_line(line: Line, p: Point) -> None:
    mirror = Isometry.reflection(line)
    assert mirror(mirror(p)) == p
    assert line.contains(mirror(line.point))
    assert line.side(mirror(p)) == -line.side(p)
    assert mirror.flips


@given(lines(), lines(), points)
def test_isometries_compose_and_invert(first: Line, second: Line, p: Point) -> None:
    both = Isometry.reflection(first) @ Isometry.reflection(second)
    assert both(p) == Isometry.reflection(first)(Isometry.reflection(second)(p))
    assert both.inverse()(both(p)) == p
    assert not both.flips


@given(lines(), points)
def test_an_isometry_maps_lines_to_lines(line: Line, p: Point) -> None:
    mirror = Isometry.reflection(Line.through((0, 0), (1, 2)))
    image = mirror.line(line)
    assert image.contains(mirror(line.point))
    assert image.contains(mirror(line.point + line.direction))
    del p


@given(lines())
def test_a_line_cuts_a_square_into_two_parts(line: Line) -> None:
    left, right = clip(SQUARE, line, 1), clip(SQUARE, line, -1)
    total = (area(left) if left else 0) + (area(right) if right else 0)
    assert total == 1


@given(points, points)
def test_overlap_of_two_squares(a: Point, b: Point) -> None:
    first = tuple(p + a for p in SQUARE)
    second = tuple(p + b for p in SQUARE)
    shift = a - b
    expected = abs(shift.x) < 1 and abs(shift.y) < 1
    assert overlaps(first, second) == expected
    if expected:
        common = intersection(first, second)
        assert area(common) == (1 - abs(shift.x)) * (1 - abs(shift.y))


def test_lines_compare_equal_whatever_their_points() -> None:
    assert Line.through((0, 0), (1, 1)) == Line.through((3, 3), (-2, -2))
    assert str(Line.through((0, "1/2"), (1, "1/2"))) == "y = 1/2"


def test_a_segment_through_the_inside() -> None:
    assert through_inside(SQUARE, Point.of(("1/2", -1)), Point.of(("1/2", 2)))
    assert not through_inside(SQUARE, Point.of((0, -1)), Point.of((0, 2)))
    assert not through_inside(SQUARE, Point.of((2, 0)), Point.of((3, 1)))


def test_constructions() -> None:
    assert fold_onto((0, 0), (1, 1)) == Line.through((1, 0), (0, 1))
    assert midpoint((0, 0), (1, 1)) == Point.of(("1/2", "1/2"))
    diagonal = Line.through((0, 0), (1, 1))
    assert meet(diagonal, Line.through((0, 1), (1, 0))) == Point.of(("1/2", "1/2"))
    assert perpendicular(diagonal, (1, 0)) == Line.through((1, 0), (0, 1))
    eighth = bisect((1, 0), (0, 0), (1, 1))
    assert eighth.contains(Point.of((1, "√2 - 1")))


def test_halving_any_angle_is_exact() -> None:
    line = bisect((1, 0), (0, 0), (1, 2))
    image = Isometry.reflection(line)(Point.of((1, 0)))
    assert image == Point.of((1, 2)) * (1 / Number.of(5).sqrt())
    eleventh = bisect((1, 0), (0, 0), (1, "√2 - 1"))
    assert not eleventh.normal.x.rational or not eleventh.normal.y.rational


@given(st.integers(min_value=-120, max_value=120))
def test_every_multiple_of_three_degrees_turns_exactly(k: int) -> None:
    unit = Point.polar(1, 3 * k)
    assert unit.dot(unit) == 1
    x, y = unit.floats()
    assert abs(x - cos(radians(3 * k))) < 1e-12
    assert abs(y - sin(radians(3 * k))) < 1e-12


def test_turns_add() -> None:
    assert Point.polar(1, 27).turned(18) == Point.polar(1, 45)
    assert Point.polar(1, 90) == Point.of((0, 1))


def test_only_multiples_of_three_degrees_turn_exactly() -> None:
    with pytest.raises(ValueError, match="multiple of 3°"):
        Point.polar(1, 10)
