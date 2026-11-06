# Copyright 2026 Maxence Faldor
"""Crease patterns: the unfolded paper's creases, each a segment and what it folds to.

Published designs often come as crease patterns rather than step by step. A script can
read one from a FOLD or ORIPA file, crease the open paper along it in one step, and
collapse it in the next, as a folder precreases a pattern and then collapses it.

A file gives its coordinates as floats, and origamy's numbers are exact. Reading snaps
each coordinate, once the paper's longer side measures 1, to the exact number that
explains it best: a fraction, or ``(p + q√r) / d`` with small integers and r one of
2, 3, 5 and 6, found by lattice reduction. A corner no number explains lies where two
lines of creases through exact corners cross. Patterns on grids, on 22.5° and 30°
angles, with golden ratios, and on crossings of their creases all read exactly.
"""

import json
from collections import defaultdict
from collections.abc import Iterable, Iterator
from dataclasses import dataclass
from fractions import Fraction
from functools import cmp_to_key
from math import floor, hypot, inf, sqrt
from pathlib import Path
from typing import Final, NoReturn

import origamy_kernel
from origamy import kernel
from origamy.field import ONE, ZERO, Number
from origamy.geometry import Line, Point, orientation
from origamy.rules import RefusalError
from origamy.state import Face, State

type Segment = tuple[Point, Point]
"""A crease of the unfolded paper, from one end to the other."""

type _Float = tuple[float, float]
"""A point as a file gives it, scaled to the paper whose longer side measures 1."""

TOLERANCE: Final = 1e-9
"""How far, as a share of the paper's longer side, a coordinate may lie from the exact
number it snaps to: room for a file's rounding."""

FIT: Final = 1e-4
"""How well an exact number must explain a coordinate, for its complexity.

A random float lies about ``1 / d²`` from the nearest fraction with denominator up to
d, and about ``1 / s³`` from the nearest ``(p + q√r) / d`` with integers up to s. A
number's fit is its error times d², or times s³: a random float's best is about 1, and
a coordinate the number explains has a fit far smaller. A snapped number's fit is at
most `FIT`.
"""

SURE: Final = 1e-8
"""The fit at which reading takes a snapped number before any crossing of creases."""

_DENOMINATOR: Final = 10**6
"""The largest denominator of a fraction a coordinate snaps to."""

_SIZE: Final = 10**4
"""The largest integer of a ``(p + q√r) / d`` a coordinate snaps to."""

_SCALE: Final = 10**12
"""The weight of a relation's error against its integers, in lattice reduction."""

_ROOTS: Final = (2, 3, 5, 6)
"""The square roots a snapped coordinate may hold besides a fraction."""

_NEAR: Final = 1e-7
"""How far apart, as a share of the paper's longer side, two float points may lie and
still be one: far looser than snapping, since it only matches corners up."""

_ORIPA: Final = {"1": "B", "2": "M", "3": "V"}
"""ORIPA's line types: an edge or a cut, a mountain, a valley; others are aid lines."""

_ORIPA_FIELDS: Final = 5
"""The fields of an ORIPA line: its type, then the coordinates of its two ends."""


@dataclass(frozen=True, slots=True)
class CreasePattern:
    """The creases of the unfolded paper, as segments, by what each folds to.

    Valleys and mountains read as seen from the paper's front. Flat creases are creased
    but stay flat, as precreases do.
    """

    width: Number
    """The paper's width."""
    height: Number
    """The paper's height."""
    outline: tuple[Point, ...]
    """The paper's corners: the convex hull of its edges' ends, counterclockwise from
    the lowest leftmost corner."""
    valleys: tuple[Segment, ...] = ()
    mountains: tuple[Segment, ...] = ()
    flats: tuple[Segment, ...] = ()

    @property
    def creases(self) -> tuple[Segment, ...]:
        """Every crease of the pattern, valleys, mountains and flat creases alike."""
        return (*self.valleys, *self.mountains, *self.flats)

    @classmethod
    def read(cls, path: str | Path, *, tolerance: float = TOLERANCE) -> CreasePattern:
        """Read a crease pattern from a FOLD file or an ORIPA ``.cp`` file.

        The box around the paper's edges goes from (0, 0), and its longer side to
        length 1.
        ``tolerance`` is how far, as a share of that side, a coordinate may lie from the
        exact number it snaps to.

        Raises:
            ValueError: The file holds no crease, a crease the pattern cannot fold, or a
                coordinate that snaps to no exact number.
        """
        path = Path(path)
        text = path.read_text(encoding="utf-8")
        lines = _oripa(text) if path.suffix == ".cp" else _fold(text)
        if not any(kind in "MVF" for kind, _, _ in lines):
            msg = f"{path.name} holds no crease"
            raise ValueError(msg)
        xs = [x for _, p, q in lines for x in (p[0], q[0])]
        ys = [y for _, p, q in lines for y in (p[1], q[1])]
        left, bottom = min(xs), min(ys)
        size = max(max(xs) - left, max(ys) - bottom)

        def scaled(point: tuple[float, float]) -> _Float:
            return ((point[0] - left) / size, (point[1] - bottom) / size)

        segments = [(kind, scaled(p), scaled(q)) for kind, p, q in lines]
        exact = _exact([(p, q) for _, p, q in segments], tolerance)
        width = snap((max(xs) - left) / size, tolerance)
        height = snap((max(ys) - bottom) / size, tolerance)
        kinds: dict[str, list[Segment]] = {"B": [], "V": [], "M": [], "F": []}
        for kind, p, q in segments:
            kinds[kind].append((exact[_key(p)], exact[_key(q)]))
        corners = {end for edge in kinds["B"] for end in edge} or {
            Point(x, y) for x in (ZERO, width) for y in (ZERO, height)
        }
        return cls(
            width,
            height,
            _hull(corners),
            tuple(kinds["V"]),
            tuple(kinds["M"]),
            tuple(kinds["F"]),
        )


