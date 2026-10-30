# Copyright 2026 Maxence Faldor
"""Exact real numbers of the form a + b√2, with rational a and b.

Every coordinate in origamy is such a number. Sums, products and quotients of them
stay of that form, so reflections and intersections of lines are exact. The form holds
every point of a design on a grid and of a design with 22.5° angles, such as the bird
base. A square root leaves it in general: `Number.sqrt` raises `OutsideFieldError` then.
"""

import re
from dataclasses import dataclass
from fractions import Fraction
from math import isqrt
from math import sqrt as float_sqrt
from typing import Self, override

from origamy.errors import OutsideFieldError

type Real = int | Fraction | Number
"""What arithmetic with a `Number` accepts."""

type Exact = int | Fraction | Number | str
"""What a script may write for a coordinate: an exact value, or text that `Number.of`
reads, such as ``"1/2"`` or ``"√2 - 1"``."""

_ROOT2 = float_sqrt(2)
_TERM = re.compile(r"\s*([+-]?)\s*(\d+)?(√2)?(?:/(\d+))?")


@dataclass(frozen=True, slots=True, eq=False)
class Number:
    """The real number ``a + b√2``, with rational ``a`` and ``b``.

    >>> half = Number.of("1/2")
    >>> root = Number.of("√2")
    >>> half * root
    Number.of('√2/2')
    >>> (half * root) ** 2 == half
    True
    >>> root - 1 < half
    True
    """

    a: Fraction
    b: Fraction = Fraction(0)

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
                return cls(Fraction(value))
            case str():
                return cls._parse(value)
            case float():
                msg = f"numbers are exact: got the float {value!r}"
                raise TypeError(msg)
        msg = f"numbers are exact: got {type(value).__name__} {value!r}"
        raise TypeError(msg)

    @classmethod
    def _parse(cls, text: str) -> Number:
        a, b, position = Fraction(0), Fraction(0), 0
        while position < len(text.rstrip()):
            match = _TERM.match(text, position)
            if match is None or match.end() == position:
                break
            sign, numerator, root, denominator = match.groups()
            if (numerator is None and root is None) or (position and not sign):
                break
            value = Fraction(int(numerator or 1), int(denominator or 1))
            value = -value if sign == "-" else value
            if root:
                b += value
            else:
                a += value
            position = match.end()
        if position < len(text.rstrip()) or not text.strip():
            msg = f"{text!r} is not a number of the form a + b√2, such as '1 - √2/2'"
            raise ValueError(msg)
        return cls(a, b)

    @property
    def rational(self) -> bool:
        """Whether the number is rational, that is ``b == 0``."""
        return not self.b

    def sign(self) -> int:
        """Return -1, 0 or 1, the sign of the number, decided exactly."""
        a, b = self.a, self.b
        if not b:
            return (a > 0) - (a < 0)
        if not a or (a > 0) == (b > 0):
            return 1 if b > 0 else -1
        # The signs differ: the term of larger magnitude wins, compared through squares.
        return (1 if a > 0 else -1) if a * a > 2 * b * b else (1 if b > 0 else -1)

    def sqrt(self) -> Number:
        """The square root, or `OutsideFieldError` when it has no form a + b√2.

        >>> Number.of("3 - 2√2").sqrt()
        Number.of('-1 + √2')
        """
        if self.sign() < 0:
            msg = f"{self} is negative, so it has no real square root"
            raise ValueError(msg)
        a, b = self.a, self.b
        if not b:
            if (root := _rational_sqrt(a)) is not None:
                return Number(root)
            if (root := _rational_sqrt(a / 2)) is not None:
                return Number(Fraction(0), root)
        elif (norm := _rational_sqrt(a * a - 2 * b * b)) is not None:
            for square in ((a + norm) / 2, (a - norm) / 2):
                if square and (x := _rational_sqrt(square)) is not None:
                    root = Number(x, b / (2 * x))
                    return root if root.sign() > 0 else -root
        msg = f"the square root of {self} is not of the form a + b√2"
        raise OutsideFieldError(msg)

    def __add__(self, other: Real) -> Number:
        """The sum."""
        o = _coerce(other)
        return Number(self.a + o.a, self.b + o.b)

    __radd__ = __add__

    def __sub__(self, other: Real) -> Number:
        """The difference."""
        o = _coerce(other)
        return Number(self.a - o.a, self.b - o.b)

    def __rsub__(self, other: Real) -> Number:
        """The difference, this number subtracted."""
        return _coerce(other) - self

    def __mul__(self, other: Real) -> Number:
        """The product."""
        o = _coerce(other)
        return Number(self.a * o.a + 2 * self.b * o.b, self.a * o.b + self.b * o.a)

    __rmul__ = __mul__

    def __truediv__(self, other: Real) -> Number:
        """The quotient."""
        o = _coerce(other)
        norm = o.a * o.a - 2 * o.b * o.b
        if not norm:
            msg = "division by zero"
            raise ZeroDivisionError(msg)
        return self * Number(o.a / norm, -o.b / norm)

    def __rtruediv__(self, other: Real) -> Number:
        """The quotient, this number the divisor."""
        return _coerce(other) / self

    def __pow__(self, exponent: int) -> Number:
        """The number to an integer power."""
        result, base = Number(Fraction(1)), self
        if exponent < 0:
            base, exponent = 1 / base, -exponent
        while exponent:
            if exponent & 1:
                result *= base
            base, exponent = base * base, exponent >> 1
        return result

    def __neg__(self) -> Number:
        """The opposite number."""
        return Number(-self.a, -self.b)

    def __pos__(self) -> Self:
        """The number itself."""
        return self

    def __abs__(self) -> Number:
        """The absolute value."""
        return -self if self.sign() < 0 else self

    def __bool__(self) -> bool:
        """Whether the number differs from zero."""
        return bool(self.a or self.b)

    @override
    def __eq__(self, other: object) -> bool:
        """Whether two numbers are equal; an int or a Fraction counts as rational."""
        if isinstance(other, Number):
            return self.a == other.a and self.b == other.b
        if isinstance(other, int | Fraction) and not isinstance(other, bool):
            return not self.b and self.a == other
        return NotImplemented

    @override
    def __hash__(self) -> int:
        """A hash equal to a rational's own when the number is rational."""
        return hash(self.a) if not self.b else hash((self.a, self.b))

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
        return float(self.a) + float(self.b) * _ROOT2

    @override
    def __str__(self) -> str:
        """Write the number as ``a + b√2`` with fractions, such as ``1 - √2/2``."""
        a, b = self.a, self.b
        if not b:
            return str(a)
        surd = _surd(abs(b))
        if not a:
            return surd if b > 0 else f"-{surd}"
        return f"{a} {'+' if b > 0 else '-'} {surd}"

    @override
    def __repr__(self) -> str:
        """Text that reads back as this number."""
        return f"Number.of({str(self)!r})"


def _surd(b: Fraction) -> str:
    numerator = "" if b.numerator == 1 else str(b.numerator)
    denominator = "" if b.denominator == 1 else f"/{b.denominator}"
    return f"{numerator}√2{denominator}"


def _coerce(value: Real) -> Number:
    if isinstance(value, Number):
        return value
    return Number.of(value)


def _rational_sqrt(value: Fraction) -> Fraction | None:
    if value < 0:
        return None
    numerator, denominator = isqrt(value.numerator), isqrt(value.denominator)
    if numerator * numerator == value.numerator and (
        denominator * denominator == value.denominator
    ):
        return Fraction(numerator, denominator)
    return None


ZERO = Number(Fraction(0))
ONE = Number(Fraction(1))
SQRT2 = Number(Fraction(0), Fraction(1))
"""The square root of 2, exactly."""
