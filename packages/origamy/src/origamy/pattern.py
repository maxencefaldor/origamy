# Copyright 2026 Maxence Faldor
"""Crease patterns: the unfolded paper's creases, each a segment and what it folds to.

Published designs often come as crease patterns rather than step by step. A script can
read one from a FOLD or ORIPA file, crease the open paper along it in one step, and
collapse it in the next, as a folder precreases a pattern and then collapses it.

A file gives its coordinates as floats, and origamy's numbers are exact. Reading snaps
each coordinate, once the paper's longer side measures 1, to the simplest exact number
within a tolerance: a fraction, or a fraction plus a fraction of √2 or √3. Patterns on
grids and on 22.5° and 30° angles all snap; a coordinate no such number matches is
refused.
"""

import json
from collections.abc import Callable, Iterable, Sequence
from dataclasses import dataclass
from fractions import Fraction
from functools import cmp_to_key
from itertools import pairwise
from math import sqrt
from pathlib import Path
from typing import Final

from origamy.field import ONE, ZERO, Number
from origamy.geometry import Point, area, covers, orientation
from origamy.rules import RefusalError
from origamy.state import Edge, Face, State

type Segment = tuple[Point, Point]
"""A crease of the unfolded paper, from one end to the other."""

TOLERANCE: Final = 1e-9
"""How far, as a share of the paper's longer side, a coordinate may lie from the exact
number it snaps to: a double's precision, with room for a file's rounding."""

_DENOMINATORS: Final = (1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 32, 48, 64, 96, 128)
"""The denominators a snapped fraction may take, simplest first."""

_SHARES: Final = (1, 2, 3, 4, 6, 8, 12, 16)
"""The denominators of a snapped coordinate's share of a square root, simplest first."""

_ROOTS: Final = (2, 3)
"""The square roots a snapped coordinate may hold besides a fraction."""

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

        def snapped(point: tuple[float, float]) -> Point:
            x, y = (point[0] - left) / size, (point[1] - bottom) / size
            return Point(snap(x, tolerance), snap(y, tolerance))

        width = snap((max(xs) - left) / size, tolerance)
        height = snap((max(ys) - bottom) / size, tolerance)
        kinds: dict[str, list[Segment]] = {"V": [], "M": [], "F": []}
        for kind, p, q in lines:
            if kind in kinds:
                kinds[kind].append((snapped(p), snapped(q)))
        return cls(
            width,
            height,
            tuple(kinds["V"]),
            tuple(kinds["M"]),
            tuple(kinds["F"]),
        )


def snap(value: float, tolerance: float = TOLERANCE) -> Number:
    """The simplest exact number within a tolerance of a float.

    The candidates are fractions, then fractions plus a fraction of √2 or √3, each with
    the smallest denominators first.

    Raises:
        ValueError: No such number lies within the tolerance.
    """
    for denominator in _DENOMINATORS:
        fraction = Fraction(round(value * denominator), denominator)
        if abs(float(fraction) - value) <= tolerance:
            return Number.of(fraction)
    for root in _ROOTS:
        for denominator in _SHARES:
            for numerator in range(-2 * denominator, 2 * denominator + 1):
                if not numerator:
                    continue
                share = Fraction(numerator, denominator)
                rest = value - float(share) * sqrt(root)
                for under in _DENOMINATORS:
                    fraction = Fraction(round(rest * under), under)
                    if abs(float(fraction) - rest) <= tolerance:
                        return (
                            Number.of(fraction)
                            + Number.of(share) * Number.of(root).sqrt()
                        )
    msg = f"the coordinate {value!r} lies within {tolerance} of no exact number"
    raise ValueError(msg)


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
