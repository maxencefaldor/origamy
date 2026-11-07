# Copyright 2026 Maxence Faldor
"""Pictures of the paper as SVG: the crease pattern, the folded paper and an x-ray.

Each view draws in exact coordinates turned into floats only here, with rulers along
two sides so that a reader can take coordinates off the picture. Shaped paper draws in
space instead, seen tilted as the 3D viewer first shows it, nearer faces over farther.
"""

from collections.abc import Iterable, Sequence
from dataclasses import dataclass, field
from fractions import Fraction
from functools import cached_property
from graphlib import CycleError, TopologicalSorter
from html import escape
from math import atan2, ceil, cos, floor, hypot, sin, sqrt
from typing import Final

from origamy.field import Number
from origamy.geometry import Line, Point
from origamy.space import Form, Motion, Shaped, Turn, Vector, apply, rotate, same
from origamy.state import State

type Flat = tuple[float, float]
"""A point of a picture's plane, as floats."""

MOUNTAIN = "#c0392b"
VALLEY = "#2c6fbb"
FLAT = "#9a9a9a"
INK = "#2b2b2b"
MARK = "#e0182d"


@dataclass
class Canvas:
    """A square SVG view of a box of the plane, with room for rulers."""

    box: tuple[float, float, float, float]
    size: int = 440
    margin: int = 36
    parts: list[str] = field(default_factory=list)

    @property
    def scale(self) -> float:
        """Pixels per unit of length."""
        x0, y0, x1, y1 = self.box
        return (self.size - 2 * self.margin) / max(x1 - x0, y1 - y0, 1e-9)

    @cached_property
    def _frame(self) -> tuple[float, float, float, float, float]:
        """The scale, where the box's left and top lie, and the box's x0 and y1."""
        x0, y0, x1, y1 = self.box
        scale = self.scale
        span = (self.size - 2 * self.margin) / scale
        left = self.margin + ((span - (x1 - x0)) / 2) * scale
        top = self.margin + ((span - (y1 - y0)) / 2) * scale
        return scale, left, top, x0, y1

    def xy(self, point: Point | Flat) -> Flat:
        """Where a point of the plane is in the picture."""
        x, y = point.floats() if isinstance(point, Point) else point
        scale, left, top, x0, y1 = self._frame
        return left + (x - x0) * scale, top + (y1 - y) * scale

    def polygon(self, points: Sequence[Point | Flat], **style: str | float) -> None:
        """Add a polygon."""
        corners = " ".join(f"{x:.2f},{y:.2f}" for x, y in map(self.xy, points))
        self.parts.append(f'<polygon points="{corners}" {_style(style)}/>')

    def segment(self, a: Point | Flat, b: Point | Flat, **style: str | float) -> None:
        """Add a segment."""
        (ax, ay), (bx, by) = self.xy(a), self.xy(b)
        self.parts.append(
            f'<line x1="{ax:.2f}" y1="{ay:.2f}" x2="{bx:.2f}" y2="{by:.2f}" '
            f"{_style(style)}/>"
        )

    def dot(self, point: Point, radius: float, **style: str | float) -> None:
        """Add a dot."""
        x, y = self.xy(point)
        self.parts.append(
            f'<circle cx="{x:.2f}" cy="{y:.2f}" r="{radius}" {_style(style)}/>'
        )

    def label(self, point: Point | Flat, text: str, **style: str | float) -> None:
        """Add a label centered on a point."""
        x, y = self.xy(point)
        self.parts.append(
            f'<text x="{x:.2f}" y="{y:.2f}" text-anchor="middle" '
            f'dominant-baseline="central" {_style(style)}>{escape(text)}</text>'
        )

    def line(self, line: Line, **style: str | float) -> None:
        """Add a line across the whole view."""
        start, end = across(line, self.box)
        (ax, ay), (bx, by) = self.xy(start), self.xy(end)
        self.parts.append(
            f'<line x1="{ax:.2f}" y1="{ay:.2f}" x2="{bx:.2f}" y2="{by:.2f}" '
            f"{_style(style)}/>"
        )

    def rulers(self, unit: Number) -> None:
        """Add ticks along the bottom and the left, every eighth of ``unit``."""
        step = float(unit) / 8
        x0, y0, x1, y1 = self.box
        bottom = self.xy((x0, y0))[1] + 6
        left = self.xy((x0, y0))[0] - 6
        for k in range(ceil(x0 / step - 1e-9), floor(x1 / step + 1e-9) + 1):
            x, _ = self.xy((k * step, y0))
            long = k % 2 == 0
            self._tick(x, bottom, x, bottom + (7 if long else 4))
            if long:
                self._text(x, bottom + 16, str(unit * Fraction(k, 8)))
        for k in range(ceil(y0 / step - 1e-9), floor(y1 / step + 1e-9) + 1):
            _, y = self.xy((x0, k * step))
            long = k % 2 == 0
            self._tick(left, y, left - (7 if long else 4), y)
            if long:
                self._text(left - 10, y, str(unit * Fraction(k, 8)), "end")

    def _tick(self, x1: float, y1: float, x2: float, y2: float) -> None:
        self.parts.append(
            f'<line x1="{x1:.2f}" y1="{y1:.2f}" x2="{x2:.2f}" y2="{y2:.2f}" '
            'stroke="#777" stroke-width="1"/>'
        )

    def _text(self, x: float, y: float, text: str, anchor: str = "middle") -> None:
        self.parts.append(
            f'<text x="{x:.2f}" y="{y:.2f}" text-anchor="{anchor}" '
            f'dominant-baseline="central" font-size="10" fill="#666">{text}</text>'
        )

    def svg(self, title: str) -> str:
        """The view as an SVG element, with a title above it."""
        return (
            f"<figure><figcaption>{escape(title)}</figcaption>"
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{self.size}" '
            f'height="{self.size}" viewBox="0 0 {self.size} {self.size}">'
            + "".join(self.parts)
            + "</svg></figure>"
        )


