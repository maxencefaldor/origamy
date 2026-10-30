# Copyright 2026 Maxence Faldor
"""The paper a fold script folds, one action per call, each recorded as a step.

Coordinates are where the paper lies now, as seen from above: x to the right, y up.
Unfolded, the paper covers the rectangle from (0, 0) to (width, height), its front up.
"""

from collections.abc import Callable, Iterable
from dataclasses import dataclass

from origamy import actions
from origamy.actions import Outcome, Sense
from origamy.checks import is_referenced, where
from origamy.errors import OrigamyError
from origamy.field import Exact, Number
from origamy.geometry import Line, Point, PointLike
from origamy.provenance import Source, capture
from origamy.rules import Diagnostic, RefusalError
from origamy.state import Edge, State

DEFAULT_FRONT = "#c8553d"
"""The color of the paper's front: the side up before any fold."""

DEFAULT_BACK = "#f2ede4"
"""The color of the paper's back."""


@dataclass(frozen=True, slots=True)
class Step:
    """One action the paper took, and its state after it."""

    action: str
    """``start``, ``valley``, ``mountain``, ``unfold``, ``precrease``, ``turn-over``."""
    description: str
    state: State
    moved: frozenset[int] = frozenset()
    """The faces the action moved, as ids in the step's state."""
    line: Line | None = None
    flap: Point | None = None
    source: Source | None = None
    diagnostics: tuple[Diagnostic, ...] = ()


type Crease = Line | tuple[PointLike, PointLike]
"""Creases of the crease pattern: all along a line, or along the segment between two
points."""


def _crease(crease: Crease) -> Line | tuple[Point, Point]:
    if isinstance(crease, Line):
        return crease
    start, end = crease
    return Point.of(start), Point.of(end)


def _describe(crease: Line | tuple[Point, Point]) -> str:
    if isinstance(crease, Line):
        return str(crease)
    return f"the segment from {crease[0]} to {crease[1]}"


class RefusedError(OrigamyError):
    """The paper could not take an action, so the script stopped there."""

    def __init__(self, paper: Paper, refused: Step) -> None:
        """Keep the paper as it was, and the refused action with its diagnostic."""
        where = f"{refused.source}: " if refused.source else ""
        super().__init__(f"{where}{refused.diagnostics[0].message}")
        self.paper = paper
        self.refused = refused


