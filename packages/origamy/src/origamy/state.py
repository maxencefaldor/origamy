# Copyright 2026 Maxence Faldor
"""The state of the paper: its crease pattern, where faces lie, and how they stack.

The crease pattern divides the unfolded paper into convex faces. Each face lies where
its isometry maps it, and wherever two faces overlap, the state says which is above.
Everything else, such as whether a crease is folded or flat, follows from these three.
"""

from collections import defaultdict
from collections.abc import Iterable, Mapping
from dataclasses import dataclass
from functools import cached_property
from typing import override

import origamy_kernel
from origamy import kernel
from origamy.geometry import (
    IDENTITY,
    Isometry,
    Line,
    Point,
    Polygon,
    center,
    covers,
    overlaps,
)
from origamy_kernel.data import PointData

type Pair = tuple[int, int]
"""Two faces, the upper first: ``(upper, lower)``."""

type Edge = tuple[int, int]
"""A side of a face, as its two vertices, the lesser id first."""


@dataclass(frozen=True, slots=True)
class Face:
    """A convex region of the paper bounded by creases and the paper's edges."""

    corners: tuple[int, ...]
    """Vertex ids, counterclockwise on the crease pattern."""
    place: Isometry
    """Where the face lies: the map from the crease pattern to the folded paper."""


