# Copyright 2026 Maxence Faldor
"""Named moves of origami, each a precrease and a collapse in one step.

A named move is a shorthand for creases a script could set itself: it computes the
creases and their assignments, then collapses. The checks and the layer solver judge the
result as they judge any collapse.
"""

from dataclasses import dataclass

from origamy.actions import Outcome, collapse, flat_with, grab, stack_at
from origamy.geometry import Line, Point
from origamy.rules import RefusalError
from origamy.state import Edge, State


def reverse(
    state: State, line: Line, flap: Point, layers: int | None, *, outside: bool
) -> Outcome:
    """Reverse fold: turn the tip of a folded flap inside out, across ``line``.

    The tip is the paper at ``flap`` on its side of the line, taken as `fold` takes it.
    Each fold inside the tip reverses. Each new crease along the line takes the
    assignment of the fold it meets there, its spine, so the tip turns between the
    layers; an outside reverse fold takes the opposite assignment, so the tip wraps
    around them.
    """
    cut, tip, hinges = grab(state, line, flap, layers, unfolding=False)
    changes: dict[Edge, str] = {
        edge: _opposite(cut.assignment(edge))
        for edge, faces in cut.edges.items()
        if set(faces) <= tip and cut.is_folded(edge)
    }
    for moving, _, edge in hinges:
        spine = _spine(cut, edge, tip)
        if spine is None:
            corner = cut.vertices[edge[0]]
            msg = f"no fold of face {moving} meets the line {line} at {corner}"
            raise RefusalError(msg, rule="no-spine", faces=(moving,), state=cut)
        kind = cut.assignment(spine)
        changes[edge] = _opposite(kind) if outside else kind
    return collapse(cut, changes, hinges[0][1])


def petal(state: State, line: Line, flap: Point) -> Outcome:
    """Petal fold: lift the top layer at ``flap`` over ``line``, its sides folding in.

    The lifted paper is the top layer at ``flap``, with every face it continues into
    across flat creases. Its creases along the line fold toward the viewer. Where the
    line meets the layer's folded sides, the creases that run from there into the
    lifted region, precreased before, fold the same way, in the layer and in the layer
    joined across each side; the sides between them open flat.
    """
    stack = stack_at(state, flap)
    side = line.side(flap)
    if not stack or not side:
        rule = "flap-off-paper" if not stack else "flap-on-line"
        msg = f"the flap point {flap} lies on no face's inside or on the line {line}"
        raise RefusalError(msg, rule=rule)
    layer = flat_with(state, stack[:1])
    kind = "M" if state.flips(stack[0]) else "V"
    changes = dict.fromkeys(_creases(state, layer, line, along_line=True), kind)
    sides = {
        edge
        for edge, faces in state.edges.items()
        if state.is_folded(edge) and len(set(faces) & layer) == 1
    }
    ends = {v for edge in changes for v in edge} & {v for edge in sides for v in edge}
    lift = _Lift(state, layer, sides, line, side, kind)
    for end in sorted(ends):
        changes |= lift.at(end)
    if "F" not in changes.values():
        msg = f"the layer at {flap} meets {line} at no folded side to open"
        raise RefusalError(msg, rule="no-crease")
    still = next(
        f for f in sorted(layer) if any(line.side(p) == -side for p in state.folded[f])
    )
    return collapse(state, changes, still)


@dataclass(frozen=True, slots=True)
class _Lift:
    """The layer a petal fold lifts, over which side of which line, and which way."""

    state: State
    layer: set[int]
    sides: set[Edge]
    line: Line
    side: int
    kind: str

    def at(self, end: int) -> dict[Edge, str]:
        """The creases the fold sets at one end of its line, on the lifted side."""
        state, line, side = self.state, self.line, self.side
        changes: dict[Edge, str] = {}
        for edge, faces in state.edges.items():
            if end not in edge or not _beyond(state, edge, end, line, side):
                continue
            if edge in self.sides:
                changes[edge] = "F"
                joined = flat_with(state, set(faces) - self.layer)
                changes |= {
                    other: self.kind
                    for other in _creases(state, joined, line, along_line=False)
                    if end in other and _beyond(state, other, end, line, side)
                }
            elif set(faces) <= self.layer and not state.is_folded(edge):
                changes[edge] = self.kind
        return changes


def _creases(
    state: State, faces: set[int], line: Line, *, along_line: bool
) -> list[Edge]:
    """The flat creases between faces of a set, along a line or off it."""
    return [
        edge
        for edge, pair in state.edges.items()
        if len(pair) == 2  # noqa: PLR2004 - a crease has two sides
        and set(pair) <= faces
        and not state.is_folded(edge)
        and _along(state, edge, line) == along_line
    ]


def _along(state: State, edge: Edge, line: Line) -> bool:
    face = state.edges[edge][0]
    return all(line.contains(state.folded_point(face, v)) for v in edge)


def _beyond(state: State, edge: Edge, end: int, line: Line, side: int) -> bool:
    """Whether an edge runs from a vertex on the line into one side of it."""
    face = state.edges[edge][0]
    other = edge[0] if edge[1] == end else edge[1]
    return line.side(state.folded_point(face, other)) == side


def _spine(state: State, hinge: Edge, tip: frozenset[int]) -> Edge | None:
    """A fold outside the tip that ends where a hinge does, in line with a tip fold."""
    found = None
    for vertex in hinge:
        folds = [
            edge
            for edge, faces in state.edges.items()
            if vertex in edge and edge != hinge and state.is_folded(edge)
        ]
        tips = [edge for edge in folds if set(state.edges[edge]) <= tip]
        for edge in folds:
            if set(state.edges[edge]) & tip:
                continue
            if any(_in_line(state, edge, other) for other in tips):
                return edge
            found = found or edge
    return found


def _in_line(state: State, first: Edge, second: Edge) -> bool:
    line = Line.through(*(state.vertices[v] for v in first))
    return all(line.contains(state.vertices[v]) for v in second)


def _opposite(kind: str) -> str:
    return {"M": "V", "V": "M"}[kind]
