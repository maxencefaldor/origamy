# Copyright 2026 Maxence Faldor
"""What the paper can do: fold, unfold and turn over, each from one state to the next.

A fold turns some paper over a line, onto the paper beside it. The paper it moves is
the paper grabbed at a point, with everything joined to it on that side of the line.
Every face it moves is reflected across the line, and the layer solver finds how the
faces then stack. The paper that stays keeps its order, and the paper that moves
reverses its own. Where the two meet, a valley fold lands the moved paper on top and a
mountain fold lands it underneath. An action the paper cannot take raises
`RefusalError`, with the facts of why.
"""

from collections.abc import Iterable
from dataclasses import dataclass
from enum import StrEnum
from functools import cmp_to_key

from origamy.geometry import Isometry, Line, Point, bounds, crosses, inside
from origamy.layers import BudgetError, Constraint, NoStackingError, Problem, solve
from origamy.rules import RefusalError
from origamy.state import Edge, Face, Pair, State


class Sense(StrEnum):
    """Which way a fold turns the paper, as seen from above."""

    VALLEY = "valley"
    """Toward the viewer: the moved paper lands on top."""
    MOUNTAIN = "mountain"
    """Away from the viewer: the moved paper lands underneath."""


@dataclass(frozen=True, slots=True)
class Outcome:
    """The state after an action, and the faces it moved, as ids in that state."""

    state: State
    moved: frozenset[int]


def fold(
    state: State, line: Line, flap: Point, layers: int | None, sense: Sense
) -> Outcome:
    """Fold the paper at ``flap`` over ``line``.

    ``layers`` is how many layers at ``flap`` to take, counted from the top; None takes
    all the paper on the flap's side of the line. Everything joined to the taken layers
    on that side moves with them.
    """
    cut, moved, hinges = _grab(state, line, flap, layers, unfolding=False)
    up = sense is Sense.VALLEY
    fixed = [(m, s) if up else (s, m) for m, s, _ in hinges]
    return _turn(cut, line, moved, fixed, up=up)


def unfold(state: State, line: Line, flap: Point, layers: int | None) -> Outcome:
    """Unfold the paper at ``flap`` back across ``line``, where it is folded.

    ``layers`` counts as in `fold`. The moved paper turns back the way it came.
    """
    cut, moved, hinges = _grab(state, line, flap, layers, unfolding=True)
    up = any(cut.is_above(m, s) for m, s, _ in hinges)
    return _turn(cut, line, moved, [], up=up)


def precrease(
    state: State, line: Line, flap: Point, layers: int | None, sense: Sense
) -> Outcome:
    """Fold the paper at ``flap`` over ``line`` and unfold it, leaving creases.

    The paper must be able to take the fold, so a refused fold refuses the precrease.
    """
    cut, moved, hinges = _grab(state, line, flap, layers, unfolding=False)
    up = sense is Sense.VALLEY
    _turn(cut, line, moved, [(m, s) if up else (s, m) for m, s, _ in hinges], up=up)
    return Outcome(cut, frozenset())


def turn_over(state: State) -> Outcome:
    """Turn the paper over from left to right, about the middle of what it covers."""
    left, _, right, _ = bounds(p for polygon in state.folded for p in polygon)
    flip = Isometry.reflection(
        Line.through(((left + right) / 2, 0), ((left + right) / 2, 1))
    )
    faces = tuple(Face(f.corners, flip @ f.place) for f in state.faces)
    above = frozenset((lower, upper) for upper, lower in state.above)
    return Outcome(State(state.vertices, faces, above), frozenset(range(len(faces))))


def stack_at(state: State, point: Point) -> list[int]:
    """The faces that cover a point once folded, from the top down."""
    under = [f for f, polygon in enumerate(state.folded) if inside(polygon, point)]

    def order(f: int, g: int) -> int:
        return -1 if state.is_above(f, g) else 1

    return sorted(under, key=cmp_to_key(order))


def _grab(
    state: State, line: Line, flap: Point, layers: int | None, *, unfolding: bool
) -> tuple[State, frozenset[int], list[tuple[int, int, Edge]]]:
    """Split the paper to move from the paper that stays, and find the hinges."""
    side = line.side(flap)
    if not side:
        msg = f"the flap point {flap} lies on the fold line {line}"
        raise RefusalError(msg, rule="flap-on-line")
    stack = stack_at(state, flap)
    if not stack:
        msg = f"the flap point {flap} lies on no face's inside"
        raise RefusalError(msg, rule="flap-off-paper")
    if layers is None and unfolding:
        seeds = _folded_over(state, line, side)
        if not seeds:
            msg = f"no paper is folded along {line} on the side of {flap}"
            raise RefusalError(msg, rule="not-folded")
    elif layers is None:
        seeds = [
            f
            for f, polygon in enumerate(state.folded)
            if any(line.side(p) == side for p in polygon)
        ]
    elif len(stack) < layers:
        msg = f"{len(stack)} layers lie at the flap point {flap}, not {layers}"
        raise RefusalError(msg, rule="too-few-layers", faces=tuple(stack))
    else:
        seeds = stack[:layers]
    moving = _joined(state, line, side, seeds)
    crossed = sorted(f for f in moving if crosses(state.folded[f], line))
    if unfolding and crossed:
        msg = f"the line {line} runs through the inside of faces {_ids(crossed)}"
        raise RefusalError(msg, rule="not-folded", faces=tuple(crossed))
    cuts = {f: state.faces[f].place.inverse().line(line) for f in crossed}
    cut, pieces = state.cut(cuts)
    moved = frozenset(
        piece
        for f in moving
        for piece in pieces[f]
        if any(line.side(p) == side for p in cut.folded[piece])
    )
    hinges = [
        (m, s, edge)
        for edge, faces in cut.edges.items()
        if len(faces) == 2  # noqa: PLR2004 - a crease has two sides
        and (faces[0] in moved) != (faces[1] in moved)
        for m, s in [faces if faces[0] in moved else faces[::-1]]
    ]
    if not hinges:
        msg = f"the paper to move meets no other paper along {line}"
        raise RefusalError(msg, rule="no-hinge", faces=tuple(sorted(moved)))
    wrong = sorted({m for m, _, edge in hinges if cut.is_folded(edge) != unfolding})
    if wrong:
        rule = "not-folded" if unfolding else "already-folded"
        state_of = "flat" if unfolding else "already folded"
        msg = f"faces {_ids(wrong)} lie {state_of} along {line}"
        raise RefusalError(msg, rule=rule, faces=tuple(wrong))
    return cut, moved, hinges


