# Copyright 2026 Maxence Faldor
"""Exact real numbers built from the rationals by square roots.

Every coordinate in origamy is such a number. The first five Huzita-Justin axioms build
fold lines with arithmetic and square roots alone, so everything they build is exact
here: a 22.5° design, the 11.25° creases of a crane, or the bisector of any angle.

The numbers live in a tower of fields. Level 0 holds the rationals, and each square
root that no level holds adds a level above the others. Level k's root is ``√d``,
where the radicand ``d`` has integer coefficients below level k. A number at level k
has an integer coefficient for each product of the roots up to level k, over one
positive denominator. Each number has one form, so two numbers are equal exactly when
their forms are. A float estimate with an error bound decides most signs, and exact
arithmetic decides the rest. Each build grows its own tower from the rationals, and
`tower` restores it while the build's numbers are read back.
"""

import re
from collections.abc import Callable, Generator, Hashable, Iterable
from contextlib import contextmanager
from contextvars import ContextVar
from dataclasses import dataclass
from fractions import Fraction
from functools import wraps
from math import gcd, inf, isqrt, lcm
from math import sqrt as float_sqrt
from typing import Final, cast, override

type Real = int | Fraction | Number
"""What arithmetic with a `Number` accepts."""

type Exact = int | Fraction | Number | str
"""What a script may write for a number: an exact value, or text that `Number.of`
reads, such as ``"1/2"`` or ``"√2 - 1"``."""

type NumberData = str | tuple[int, NumberData, NumberData]
"""A number as JSON: a rational's text, or its level with ``a`` and ``b``."""

type Coefficients = tuple[int, ...]
"""Integer coefficients of the products of a tower's roots: 1, √d1, √d2, √d1√d2..."""

_TERM = re.compile(r"\s*([+-]?)\s*(\d+)?(?:√(\d+))?(?:/(\d+))?")
_ROUNDING: Final = 2.0**-50
"""A float's relative rounding error, with room to spare."""


@dataclass(frozen=True, slots=True)
class Estimate:
    """A float near a number, and a bound on how far from it the float lies.

    Arithmetic on estimates bounds its own rounding, so a sign that the bound leaves
    clear is the exact number's sign.
    """

    value: float
    error: float

    def __add__(self, other: Estimate) -> Estimate:
        """An estimate of the sum."""
        value = self.value + other.value
        return Estimate(value, self.error + other.error + abs(value) * _ROUNDING)

    def __sub__(self, other: Estimate) -> Estimate:
        """An estimate of the difference."""
        value = self.value - other.value
        return Estimate(value, self.error + other.error + abs(value) * _ROUNDING)

    def __mul__(self, other: Estimate) -> Estimate:
        """An estimate of the product."""
        value = self.value * other.value
        error = (
            abs(self.value) * other.error
            + abs(other.value) * self.error
            + self.error * other.error
            + abs(value) * _ROUNDING
        )
        return Estimate(value, error)

    def sign(self) -> int | None:
        """The sign of the number estimated, or None when the error leaves it open."""
        if abs(self.value) > self.error:
            return 1 if self.value > 0 else -1
        return None


