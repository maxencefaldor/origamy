# Copyright 2026 Maxence Faldor
"""What the paper can do, each action a function from one state to the next.

The actions are fold, unfold, precrease, collapse and turn over. A fold turns some paper
over a line, onto the paper beside it. The paper it moves is
the paper grabbed at a point, with everything joined to it on that side of the line.
Every face it moves is reflected across the line, and the layer solver finds how the
faces then stack. The paper that stays keeps its order, and the paper that moves
reverses its own. Where the two meet, a valley fold lands the moved paper on top and a
mountain fold lands it underneath. An action the paper cannot take raises
`RefusalError`, with the facts of why.
"""

from collections.abc import Callable, Iterable, Mapping, Sequence
from dataclasses import dataclass
from enum import StrEnum
from functools import cmp_to_key, partial
from itertools import permutations
from typing import NoReturn

from origamy import kernel
from origamy.geometry import (
    IDENTITY,
    Isometry,
    Line,
    Point,
    bounds,
    crosses,
    inside,
)
from origamy.layers import (
    BUDGET,
    BudgetError,
    Constraint,
    NoStackingError,
    solve,
    unstacked,
)
from origamy.rules import RefusalError
from origamy.space import Form, Turn
from origamy.state import Edge, Face, Pair, State, sides_of
from origamy_kernel.data import ConstraintData


class Sense(StrEnum):
    """Which way a fold turns the paper, as seen from above."""

    VALLEY = "valley"
    """Toward the viewer: the moved paper lands on top."""
    MOUNTAIN = "mountain"
    """Away from the viewer: the moved paper lands underneath."""


type Hinge = tuple[int, int, Edge]
"""A crease on a fold line: the face that moves, the face that stays, and their edge."""


@dataclass(frozen=True, slots=True)
class Outcome:
    """The state after an action, and the faces it moved, as ids in that state."""

    state: State
    moved: frozenset[int]
    form: Form | None = None
    """The paper's form in space, once shaping turns it out of the plane."""
    turn: Turn | None = None
    """The turn in space that a shape step, or turning shaped paper over, makes."""
    path: tuple[Form, ...] = ()
    """The forms a flex passes through on its way, in order."""
    through: State | None = None
    """The folded paper a precrease passes through before it unfolds."""
    settle: tuple[tuple[Edge, float], ...] = ()
    """The fold angles, in radians, valleys positive, that settling turns creases
    toward; the simulator alone shows the paper they bend."""
    ease: float = 0.0
    """The angle, in radians, by which settling opens every fold it does not name."""


def fold(
    state: State, line: Line, flap: Point, layers: int | None, sense: Sense
) -> Outcome:
    """Fold the paper at ``flap`` over ``line``.

    ``layers`` is how many layers at ``flap`` to take, counted from the top; None takes
    all the paper on the flap's side of the line. Everything joined to the taken layers
    on that side moves with them.
    """
    cut, moved, hinges = grab(state, line, flap, layers, unfolding=False)
    up = sense is Sense.VALLEY
    check_hinges(cut, line, hinges, up=up)
    return turn(cut, line, [moved], _hinged(hinges, sense), up=up)


def unfold(state: State, line: Line, flap: Point, layers: int | None) -> Outcome:
    """Unfold the paper at ``flap`` back across ``line``, where it is folded.

    ``layers`` counts as in `fold`. The moved paper turns back the way it came.
    """
    cut, moved, hinges = grab(state, line, flap, layers, unfolding=True)
    up = any(cut.is_above(m, s) for m, s, _ in hinges)
    return turn(cut, line, [moved], {}, up=up)


def precrease(
    state: State, line: Line, flap: Point, layers: int | None, sense: Sense
) -> Outcome:
    """Fold the paper at ``flap`` over ``line`` and unfold it, leaving creases.

    The paper must be able to take the fold, so a refused fold refuses the precrease.
    """
    cut, moved, hinges = grab(state, line, flap, layers, unfolding=False)
    up = sense is Sense.VALLEY
    check_hinges(cut, line, hinges, up=up)
    folded = turn(cut, line, [moved], _hinged(hinges, sense), up=up)
    return Outcome(cut, frozenset(), through=folded.state)