def across(
    line: Line, box: tuple[float, float, float, float]
) -> tuple[tuple[float, float], tuple[float, float]]:
    """Two points of a line on either side of a box, far enough to cross all of it."""
    x0, y0, x1, y1 = box
    nx, ny = float(line.normal.x), float(line.normal.y)
    norm = hypot(nx, ny)
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    shift = (nx * cx + ny * cy - float(line.offset)) / norm / norm
    px, py = cx - nx * shift, cy - ny * shift
    reach = 2 * max(x1 - x0, y1 - y0, 1e-9)
    dx, dy = -ny / norm * reach, nx / norm * reach
    return (px - dx, py - dy), (px + dx, py + dy)


def stacking(state: State) -> Sequence[int]:
    """The faces in an order to paint them, from the bottom up.

    A stacking with a cycle paints the faces with fewer faces below them first.
    """
    if (found := state.bottom_up) is not None:
        return found[0]
    below = [0] * len(state.faces)
    for upper, _ in state.above:
        below[upper] += 1
    return sorted(range(len(state.faces)), key=lambda f: below[f])


GAP: Final = 0.0015
"""The height between layers in pictures and the viewer, as a share of the unit."""


@dataclass(frozen=True, slots=True)
class Layers:
    """How a picture parts the layers of paper, in gaps between layers.

    Faces that lie in one plane in space stack in it, each as high as the faces under
    it need. A stack that shaping turned out of the plane then shifts so that its
    crease meets the face it turned from, at that face's height.
    """

    heights: tuple[int, ...]
    """How many layers lie under each face, in its own stack."""
    shifts: tuple[Vector, ...]
    """How far each face's stack shifts in space."""


def layers(paper: Shaped, *, tight: bool = False) -> Layers:
    """How a picture parts the layers of a paper, flat or shaped.

    Paper in one plane stacks every face on every face below it, unless ``tight``: then
    each face sits only as high as the faces it lies over need.
    """
    state = paper.state
    form = paper.form or Form.flat(len(state.faces))
    stacks = _stacks(form)
    found = state.bottom_up
    if found is not None and len(set(stacks)) == 1 and not tight:
        # Paper in one plane stacks every face on every face below it.
        height = list(found[1])
    else:
        below: dict[int, set[int]] = {f: set() for f in range(len(state.faces))}
        for upper, lower in state.above:
            if stacks[upper] == stacks[lower]:
                below[upper].add(lower)
        height = [0] * len(state.faces)
        for face in stacking(state):
            height[face] = max((height[g] + 1 for g in below[face]), default=0)
    lifts = [
        tuple(height[f] * c for c in rotate(form.motions[f], (0.0, 0.0, 1.0)))
        for f in range(len(state.faces))
    ]
    turned = [(f, g) for f, g in state.creases.values() if stacks[f] != stacks[g]]
    shift = {max(set(stacks), key=stacks.count): (0.0, 0.0, 0.0)}
    while True:
        found = [
            (a, b)
            for f, g in turned
            for a, b in ((f, g), (g, f))
            if stacks[a] in shift and stacks[b] not in shift
        ]
        if not found:
            break
        a, b = found[0]
        base = shift[stacks[a]]
        shift[stacks[b]] = tuple(
            s + la - lb for s, la, lb in zip(base, lifts[a], lifts[b], strict=True)
        )
    still = (0.0, 0.0, 0.0)
    return Layers(
        tuple(height),
        tuple(_vector(shift.get(stacks[f], still)) for f in range(len(state.faces))),
    )


