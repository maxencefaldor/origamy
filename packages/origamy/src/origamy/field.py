# Copyright 2026 Maxence Faldor
"""Exact real numbers built from the rationals by square roots.

Every coordinate in origamy is such a number. The first five Huzita-Justin axioms build
fold lines with arithmetic and square roots alone, so everything they build is exact
here: a 22.5° design, the 11.25° creases of a crane, or the bisector of any angle.

The numbers live in a tower of fields. Level 0 holds the rationals, level 1 adds √2,
and each square root that no level holds yet adds a level above the others. A number at
level k is ``a + b√d``, where ``d`` is level k's radicand and ``a`` and ``b`` lie below.
Each number has one form, so two numbers are equal exactly when their forms are, and
the sign of every number is decided exactly. A build keeps its own tower, which
`tower` sets while a build's data is read back.
"""

import re
from collections.abc import Generator, Sequence
from contextlib import contextmanager
from contextvars import ContextVar
from fractions import Fraction
from math import isqrt
from math import sqrt as float_sqrt
from typing import Final, override

from pydantic import JsonValue

from origamy.errors import OrigamyError

type Real = int | Fraction | Number
"""What arithmetic with a `Number` accepts."""

type Exact = int | Fraction | Number | str
"""What a script may write for a number: an exact value, or text that `Number.of`
reads, such as ``"1/2"`` or ``"√2 - 1"``."""

_TERM = re.compile(r"\s*([+-]?)\s*(\d+)?(?:√(\d+))?(?:/(\d+))?")