def _hinged(hinges: Iterable[Hinge], sense: Sense) -> dict[Pair, str]:
    """The order a fold sets at each hinge, with its reason."""
    where = "above" if sense is Sense.VALLEY else "below"
    return {
        ((m, s) if sense is Sense.VALLEY else (s, m)): (
            f"the {sense} fold puts moved face {m} {where} face {s}"
        )
        for m, s, _ in hinges
    }


def collapse(state: State, changes: Mapping[Edge, str], fixed: int) -> Outcome:
    """Set creases to ``M``, ``V`` or ``F`` at once, and fold the paper to match.

    Assignments read as on the crease pattern, from the paper's front. Every other
    crease keeps its assignment. Face ``fixed`` stays where it lies, and each other
    face lies where the creases between it and ``fixed`` put it. The faces that move
    land as near their old order as a valid stacking allows.
    """
    kinds = list(state.kernel.assignments())
    places = state.crease_index
    for edge, kind in changes.items():
        if (place := places.get(edge)) is not None:
            kinds[place] = kind
    outcome, hold, moved, facts, found, conflict = state.kernel.collapse(
        "".join(kinds), fixed, BUDGET
    )
    if hold is None:
        target = dict(zip(state.creases, kinds, strict=True))
        _refuse_collapse(state, target, outcome, (facts, found, conflict))
    after = State.held(hold, vertices=vars(state).get("vertices"))
    return Outcome(after, frozenset(moved))


def _refuse_collapse(
    state: State,
    target: Mapping[Edge, str],
    outcome: str,
    refusal: tuple[
        tuple[int, int, int, float, float] | None, list[ConstraintData], list[Pair]
    ],
) -> NoReturn:
    """Refuse a collapse the kernel refused, with the facts it found.

    Raises:
        RefusalError: Always.
    """
    facts, found, conflict = refusal
    if facts is not None:
        _refuse_vertex(state, outcome, facts)
    error = unstacked(state, outcome, found, conflict)
    _refuse_stacking(state, error, partial(_collapse_reason, state, target))


def _refuse_vertex(
    state: State, rule: str, facts: tuple[int, int, int, float, float]
) -> NoReturn:
    """Refuse an assignment that no flat fold meets at a vertex.

    Raises:
        RefusalError: Always.
    """
    vertex, mountains, valleys, odd, even = facts
    point = state.vertices[vertex]
    faces = tuple(f for f, face in enumerate(state.faces) if vertex in face.corners)
    if rule == "count":
        msg = f"{mountains} mountains and {valleys} valleys meet at {point}"
        raise RefusalError(msg, rule="mountain-valley-count", faces=faces, state=state)
    msg = (
        f"the alternate angles between the folds at {point} sum to "
        f"{odd:.4g}° and {even:.4g}°"
    )
    raise RefusalError(msg, rule="unbalanced-angles", faces=faces, state=state)


def _collapse_reason(state: State, target: Mapping[Edge, str], pair: Pair) -> str:
    """Why a collapse requires an order.

    The fold between the two faces sets it, or else the two faces keep it.
    """
    upper, lower = pair
    for edge, faces in state.creases.items():
        if set(faces) == {upper, lower} and target[edge] in "MV":
            name = "valley" if target[edge] == "V" else "mountain"
            f, g = faces
            return f"the {name} between faces {f} and {g} puts face {upper} above"
    return f"face {upper} stays above face {lower}"


def _beside(
    state: State, turns: Iterable[tuple[int, int]], change: Sequence[Isometry]
) -> list[Pair]:
    """The orders that land each turned face beside the face across its fold.

    With the paper that moved as that face did, and that the turned face did not overlap
    before, the turned face takes that face's order.
    """
    overlapped = {frozenset(pair) for pair in state.above}
    return [
        (turned, other) if upper == partner else (other, turned)
        for turned, partner in turns
        for upper, lower in state.above
        if partner in (upper, lower)
        for other in [lower if upper == partner else upper]
        if change[other] == change[partner]
        and frozenset((turned, other)) not in overlapped
    ]


