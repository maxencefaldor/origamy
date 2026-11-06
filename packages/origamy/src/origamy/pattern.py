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
from collections.abc import Callable, Iterable, Iterator, Sequence
from dataclasses import dataclass
from fractions import Fraction
from functools import cmp_to_key
from itertools import pairwise
from math import floor, hypot, inf, sqrt
from pathlib import Path
from typing import Final

from origamy.field import ONE, ZERO, Number
from origamy.geometry import Line, Point, area, covers, orientation
from origamy.rules import RefusalError
from origamy.state import Edge, Face, State

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

        The paper's lower left corner goes to (0, 0) and its longer side to length 1.
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
        kinds: dict[str, list[Segment]] = {"V": [], "M": [], "F": []}
        for kind, p, q in segments:
            if kind in kinds:
                kinds[kind].append((exact[_key(p)], exact[_key(q)]))
        return cls(
            width,
            height,
            tuple(kinds["V"]),
            tuple(kinds["M"]),
            tuple(kinds["F"]),
        )


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


def _explanation(value: float, tolerance: float) -> tuple[float, Number] | None:
    """The candidate that explains a float best, and its fit, within a tolerance."""
    found: list[tuple[float, int, Number]] = []
    for fraction in _convergents(Fraction(value)):
        error = abs(value - float(fraction))
        if error <= tolerance:
            fit = error * fraction.denominator**2
            found.append((fit, fraction.denominator, Number.of(fraction)))
    if any(fit <= SURE for fit, _, _ in found):
        return _best(found)
    for root in _ROOTS:
        surd = sqrt(root)
        for d, p, q in _relations(value, root):
            if abs(value - (p + q * surd) / d) <= tolerance:
                size = max(d, abs(p), abs(q))
                fit = abs(d * value - p - q * surd) * size**2
                number = (
                    Number.of(Fraction(p, d))
                    + Number.of(Fraction(q, d)) * Number.of(root).sqrt()
                )
                found.append((fit, size, number))
    if not found:
        return None
    return _best(found)


def _best(found: list[tuple[float, int, Number]]) -> tuple[float, Number]:
    """The candidate with the smallest fit, the smallest size among equals."""
    fit, _, number = min(found, key=lambda candidate: candidate[:2])
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
    short vectors: those with small integers and a small error.
    """
    surd = sqrt(root)
    basis = [
        [1, 0, 0, round(_SCALE * value)],
        [0, 1, 0, _SCALE],
        [0, 0, 1, round(_SCALE * surd)],
    ]
    found = []
    for times, minus_p, minus_q, _ in _reduced(basis):
        sign = 1 if times > 0 else -1
        d, p, q = sign * times, -sign * minus_p, -sign * minus_q
        if d and q and max(d, abs(p), abs(q)) <= _SIZE:
            found.append((d, p, q))
    return found


def _reduced(basis: list[list[int]]) -> list[list[int]]:
    """An LLL-reduced basis of the lattice the integer vectors span."""
    vectors = [list(vector) for vector in basis]
    lengths, mu = _projections(vectors)
    k = 1
    while k < len(vectors):
        for j in range(k - 1, -1, -1):
            if shift := round(mu[k][j]):
                vectors[k] = [
                    a - shift * b for a, b in zip(vectors[k], vectors[j], strict=True)
                ]
                mu[k][j] -= shift
                for i in range(j):
                    mu[k][i] -= shift * mu[j][i]
        if lengths[k] >= (Fraction(3, 4) - mu[k][k - 1] ** 2) * lengths[k - 1]:
            k += 1
        else:
            vectors[k], vectors[k - 1] = vectors[k - 1], vectors[k]
            lengths, mu = _projections(vectors)
            k = max(k - 1, 1)
    return vectors


def _projections(
    vectors: list[list[int]],
) -> tuple[list[Fraction], list[list[Fraction]]]:
    """Gram-Schmidt: each orthogonal part's squared length, and the coefficients."""
    parts: list[list[Fraction]] = []
    lengths: list[Fraction] = []
    mu = [[Fraction(0)] * len(vectors) for _ in vectors]
    for i, vector in enumerate(vectors):
        part = [Fraction(a) for a in vector]
        for j, other in enumerate(parts):
            mu[i][j] = (
                sum((a * b for a, b in zip(vector, other, strict=True)), Fraction(0))
                / lengths[j]
            )
            part = [a - mu[i][j] * b for a, b in zip(part, other, strict=True)]
        parts.append(part)
        lengths.append(sum((a * a for a in part), Fraction(0)))
    return lengths, mu


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
    outline = _outline(state)
    lines = [(state.vertices[u], state.vertices[v]) for u, v in state.edges]
    for p, q in segments:
        if p == q:
            continue
        if not (covers(outline, p) and covers(outline, q)):
            msg = f"the crease from {p} to {q} runs off the paper"
            raise RefusalError(msg, rule="crease-off-paper", state=state)
        lines.append((p, q))
    vertices = list(state.vertices)
    index = {p: i for i, p in enumerate(vertices)}

    def vertex(p: Point) -> int:
        if p not in index:
            index[p] = len(vertices)
            vertices.append(p)
        return index[p]

    edges: set[Edge] = set()
    for p, q, points in _split(lines):
        direction = q - p
        ordered = sorted({p, q, *points}, key=lambda r: (r - p).dot(direction))
        for a, b in pairwise(ordered):
            u, v = vertex(a), vertex(b)
            edges.add((min(u, v), max(u, v)))
    faces = _faces(vertices, edges, state)
    return State(tuple(vertices), tuple(Face(corners, place) for corners in faces))


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


