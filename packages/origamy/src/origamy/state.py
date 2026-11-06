# Copyright 2026 Maxence Faldor
"""The state of the paper: its crease pattern, where faces lie, and how they stack.

The crease pattern divides the unfolded paper into convex faces. Each face lies where
its isometry maps it, and wherever two faces overlap, the state says which is above.
Everything else, such as whether a crease is folded or flat, follows from these three.
"""

from collections import defaultdict
from collections.abc import Iterable, Mapping
from dataclasses import dataclass, field
from functools import cached_property
from itertools import combinations

from origamy.geometry import (
    IDENTITY,
    Isometry,
    Line,
    Point,
    Polygon,
    center,
    clip,
    covers,
    overlaps,
    sides,
)

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


@dataclass(frozen=True)
class State:
    """The paper at one step: crease pattern, faces' places and their stacking."""

    vertices: tuple[Point, ...]
    """Where each vertex is on the crease pattern."""
    faces: tuple[Face, ...]
    above: frozenset[Pair] = field(default=frozenset())
    """For each pair of faces that overlap once folded, which one is above."""

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
            f.place.polygon(polygon)
            for f, polygon in zip(self.faces, self.patterns, strict=True)
        )

    @cached_property
    def edges(self) -> Mapping[Edge, tuple[int, ...]]:
        """The faces on each edge: two along a crease, one along the paper's edge."""
        sides: dict[Edge, list[int]] = defaultdict(list)
        for index, face in enumerate(self.faces):
            for edge in sides_of(face.corners):
                sides[edge].append(index)
        return {edge: tuple(faces) for edge, faces in sides.items()}

    @cached_property
    def creases(self) -> Mapping[Edge, tuple[int, int]]:
        """The edges inside the paper, each with its two faces."""
        return {
            edge: (faces[0], faces[1])
            for edge, faces in self.edges.items()
            if len(faces) > 1
        }

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
    def footprints(self) -> tuple[Polygon, ...]:
        """Each region a face covers once folded, faces stacked exactly sharing one."""
        found: dict[frozenset[Point], Polygon] = {}
        for polygon in self.folded:
            found.setdefault(frozenset(polygon), polygon)
        return tuple(found.values())

    @cached_property
    def footprint(self) -> tuple[int, ...]:
        """The index of each face's footprint."""
        index = {frozenset(polygon): k for k, polygon in enumerate(self.footprints)}
        return tuple(index[frozenset(polygon)] for polygon in self.folded)

    @cached_property
    def stacked(self) -> tuple[tuple[int, ...], ...]:
        """The faces on each footprint, in order."""
        faces: list[list[int]] = [[] for _ in self.footprints]
        for f, k in enumerate(self.footprint):
            faces[k].append(f)
        return tuple(tuple(group) for group in faces)

    @cached_property
    def meeting(self) -> frozenset[Pair]:
        """Every pair of footprints that share area, the lesser index first."""
        shapes = self.footprints
        return frozenset(
            (a, b)
            for a in range(len(shapes))
            for b in range(a + 1, len(shapes))
            if overlaps(shapes[a], shapes[b])
        )

    @cached_property
    def overlapping(self) -> frozenset[Pair]:
        """Every pair of faces that share area once folded, the lesser id first.

        Faces on one footprint all overlap, and faces on two footprints overlap when
        the footprints do.
        """
        stacked = self.stacked
        pairs = {pair for group in stacked for pair in combinations(group, 2)}
        for a, b in self.meeting:
            pairs.update((min(f, g), max(f, g)) for f in stacked[a] for g in stacked[b])
        return frozenset(pairs)

    @cached_property
    def thickest(self) -> tuple[int, Point]:
        """The most layers of paper at one point once folded, and a point inside them.

        Cutting a footprint along the sides of every footprint that overlaps it leaves
        cells that each of those footprints covers whole or misses. A cell on the outer
        side of any side of a footprint misses it, so each cut lowers a bound on the
        cell's layers. Depth first, a cell whose bound reaches no further than the most
        found so far is dropped uncut.
        """
        shapes, stacked = self.footprints, self.stacked
        near: dict[int, set[int]] = defaultdict(set)
        for a, b in self.meeting:
            near[a].add(b)
            near[b].add(a)
        depth = [len(group) for group in stacked]
        most, where = depth[0], center(shapes[0])

        def bound(a: int) -> int:
            return depth[a] + sum(depth[b] for b in near[a])

        for a in sorted(range(len(shapes)), key=lambda a: -bound(a)):
            if bound(a) <= most:
                break
            # Each line cut holds the footprints with a side along it: their bit, the
            # side their inside lies on, and their layers.
            users: dict[Line, list[tuple[int, int, int]]] = defaultdict(list)
            for k, b in enumerate(sorted(near[a])):
                for line, side in _inner_sides(shapes[b]).items():
                    users[line].append((1 << k, side, depth[b]))
            if deeper := _deepest(shapes[a], users, bound(a), most):
                most, where = deeper
        return most, where

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


def _inner_sides(polygon: Polygon) -> dict[Line, int]:
    """The line of each side of a convex polygon, and the side its inside lies on."""
    inner = {}
    for p, q in sides(polygon):
        line = Line.through(p, q)
        inner[line] = next(side for corner in polygon if (side := line.side(corner)))
    return inner


def _deepest(
    shape: Polygon,
    users: Mapping[Line, list[tuple[int, int, int]]],
    upper: int,
    most: int,
) -> tuple[int, Point] | None:
    """The deepest cell of a footprint, if deeper than ``most``, and a point inside it.

    Each cell carries how many lines it has been cut along, the footprints it misses
    as bits, and its bound on layers.
    """
    lines = sorted(users, key=str)
    found: tuple[int, Point] | None = None
    cells = [(shape, 0, 0, upper)]
    while cells:
        cell, cuts, missed, upper = cells.pop()
        if upper <= most:
            continue
        if cuts == len(lines):
            most = upper
            found = most, center(cell)
            continue
        line = lines[cuts]
        sides_of = {line.side(p) for p in cell}
        parts = (
            [(clip(cell, line, side), side) for side in (1, -1)]
            if 1 in sides_of and -1 in sides_of
            else [(cell, 1 if 1 in sides_of else -1)]
        )
        split = []
        for part, side in parts:
            lost = [
                (bit, layers)
                for bit, inside, layers in users[line]
                if inside != side and not missed & bit
            ]
            split.append(
                (
                    part,
                    cuts + 1,
                    missed | sum(bit for bit, _ in lost),
                    upper - sum(layers for _, layers in lost),
                )
            )
        cells += sorted(split, key=lambda entry: entry[3])
    return found
