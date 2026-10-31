# Copyright 2026 Maxence Faldor
"""Shaping: turning folded paper out of the plane, as the last steps of a model.

Folding presses every crease flat; shaping then opens some of them part of the way,
as hands spread a crane's wings or level a plane's. A shape step turns the paper at a
flap point about a line, by an angle. The step finds the paper to turn as a fold does,
in the paper as it lies flat, and turns it as one rigid piece about its hinge.
"""

from fractions import Fraction
from math import acos, degrees, radians, sqrt

from origamy import actions
from origamy.actions import Outcome, grab
from origamy.geometry import Line, Point, bounds, center, covers
from origamy.rules import RefusalError
from origamy.space import Form, Motion, Shaped, Turn, apply, compose, rotate, same
from origamy.state import Edge, State

_LEAN = 1e-9
"""How far a face must lie off another's plane to count as off it."""


def shape(
    paper: Shaped, line: Line, flap: Point, layers: int | None, angle: Fraction
) -> Outcome:
    """Turn the paper at ``flap`` about ``line`` by ``angle`` degrees, up when positive.

    Up is toward the viewer, as the paper lies flat. The paper turned is the paper a
    fold would move, and it turns as one piece about its hinge, the crease along the
    line between it and the paper that stays.
    """
    cut, moved, hinges = grab(paper.state, line, flap, layers, unfolding=False)
    form = paper.form or Form.flat(len(paper.state.faces))
    motions = tuple(form.motions[f] for f in _parents(paper.state, cut))
    hinge = motions[hinges[0][1]]
    bent = sorted({s for _, s, _ in hinges if not same(motions[s], hinge)})
    if bent:
        msg = f"earlier shaping turned hinge faces {_ids(bent)} apart"
        raise RefusalError(msg, rule="hinge-bent", faces=tuple(bent), state=cut)
    up = angle > 0
    blocked = sorted(
        {
            m
            for upper, lower in cut.above
            for m, s in ((upper, lower), (lower, upper))
            if m in moved
            and s not in moved
            and same(motions[m], motions[s])
            and (m == lower) == up
        }
    )
    if blocked:
        where = "under" if up else "over"
        msg = f"faces {_ids(blocked)} lie {where} paper that stays"
        raise RefusalError(
            msg, rule="turns-through-paper", faces=tuple(blocked), state=cut
        )
    turn = _turn(line, line.side(flap), hinge, radians(angle))
    rotation = turn.motion()
    turned = tuple(
        compose(motion, rotation) if face in moved else motion
        for face, motion in enumerate(motions)
    )
    return Outcome(cut, moved, Form(turned), turn)


def turn_over(paper: Shaped) -> Outcome:
    """Turn the paper over, keeping its form in space.

    Turning over is half a turn about an upright line in the paper's plane, which
    reflects the flat paper across it. Each face's motion from its new flat place is
    its old motion seen through that half turn.
    """
    outcome = actions.turn_over(paper.state)
    if paper.form is None:
        return outcome
    left, _, right, _ = bounds(p for polygon in paper.state.folded for p in polygon)
    turn = Turn((float((left + right) / 2), 0.0, 0.0), (0.0, 1.0, 0.0), radians(180))
    half = turn.motion()
    form = Form(tuple(compose(compose(half, m), half) for m in paper.form.motions))
    return Outcome(outcome.state, outcome.moved, form, turn)


def fold_angles(state: State, form: Form) -> dict[Edge, float]:
    """Each crease's fold angle in degrees, as FOLD writes it: valleys positive."""
    angles = {}
    for edge, (f, g) in state.creases.items():
        front = form.normal(f, flipped=state.flips(f))
        other = form.normal(g, flipped=state.flips(g))
        cosine = sum(a * b for a, b in zip(front, other, strict=True))
        magnitude = degrees(acos(max(-1.0, min(1.0, cosine))))
        on_edge = form.point(f, state.folded_point(f, edge[0]))
        beyond = form.point(g, center(state.folded[g]))
        lean = sum(n * (b - e) for n, b, e in zip(front, beyond, on_edge, strict=True))
        if abs(lean) > _LEAN:
            sense = 1.0 if lean > 0 else -1.0
        else:
            sense = {"V": 1.0, "M": -1.0}.get(state.assignment(edge), 0.0)
        angles[edge] = sense * magnitude
    return angles


def _turn(line: Line, side: int, hinge: Motion, angle: float) -> Turn:
    """The turn about a line that lifts its side ``side`` up by ``angle`` radians."""
    nx, ny = float(line.normal.x) * side, float(line.normal.y) * side
    length = sqrt(nx * nx + ny * ny)
    axis = rotate(hinge, (ny / length, -nx / length, 0.0))
    point = line.point
    return Turn(apply(hinge, (float(point.x), float(point.y), 0.0)), axis, angle)


def _parents(before: State, after: State) -> list[int]:
    """The face of ``before`` that holds each face of ``after``."""
    return [
        next(f for f, pattern in enumerate(before.patterns) if covers(pattern, inner))
        for inner in (center(polygon) for polygon in after.patterns)
    ]


def _ids(faces: list[int]) -> str:
    return ", ".join(str(f) for f in faces)