class State:
    """The paper at one step: crease pattern, faces' places and their stacking.

    A state the kernel made, as creasing and collapsing make them, keeps its numbers
    in the kernel. Python reads its faces and stacking back the first time a question
    asks for them.
    """

    def __init__(
        self,
        vertices: tuple[Point, ...],
        faces: tuple[Face, ...],
        above: frozenset[Pair] = frozenset(),
    ) -> None:
        """The state of these vertices, faces and stacking."""
        vars(self).update(vertices=vertices, faces=faces, above=above)

    @classmethod
    def held(
        cls,
        hold: origamy_kernel.State,
        vertices: tuple[Point, ...] | None = None,
        faces: tuple[Face, ...] | None = None,
    ) -> State:
        """A state the kernel holds, with what Python already knows of it."""
        state = cls.__new__(cls)
        known = {"kernel": hold, "vertices": vertices, "faces": faces}
        vars(state).update(
            {name: value for name, value in known.items() if value is not None}
        )
        return state

    @cached_property
    def vertices(self) -> tuple[Point, ...]:
        """Where each vertex is on the crease pattern."""
        return tuple(kernel.from_point(p) for p in self.kernel.vertices())

    @cached_property
    def faces(self) -> tuple[Face, ...]:
        """Each face's corners on the crease pattern, and its place."""
        corners, places = self.kernel.corners(), self.kernel.places()
        return tuple(
            Face(tuple(c), kernel.from_place(p))
            for c, p in zip(corners, places, strict=True)
        )

    @cached_property
    def above(self) -> frozenset[Pair]:
        """For each pair of faces that overlap once folded, which one is above."""
        return frozenset(self.stacking)

    @cached_property
    def stacking(self) -> tuple[Pair, ...]:
        """The pairs of `above` in order, as data and files list them."""
        if "kernel" in vars(self) or "above" not in vars(self):
            return tuple(self.kernel.stacking())
        return tuple(sorted(self.above))

    @cached_property
    def bottom_up(self) -> tuple[int, ...] | None:
        """The faces from the bottom up, each after every face below it.

        Faces with no face below come first, by number, and each face follows as soon
        as every face below it has come. None comes back when the stacking holds a
        cycle.
        """
        order = self.kernel.bottom_up()
        return None if order is None else tuple(order)

    @override
    def __eq__(self, other: object) -> bool:
        """Whether two states have the same vertices, faces and stacking."""
        if not isinstance(other, State):
            return NotImplemented
        return (self.vertices, self.faces, self.above) == (
            other.vertices,
            other.faces,
            other.above,
        )

    @override
    def __hash__(self) -> int:
        """A hash of the vertices, faces and stacking."""
        return hash((self.vertices, self.faces, self.above))

    @override
    def __repr__(self) -> str:
        """The state's vertices, faces and stacking, as code would build it."""
        return f"State({self.vertices!r}, {self.faces!r}, {self.above!r})"

    @classmethod
    def sheet(cls, corners: Iterable[Point]) -> State:
        """Unfolded paper of a convex shape, its front facing up."""
        vertices = tuple(corners)
        return cls(vertices, (Face(tuple(range(len(vertices))), IDENTITY),))

    @cached_property
    def patterns(self) -> tuple[Polygon, ...]:
        """Each face's corners on the crease pattern."""
        return tuple(tuple(self.vertices[i] for i in f.corners) for f in self.faces)

    @cached_property
    def folded(self) -> tuple[Polygon, ...]:
        """Each face's corners where it lies, counterclockwise as seen from above."""
        return tuple(
            tuple(kernel.from_point(p) for p, _ in face) for face in self._folded
        )

    @cached_property
    def folded_floats(self) -> tuple[tuple[tuple[float, float], ...], ...]:
        """Each face's corners where it lies, as `folded` lists them, in floats."""
        return tuple(tuple(floats for _, floats in face) for face in self._folded)

    @cached_property
    def _folded(self) -> list[list[tuple[PointData, tuple[float, float]]]]:
        """Each face's folded corners as the kernel finds them, exact and in floats."""
        return self.kernel.folded()

    @cached_property
    def edges(self) -> Mapping[Edge, tuple[int, ...]]:
        """The faces on each edge: two along a crease, one along the paper's edge.

        The edges come in the order the faces first meet them, each face's sides in
        turn, as the kernel gives them when it holds the state.
        """
        if "kernel" in vars(self):
            return {edge: tuple(faces) for edge, faces in self.kernel.edges()}
        sides: dict[Edge, list[int]] = defaultdict(list)
        for index, face in enumerate(self.faces):
            for edge in sides_of(face.corners):
                sides[edge].append(index)
        return {edge: tuple(faces) for edge, faces in sides.items()}

    @cached_property
    def creases(self) -> Mapping[Edge, tuple[int, int]]:
        """The edges inside the paper, each with its two faces."""
        if "kernel" in vars(self):
            return dict(self.kernel.creases())
        return {
            edge: (faces[0], faces[1])
            for edge, faces in self.edges.items()
            if len(faces) > 1
        }

    @cached_property
    def crease_index(self) -> Mapping[Edge, int]:
        """Each crease's place among the creases."""
        return {edge: k for k, edge in enumerate(self.creases)}

    @cached_property
    def lines(self) -> Mapping[Line, tuple[Edge, ...]]:
        """The creases along each line of the crease pattern."""
        found: dict[Line, list[Edge]] = defaultdict(list)
        for edge in self.creases:
            found[Line.through(*(self.vertices[v] for v in edge))].append(edge)
        return {line: tuple(edges) for line, edges in found.items()}

    @cached_property
    def border(self) -> frozenset[Edge]:
        """The edges along the paper's own edge."""
        return frozenset(self.edges.keys() - self.creases.keys())

    @cached_property
    def kernel(self) -> origamy_kernel.State:
        """The kernel's hold on the state: its crease pattern and each face's place.

        The kernel folds the faces once, the first time a question needs them.
        """
        return origamy_kernel.State(
            kernel.tower(),
            [kernel.point(p) for p in self.vertices],
            [list(face.corners) for face in self.faces],
            [kernel.place(face.place) for face in self.faces],
            list(self.above),
        )

    @cached_property
    def overlapping(self) -> frozenset[Pair]:
        """Every pair of faces that share area once folded, the lesser id first.

        The kernel overlays the faces' footprints. Two faces overlap exactly when they
        meet in one of the overlay's regions where the most footprints meet.
        """
        return frozenset(self.kernel.overlapping())

    @cached_property
    def thickest(self) -> tuple[int, Point]:
        """The most layers of paper at one point once folded, and a point inside them.

        The point is the middle of the region of the footprints' overlay with the most
        layers, the first such region when several tie.
        """
        most, point = self.kernel.thickest()
        return most, kernel.from_point(point)

    def is_above(self, upper: int, lower: int) -> bool:
        """Whether face ``upper`` lies above face ``lower``, where they overlap."""
        return (upper, lower) in self.above

    def folded_point(self, face: int, vertex: int) -> Point:
        """Where a vertex of a face lies once folded."""
        return self.faces[face].place(self.vertices[vertex])

    def is_folded(self, edge: Edge) -> bool:
        """Whether a crease is folded: its two faces lie on the same side of it."""
        faces = self.creases.get(edge)
        return faces is not None and (
            self.faces[faces[0]].place != self.faces[faces[1]].place
        )

    def parents(self, later: State) -> list[int]:
        """The face of this state that holds each face of a later state.

        Actions only ever cut faces, so each face of a later crease pattern lies inside
        exactly one face of an earlier one; a face no action cut is that face itself.
        """
        same = {pattern: f for f, pattern in enumerate(self.patterns)}
        return [
            same[pattern]
            if pattern in same
            else next(
                f
                for f, outline in enumerate(self.patterns)
                if covers(outline, center(pattern))
            )
            for pattern in later.patterns
        ]

    def flips(self, face: int) -> bool:
        """Whether a face shows the paper's back."""
        return self.faces[face].place.flips

    def assignment(self, edge: Edge) -> str:
        """What an edge is, as FOLD writes it, seen from the paper's front.

        ``M`` and ``V`` are folded mountain and valley creases, ``F`` a crease lying
        flat, and ``B`` the paper's own edge. A fold is a valley when the face under it
        shows the paper's front: the front then faces inward.
        """
        if edge in self.border:
            return "B"
        return self.sense(*self.creases[edge])

    def sense(self, f: int, g: int) -> str:
        """How two faces joined at a crease meet: ``V``, ``M`` or ``F``, as there."""
        if self.faces[f].place == self.faces[g].place:
            return "F"
        lower = g if self.is_above(f, g) else f
        return "M" if self.flips(lower) else "V"

    def cut(self, cuts: Mapping[int, Line]) -> tuple[State, dict[int, tuple[int, ...]]]:
        """Split faces by lines of the crease pattern, without moving any paper.

        Each listed face is split in two by its line, which must cross it. The face
        keeps its id for the part on the line's positive side; the other part takes a
        new id.
        Vertices where a line crosses an edge join every face along that edge. Each part
        keeps its face's stacking with every face it still overlaps.

        Returns the new state and each old face's ids in it.
        """
        vertices = list(self.vertices)
        index = {p: i for i, p in enumerate(vertices)}
        inserted: dict[Edge, set[int]] = defaultdict(set)
        for face, line in cuts.items():
            for u, v in sides_of(self.faces[face].corners):
                p, q = vertices[u], vertices[v]
                if line.side(p) * line.side(q) < 0:
                    point = line.cut(p, q)
                    if point not in index:
                        index[point] = len(vertices)
                        vertices.append(point)
                    inserted[u, v].add(index[point])
        faces = [
            Face(_insert(face.corners, inserted, vertices), face.place)
            for face in self.faces
        ]
        pieces: dict[int, tuple[int, ...]] = {f: (f,) for f in range(len(faces))}
        for face, line in cuts.items():
            corners = faces[face].corners
            sides = [line.side(vertices[c]) for c in corners]
            plus = tuple(c for c, s in zip(corners, sides, strict=True) if s >= 0)
            minus = tuple(c for c, s in zip(corners, sides, strict=True) if s <= 0)
            # A line through a convex face leaves each side's corners in one run, so
            # keeping them in order around the face keeps each part counterclockwise.
            place = faces[face].place
            faces[face] = Face(plus, place)
            faces.append(Face(minus, place))
            pieces[face] = (face, len(faces) - 1)
        state = State(tuple(vertices), tuple(faces))
        above = frozenset(
            (pu, pl)
            for upper, lower in self.above
            for pu in pieces[upper]
            for pl in pieces[lower]
            if overlaps(state.folded[pu], state.folded[pl])
        )
        return State(state.vertices, state.faces, above), pieces


def sides_of(corners: tuple[int, ...]) -> list[Edge]:
    """The edges of a face, in order around it."""
    return [
        (min(u, v), max(u, v))
        for u, v in zip(corners, corners[1:] + corners[:1], strict=True)
    ]


def _insert(
    corners: tuple[int, ...], inserted: Mapping[Edge, set[int]], vertices: list[Point]
) -> tuple[int, ...]:
    result: list[int] = []
    for u, v in zip(corners, corners[1:] + corners[:1], strict=True):
        result.append(u)
        between = inserted.get((min(u, v), max(u, v)))
        if between is not None:
            start, direction = vertices[u], vertices[v] - vertices[u]
            result.extend(
                sorted(between, key=lambda w: direction.dot(vertices[w] - start))
            )
    return tuple(result)