def _split(
    lines: Sequence[Segment],
) -> list[tuple[Point, Point, set[Point]]]:
    """Each segment with the points where other segments cross it or end on it."""
    boxes = [_box(p, q) for p, q in lines]
    points: list[set[Point]] = [set() for _ in lines]
    for i, (p, q) in enumerate(lines):
        for j in range(i + 1, len(lines)):
            if not _boxes_meet(boxes[i], boxes[j]):
                continue
            r, s = lines[j]
            for point in _meeting(p, q, r, s):
                points[i].add(point)
                points[j].add(point)
    return [(p, q, found) for (p, q), found in zip(lines, points, strict=True)]


def _meeting(p: Point, q: Point, r: Point, s: Point) -> list[Point]:
    """Where two segments meet: a crossing, an end on the other, or shared stretches."""
    d, e = q - p, s - r
    denominator = d.cross(e)
    if not denominator:
        if (r - p).cross(d):
            return []
        # Collinear: each end that lies on the other segment splits it.
        return [a for a in (p, q) if _on(r, s, a)] + [a for a in (r, s) if _on(p, q, a)]
    t = (r - p).cross(e) / denominator
    u = (r - p).cross(d) / denominator
    if ZERO <= t <= ONE and ZERO <= u <= ONE:
        return [p + d * t]
    return []


def _on(p: Point, q: Point, point: Point) -> bool:
    """Whether a point lies on a segment, ends included."""
    d = q - p
    offset = point - p
    return not d.cross(offset) and ZERO <= offset.dot(d) <= d.dot(d)


def _box(p: Point, q: Point) -> tuple[float, float, float, float]:
    (px, py), (qx, qy) = p.floats(), q.floats()
    return min(px, qx), min(py, qy), max(px, qx), max(py, qy)


def _boxes_meet(
    first: tuple[float, float, float, float], second: tuple[float, float, float, float]
) -> bool:
    slack = 1e-9
    return (
        first[0] <= second[2] + slack
        and second[0] <= first[2] + slack
        and first[1] <= second[3] + slack
        and second[1] <= first[3] + slack
    )


def _faces(
    vertices: Sequence[Point], edges: set[Edge], state: State
) -> list[tuple[int, ...]]:
    """The faces the edges divide the paper into, each counterclockwise.

    Each face lies to the left of its edges, walked in turn: at each corner, the walk
    turns onto the next edge clockwise from the one it came along.
    """
    around: dict[int, list[int]] = {}
    for u, v in edges:
        around.setdefault(u, []).append(v)
        around.setdefault(v, []).append(u)
    for v, others in around.items():
        others.sort(key=cmp_to_key(_by_angle(vertices[v], vertices)))
    position = {(v, w): i for v, others in around.items() for i, w in enumerate(others)}
    seen: set[tuple[int, int]] = set()
    faces: list[tuple[int, ...]] = []
    for u, v in sorted(edges):
        for start in ((u, v), (v, u)):
            if start in seen:
                continue
            walk = []
            here = start
            while here not in seen:
                seen.add(here)
                walk.append(here[0])
                a, b = here
                others = around[b]
                here = (b, others[(position[b, a] - 1) % len(others)])
            polygon = tuple(vertices[i] for i in walk)
            if area(polygon).sign() > 0:
                _check(polygon, state)
                faces.append(tuple(walk))
    return faces


def _check(polygon: Sequence[Point], state: State) -> None:
    """Refuse a face a loose crease leaves: one it borders twice, or one not convex."""
    if len(set(polygon)) < len(polygon):
        msg = "a crease ends inside the paper, meeting no other crease"
        raise RefusalError(msg, rule="loose-crease", state=state)
    n = len(polygon)
    if any(
        orientation(polygon[i - 1], polygon[i], polygon[(i + 1) % n]) < 0
        for i in range(n)
    ):
        msg = "a crease ends inside the paper, leaving a face that is not convex"
        raise RefusalError(msg, rule="loose-crease", state=state)


def _by_angle(center: Point, vertices: Sequence[Point]) -> Callable[[int, int], int]:
    """Compare neighbors of a vertex counterclockwise, from the positive x axis on."""

    def half(vector: Point) -> int:
        return 0 if vector.y.sign() > 0 or (not vector.y and vector.x.sign() > 0) else 1

    def compare(a: int, b: int) -> int:
        va, vb = vertices[a] - center, vertices[b] - center
        ha, hb = half(va), half(vb)
        if ha != hb:
            return ha - hb
        return -va.cross(vb).sign()

    return compare


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
