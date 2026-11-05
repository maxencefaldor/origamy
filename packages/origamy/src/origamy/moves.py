# Copyright 2026 Maxence Faldor
"""Named moves of origami, each one action that sets many creases at once.

A named move is a shorthand for creases a script could set itself. A reverse fold turns
a tip inside out between its layers; petal and squash folds collapse creases made
before. The checks and the layer solver judge the result as they judge any action.
"""

from dataclasses import dataclass

from origamy.actions import (
    Outcome,
    check_hinges,
    collapse,
    flap_stack,
    flat_with,
    grab,
    stack_at,
    turn,
)
from origamy.geometry import Line, Point
from origamy.rules import RefusalError
from origamy.state import Edge, State, sides_of


def reverse(
    state: State, line: Line, flap: Point, layers: int | None, *, outside: bool
) -> Outcome:
    """Reverse fold: turn the tip of a folded flap inside out, across ``line``.

    The tip is the paper at ``flap`` on its side of the line, taken as `fold` takes it.
    The tip opens into an upper and a lower half, joined by its spine; where it could
    open in several places, it opens where it splits the layers at ``flap`` most evenly.
    Each half turns over as one, the upper half staying above the lower. An inside
    reverse fold tucks each half's tip between the halves; an outside one wraps it
    around them.
    """
    cut, tip, hinges = grab(state, line, flap, layers, unfolding=False)
    check_hinges(cut, line, hinges, up=None)
    upper, lower = _halves(cut, tip, flap)
    kind = "outside" if outside else "inside"
    required = {}
    for moving, staying, _ in hinges:
        over = (moving in upper) == outside
        pair = (moving, staying) if over else (staying, moving)
        where = "over" if over else "under"
        required[pair] = (
            f"the {kind} reverse fold turns face {moving} {where} {staying}"
        )
    return turn(cut, line, (upper, lower), required, up=None)


def _halves(
    state: State, tip: frozenset[int], flap: Point
) -> tuple[frozenset[int], frozenset[int]]:
    """Split a tip where it opens: an upper and a lower half, joined only by folds.

    Each half is one piece of paper through its own creases, and lies above the other
    or below it wherever they overlap. The folds between the halves are the tip's spine.
    Of several such splits, the one that splits the layers at ``flap`` most evenly wins.
    """
    layers = _pieces(state, tip)
    index = {face: i for i, piece in enumerate(layers) for face in piece}
    over = [0] * len(layers)
    joined = [0] * len(layers)
    for upper, lower in state.above:
        if upper in tip and lower in tip:
            over[index[lower]] |= 1 << index[upper]
    for f, g in state.creases.values():
        if f in tip and g in tip:
            a, b = index[f], index[g]
            joined[a] |= 1 << b
            joined[b] |= 1 << a
    whole = (1 << len(layers)) - 1
    splits = [
        mask
        for mask in range(1, whole)
        if not any(over[i] & ~mask for i in _bits(mask))
        and _whole(mask, joined)
        and _whole(whole & ~mask, joined)
    ]
    stack = [index[face] for face in stack_at(state, flap) if face in tip]

    def unevenness(mask: int) -> int:
        above = sum(mask >> i & 1 for i in stack)
        return abs(2 * above - len(stack))

    best = sorted(splits, key=unevenness)[:2]
    if not best or (len(best) > 1 and unevenness(best[0]) == unevenness(best[1])):
        rule = "several-spines" if best else "no-spine"
        ways = f"in {len(splits)} equally even ways" if best else "in no way"
        msg = f"the paper to reverse at {flap} opens into two halves {ways}"
        raise RefusalError(msg, rule=rule, faces=tuple(sorted(tip)), state=state)
    top = frozenset(face for i in _bits(best[0]) for face in layers[i])
    return top, tip - top


def _pieces(state: State, faces: frozenset[int]) -> list[frozenset[int]]:
    """The faces split into pieces of paper, each joined through its flat creases."""
    pieces: list[frozenset[int]] = []
    left = set(faces)
    while left:
        queue = [left.pop()]
        piece = set(queue)
        while queue:
            face = queue.pop()
            for edge in sides_of(state.faces[face].corners):
                for other in state.edges[edge]:
                    if other in left and not state.is_folded(edge):
                        left.discard(other)
                        piece.add(other)
                        queue.append(other)
        pieces.append(frozenset(piece))
    return pieces


def _bits(mask: int) -> list[int]:
    return [i for i in range(mask.bit_length()) if mask >> i & 1]


def _whole(mask: int, joined: list[int]) -> bool:
    """Whether the pieces of a mask form one piece of paper through their creases."""
    reached = frontier = mask & -mask
    while frontier:
        grown = 0
        for i in _bits(frontier):
            grown |= joined[i]
        frontier = grown & mask & ~reached
        reached |= frontier
    return reached == mask


