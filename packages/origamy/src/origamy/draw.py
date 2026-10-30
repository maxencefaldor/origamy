# Copyright 2026 Maxence Faldor
"""Pictures of the paper as SVG: the crease pattern, the folded paper and an x-ray.

Each view draws in exact coordinates turned into floats only here, with rulers along
two sides so that a reader can take coordinates off the picture.
"""

from collections.abc import Iterable, Sequence
from dataclasses import dataclass, field
from fractions import Fraction
from graphlib import CycleError, TopologicalSorter
from html import escape
from math import ceil, floor

from origamy.field import Number
from origamy.geometry import Line, Point, bounds
from origamy.state import State

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

    def xy(self, point: Point | tuple[float, float]) -> tuple[float, float]:
        """Where a point of the plane is in the picture."""
        x, y = point.floats() if isinstance(point, Point) else point
        x0, y0, x1, y1 = self.box
        span = (self.size - 2 * self.margin) / self.scale
        left = self.margin + ((span - (x1 - x0)) / 2) * self.scale
        top = self.margin + ((span - (y1 - y0)) / 2) * self.scale
        return left + (x - x0) * self.scale, top + (y1 - y) * self.scale

    def polygon(self, points: Sequence[Point], **style: str | float) -> None:
        """Add a polygon."""
        corners = " ".join(f"{x:.2f},{y:.2f}" for x, y in map(self.xy, points))
        self.parts.append(f'<polygon points="{corners}" {_style(style)}/>')

    def segment(self, a: Point, b: Point, **style: str | float) -> None:
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

    def label(
        self, point: Point | tuple[float, float], text: str, **style: str | float
    ) -> None:
        """Add a label centered on a point."""
        x, y = self.xy(point)
        self.parts.append(
            f'<text x="{x:.2f}" y="{y:.2f}" text-anchor="middle" '
            f'dominant-baseline="central" {_style(style)}>{escape(text)}</text>'
        )

    def line(self, line: Line, **style: str | float) -> None:
        """Add a line across the whole view."""
        x0, y0, x1, y1 = self.box
        pad = max(x1 - x0, y1 - y0)
        corners = [
            Point.of((Fraction(x), Fraction(y)))
            for x, y in (
                (x0 - pad, y0 - pad),
                (x1 + pad, y0 - pad),
                (x1 + pad, y1 + pad),
                (x0 - pad, y1 + pad),
            )
        ]
        ends = [
            line.cut(p, q)
            for p, q in zip(corners, corners[1:] + corners[:1], strict=True)
            if line.side(p) * line.side(q) < 0
        ]
        if len(ends) >= 2:  # noqa: PLR2004 - a line crosses a box twice
            self.segment(ends[0], ends[1], **style)

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
                self._text(x, bottom + 16, _fraction(Fraction(k, 8) * unit.a))
        for k in range(ceil(y0 / step - 1e-9), floor(y1 / step + 1e-9) + 1):
            _, y = self.xy((x0, k * step))
            long = k % 2 == 0
            self._tick(left, y, left - (7 if long else 4), y)
            if long:
                self._text(left - 10, y, _fraction(Fraction(k, 8) * unit.a), "end")

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


def stacking(state: State) -> list[int]:
    """The faces in an order to paint them, from the bottom up."""
    below: dict[int, set[int]] = {f: set() for f in range(len(state.faces))}
    for upper, lower in state.above:
        below[upper].add(lower)
    try:
        return list(TopologicalSorter(below).static_order())
    except CycleError:
        return sorted(below, key=lambda f: len(below[f]))


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


@dataclass(frozen=True, slots=True)
class Look:
    """How to draw a paper: its two sides' colors, and the length its rulers divide."""

    front: str
    back: str
    unit: Number


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
            state.folded[face],
            fill=back if state.flips(face) else front,
            stroke=INK,
            **{"stroke-width": 0.9, "stroke-linejoin": "round"},
        )
    _mark_faces(canvas, state.folded, marked)
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
    for polygon in state.folded:
        canvas.polygon(
            polygon,
            fill=INK,
            stroke=INK,
            **{"fill-opacity": 0.14, "stroke-opacity": 0.55, "stroke-width": 0.8},
        )
    _mark_faces(canvas, state.folded, marked)
    canvas.rulers(unit)
    return canvas


def folded_box(*states: State) -> tuple[float, float, float, float]:
    """A box around the folded paper of every state given."""
    x0, y0, x1, y1 = bounds(
        p for state in states for poly in state.folded for p in poly
    )
    return float(x0), float(y0), float(x1), float(y1)


def _mark_faces(
    canvas: Canvas, polygons: Sequence[Sequence[Point]], marked: Iterable[int]
) -> None:
    for face in marked:
        polygon = polygons[face]
        canvas.polygon(polygon, fill="none", stroke=MARK, **{"stroke-width": 2.2})
        center = sum(polygon[1:], polygon[0]) * Number(Fraction(1, len(polygon)))
        canvas.label(
            center, str(face), fill=MARK, **{"font-size": 12, "font-weight": 700}
        )


def _style(style: dict[str, str | float]) -> str:
    return " ".join(f'{key}="{value}"' for key, value in style.items())


def _fraction(value: Fraction) -> str:
    return str(value)