def _stacks(form: Form) -> list[int]:
    """Which stack each face lies in: faces that one rigid motion puts in space."""
    kinds: list[Motion] = []
    stacks = []
    for motion in form.motions:
        kind = next((k for k, m in enumerate(kinds) if same(m, motion)), len(kinds))
        if kind == len(kinds):
            kinds.append(motion)
        stacks.append(kind)
    return stacks


def _vector(values: Sequence[float]) -> Vector:
    return values[0], values[1], values[2]


def pattern_view(
    state: State, paper_box: tuple[Number, Number], marked: Iterable[int] = ()
) -> Canvas:
    """The crease pattern: mountains red, valleys blue, flat creases gray."""
    width, height = paper_box
    canvas = Canvas((0.0, 0.0, float(width), float(height)))
    for polygon in state.patterns:
        canvas.polygon(polygon, fill="#fbfaf7", stroke="none")
    styles = {
        "M": {"stroke": MOUNTAIN, "stroke-width": 1.6, "stroke-dasharray": "7 3 1.5 3"},
        "V": {"stroke": VALLEY, "stroke-width": 1.6, "stroke-dasharray": "6 3"},
        "F": {"stroke": FLAT, "stroke-width": 0.8},
        "B": {"stroke": INK, "stroke-width": 1.6},
    }
    for edge in sorted(state.edges, key=lambda e: len(state.edges[e])):
        kind = state.assignment(edge)
        canvas.segment(state.vertices[edge[0]], state.vertices[edge[1]], **styles[kind])
    _mark_faces(canvas, state.patterns, marked)
    canvas.rulers(max(width, height))
    return canvas


def folded_view(
    state: State,
    look: Look,
    box: tuple[float, float, float, float],
    marked: Iterable[int] = (),
    size: int = 440,
) -> Canvas:
    """The folded paper seen from above, each face in the color of its side up."""
    canvas = Canvas(box, size, 36)
    front, back, unit = look.front, look.back, look.unit
    for face in stacking(state):
        canvas.polygon(
            state.folded_floats[face],
            fill=back if state.flips(face) else front,
            stroke=INK,
            **{"stroke-width": 0.9, "stroke-linejoin": "round"},
        )
    _mark_faces(canvas, state.folded_floats, marked)
    canvas.rulers(unit)
    return canvas


def xray_view(
    state: State,
    box: tuple[float, float, float, float],
    unit: Number,
    marked: Iterable[int] = (),
) -> Canvas:
    """Every layer at once: the more layers at a point, the darker it is."""
    canvas = Canvas(box)
    for polygon in state.folded_floats:
        canvas.polygon(
            polygon,
            fill=INK,
            stroke=INK,
            **{"fill-opacity": 0.14, "stroke-opacity": 0.55, "stroke-width": 0.8},
        )
    _mark_faces(canvas, state.folded_floats, marked)
    canvas.rulers(unit)
    return canvas


def _unit(vector: Vector) -> Vector:
    length = sqrt(sum(c * c for c in vector))
    return vector[0] / length, vector[1] / length, vector[2] / length


def _dot(a: Vector, b: Vector) -> float:
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


_TILT: Final = (0.35, -0.75, 0.75)
"""Where the 3D viewer's eye first sits, from the paper's center: in front, above."""
_STANDING: Final = (0.35, -0.75, 0.3)
"""Where the eye sits for paper that stands up: in front, about 20° above the table."""
_RISING: Final = 0.25
"""How tall paper stands, against its reach on the table, for the eye to sit low."""
_TINY: Final = 1e-9
"""How small a length or area of a picture of shaped paper counts as none."""