class Number:
    """A real number of the tower: a rational, or ``a + b√d`` at some level.

    Examples:
        >>> half = Number.of("1/2")
        >>> root = Number.of("√2")
        >>> half * root
        Number.of('√2/2')
        >>> (half * root) ** 2 == half
        True
        >>> print((2 + root).sqrt() ** 2)
        2 + √2
    """

    __slots__ = ("_estimate", "_hash", "denominator", "numerators")

    numerators: Coefficients
    """One integer for each product of the tower's roots, 2 to the level of them."""
    denominator: int

    def __init__(self, numerators: Coefficients, denominator: int = 1) -> None:
        """The number ``numerators / denominator``, brought to its one form.

        Scripts make numbers with `Number.of`.
        """
        while len(numerators) > 1 and not any(numerators[len(numerators) // 2 :]):
            numerators = numerators[: len(numerators) // 2]
        common = gcd(*numerators, denominator)
        if denominator < 0:
            common = -common
        if common not in (0, 1):
            numerators = tuple(n // common for n in numerators)
            denominator //= common
        if not any(numerators):
            denominator = 1
        self.numerators, self.denominator = numerators, denominator
        self._estimate: Estimate | None = None
        self._hash: int | None = None

    @classmethod
    def of_form(cls, numerators: Coefficients, denominator: int) -> Number:
        """The number whose one form is ``numerators / denominator``, taken as given.

        The kernel gives its numbers in their one form, so reading them back skips the
        reduction. Scripts make numbers with `Number.of`.
        """
        number = cls.__new__(cls)
        number.numerators, number.denominator = numerators, denominator
        number._estimate = number._hash = None  # noqa: SLF001 - the class's own
        return number

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
                fraction = Fraction(value)
                return cls((fraction.numerator,), fraction.denominator)
            case str():
                return _parse(value)
            case float():
                msg = f"numbers are exact: got the float {value!r}"
                raise TypeError(msg)
        msg = f"numbers are exact: got {type(value).__name__} {value!r}"
        raise TypeError(msg)

    @property
    def level(self) -> int:
        """The lowest level of the tower that holds the number."""
        return len(self.numerators).bit_length() - 1

    @property
    def rational(self) -> bool:
        """Whether the number is rational."""
        return len(self.numerators) == 1

    def fraction(self) -> Fraction:
        """The number as a fraction, when it is rational."""
        if not self.rational:
            msg = f"{self} is irrational"
            raise ArithmeticError(msg)
        return Fraction(self.numerators[0], self.denominator)

    def sign(self) -> int:
        """Return -1, 0 or 1, the sign of the number, decided exactly."""
        if (estimated := self.estimate().sign()) is not None:
            return estimated
        if self.rational:
            return (self.numerators[0] > 0) - (self.numerators[0] < 0)
        a, b = self._halves()
        sa, sb = a.sign(), b.sign()
        if not sa or sa == sb:
            return sb
        # The signs differ: the larger part wins, compared through squares.
        return sa if (a * a - b * b * _radicand(self.level)).sign() > 0 else sb

    def sqrt(self) -> Number:
        """The square root, adding a level to the tower when no level holds it.

        Examples:
            >>> print(Number.of(8).sqrt())
            2√2
        """
        sign = self.sign()
        if sign < 0:
            msg = f"{self} is negative, so it has no real square root"
            raise ValueError(msg)
        if not sign:
            return self
        levels = _tower()
        found = _root(self, len(levels.radicands))
        if found is not None:
            return abs(found)
        radicand, factor = _reduced(self)
        levels.add(radicand)
        return _joined(ZERO, Number.of(factor), len(levels.radicands))

    def __add__(self, other: Real) -> Number:
        """The sum."""
        o = _coerce(other)
        common = lcm(self.denominator, o.denominator)
        x = _scaled(self.numerators, common // self.denominator)
        y = _scaled(o.numerators, common // o.denominator)
        return Number(_plus(x, y), common)

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
        denominator = self.denominator * o.denominator
        if o.rational:
            return Number(_scaled(self.numerators, o.numerators[0]), denominator)
        if self.rational:
            return Number(_scaled(o.numerators, self.numerators[0]), denominator)
        size = max(len(self.numerators), len(o.numerators))
        x, y = _padded(self.numerators, size), _padded(o.numerators, size)
        return Number(_product(x, y, _tower().vectors), denominator)

    __rmul__ = __mul__

    def __truediv__(self, other: Real) -> Number:
        """The quotient."""
        return self * _coerce(other).inverse()

    def __rtruediv__(self, other: Real) -> Number:
        """The quotient, this number the divisor."""
        return _coerce(other) * self.inverse()

    def inverse(self) -> Number:
        """One over the number: its conjugate over its norm, one level down."""
        if not self:
            msg = "division by zero"
            raise ZeroDivisionError(msg)
        if self.rational:
            return Number((self.denominator,), self.numerators[0])
        a, b = self._halves()
        norm = a * a - b * b * _radicand(self.level)
        return _joined(a, -b, self.level) * norm.inverse()

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
        return Number(tuple(-n for n in self.numerators), self.denominator)

    def __pos__(self) -> Number:
        """The number itself."""
        return self

    def __abs__(self) -> Number:
        """The absolute value."""
        return -self if self.sign() < 0 else self

    def __bool__(self) -> bool:
        """Whether the number differs from zero."""
        return any(self.numerators)

    @override
    def __eq__(self, other: object) -> bool:
        """Whether two numbers are equal; an int or a Fraction counts as rational."""
        if isinstance(other, Number):
            return (
                self.numerators == other.numerators
                and self.denominator == other.denominator
            )
        if isinstance(other, int | Fraction) and not isinstance(other, bool):
            return self.rational and self.fraction() == other
        return NotImplemented

    @override
    def __hash__(self) -> int:
        """A hash equal to a rational's own when the number is rational."""
        if self._hash is None:
            self._hash = (
                hash(self.fraction())
                if self.rational
                else hash((self.numerators, self.denominator))
            )
        return self._hash

    def __lt__(self, other: Real) -> bool:
        """Whether this number is less than another."""
        return self._compare(other) < 0

    def __le__(self, other: Real) -> bool:
        """Whether this number is at most another."""
        return self._compare(other) <= 0

    def __gt__(self, other: Real) -> bool:
        """Whether this number is greater than another."""
        return self._compare(other) > 0

    def __ge__(self, other: Real) -> bool:
        """Whether this number is at least another."""
        return self._compare(other) >= 0

    def __float__(self) -> float:
        """A float near the number, near enough for pictures."""
        return self.estimate().value

    @override
    def __str__(self) -> str:
        """Write the number with fractions and radicals, such as ``1 - √2/2``."""
        if self.rational:
            return str(self.fraction())
        a, b = self._halves()
        term = _term(b, _radical(self.level))
        if not a:
            return term
        if term.startswith("-"):
            return f"{a} - {term[1:]}"
        return f"{a} + {term}"

    @override
    def __repr__(self) -> str:
        """Text that reads back as this number, when its radicals are integers."""
        return f"Number.of({str(self)!r})"

    def data(self) -> NumberData:
        """The number as JSON: a fraction's text, or its level with ``a`` and ``b``."""
        if self.rational:
            # The one form is in lowest terms, its denominator positive.
            numerator, denominator = self.numerators[0], self.denominator
            return str(numerator) if denominator == 1 else f"{numerator}/{denominator}"
        a, b = self._halves()
        return self.level, a.data(), b.data()

    @classmethod
    def from_data(cls, data: NumberData) -> Number:
        """The number that `data` wrote, in the tower it was written in."""
        match data:
            case str():
                numerator, _, denominator = data.partition("/")
                try:
                    whole, part = int(numerator), int(denominator or 1)
                except ValueError:
                    whole = part = 0
                # Text that Number.data did not write reads as a Fraction reads it.
                return cls((whole,), part) if part else cls.of(Fraction(data))
            case level, a, b:
                return _joined(cls.from_data(a), cls.from_data(b), level)

    def estimate(self) -> Estimate:
        """A float near the number, and a bound on how far from it the float lies."""
        if self._estimate is None:
            self._estimate = _tower().estimate(self)
        return self._estimate

    def _compare(self, other: Real) -> int:
        o = _coerce(other)
        estimated = (self.estimate() - o.estimate()).sign()
        return (self - o).sign() if estimated is None else estimated

    def _halves(self) -> tuple[Number, Number]:
        """The number as ``a + b√d`` at its level, ``a`` and ``b`` one level down."""
        half = len(self.numerators) // 2
        return (
            Number(self.numerators[:half], self.denominator),
            Number(self.numerators[half:], self.denominator),
        )


ZERO: Final = Number((0,))
ONE: Final = Number((1,))


class _Tower:
    """The radicands of a tower, with what arithmetic and estimates need of them."""

    def __init__(self, radicands: Iterable[Number] = ()) -> None:
        self.radicands: list[Number] = []
        self.vectors: list[Coefficients] = [()]
        """Each level's radicand as integer coefficients, as long as half that level."""
        self._roots: list[tuple[float, float]] = []
        self._bases: dict[int, list[tuple[float, float]]] = {}
        self.memo: dict[Hashable, object] = {}
        """Answers worked out from numbers of this tower, true for as long as it lasts.

        A level added later leaves every number already made, and so every answer,
        as it was.
        """
        for radicand in radicands:
            self.add(radicand)

    def add(self, radicand: Number) -> None:
        """Add a level whose root is the square root of ``radicand``."""
        self.vectors.append(_padded(radicand.numerators, 1 << len(self.radicands)))
        self.radicands.append(radicand)
        value, error = self.estimate(radicand).value, self.estimate(radicand).error
        low = value - error
        root = float_sqrt(max(value, 0.0))
        slack = error / (root + float_sqrt(low)) if low > 0 else inf
        self._roots.append((root, slack + root * _ROUNDING))
        self._bases.clear()

    def estimate(self, number: Number) -> Estimate:
        """A float near a number of this tower, and a bound on its error."""
        value = size = error = 0.0
        try:
            for n, (root, slack) in zip(
                number.numerators, self.basis(len(number.numerators)), strict=True
            ):
                if n:
                    term = n / number.denominator
                    value += term * root
                    size += abs(term * root)
                    error += abs(term) * slack
        except OverflowError:
            value, size, error = 0.0, inf, inf
        return Estimate(value, error + 4 * len(number.numerators) * size * _ROUNDING)

    def basis(self, size: int) -> list[tuple[float, float]]:
        """The first ``size`` products of roots, each a float and its error bound."""
        if size not in self._bases:
            products = [(1.0, 0.0)]
            for root, slack in self._roots[: size.bit_length() - 1]:
                products += [
                    (value * root, value * slack + error * (root + slack))
                    for value, error in products
                ]
            self._bases[size] = products[:size]
        return self._bases[size]


_TOWER: ContextVar[_Tower] = ContextVar("tower")


def _tower() -> _Tower:
    """The current tower: the rationals alone, until a root needs a level."""
    levels = _TOWER.get(None)
    if levels is None:
        levels = _Tower()
        _TOWER.set(levels)
    return levels


def remembered[**P, T](compute: Callable[P, T]) -> Callable[P, T]:
    """Remember a function's answers for the numbers of the current tower.

    Folding asks the same questions of the same faces step after step, and a tower's
    numbers keep their meaning while it lasts, so its answers keep too. The arguments
    must be hashable.
    """

    @wraps(compute)
    def answer(*args: P.args, **kwargs: P.kwargs) -> T:
        memo = _tower().memo
        key = (compute, args, tuple(kwargs.items()))
        if key in memo:
            return cast("T", memo[key])
        value = memo[key] = compute(*args, **kwargs)
        return value

    return answer


def radicands() -> tuple[Number, ...]:
    """The radicands of the current tower, level 1 first."""
    return tuple(_tower().radicands)


def extend(levels: Iterable[Number]) -> None:
    """Add levels above the current tower's, as the kernel's square roots added them.

    Each radicand has integer coefficients below its own level, and its root lies in no
    level below.
    """
    current = _tower()
    for radicand in levels:
        current.add(radicand)


@contextmanager
def tower(levels: Iterable[Number]) -> Generator[None]:
    """Use the tower of these radicands, level 1 first, until the block ends."""
    token = _TOWER.set(_Tower(levels))
    try:
        yield
    finally:
        _TOWER.reset(token)


def _radicand(level: int) -> Number:
    return _tower().radicands[level - 1]


def _coerce(value: Real) -> Number:
    return value if isinstance(value, Number) else Number.of(value)


def _padded(numerators: Coefficients, size: int) -> Coefficients:
    return numerators + (0,) * (size - len(numerators))


def _scaled(numerators: Coefficients, factor: int) -> Coefficients:
    return tuple(n * factor for n in numerators)


def _plus(x: Coefficients, y: Coefficients) -> Coefficients:
    if len(x) < len(y):
        x, y = y, x
    return tuple(a + b for a, b in zip(x, _padded(y, len(x)), strict=True))


def _minus(x: Coefficients, y: Coefficients) -> Coefficients:
    return tuple(a - b for a, b in zip(x, y, strict=True))


def _product(
    x: Coefficients, y: Coefficients, vectors: list[Coefficients]
) -> Coefficients:
    """The product of two equally long coefficient vectors, in integers.

    With ``x = a + b√d`` and ``y = c + e√d``, the product is ``ac + be·d`` plus
    ``(a + b)(c + e) - ac - be`` times ``√d``: three products one level down.
    """
    if len(x) == 1:
        return (x[0] * y[0],)
    half = len(x) // 2
    a, b, c, e = x[:half], x[half:], y[:half], y[half:]
    ac, be = _product(a, c, vectors), _product(b, e, vectors)
    mixed = _product(_plus(a, b), _plus(c, e), vectors)
    radicand = vectors[half.bit_length()]
    return _plus(ac, _product(be, radicand, vectors)) + _minus(_minus(mixed, ac), be)


def _joined(a: Number, b: Number, level: int) -> Number:
    """The number ``a + b√d`` at a level, from ``a`` and ``b`` below it."""
    half = 1 << (level - 1)
    common = lcm(a.denominator, b.denominator)
    first = _padded(_scaled(a.numerators, common // a.denominator), half)
    second = _padded(_scaled(b.numerators, common // b.denominator), half)
    return Number(first + second, common)


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
        return None if scaled is None else _joined(ZERO, scaled, level)
    a, b = number._halves()  # noqa: SLF001 - the field's own
    norm = _root(a * a - b * b * _radicand(level), level - 1)
    for square in () if norm is None else ((a + norm) / 2, (a - norm) / 2):
        if square.sign() > 0 and (part := _root(square, level - 1)) is not None:
            return _joined(part, b / (2 * part), level)
    return None


def _rational_root(number: Number) -> Number | None:
    if not number.rational or number.sign() < 0:
        return None
    value = number.fraction()
    numerator, denominator = isqrt(value.numerator), isqrt(value.denominator)
    if numerator * numerator == value.numerator and (
        denominator * denominator == value.denominator
    ):
        return Number((numerator,), denominator)
    return None


def _reduced(number: Number) -> tuple[Number, Fraction]:
    """Write a positive number as ``f²y``, so that its square root is ``f√y``.

    The coefficients of ``y`` are integers with no square factor in common, so a
    rational ``y`` is a square-free integer.
    """
    content = number.denominator * gcd(*number.numerators)
    factor = Fraction(_square_root_part(content), number.denominator)
    return number / Number.of(factor * factor), factor


def _square_root_part(n: int) -> int:
    """The largest integer whose square divides a positive integer.

    Trial division removes every prime up to the cube root of what remains; what then
    remains is 1, a prime, a product of two primes, or a prime's square.
    """
    root, rest, p = 1, n, 2
    while p * p * p <= rest:
        while not rest % (p * p):
            rest //= p * p
            root *= p
        if not rest % p:
            rest //= p
        p += 1
    last = isqrt(rest)
    return root * last if last * last == rest else root


def _radical(level: int) -> str:
    radicand = _radicand(level)
    return f"√{radicand}" if radicand.rational else f"√({radicand})"


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
        value = Number((int(numerator or 1),), int(denominator or 1))
        if radicand is not None:
            value *= Number((int(radicand),)).sqrt()
        total += -value if sign == "-" else value
        position = match.end()
    if position < end or not text.strip():
        msg = f"{text!r} is not a number such as '1 - √2/2'"
        raise ValueError(msg)
    return total