def _hull(points: Iterable[Point]) -> tuple[Point, ...]:
    """The convex hull's corners, counterclockwise from the lowest leftmost point."""
    ordered = sorted(set(points), key=cmp_to_key(_by_place))
    chains: list[list[Point]] = [[], []]
    for chain, run in zip(chains, (ordered, ordered[::-1]), strict=True):
        for point in run:
            while len(chain) >= 2 and orientation(chain[-2], chain[-1], point) <= 0:  # noqa: PLR2004 - a turn needs two corners
                chain.pop()
            chain.append(point)
    lower, upper = chains
    return (*lower[:-1], *upper[:-1])


def _by_place(p: Point, q: Point) -> int:
    """Order points by x, then by y."""
    if p.x != q.x:
        return -1 if p.x < q.x else 1
    return (p.y > q.y) - (p.y < q.y)


def _key(point: _Float) -> _Float:
    """A float point rounded, so that one corner written twice reads as one."""
    return (round(point[0], 10), round(point[1], 10))


def _exact(
    segments: list[tuple[_Float, _Float]], tolerance: float
) -> dict[_Float, Point]:
    """The exact point of each segment's ends, keyed by `_key`.

    A corner whose coordinates both snap surely, within `SURE`, takes them. Any other
    corner lies where two lines of creases cross, each through exact corners; corners
    found so let more be found. Snapping within `FIT` then finds the corners left, and
    lets crossings find more, until none is left.

    Raises:
        ValueError: A corner snaps to no exact point, and no two lines of creases
            through exact corners cross there.
    """
    explanations: dict[float, tuple[float, Number] | None] = {}

    def snapped(value: float, fit: float) -> Number | None:
        if value not in explanations:
            explanations[value] = _explanation(value, tolerance)
        found = explanations[value]
        return found[1] if found is not None and found[0] <= fit else None

    near: dict[_Float, set[_Float]] = defaultdict(set)
    floats: dict[_Float, _Float] = {}
    for p, q in segments:
        near[_key(p)].add(_key(q))
        near[_key(q)].add(_key(p))
        floats.setdefault(_key(p), p)
        floats.setdefault(_key(q), q)
    exact: dict[_Float, Point] = {}
    fit = SURE
    while left := sorted(floats.keys() - exact.keys()):
        found: dict[_Float, Point] = {}
        for key in left:
            x, y = floats[key]
            if (sx := snapped(x, fit)) is not None and (
                sy := snapped(y, fit)
            ) is not None:
                found[key] = Point(sx, sy)
        if not found:
            found = {
                key: point
                for key in left
                if (point := _crossing(key, near, exact, tolerance)) is not None
            }
        if found:
            exact |= found
        elif fit < FIT:
            fit = FIT
        else:
            x, y = floats[left[0]]
            value = x if snapped(x, fit) is None else y
            msg = (
                f"the coordinate {value!r} lies within {tolerance} of no exact number, "
                f"and no two lines of creases cross at ({x!r}, {y!r})"
            )
            raise ValueError(msg)
    return exact