@dataclass(frozen=True, slots=True)
class Eye:
    """Where an eye sees shaped paper from, far away: the direction toward it."""

    toward: Vector

    @classmethod
    def facing(cls, paper: Shaped) -> Eye:
        """The eye in front of paper and above it, square to its longest reach.

        The eye sits as the 3D viewer's first does for flat paper, turned about the
        upright axis so that the paper's longest reach on the table runs across it.
        Paper that stands up, as a finished model does, shows from lower down, as a
        photograph shows it.
        """
        state, form = paper.state, paper.form or Form.flat(len(paper.state.faces))
        points = [
            form.point(f, p) for f, polygon in enumerate(state.folded) for p in polygon
        ]
        mean = [sum(p[k] for p in points) / len(points) for k in range(2)]
        xx, yy, xy = (
            sum((p[i] - mean[i]) * (p[j] - mean[j]) for p in points)
            for i, j in ((0, 0), (1, 1), (0, 1))
        )
        spin = atan2(2 * xy, xx - yy) / 2
        if abs(xy) < _TINY and abs(xx - yy) < _TINY:
            spin = 0.0
        span = [max(p[k] for p in points) - min(p[k] for p in points) for k in range(3)]
        standing = span[2] >= _RISING * hypot(span[0], span[1])
        x, y, z = _STANDING if standing else _TILT
        c, s = cos(spin), sin(spin)
        return cls(_unit((c * x - s * y, s * x + c * y, z)))

    @property
    def right(self) -> Vector:
        """The direction to the right in the picture."""
        return _unit((-self.toward[1], self.toward[0], 0.0))

    @property
    def up(self) -> Vector:
        """The direction up in the picture."""
        (a, b, c), (d, e, f) = self.toward, self.right
        return b * f - c * e, c * d - a * f, a * e - b * d

    def project(self, point: Vector) -> Flat:
        """Where a point of space lies in the picture."""
        return _dot(point, self.right), _dot(point, self.up)

    def depth(self, normal: Vector, origin: Vector, point: Flat) -> float | None:
        """How near the eye a plane lies behind a point of the picture.

        The plane runs through ``origin``, square to ``normal``. Seen edge on, it has
        no one depth there, and the depth is None.
        """
        facing = _dot(normal, self.toward)
        if abs(facing) < _TINY:
            return None
        right, up = self.right, self.up
        on = tuple(point[0] * r + point[1] * u for r, u in zip(right, up, strict=True))
        shift = (origin[0] - on[0], origin[1] - on[1], origin[2] - on[2])
        return _dot(normal, shift) / facing


@dataclass(frozen=True, slots=True)
class Look:
    """How to draw a paper: its sides' colors, the length its rulers divide, the eye."""

    front: str
    back: str
    unit: Number
    eye: Eye = Eye(_unit(_TILT))
    """Where an eye sees shaped paper from."""


@dataclass(frozen=True, slots=True)
class _Solid:
    """A face of shaped paper in space, and its outline in the picture."""

    corners: tuple[Vector, ...]
    outline: tuple[Flat, ...]
    normal: Vector
    """The direction its front looks toward."""


def _solids(paper: Shaped, look: Look) -> list[_Solid]:
    """Each face in space, its layers parted as a picture parts them."""
    state = paper.state
    form = paper.form or Form.flat(len(state.faces))
    gap = float(look.unit) * GAP
    parted = layers(paper)
    solids = []
    for face, (height, shift) in enumerate(
        zip(parted.heights, parted.shifts, strict=True)
    ):
        motion = form.motions[face]
        corners = tuple(
            tuple(
                c + s * gap
                for c, s in zip(
                    apply(motion, (*p.floats(), height * gap)), shift, strict=True
                )
            )
            for p in state.folded[face]
        )
        normal = form.normal(face, flipped=state.flips(face))
        points = tuple(_vector(c) for c in corners)
        solids.append(_Solid(points, tuple(map(look.eye.project, points)), normal))
    return solids


def shaped_box(*papers: Shaped, look: Look) -> tuple[float, float, float, float]:
    """A box around the pictures of every shaped paper given."""
    points = [p for paper in papers for s in _solids(paper, look) for p in s.outline]
    xs, ys = [x for x, _ in points], [y for _, y in points]
    return min(xs), min(ys), max(xs), max(ys)


def shaped_view(
    paper: Shaped,
    look: Look,
    box: tuple[float, float, float, float],
    marked: Iterable[int] = (),
    size: int = 440,
) -> Canvas:
    """Shaped paper in space, each face in the color of the side it shows the eye."""
    canvas = Canvas(box, size, 36)
    solids = _solids(paper, look)
    for face in _far_to_near(solids, look.eye):
        solid = solids[face]
        facing = _dot(solid.normal, look.eye.toward)
        canvas.polygon(
            solid.outline,
            fill=look.front if facing > 0 else look.back,
            stroke=INK,
            **{"stroke-width": 0.9, "stroke-linejoin": "round"},
        )
        canvas.polygon(
            solid.outline,
            fill="#000",
            stroke="none",
            **{"fill-opacity": f"{0.3 * (1 - abs(facing)):.3f}"},
        )
    _mark_faces(canvas, [s.outline for s in solids], marked)
    return canvas