def _folded_over(state: State, line: Line, side: int) -> list[int]:
    """The upper face of each fold along a line, with its faces on one side of it."""
    seeds = []
    for (u, v), faces in state.edges.items():
        if not state.is_folded((u, v)):
            continue
        f, g = faces
        ends = state.folded_point(f, u), state.folded_point(f, v)
        if (
            line.contains(ends[0])
            and line.contains(ends[1])
            and any(line.side(p) == side for p in state.folded[f])
        ):
            seeds.append(f if state.is_above(f, g) else g)
    return seeds


def _joined(state: State, line: Line, side: int, seeds: Iterable[int]) -> set[int]:
    """The seeds and every face joined to them on one side of a line."""
    moving = set(seeds)
    queue = list(moving)
    while queue:
        face = queue.pop()
        corners = state.faces[face].corners
        for u, v in zip(corners, corners[1:] + corners[:1], strict=True):
            a, b = state.folded_point(face, u), state.folded_point(face, v)
            if max(line.side(a) * side, line.side(b) * side) <= 0:
                continue  # the edge lies on the line, so the paper bends there
            for neighbor in state.edges[min(u, v), max(u, v)]:
                if neighbor not in moving:
                    moving.add(neighbor)
                    queue.append(neighbor)
    return moving


def _turn(
    cut: State, line: Line, moved: frozenset[int], fixed: list[Pair], *, up: bool
) -> Outcome:
    """Reflect the moved faces across a line and stack the result."""
    reflection = Isometry.reflection(line)
    faces = tuple(
        Face(f.corners, reflection @ f.place) if index in moved else f
        for index, f in enumerate(cut.faces)
    )
    for upper, lower in cut.above:
        if (upper in moved) == (lower in moved):
            fixed.append((lower, upper) if upper in moved else (upper, lower))
    turned = State(cut.vertices, faces)
    problem = Problem(turned)
    prefer = [
        (f, g) if (f in moved) == up else (g, f)
        for f, g in problem.pairs
        if (f in moved) != (g in moved)
    ]
    try:
        above = solve(problem, fixed, prefer)
    except NoStackingError as error:
        message = _conflict(error, moved)
        raise RefusalError(
            message, rule="layer-crossing", faces=_faces(error)
        ) from None
    except BudgetError as error:
        raise RefusalError(str(error), rule="layer-order-unknown") from None
    return Outcome(State(turned.vertices, turned.faces, above), moved)


def _conflict(error: NoStackingError, moved: frozenset[int]) -> str:
    """State which constraints and which required orders leave no stacking."""
    facts = [_constraint(c) for c in dict.fromkeys(error.constraints)]
    for upper, lower in error.fixed:
        if upper in moved and lower not in moved:
            facts.append(f"moved face {upper} lands above face {lower}")
        elif lower in moved and upper not in moved:
            facts.append(f"moved face {lower} lands below face {upper}")
        else:
            facts.append(f"face {upper} stays above face {lower}")
    if not facts:
        return "no stacking of the faces keeps paper from passing through paper"
    return "no stacking keeps paper from passing through paper: " + "; ".join(facts)


def _constraint(constraint: Constraint) -> str:
    faces = constraint.faces
    where = (
        f" along {constraint.where[0]} to {constraint.where[1]}"
        if constraint.where
        else ""
    )
    match constraint.kind:
        case "taco-taco":
            return (
                f"the folds of faces {faces[0]}, {faces[1]} and of faces {faces[2]}, "
                f"{faces[3]} meet{where} (taco-taco)"
            )
        case "taco-tortilla":
            return (
                f"face {faces[-1]} runs on across the fold of faces {faces[0]} and "
                f"{faces[1]}{where} (taco-tortilla)"
            )
        case "tortilla-tortilla":
            return (
                f"faces {_ids(faces)} lie flat across one crease{where} "
                "(tortilla-tortilla)"
            )
        case _:
            return f"faces {_ids(faces)} share area (transitivity)"


def _faces(error: NoStackingError) -> tuple[int, ...]:
    faces = {f for c in error.constraints for f in c.faces}
    faces |= {f for pair in error.fixed for f in pair}
    return tuple(sorted(faces))


def _ids(faces: Iterable[int]) -> str:
    return ", ".join(str(f) for f in faces)