def _crossing(
    key: _Float,
    near: dict[_Float, set[_Float]],
    exact: dict[_Float, Point],
    tolerance: float,
) -> Point | None:
    """Where two lines of creases through exact corners cross at a corner.

    A line runs from the corner along creases that continue straight, in both
    directions, through two exact corners on its way. Failing two such lines, a crease
    to an exact corner gives a line too, when its slope snaps to an exact number.
    """
    lines: list[Line] = []
    for line in _lines_through(key, near, exact, tolerance):
        if any(line.parallel(other) for other in lines):
            continue
        lines.append(line)
        if len(lines) == 2:  # noqa: PLR2004 - two lines make a point
            point = lines[0].meet(lines[1])
            x, y = point.floats() if point else (inf, inf)
            if max(abs(x - key[0]), abs(y - key[1])) <= _NEAR:
                return point
            return None
    return None


def _lines_through(
    key: _Float,
    near: dict[_Float, set[_Float]],
    exact: dict[_Float, Point],
    tolerance: float,
) -> Iterator[Line]:
    """The lines of creases through a corner, those through two exact corners first."""
    for start in sorted(near[key]):
        known = {
            exact[corner]
            for direction in (start, _opposite(key, start, near[key]))
            if direction is not None
            for corner in _straight(key, direction, near)
            if corner in exact
        }
        if len(known) >= 2:  # noqa: PLR2004 - two points make a line
            yield Line.through(*sorted(known, key=Point.floats)[:2])
    for start in sorted(near[key] & exact.keys()):
        dx, dy = key[0] - start[0], key[1] - start[1]
        steep = abs(dy) > abs(dx)
        try:
            slope = snap(dx / dy if steep else dy / dx, tolerance / hypot(dx, dy))
        except ValueError:
            continue
        step = Point(slope, ONE) if steep else Point(ONE, slope)
        yield Line.through(exact[start], exact[start] + step)


def _opposite(key: _Float, start: _Float, near: set[_Float]) -> _Float | None:
    """The neighbor of a corner straight across it from another, if any."""
    return next(
        (other for other in sorted(near) if _straight_on(start, key, other)), None
    )


def _straight(
    key: _Float, start: _Float, near: dict[_Float, set[_Float]]
) -> list[_Float]:
    """The corners from a corner's neighbor on, while the creases run straight."""
    corners: list[_Float] = []
    previous: _Float = key
    current: _Float | None = start
    while current is not None:
        corners.append(current)
        previous, current = (
            current,
            next(
                (
                    n
                    for n in sorted(near[current])
                    if _straight_on(previous, current, n)
                ),
                None,
            ),
        )
    return corners


def _straight_on(a: _Float, b: _Float, c: _Float) -> bool:
    """Whether the path from a through b to c runs straight on at b."""
    ux, uy = b[0] - a[0], b[1] - a[1]
    vx, vy = c[0] - b[0], c[1] - b[1]
    lengths = hypot(ux, uy) * hypot(vx, vy)
    return (
        lengths > 0
        and ux * vx + uy * vy > 0
        and abs(ux * vy - uy * vx) <= _NEAR * lengths
    )


def snap(value: float, tolerance: float = TOLERANCE, fit: float = FIT) -> Number:
    """The exact number that explains a float best, within a tolerance and a fit.

    The candidates are fractions with denominators up to a million, and
    ``(p + q√r) / d`` for r one of 2, 3, 5 and 6 with integers up to 10,000. The best
    has the smallest fit, as `FIT` defines it.

    Raises:
        ValueError: No candidate lies within the tolerance and the fit.
    """
    found = _explanation(value, tolerance)
    if found is None or found[0] > fit:
        msg = f"the coordinate {value!r} lies within {tolerance} of no exact number"
        raise ValueError(msg)
    return found[1]


type _Candidate = tuple[float, int, Fraction, Fraction, int]
"""A candidate ``a + b√r``: its fit, its size, a, b and r, with r 1 for a fraction."""


def _explanation(value: float, tolerance: float) -> tuple[float, Number] | None:
    """The candidate that explains a float best, and its fit, within a tolerance.

    Only the best candidate becomes a number, so the tower gains a level only for a
    root that some coordinate needs.
    """
    found: list[_Candidate] = []
    for fraction in _convergents(Fraction(value)):
        error = abs(value - float(fraction))
        if error <= tolerance:
            fit = error * fraction.denominator**2
            found.append((fit, fraction.denominator, fraction, Fraction(0), 1))
    if not any(fit <= SURE for fit, *_ in found):
        for root in _ROOTS:
            surd = sqrt(root)
            for d, p, q in _relations(value, root):
                if abs(value - (p + q * surd) / d) <= tolerance:
                    size = max(d, abs(p), abs(q))
                    fit = abs(d * value - p - q * surd) * size**2
                    found.append((fit, size, Fraction(p, d), Fraction(q, d), root))
    if not found:
        return None
    fit, _, rational, surd, root = min(found, key=lambda candidate: candidate[:2])
    number = Number.of(rational)
    if surd:
        number += Number.of(surd) * Number.of(root).sqrt()
    return fit, number


