# Copyright 2026 Maxence Faldor
from fractions import Fraction
from math import isclose

import pytest
from hypothesis import given
from hypothesis import strategies as st

from origamy.field import SQRT2, Number

rationals = st.fractions(max_denominator=50).filter(lambda q: abs(q) < 1000)
numbers = st.builds(
    lambda a, b: Number.of(a) + Number.of(b) * SQRT2, rationals, rationals
)


def approx(number: Number) -> float:
    return float(number)


@given(numbers, numbers)
def test_arithmetic_matches_floats(x: Number, y: Number) -> None:
    assert isclose(approx(x + y), approx(x) + approx(y), abs_tol=1e-6)
    assert isclose(approx(x * y), approx(x) * approx(y), rel_tol=1e-9, abs_tol=1e-6)
    if y:
        assert x / y * y == x


@given(numbers, numbers)
def test_comparison_is_exact(x: Number, y: Number) -> None:
    assert (x < y) == ((x - y).sign() < 0)
    assert (x == y) == (not (x - y))
    if abs(approx(x) - approx(y)) > 1e-6:
        assert (x < y) == (approx(x) < approx(y))


@given(numbers)
def test_text_reads_back(x: Number) -> None:
    assert Number.of(str(x)) == x
    assert eval(repr(x), {"Number": Number}) == x  # noqa: S307 - the repr is the subject


@given(numbers)
def test_square_root_of_a_square(x: Number) -> None:
    assert (x * x).sqrt() == abs(x)


def test_square_roots_add_levels_only_when_needed() -> None:
    assert Number.of(2).sqrt() == SQRT2
    assert Number.of("1/2").sqrt() == SQRT2 / 2
    assert Number.of("3 - 2√2").sqrt() == SQRT2 - 1
    root = (2 + SQRT2).sqrt()
    assert root**2 == 2 + SQRT2
    assert root.level > 1
    assert (root * root).sqrt() == root
    assert (2 - SQRT2).sqrt() * root == SQRT2
    with pytest.raises(ValueError, match="negative"):
        Number.of(-1).sqrt()


def test_exact_values_only() -> None:
    with pytest.raises(TypeError, match="float"):
        Number.of(0.5)  # ty: ignore[invalid-argument-type] - the float is the subject
    with pytest.raises(TypeError, match="bool"):
        Number.of(True)  # noqa: FBT003 - the bool is the subject
    with pytest.raises(ValueError, match="not a number"):
        Number.of("1 2")


def test_rationals_compare_and_hash_as_rationals() -> None:
    half = Number.of("1/2")
    assert half == Fraction(1, 2)
    assert hash(half) == hash(Fraction(1, 2))
    assert str(Number.of("1 - √2/2")) == "1 - √2/2"