class Number:
    """A real number of the tower: a rational, or ``a + b√d`` at some level.

    >>> half = Number.of("1/2")
    >>> root = Number.of("√2")
    >>> half * root
    Number.of('√2/2')
    >>> (half * root) ** 2 == half
    True
    >>> print((2 + root).sqrt() ** 2)
    2 + √2
    """

    __slots__ = ("a", "b", "level")

    level: int
    a: Fraction | Number
    b: Number | None

    def __init__(
        self, level: int, a: Fraction | Number, b: Number | None = None
    ) -> None:
        """A number of a level: the rational ``a``, or ``a + b√d`` with ``b`` nonzero.

        Scripts make numbers with `Number.of`; arithmetic keeps every form canonical.
        """
        self.level, self.a, self.b = level, a, b

    @classmethod
    def of(cls, value: Exact) -> Number:
        """Convert an exact value, or text such as ``"1 - √2/2"``, to a `Number`.

        A float is refused, because its value is rarely the number written.
        """
        match value:
            case Number():
                return value
            case bool():
                msg = f"numbers are exact: got the bool {value!r}"
                raise TypeError(msg)
            case int() | Fraction():
                return cls(0, Fraction(value))
            case str():
                return _parse(value)
            case float():
                msg = f"numbers are exact: got the float {value!r}"
                raise TypeError(msg)
        msg = f"numbers are exact: got {type(value).__name__} {value!r}"
        raise TypeError(msg)

    @property
    def rational(self) -> bool:
        """Whether the number is rational."""
        return not self.level

    def fraction(self) -> Fraction:
        """The number as a fraction, when it is rational."""
        match self.a:
            case Fraction():
                return self.a
        msg = f"{self} is irrational"
        raise ArithmeticError(msg)

    def sign(self) -> int:
        """Return -1, 0 or 1, the sign of the number, decided exactly."""
        if isinstance(self.a, Fraction):
            return (self.a > 0) - (self.a < 0)
        a, b = self.a, _known(self.b)
        sa, sb = a.sign(), b.sign()
        if not sa or sa == sb:
            return sb
        # The signs differ: the larger part wins, compared through squares.
        return sa if (a * a - b * b * _radicand(self.level)).sign() > 0 else sb

    def sqrt(self) -> Number:
        """The square root, adding a level to the tower when no level holds it.

        >>> print(Number.of(8).sqrt())
        2√2
        """
        sign = self.sign()
        if sign < 0:
            msg = f"{self} is negative, so it has no real square root"
            raise ValueError(msg)
        if not sign:
            return self
        levels = _levels()
        found = _root(self, len(levels))
        if found is not None:
            return abs(found)
        levels.append(self)
        return Number(len(levels), ZERO, ONE)

    def __add__(self, other: Real) -> Number:
        """The sum."""
        o = _coerce(other)
        if isinstance(self.a, Fraction) and isinstance(o.a, Fraction):
            return Number(0, self.a + o.a)
        level = max(self.level, o.level)
        a1, b1 = _parts(self, level)
        a2, b2 = _parts(o, level)
        return _make(level, a1 + a2, b1 + b2)

    __radd__ = __add__

    def __sub__(self, other: Real) -> Number:
        """The difference."""
        return self + -_coerce(other)

    def __rsub__(self, other: Real) -> Number:
        """The difference, this number subtracted."""
        return _coerce(other) - self

    def __mul__(self, other: Real) -> Number:
        """The product."""
        o = _coerce(other)
        if isinstance(self.a, Fraction) and isinstance(o.a, Fraction):
            return Number(0, self.a * o.a)
        level = max(self.level, o.level)
        a1, b1 = _parts(self, level)
        a2, b2 = _parts(o, level)
        d = _radicand(level)
        return _make(level, a1 * a2 + b1 * b2 * d, a1 * b2 + a2 * b1)

    __rmul__ = __mul__

    def __truediv__(self, other: Real) -> Number:
        """The quotient."""
        return self * _coerce(other).inverse()

    def __rtruediv__(self, other: Real) -> Number:
        """The quotient, this number the divisor."""
        return _coerce(other) * self.inverse()

    def inverse(self) -> Number:
        """One over the number."""
        if isinstance(self.a, Fraction):
            if not self.a:
                msg = "division by zero"
                raise ZeroDivisionError(msg)
            return Number(0, 1 / self.a)
        a, b = self.a, _known(self.b)
        norm = a * a - b * b * _radicand(self.level)
        return _make(self.level, a / norm, -b / norm)

    def __pow__(self, exponent: int) -> Number:
        """The number to an integer power."""
        result, base = ONE, self
        if exponent < 0:
            base, exponent = self.inverse(), -exponent
        while exponent:
            if exponent & 1:
                result *= base
            base, exponent = base * base, exponent >> 1
        return result

    def __neg__(self) -> Number:
        """The opposite number."""
        if isinstance(self.a, Fraction):
            return Number(0, -self.a)
        return Number(self.level, -self.a, -_known(self.b))

    def __pos__(self) -> Number:
        """The number itself."""
        return self

    def __abs__(self) -> Number:
        """The absolute value."""
        return -self if self.sign() < 0 else self

    def __bool__(self) -> bool:
        """Whether the number differs from zero."""
        return self.level > 0 or bool(self.a)

    @override
    def __eq__(self, other: object) -> bool:
        """Whether two numbers are equal; an int or a Fraction counts as rational."""
        if isinstance(other, Number):
            return self.level == other.level and self.a == other.a and self.b == other.b
        if isinstance(other, int | Fraction) and not isinstance(other, bool):
            return isinstance(self.a, Fraction) and self.a == other
        return NotImplemented

    @override
    def __hash__(self) -> int:
        """A hash equal to a rational's own when the number is rational."""
        if isinstance(self.a, Fraction):
            return hash(self.a)
        return hash((self.level, self.a, self.b))

    def __lt__(self, other: Real) -> bool:
        """Whether this number is less than another."""
        return (self - other).sign() < 0

    def __le__(self, other: Real) -> bool:
        """Whether this number is at most another."""
        return (self - other).sign() <= 0

    def __gt__(self, other: Real) -> bool:
        """Whether this number is greater than another."""
        return (self - other).sign() > 0

    def __ge__(self, other: Real) -> bool:
        """Whether this number is at least another."""
        return (self - other).sign() >= 0

    def __float__(self) -> float:
        """The nearest float, for pictures."""
        if isinstance(self.a, Fraction):
            return float(self.a)
        root = float_sqrt(float(_radicand(self.level)))
        return float(self.a) + float(_known(self.b)) * root

    @override
    def __str__(self) -> str:
        """Write the number with fractions and radicals, such as ``1 - √2/2``."""
        if isinstance(self.a, Fraction):
            return str(self.a)
        term = _term(_known(self.b), _radical(self.level))
        if not self.a:
            return term
        if term.startswith("-"):
            return f"{self.a} - {term[1:]}"
        return f"{self.a} + {term}"

    @override
    def __repr__(self) -> str:
        """Text that reads back as this number, when its radicals are integers."""
        return f"Number.of({str(self)!r})"

    def data(self) -> JsonValue:
        """The number as JSON: a fraction's text, or ``[level, a, b]``."""
        if isinstance(self.a, Fraction):
            return str(self.a)
        return [self.level, self.a.data(), _known(self.b).data()]

    @classmethod
    def from_data(cls, data: JsonValue) -> Number:
        """The number that `data` wrote."""
        if isinstance(data, str):
            return cls(0, Fraction(data))
        if isinstance(data, list) and len(data) == 3:  # noqa: PLR2004 - level, a, b
            level, a, b = data
            if isinstance(level, int):
                return cls(level, cls.from_data(a), cls.from_data(b))
        msg = f"{data!r} is not a number's data"
        raise ValueError(msg)


