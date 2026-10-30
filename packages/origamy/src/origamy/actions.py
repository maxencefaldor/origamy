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

from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass
from enum import StrEnum
from functools import cmp_to_key, partial
from itertools import permutations
from math import atan2, degrees

from origamy.geometry import (
    IDENTITY,
    Isometry,
    Line,
    Point,
    area,
    bounds,
    crosses,
    inside,
)
from origamy.layers import BudgetError, Constraint, NoStackingError, Problem, solve
from origamy.rules import RefusalError
from origamy.state import Edge, Face, Pair, State, sides_of


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
    turn(cut, line, [moved], _hinged(hinges, sense), up=up)
    return Outcome(cut, frozenset())


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
    face lies where the creases between it and ``fixed`` put it.
    """
    target = {
        edge: changes.get(edge, state.assignment(edge))
        for edge, faces in state.edges.items()
        if len(faces) == 2  # noqa: PLR2004 - a crease has two sides
    }
    _check_vertices(state, target)
    places = _places(state, target, fixed)
    faces = tuple(
        Face(f.corners, place) for f, place in zip(state.faces, places, strict=True)
    )
    moved = frozenset(
        f
        for f, (old, new) in enumerate(zip(state.faces, faces, strict=True))
        if old.place != new.place
    )
    collapsed = State(state.vertices, faces)
    required = {
        (upper, lower): f"face {upper} stays above face {lower}"
        for upper, lower in state.above
        if upper not in moved and lower not in moved
    }
    for edge, kind in target.items():
        if kind in "MV":
            f, g = state.edges[edge]
            front, back = (g, f) if collapsed.flips(f) else (f, g)
            # A valley folds the front inward: the face showing its back lies above.
            upper, lower = (back, front) if kind == "V" else (front, back)
            name = "valley" if kind == "V" else "mountain"
            required[upper, lower] = (
                f"the {name} between faces {f} and {g} puts face {upper} above"
            )
    change = [
        new.place @ old.place.inverse()
        for old, new in zip(state.faces, faces, strict=True)
    ]
    above = _stack(collapsed, required, _least_motion(state, target, change), state)
    return Outcome(State(collapsed.vertices, collapsed.faces, above), moved)


def _least_motion(
    state: State, target: Mapping[Edge, str], change: Sequence[Isometry]
) -> list[Pair]:
    """The orders that move the paper least through itself, for the solver to try first.

    Each face that turns about a fold lands beside the face across it. Faces that
    overlapped keep their order, reversed where they moved together and the move turned
    them over.
    """
    turns = [
        (turned, partner)
        for edge, kind in target.items()
        if kind in "MV"
        for turned, partner in permutations(state.edges[edge])
        if change[turned] != change[partner]
    ]
    kept = [
        (lower, upper)
        if change[upper] == change[lower] and change[upper].flips
        else (upper, lower)
        for upper, lower in state.above
        if change[upper] != IDENTITY or change[lower] != IDENTITY
    ]
    return _beside(state, turns, change) + kept


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


def along(state: State, crease: Line | tuple[Point, Point]) -> list[Edge]:
    """The creases inside the paper along a line or segment of the crease pattern."""
    if isinstance(crease, Line):
        line, ends = crease, None
    else:
        line, ends = Line.through(*crease), crease
    found = []
    for edge, faces in state.edges.items():
        p, q = (state.vertices[v] for v in edge)
        if len(faces) != 2 or not (line.contains(p) and line.contains(q)):  # noqa: PLR2004
            continue
        if ends is None or (_within(ends, p) and _within(ends, q)):
            found.append(edge)
    return found


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
    areas = [area(polygon) for polygon in state.patterns]
    return max(range(len(areas)), key=lambda f: (areas[f], -f))


def _within(ends: tuple[Point, Point], point: Point) -> bool:
    start, end = ends
    return (point - start).dot(point - end).sign() <= 0


def _around(state: State) -> dict[int, list[Edge]]:
    """Each vertex inside the paper, with its creases in counterclockwise order."""
    border = {v for edge, faces in state.edges.items() if len(faces) == 1 for v in edge}
    creases: dict[int, list[Edge]] = {}
    for edge, faces in state.edges.items():
        for vertex in edge:
            if vertex not in border and len(faces) == 2:  # noqa: PLR2004 - a crease
                creases.setdefault(vertex, []).append(edge)
    for vertex, edges in creases.items():
        edges.sort(key=cmp_to_key(partial(_compare, state, vertex)))
    return creases


def _compare(state: State, vertex: int, first: Edge, second: Edge) -> int:
    """Order two creases at a vertex by their direction, counterclockwise from east."""
    center = state.vertices[vertex]
    a, b = _away(state, first, vertex) - center, _away(state, second, vertex) - center
    if (half := _half(a) - _half(b)) != 0:
        return half
    return -a.cross(b).sign()


def _away(state: State, edge: Edge, vertex: int) -> Point:
    return state.vertices[edge[0] if edge[1] == vertex else edge[1]]


def _half(vector: Point) -> int:
    return 0 if vector.y.sign() > 0 or (not vector.y and vector.x.sign() > 0) else 1


def _check_vertices(state: State, target: Mapping[Edge, str]) -> None:
    """Refuse an assignment that no flat fold meets at some vertex."""
    for vertex, edges in _around(state).items():
        folds = [edge for edge in edges if target[edge] in "MV"]
        if not folds:
            continue
        point = state.vertices[vertex]
        mountains = sum(target[edge] == "M" for edge in folds)
        valleys = len(folds) - mountains
        if abs(mountains - valleys) != 2:  # noqa: PLR2004 - Maekawa's theorem
            msg = f"{mountains} mountains and {valleys} valleys meet at {point}"
            raise RefusalError(msg, rule="mountain-valley-count")
        turn = IDENTITY
        for edge in folds:
            turn = (
                Isometry.reflection(Line.through(*(state.vertices[v] for v in edge)))
                @ turn
            )
        if turn != IDENTITY:
            odd, even = _alternate_sums(state, vertex, folds)
            msg = (
                f"the alternate angles between the folds at {point} sum to "
                f"{odd:.4g}° and {even:.4g}°"
            )
            raise RefusalError(msg, rule="unbalanced-angles")


def _alternate_sums(
    state: State, vertex: int, folds: list[Edge]
) -> tuple[float, float]:
    center = state.vertices[vertex].floats()
    angles = []
    for edge in folds:
        x, y = _away(state, edge, vertex).floats()
        angles.append(degrees(atan2(y - center[1], x - center[0])) % 360)
    gaps = [(angles[(i + 1) % len(angles)] - a) % 360 for i, a in enumerate(angles)]
    return sum(gaps[0::2]), sum(gaps[1::2])


def _places(state: State, target: Mapping[Edge, str], fixed: int) -> list[Isometry]:
    """Where each face lies, from the creases between it and the fixed face."""
    places: dict[int, Isometry] = {fixed: state.faces[fixed].place}
    queue = [fixed]
    while queue:
        face = queue.pop()
        for edge in sides_of(state.faces[face].corners):
            neighbors = state.edges[edge]
            if len(neighbors) != 2:  # noqa: PLR2004 - a crease has two sides
                continue
            other = neighbors[0] if neighbors[1] == face else neighbors[1]
            if other in places:
                continue
            place = places[face]
            if target[edge] in "MV":
                crease = Line.through(*(state.vertices[v] for v in edge))
                place = place @ Isometry.reflection(crease)
            places[other] = place
            queue.append(other)
    return [places[f] for f in range(len(state.faces))]


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
        raise RefusalError(msg, rule="no-hinge", faces=tuple(sorted(moved)), state=cut)
    flat = sorted({m for m, _, edge in hinges if not cut.is_folded(edge)})
    if unfolding and flat:
        msg = f"faces {_ids(flat)} lie flat along {line}"
        raise RefusalError(msg, rule="not-folded", faces=tuple(flat), state=cut)
    return cut, moved, hinges


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
            for pair in cut.edges.values()
            for m, s in permutations(pair, 2)
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
        return solve(Problem(state), required, prefer)
    except NoStackingError as error:
        facts = [_constraint(c) for c in dict.fromkeys(error.constraints)]
        facts += [required[pair] for pair in error.fixed if pair in required]
        message = "no stacking keeps paper from passing through paper"
        if facts:
            message += ": " + "; ".join(facts)
        raise RefusalError(
            message, rule="layer-crossing", faces=_faces(error), state=before
        ) from None
    except BudgetError as error:
        message = str(error)
        raise RefusalError(message, rule="layer-order-unknown", state=before) from None


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