def along(
    state: State, creases: Iterable[Line | tuple[Point, Point]]
) -> list[list[Edge]]:
    """The creases inside the paper along each line or segment of the crease pattern."""
    queries = [
        (
            kernel.point(crease.point),
            kernel.point(crease.point + crease.direction),
            False,
        )
        if isinstance(crease, Line)
        else (kernel.point(crease[0]), kernel.point(crease[1]), True)
        for crease in creases
    ]
    edges = list(state.creases)
    return [[edges[index] for index in found] for found in state.kernel.along(queries)]


def flat_with(state: State, faces: Iterable[int]) -> set[int]:
    """The faces given, and every face they continue into across flat creases."""
    found = set(faces)
    queue = list(found)
    while queue:
        face = queue.pop()
        for edge in sides_of(state.faces[face].corners):
            for other in state.edges[edge]:
                if other not in found and (
                    state.faces[other].place == state.faces[face].place
                ):
                    found.add(other)
                    queue.append(other)
    return found


def largest(state: State) -> int:
    """The face of largest area, the first of them when several tie."""
    return state.kernel.largest()


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


def grab(
    state: State, line: Line, flap: Point, layers: int | None, *, unfolding: bool
) -> tuple[State, frozenset[int], list[Hinge]]:
    """Split the paper to move from the paper that stays, and find the hinges.

    Returns the state with the moving faces cut along the line, the moving faces as
    ids in it, and each hinge as the moving face, the face that stays, and their edge.
    """
    side, stack = flap_stack(state, line, flap)
    if layers is None and unfolding:
        seeds = _folded_over(state, line, side)
        if not seeds:
            msg = f"no paper is folded along {line} on the side of {flap}"
            raise RefusalError(msg, rule="not-folded", state=state)
    elif layers is None:
        seeds = [
            f
            for f, polygon in enumerate(state.folded)
            if any(line.side(p) == side for p in polygon)
        ]
    elif len(stack) < layers:
        msg = f"{len(stack)} layers lie at the flap point {flap}, not {layers}"
        raise RefusalError(msg, rule="too-few-layers", faces=tuple(stack), state=state)
    else:
        seeds = stack[:layers]
    moving = _joined(state, line, side, seeds)
    crossed = sorted(f for f in moving if crosses(state.folded[f], line))
    if unfolding and crossed:
        msg = f"the line {line} runs through the inside of faces {_ids(crossed)}"
        raise RefusalError(msg, rule="not-folded", faces=tuple(crossed), state=state)
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
        for edge, faces in cut.creases.items()
        for m, s in permutations(faces)
        if m in moved and s not in moved
    ]
    if not hinges:
        msg = f"the paper to move meets no other paper along {line}"
        raise RefusalError(msg, rule="no-hinge", faces=tuple(sorted(moved)), state=cut)
    flat = sorted({m for m, _, edge in hinges if not cut.is_folded(edge)})
    if unfolding and flat:
        msg = f"faces {_ids(flat)} lie flat along {line}"
        raise RefusalError(msg, rule="not-folded", faces=tuple(flat), state=cut)
    return cut, moved, hinges


def flap_stack(state: State, line: Line, flap: Point) -> tuple[int, list[int]]:
    """The side of a line a flap point names, and the faces at it from the top down.

    A flap point on the line, or on no face's inside, names no paper to move.
    """
    side = line.side(flap)
    if not side:
        msg = f"the flap point {flap} lies on the line {line}"
        raise RefusalError(msg, rule="flap-on-line", state=state)
    stack = stack_at(state, flap)
    if not stack:
        msg = f"the flap point {flap} lies on no face's inside"
        raise RefusalError(msg, rule="flap-off-paper", state=state)
    return side, stack


def check_hinges(
    cut: State, line: Line, hinges: Iterable[Hinge], *, up: bool | None
) -> None:
    """Refuse to turn paper about a crease already folded the way it would turn.

    Turning paper up unfolds a fold whose moving face lies above the face that stays,
    and turning it down unfolds one whose moving face lies below. With ``up`` None, as
    for a reverse fold, the move unfolds no fold.
    """
    wrong = sorted(
        {
            m
            for m, s, edge in hinges
            if cut.is_folded(edge) and (up is None or cut.is_above(s, m) == up)
        }
    )
    if wrong:
        where = {True: " under", False: " over", None: ""}[up]
        msg = f"faces {_ids(wrong)} already lie folded{where} along {line}"
        raise RefusalError(msg, rule="already-folded", faces=tuple(wrong), state=cut)


