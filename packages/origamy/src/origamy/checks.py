# Copyright 2026 Maxence Faldor
"""Checks of an action that let it happen but state a fact about it."""

from origamy.geometry import Isometry, Line, Point, covers
from origamy.state import State


def is_referenced(state: State, line: Line) -> bool:
    """Whether the first four Huzita-Justin axioms build a line from the folded paper.

    The references are the folded positions of every vertex and the lines of every edge
    and crease. The line is referenced when it passes through two references, puts one
    reference point onto another, puts one reference line onto another, or is
    perpendicular to a reference line through a reference point.
    """
    points = {
        state.folded_point(f, v)
        for f, face in enumerate(state.faces)
        for v in face.corners
    }
    lines = {
        Line.through(state.folded_point(faces[0], u), state.folded_point(faces[0], v))
        for (u, v), faces in state.edges.items()
    }
    on_line = [p for p in points if line.contains(p)]
    if len(on_line) >= 2:  # noqa: PLR2004 - axiom 1 takes two points
        return True
    reflection = Isometry.reflection(line)
    if any(reflection(p) in points and reflection(p) != p for p in points):
        return True
    if any(
        (image := reflection.line(other)) in lines and image != other for other in lines
    ):
        return True
    return bool(on_line) and any(
        not line.normal.cross(other.direction) for other in lines
    )


def where(state: State, point: Point) -> Point | None:
    """Where a point of the crease pattern lies once folded, or None off the paper."""
    for f, polygon in enumerate(state.patterns):
        if covers(polygon, point):
            return state.faces[f].place(point)
    return None