ZERO: Final = Number(0, Fraction(0))
ONE: Final = Number(0, Fraction(1))
SQRT2: Final = Number(1, ZERO, ONE)
"""The square root of 2, exactly: level 1 of every tower."""

_TOWER: ContextVar[list[Number]] = ContextVar("tower")


def _levels() -> list[Number]:
    """The radicands of the current tower, level 1 first."""
    levels = _TOWER.get(None)
    if levels is None:
        levels = [Number(0, Fraction(2))]
        _TOWER.set(levels)
    return levels


def radicands() -> tuple[Number, ...]:
    """The radicands of the current tower, level 1 first."""
    return tuple(_levels())


@contextmanager
def tower(levels: Sequence[JsonValue]) -> Generator[None]:
    """Use a build's own tower, from its radicands' data, while reading its numbers."""
    found = [Number.from_data(data) for data in levels]
    if not found or found[0] != 2:  # noqa: PLR2004 - every tower starts with √2
        msg = "a tower's first radicand is 2"
        raise OrigamyError(msg)
    token = _TOWER.set(found)
    try:
        yield
    finally:
        _TOWER.reset(token)


def _radicand(level: int) -> Number:
    return _levels()[level - 1]


def _known(part: Number | None) -> Number:
    if part is None:
        msg = "a rational number has no radical part"
        raise ArithmeticError(msg)
    return part


def _coerce(value: Real) -> Number:
    return value if isinstance(value, Number) else Number.of(value)


def _parts(number: Number, level: int) -> tuple[Number, Number]:
    """The number as ``a + b√d`` at a level at or above its own."""
    if number.level == level and number.b is not None:
        return _coerce_part(number.a), number.b
    return number, ZERO


def _coerce_part(part: Fraction | Number) -> Number:
    return Number(0, part) if isinstance(part, Fraction) else part


def _make(level: int, a: Number, b: Number) -> Number:
    """The canonical form of ``a + b√d``: below the level when ``b`` is zero."""
    return Number(level, a, b) if b else a


def _root(number: Number, level: int) -> Number | None:
    """A square root of a number within the tower's levels up to ``level``.

    A root of ``a + b√d`` is ``u + v√d`` with ``u² + v²d = a`` and ``2uv = b``, so
    ``u²`` is half of ``a`` plus or minus the root of ``a² - b²d``, one level down.
    """
    if not level:
        return _rational_root(number)
    if number.level < level:
        if (found := _root(number, level - 1)) is not None:
            return found
        scaled = _root(number / _radicand(level), level - 1)
        return None if scaled is None else _make(level, ZERO, scaled)
    a, b = _coerce_part(number.a), _known(number.b)
    norm = _root(a * a - b * b * _radicand(level), level - 1)
    for square in () if norm is None else ((a + norm) / 2, (a - norm) / 2):
        if square.sign() > 0 and (part := _root(square, level - 1)) is not None:
            return _make(level, part, b / (2 * part))
    return None


def _rational_root(number: Number) -> Number | None:
    if not isinstance(number.a, Fraction) or number.a < 0:
        return None
    value = number.a
    numerator, denominator = isqrt(value.numerator), isqrt(value.denominator)
    if numerator * numerator == value.numerator and (
        denominator * denominator == value.denominator
    ):
        return Number(0, Fraction(numerator, denominator))
    return None


def _radical(level: int) -> str:
    radicand = _radicand(level)
    if radicand.rational and radicand.fraction().denominator == 1:
        return f"√{radicand}"
    return f"√({radicand})"


def _term(coefficient: Number, radical: str) -> str:
    """A coefficient times a radical, such as ``3√2/4`` or ``(1 + √2)√(2 + √2)``."""
    if not coefficient.rational:
        return f"({coefficient}){radical}"
    value = coefficient.fraction()
    sign = "-" if value < 0 else ""
    numerator = "" if abs(value.numerator) == 1 else str(abs(value.numerator))
    denominator = "" if value.denominator == 1 else f"/{value.denominator}"
    return f"{sign}{numerator}{radical}{denominator}"


def _parse(text: str) -> Number:
    """Read a sum of terms such as ``1``, ``-3/4`` and ``3√2/4``."""
    total, position, end = ZERO, 0, len(text.rstrip())
    while position < end:
        match = _TERM.match(text, position)
        if match is None or match.end() == position:
            break
        sign, numerator, radicand, denominator = match.groups()
        if (numerator is None and radicand is None) or (position and not sign):
            break
        value = Number(0, Fraction(int(numerator or 1), int(denominator or 1)))
        if radicand is not None:
            value *= Number(0, Fraction(int(radicand))).sqrt()
        total += -value if sign == "-" else value
        position = match.end()
    if position < end or not text.strip():
        msg = f"{text!r} is not a number such as '1 - √2/2'"
        raise ValueError(msg)
    return total