def axis(turn: Turn, reach: float, eye: Eye) -> tuple[Flat, Flat]:
    """The axis of a turn in a picture of shaped paper, ``reach`` each way."""
    ends = (
        tuple(
            c + sense * reach * d
            for c, d in zip(turn.point, turn.direction, strict=True)
        )
        for sense in (-1, 1)
    )
    first, last = (eye.project((e[0], e[1], e[2])) for e in ends)
    return first, last


def _far_to_near(solids: Sequence[_Solid], eye: Eye) -> list[int]:
    """An order to paint faces in: each after every face it hides part of."""
    behind: dict[int, set[int]] = {f: set() for f in range(len(solids))}
    for f, g in _pairs(solids):
        point = _common(solids[f].outline, solids[g].outline)
        if point is None:
            continue
        first, second = (
            eye.depth(solids[h].normal, solids[h].corners[0], point) for h in (f, g)
        )
        if first is not None and second is not None and first != second:
            nearer, farther = (f, g) if first > second else (g, f)
            behind[nearer].add(farther)
    try:
        return list(TopologicalSorter(behind).static_order())
    except CycleError:
        return sorted(
            behind,
            key=lambda f: (
                sum(_dot(c, eye.toward) for c in solids[f].corners)
                / len(solids[f].corners)
            ),
        )


def _pairs(solids: Sequence[_Solid]) -> Iterable[tuple[int, int]]:
    """The pairs of faces whose outlines' boxes overlap."""
    boxes = [
        (
            min(x for x, _ in s.outline),
            min(y for _, y in s.outline),
            max(x for x, _ in s.outline),
            max(y for _, y in s.outline),
        )
        for s in solids
    ]
    for f, a in enumerate(boxes):
        for g in range(f + 1, len(boxes)):
            b = boxes[g]
            if a[0] < b[2] and b[0] < a[2] and a[1] < b[3] and b[1] < a[3]:
                yield f, g


def _common(first: Sequence[Flat], second: Sequence[Flat]) -> Flat | None:
    """A point inside both convex outlines, or None where they barely touch."""
    clipped = list(first)
    turning = 1 if _area(second) > 0 else -1
    for a, b in zip(second, (*second[1:], second[0]), strict=True):
        kept: list[Flat] = []
        for p, q in zip(clipped, (*clipped[1:], clipped[0]), strict=True):
            sp, sq = turning * _cross(a, b, p), turning * _cross(a, b, q)
            if sp >= 0:
                kept.append(p)
            if (sp >= 0) != (sq >= 0):
                share = sp / (sp - sq)
                kept.append(
                    (p[0] + share * (q[0] - p[0]), p[1] + share * (q[1] - p[1]))
                )
        clipped = kept
        if not clipped:
            return None
    if abs(_area(clipped)) < _TINY:
        return None
    return (
        sum(x for x, _ in clipped) / len(clipped),
        sum(y for _, y in clipped) / len(clipped),
    )


def _cross(a: Flat, b: Flat, p: Flat) -> float:
    return (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])


def _area(polygon: Sequence[Flat]) -> float:
    return (
        sum(
            p[0] * q[1] - q[0] * p[1]
            for p, q in zip(polygon, (*polygon[1:], polygon[0]), strict=True)
        )
        / 2
    )


def folded_box(*states: State) -> tuple[float, float, float, float]:
    """A box around the folded paper of every state given, as floats for drawing."""
    points = [p for state in states for poly in state.folded_floats for p in poly]
    xs, ys = [x for x, _ in points], [y for _, y in points]
    return min(xs), min(ys), max(xs), max(ys)


def _mark_faces(
    canvas: Canvas, polygons: Sequence[Sequence[Point | Flat]], marked: Iterable[int]
) -> None:
    for face in marked:
        polygon = polygons[face]
        canvas.polygon(polygon, fill="none", stroke=MARK, **{"stroke-width": 2.2})
        points = [p.floats() if isinstance(p, Point) else p for p in polygon]
        canvas.label(
            (
                sum(x for x, _ in points) / len(points),
                sum(y for _, y in points) / len(points),
            ),
            str(face),
            fill=MARK,
            **{"font-size": 12, "font-weight": 700},
        )


def _style(style: dict[str, str | float]) -> str:
    return " ".join(f'{key}="{value}"' for key, value in style.items())
