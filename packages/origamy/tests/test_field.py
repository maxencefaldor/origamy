# Copyright 2026 Maxence Faldor
from contextvars import Context
from fractions import Fraction
from math import isclose

import pytest
from hypothesis import given
from hypothesis import strategies as st

from origamy.field import Number, radicands, tower

rationals = st.fractions(max_denominator=50).filter(lambda q: abs(q) < 1000)
numbers = st.builds(
    lambda a, b, c: (
        Number.of(a) + Number.of(b) * Number.of("√2") + Number.of(c) * Number.of("√3")
    ),
    rationals,
    rationals,
    rationals,
)
"""Numbers two levels up the tower, ``a + b√2 + c√3``."""


@given(numbers, numbers)
def test_arithmetic_matches_floats(x: Number, y: Number) -> None:
    assert isclose(float(x + y), float(x) + float(y), abs_tol=1e-6)
    assert isclose(float(x * y), float(x) * float(y), rel_tol=1e-9, abs_tol=1e-6)
    if y:
        assert x / y * y == x


@given(numbers, numbers)
def test_comparison_is_exact(x: Number, y: Number) -> None:
    assert (x < y) == ((x - y).sign() < 0)
    assert (x == y) == (not (x - y))
    if abs(float(x) - float(y)) > 1e-6:
        assert (x < y) == (float(x) < float(y))


@given(numbers)
def test_an_estimate_lies_within_its_error(x: Number) -> None:
    estimate = x.estimate()
    distance = abs(x - Fraction(estimate.value))
    assert distance <= Fraction(estimate.error)


@given(numbers)
def test_text_reads_back(x: Number) -> None:
    assert Number.of(str(x)) == x
    assert eval(repr(x), {"Number": Number}) == x  # noqa: S307 - the repr is the subject
    assert Number.from_data(x.data()) == x


@given(numbers)
def test_square_root_of_a_square(x: Number) -> None:
    assert (x * x).sqrt() == abs(x)


def test_the_tower_grows_from_the_rationals_as_roots_need() -> None:
    assert radicands() == ()
    root2 = Number.of(8).sqrt() / 2
    assert radicands() == (2,)
    assert Number.of("1/2").sqrt() == root2 / 2
    assert Number.of("3 - 2√2").sqrt() == root2 - 1
    root = (2 + root2).sqrt()
    assert root**2 == 2 + root2
    assert (root * root).sqrt() == root
    assert (2 - root2).sqrt() * root == root2
    assert (Number.of("1/2") + root2 / 4).sqrt() == root / 2
    assert radicands() == (2, 2 + root2)
    assert str(root) == "√(2 + √2)"
    assert str(Number.of("12/5").sqrt()) == "2√15/5"
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


def test_a_tower_reads_back_in_a_fresh_context() -> None:
    root = (2 + Number.of("√2")).sqrt()
    levels, data = radicands(), root.data()

    def read() -> float:
        with tower(levels):
            return float(Number.from_data(data))

    assert Context().run(read) == float(root)