def _folded_over(state: State, line: Line, side: int) -> list[int]:
    """The upper face of each fold along a line, with its faces on one side of it."""
    seeds = []
    for (u, v), (f, g) in state.creases.items():
        if not state.is_folded((u, v)):
            continue
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
        for u, v in sides_of(state.faces[face].corners):
            a, b = state.folded_point(face, u), state.folded_point(face, v)
            if max(line.side(a) * side, line.side(b) * side) <= 0:
                continue  # the edge lies on the line, so the paper bends there
            for neighbor in state.edges[u, v]:
                if neighbor not in moving:
                    moving.add(neighbor)
                    queue.append(neighbor)
    return moving


def turn(
    cut: State,
    line: Line,
    blocks: Sequence[frozenset[int]],
    required: dict[Pair, str],
    *,
    up: bool | None,
) -> Outcome:
    """Reflect the moved faces across a line and stack the result.

    Each block of moved faces turns over as one, so its order reverses; blocks keep
    their order with each other, and the paper that stays keeps its own. The moved paper
    lands on top when ``up`` is true and underneath when it is false. When ``up`` is
    None, each moved face lands beside the face it is hinged to.
    """
    moved = frozenset[int]().union(*blocks)
    block = {face: index for index, faces in enumerate(blocks) for face in faces}
    reflection = Isometry.reflection(line)
    faces = tuple(
        Face(f.corners, reflection @ f.place) if index in moved else f
        for index, f in enumerate(cut.faces)
    )
    for upper, lower in cut.above:
        if upper not in moved and lower not in moved:
            required[upper, lower] = f"face {upper} stays above face {lower}"
        elif block.get(upper) == block.get(lower):
            required[lower, upper] = (
                f"moved faces {lower} and {upper} turn over together"
            )
        elif upper in moved and lower in moved:
            required[upper, lower] = (
                f"moved face {upper} stays above moved face {lower}"
            )
    turned = State(cut.vertices, faces)
    if up is None:
        change = [reflection if f in moved else IDENTITY for f in range(len(faces))]
        hinges = [
            (m, s)
            for pair in cut.creases.values()
            for m, s in permutations(pair)
            if m in moved and s not in moved
        ]
        prefer = _beside(cut, hinges, change)
    else:
        prefer = [
            (f, g) if (f in moved) == up else (g, f)
            for f, g in sorted(turned.overlapping)
            if (f in moved) != (g in moved)
        ]
    above = _stack(turned, required, prefer, cut)
    return Outcome(State(turned.vertices, turned.faces, above), moved)


def _stack(
    state: State, required: Mapping[Pair, str], prefer: list[Pair], before: State
) -> frozenset[Pair]:
    """Solve the stacking, or refuse with the constraints and orders that conflict.

    ``before`` is the paper just before the move, with the same faces.
    """
    try:
        return solve(state, required, prefer, BUDGET)
    except (NoStackingError, BudgetError) as error:
        _refuse_stacking(before, error, required.__getitem__)


def _refuse_stacking(
    state: State, error: NoStackingError | BudgetError, reason: Callable[[Pair], str]
) -> NoReturn:
    """Refuse a move no stacking allows, naming what conflicts.

    The facts name the constraints and the orders, and ``reason`` says why each order
    was required.

    Raises:
        RefusalError: Always.
    """
    if isinstance(error, BudgetError):
        raise RefusalError(
            str(error), rule="layer-order-unknown", state=state
        ) from None
    facts = [_constraint(c) for c in dict.fromkeys(error.constraints)]
    facts += [reason(pair) for pair in error.fixed]
    message = "no stacking keeps paper from passing through paper"
    if facts:
        message += ": " + "; ".join(facts)
    raise RefusalError(
        message, rule="layer-crossing", faces=_faces(error), state=state
    ) from None


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