def _convergents(value: Fraction) -> Iterator[Fraction]:
    """The continued fraction's convergents, while their denominators stay small."""
    previous, current = (0, 1), (1, 0)
    rest = value
    while True:
        whole = floor(rest)
        previous, current = (
            current,
            (
                whole * current[0] + previous[0],
                whole * current[1] + previous[1],
            ),
        )
        if current[1] > _DENOMINATOR:
            return
        yield Fraction(*current)
        if rest == whole:
            return
        rest = 1 / (rest - whole)


def _relations(value: float, root: int) -> list[tuple[int, int, int]]:
    """Small integers ``(d, p, q)``, with q nonzero, that make ``d·value ≈ p + q√root``.

    Lattice reduction of the integer combinations of ``value``, 1 and ``√root`` leaves
    short vectors: those with small integers and a small error. The kernel reduces
    the lattice exactly, by Cohen's integral LLL.
    """
    return origamy_kernel.relations(value, root, _SCALE, _SIZE)


def crease(state: State, segments: Iterable[Segment]) -> State:
    """Crease open paper along segments of the crease pattern, folding nothing.

    The paper must lie open in one layer. The new creases, the paper's edges and its
    creases so far divide it into faces, which keep the paper's place. Every crease
    must end on the paper's edge or on another crease, and every face must be convex.
    """
    places = {face.place for face in state.faces}
    if len(places) > 1:
        msg = "a crease pattern creases paper that lies open in one layer"
        raise RefusalError(msg, rule="paper-folded", state=state)
    (place,) = places
    segments = list(segments)
    outcome, hold, added, faces, index = state.kernel.crease(
        [kernel.point(p) for p in _outline(state)],
        [(kernel.point(p), kernel.point(q)) for p, q in segments],
    )
    if hold is None:
        _refuse_crease(state, outcome, segments, index)
    vertices = (*state.vertices, *(kernel.from_point(p) for p in added))
    corners = tuple(Face(tuple(c), place) for c in faces)
    return State.held(hold, vertices=vertices, faces=corners)


def _refuse_crease(
    state: State, outcome: str, segments: list[Segment], index: int | None
) -> NoReturn:
    """Refuse creasing the kernel refused, with the facts it found.

    Raises:
        RefusalError: Always.
    """
    if outcome == "off-paper" and index is not None:
        p, q = segments[index]
        msg = f"the crease from {p} to {q} runs off the paper"
        raise RefusalError(msg, rule="crease-off-paper", state=state)
    side = (
        "meeting no other crease"
        if outcome == "loose"
        else "leaving a face that is not convex"
    )
    msg = f"a crease ends inside the paper, {side}"
    raise RefusalError(msg, rule="loose-crease", state=state)


def _outline(state: State) -> tuple[Point, ...]:
    """The open paper's outline on the crease pattern, counterclockwise."""
    points = {state.vertices[v] for edge in state.border for v in edge}
    start = min(points, key=lambda p: (p.y, p.x))
    rest = sorted(
        points - {start},
        key=cmp_to_key(lambda a, b: -orientation(start, a, b)),
    )
    hull = [start]
    for p in rest:
        while len(hull) > 1 and orientation(hull[-2], hull[-1], p) <= 0:
            hull.pop()
        hull.append(p)
    return tuple(hull)


def _fold(text: str) -> list[tuple[str, tuple[float, float], tuple[float, float]]]:
    """A FOLD file's edges, each as its assignment and its two ends."""
    document = json.loads(text)
    coords = document["vertices_coords"]
    edges = document["edges_vertices"]
    kinds = document.get("edges_assignment", ["U"] * len(edges))
    unknown = sorted({k for k in kinds if k not in "BMVFU"})
    if unknown:
        msg = f"edges assigned {', '.join(unknown)} are not creases origamy folds"
        raise ValueError(msg)
    return [
        ("F" if kind == "U" else kind, tuple(coords[u][:2]), tuple(coords[v][:2]))
        for (u, v), kind in zip(edges, kinds, strict=True)
    ]


def _oripa(text: str) -> list[tuple[str, tuple[float, float], tuple[float, float]]]:
    """An ORIPA file's lines, each as its kind and its two ends, aid lines left out."""
    found = []
    for row in text.split("\n"):
        fields = row.split()
        if len(fields) != _ORIPA_FIELDS or fields[0] not in _ORIPA:
            continue
        x1, y1, x2, y2 = (float(f) for f in fields[1:])
        found.append((_ORIPA[fields[0]], (x1, y1), (x2, y2)))
    return found