class Paper:
    """A sheet of paper that a fold script folds, step by step.

    >>> from origamy import Line
    >>> paper = Paper.square()
    >>> paper.valley(Line.through(("1/2", 0), ("1/2", 1)), flap=("3/4", "1/2"))
    >>> paper.layers(("1/4", "1/2"))
    2
    """

    def __init__(
        self,
        width: Exact = 1,
        height: Exact = 1,
        *,
        front: str = DEFAULT_FRONT,
        back: str = DEFAULT_BACK,
    ) -> None:
        """Unfolded paper from (0, 0) to (width, height), its front up."""
        w, h = Number.of(width), Number.of(height)
        if w.sign() <= 0 or h.sign() <= 0:
            msg = f"the paper needs a positive width and height: got {w} by {h}"
            raise ValueError(msg)
        self.width, self.height = w, h
        self.front, self.back = front, back
        corners = [Point.of(c) for c in ((0, 0), (w, 0), (w, h), (0, h))]
        self.steps = [
            Step("start", f"{w} by {h} paper, front up", State.sheet(corners))
        ]

    @classmethod
    def square(
        cls, size: Exact = 1, *, front: str = DEFAULT_FRONT, back: str = DEFAULT_BACK
    ) -> Paper:
        """A square of paper from (0, 0) to (size, size), its front up."""
        return cls(size, size, front=front, back=back)

    @property
    def state(self) -> State:
        """The paper now."""
        return self.steps[-1].state

    def valley(self, line: Line, flap: PointLike, *, layers: int | None = None) -> None:
        """Valley fold: turn the paper at ``flap`` over ``line``, onto the paper above.

        ``layers`` takes that many layers at ``flap``, from the top, and everything
        joined to them on the flap's side of the line. None takes all the paper on that
        side.
        """
        self._fold(Sense.VALLEY, line, flap, layers)

    def mountain(
        self, line: Line, flap: PointLike, *, layers: int | None = None
    ) -> None:
        """Mountain fold: turn the paper at ``flap`` behind, over ``line``.

        ``layers`` counts as in `valley`.
        """
        self._fold(Sense.MOUNTAIN, line, flap, layers)

    def unfold(self, line: Line, flap: PointLike, *, layers: int | None = None) -> None:
        """Unfold the paper at ``flap``, folded along ``line``, back the way it came.

        ``layers`` counts as in `valley`; None takes the upper side of every fold along
        the line on the flap's side.
        """
        point = Point.of(flap)
        self._act(
            "unfold",
            f"unfold along {line}, the paper at {point}",
            lambda state: actions.unfold(state, line, point, layers),
            line,
            point,
        )

    def precrease(
        self,
        line: Line,
        flap: PointLike,
        *,
        layers: int | None = None,
        mountain: bool = False,
    ) -> None:
        """Fold and unfold, leaving creases along ``line`` in the paper taken.

        ``layers`` counts as in `valley`; the fold is a valley fold unless ``mountain``.
        """
        point = Point.of(flap)
        sense = Sense.MOUNTAIN if mountain else Sense.VALLEY
        self._act(
            "precrease",
            f"precrease along {line} with a {sense} fold, the paper at {point}",
            lambda state: actions.precrease(state, line, point, layers, sense),
            line,
            point,
        )

    def collapse(
        self,
        *,
        valleys: Iterable[Crease] = (),
        mountains: Iterable[Crease] = (),
        flats: Iterable[Crease] = (),
        fixed: PointLike | None = None,
    ) -> None:
        """Fold, unfold or reverse many creases at once, and let the paper follow.

        Each crease is a line of the crease pattern, or a segment of one as its two
        ends, and takes every crease of the paper along it. Valleys and mountains
        read as on the crease pattern, from the paper's front; other creases keep
        their state. The top face at ``fixed`` stays where it lies; by default, the
        largest face stays.
        """
        changes = [
            (_crease(crease), kind)
            for creases, kind in ((valleys, "V"), (mountains, "M"), (flats, "F"))
            for crease in creases
        ]
        point = None if fixed is None else Point.of(fixed)

        def act(state: State) -> Outcome:
            assignments: dict[Edge, str] = {}
            for crease, kind in changes:
                edges = actions.along(state, crease)
                if not edges:
                    msg = f"no crease of the paper lies along {_describe(crease)}"
                    raise RefusalError(msg, rule="no-crease")
                assignments.update(dict.fromkeys(edges, kind))
            face = actions.largest(state)
            if point is not None:
                stack = actions.stack_at(state, point)
                if not stack:
                    msg = f"the fixed point {point} lies on no face's inside"
                    raise RefusalError(msg, rule="flap-off-paper")
                face = stack[0]
            return actions.collapse(state, assignments, face)

        counts = {kind: sum(k == kind for _, k in changes) for kind in "VMF"}
        self._act(
            "collapse",
            f"collapse {counts['V']} valleys, {counts['M']} mountains and "
            f"{counts['F']} flat creases",
            act,
        )

    def turn_over(self) -> None:
        """Turn the paper over, from left to right."""
        self._act("turn-over", "turn over", actions.turn_over)

    def layers(self, at: PointLike) -> int:
        """How many layers of paper lie at a point."""
        return len(actions.stack_at(self.state, Point.of(at)))

    def origin(self, at: PointLike, layer: int = 1) -> Point:
        """Where a point of a layer lies on the unfolded paper, the top layer first."""
        point = Point.of(at)
        stack = actions.stack_at(self.state, point)
        if len(stack) < layer:
            msg = f"{len(stack)} layers lie at {point}, not {layer}"
            raise OrigamyError(msg)
        return self.state.faces[stack[layer - 1]].place.inverse()(point)

    def creases(
        self, along: Line, *, at: PointLike | None = None, layers: int | None = None
    ) -> list[tuple[Point, Point]]:
        """The creases lying along a line now, as segments of the crease pattern.

        With ``at``, only creases of the top ``layers`` layers there count, each layer
        taken with the paper it continues into across flat creases.
        """
        state = self.state
        faces = set(range(len(state.faces)))
        if at is not None:
            stack = actions.stack_at(state, Point.of(at))
            faces = actions.flat_with(state, stack[: layers or len(stack)])
        found = []
        for edge, pair in state.edges.items():
            if len(pair) != 2 or not faces & set(pair):  # noqa: PLR2004 - a crease
                continue
            ends = tuple(state.folded_point(pair[0], v) for v in edge)
            if along.contains(ends[0]) and along.contains(ends[1]):
                found.append((state.vertices[edge[0]], state.vertices[edge[1]]))
        return found

    def where(self, point: PointLike) -> Point:
        """Where a point of the unfolded paper lies now, such as a corner."""
        point = Point.of(point)
        found = where(self.state, point)
        if found is None:
            msg = f"the point {point} is not on the paper"
            raise OrigamyError(msg)
        return found

    def _fold(
        self, sense: Sense, line: Line, flap: PointLike, layers: int | None
    ) -> None:
        point = Point.of(flap)
        taken = "" if layers is None else f", {layers} layers"
        self._act(
            sense.value,
            f"{sense} fold along {line}, the paper at {point}{taken}",
            lambda state: actions.fold(state, line, point, layers, sense),
            line,
            point,
        )

    def _act(
        self,
        action: str,
        description: str,
        act: Callable[[State], Outcome],
        line: Line | None = None,
        flap: Point | None = None,
    ) -> None:
        source = capture()
        before = self.state
        diagnostics: list[Diagnostic] = []
        if line is not None and action != "unfold" and not is_referenced(before, line):
            message = f"no point or line on the paper locates the fold line {line}"
            diagnostics.append(Diagnostic.of("unreferenced-fold", message))
        try:
            outcome = act(before)
        except RefusalError as error:
            refused = Step(
                action,
                description,
                before,
                frozenset(),
                line,
                flap,
                source,
                (error.diagnostic, *diagnostics),
            )
            raise RefusedError(self, refused) from None
        self.steps.append(
            Step(
                action,
                description,
                outcome.state,
                outcome.moved,
                line,
                flap,
                source,
                tuple(diagnostics),
            )
        )