def petal(state: State, line: Line, flap: Point) -> Outcome:
    """Petal fold: lift the top layer at ``flap`` over ``line``, its sides folding in.

    The lifted paper is the top layer at ``flap``, with every face it continues into
    across flat creases. Its creases along the line fold toward the viewer. Where the
    line meets the layer's folded sides, the creases that run from there into the
    lifted region, precreased before, fold the same way, in the layer and in the layer
    joined across each side; the sides between them open flat.
    """
    side, stack = flap_stack(state, line, flap)
    layer = flat_with(state, stack[:1])
    kind = "M" if state.flips(stack[0]) else "V"
    along = _flat_creases(state, layer, line, along_line=True)
    if not along:
        msg = f"no flat crease of the layer at {flap} lies along {line}"
        raise RefusalError(
            msg, rule="no-crease", faces=tuple(sorted(layer)), state=state
        )
    changes = dict.fromkeys(along, kind)
    sides = {
        edge
        for edge, faces in state.creases.items()
        if state.is_folded(edge) and len(set(faces) & layer) == 1
    }
    ends = {v for edge in changes for v in edge} & {v for edge in sides for v in edge}
    lift = _Lift(state, layer, sides, line, side, kind)
    for end in sorted(ends):
        changes |= lift.at(end)
    if "F" not in changes.values():
        msg = f"the layer at {flap} meets {line} at no folded side to open"
        raise RefusalError(msg, rule="no-side", faces=tuple(sorted(layer)), state=state)
    still = next(
        f for f in sorted(layer) if any(line.side(p) == -side for p in state.folded[f])
    )
    return collapse(state, changes, still)


def squash(state: State, line: Line, flap: Point) -> Outcome:
    """Squash fold: swing the top flap at ``flap`` over ``line`` and press it flat.

    The flap's top layer is the top face at ``flap`` and every face it continues into
    across flat creases, on the flap's side of the line. Its creases along the line
    are its hinge, and its folded side, off the line, joins it to the layer beneath.
    The top layer swings over the hinge onto the paper beyond, and the side opens flat,
    so the layer beneath spreads out on the near side. Where the side meets the line,
    the creases that run from there into each layer, precreased before, fold each
    layer in two halves, one on top of the other.
    """
    side, stack = flap_stack(state, line, flap)
    top = _side_of(state, flat_with(state, stack[:1]), line, side)
    hinge = [
        edge
        for edge, faces in state.creases.items()
        if len(set(faces) & top) == 1 and _along(state, edge, line)
    ]
    if not hinge:
        msg = f"no crease of the top layer at {flap} lies along {line}"
        raise RefusalError(msg, rule="no-crease", faces=tuple(sorted(top)), state=state)
    spine = [
        edge
        for edge, faces in state.creases.items()
        if state.is_folded(edge)
        and len(set(faces) & top) == 1
        and not _along(state, edge, line)
    ]
    if not spine:
        msg = f"the top layer at {flap} has no folded side to open"
        raise RefusalError(msg, rule="no-side", faces=tuple(sorted(top)), state=state)
    beneath = {f for edge in spine for f in state.creases[edge]} - top
    bottom = _side_of(state, flat_with(state, beneath), line, side)
    ends = {
        v
        for edge in spine
        for v in edge
        if line.contains(state.folded_point(state.edges[edge][0], v))
    }
    over = _toward_viewer(state, top)
    changes = dict.fromkeys(spine, "F")
    changes |= dict.fromkeys(hinge, over)
    for layer, kind in (
        (top, "V" if over == "M" else "M"),
        (bottom, _toward_viewer(state, bottom)),
    ):
        halves = [
            edge
            for edge in _flat_creases(state, layer, line, along_line=False)
            if set(edge) & ends
        ]
        if not halves:
            msg = f"no precreased crease of the flap at {flap} runs from its side"
            raise RefusalError(
                msg, rule="no-crease", faces=tuple(sorted(layer)), state=state
            )
        changes |= dict.fromkeys(halves, kind)
    still = next(f for edge in hinge for f in state.creases[edge] if f not in top)
    return collapse(state, changes, still)


def _side_of(state: State, faces: set[int], line: Line, side: int) -> set[int]:
    """The faces of a set that lie on one side of a line once folded."""
    return {f for f in faces if all(line.side(p) in {0, side} for p in state.folded[f])}


def _toward_viewer(state: State, layer: set[int]) -> str:
    """How a crease of a layer reads that folds part of it up toward the viewer."""
    return "M" if state.flips(min(layer)) else "V"


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
                    for other in _flat_creases(state, joined, line, along_line=False)
                    if end in other and _beyond(state, other, end, line, side)
                }
            elif set(faces) <= self.layer and not state.is_folded(edge):
                changes[edge] = self.kind
        return changes


def _flat_creases(
    state: State, faces: set[int], line: Line, *, along_line: bool
) -> list[Edge]:
    """The flat creases between faces of a set, along a line or off it."""
    return [
        edge
        for edge, pair in state.creases.items()
        if set(pair) <= faces
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
