# Copyright 2026 Maxence Faldor
"""The paper a fold script folds, one action per call, each recorded as a step.

Coordinates are where the paper lies now, as seen from above: x to the right, y up.
Unfolded, the paper covers the rectangle from (0, 0) to (width, height), its front up.
"""

from collections.abc import Callable, Iterable, Mapping
from dataclasses import dataclass
from fractions import Fraction
from math import radians

from origamy import actions, moves
from origamy import flex as flexing
from origamy import shape as shaping
from origamy.actions import Outcome, Sense
from origamy.checks import is_referenced, where
from origamy.errors import OrigamyError
from origamy.field import Exact, Number
from origamy.geometry import Line, Point, PointLike
from origamy.provenance import Source, capture
from origamy.rules import Diagnostic, RefusalError
from origamy.space import Form, Shaped, Turn
from origamy.state import Edge, State

DEFAULT_FRONT = "#c8553d"
"""The color of the paper's front: the side up before any fold."""

DEFAULT_BACK = "#f2ede4"
"""The color of the paper's back."""


@dataclass(frozen=True, slots=True)
class Step:
    """One action and the paper after it; for a refused action, the paper it began on.

    A refused action may have cut the paper along its line before its refusal, so its
    facts name faces of the cut paper.
    """

    action: str
    """The action's name, such as ``valley``, ``reverse`` or ``turn-over``; ``start``
    names the unfolded paper."""
    description: str
    state: State
    moved: frozenset[int] = frozenset()
    """The faces the action moved, as ids in the step's state."""
    line: Line | None = None
    flap: Point | None = None
    source: Source | None = None
    diagnostics: tuple[Diagnostic, ...] = ()
    """The facts the step states, besides any refusal."""
    refusal: Diagnostic | None = None
    """The fact that refused the action, for a refused action."""
    form: Form | None = None
    """The paper's form in space once shaping turns it; None while it lies flat."""
    turn: Turn | None = None
    """The turn the step makes in space, when it shapes or turns shaped paper over."""
    path: tuple[Form, ...] = ()
    """The forms a flex passes through on its way, in order, for the viewer."""
    through: State | None = None
    """The folded paper a precrease passes through before it unfolds; its faces are the
    step's faces."""


type CreaseLike = Line | tuple[PointLike, PointLike]
"""Creases of the crease pattern: all along a line, or along the segment between two
points."""


def _crease(crease: CreaseLike) -> Line | tuple[Point, Point]:
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

    def __init__(self, paper: Paper, refused: Step, refusal: Diagnostic) -> None:
        """Keep the paper as it was, the refused action, and the fact refusing it."""
        where = f"{refused.source}: " if refused.source else ""
        super().__init__(f"{where}{refusal.message}")
        self.paper = paper
        self.refused = refused
        self.refusal = refusal


class Paper:
    """A sheet of paper that a fold script folds, step by step.

    Examples:
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
        """The paper now, as it lies flat."""
        return self.steps[-1].state

    @property
    def form(self) -> Form | None:
        """The paper's form in space, once shaping turns it; None while it lies flat."""
        return self.steps[-1].form

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
            _flat(lambda state: actions.unfold(state, line, point, layers)),
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
            _flat(lambda state: actions.precrease(state, line, point, layers, sense)),
            line,
            point,
        )

    def reverse(
        self,
        line: Line,
        flap: PointLike,
        *,
        layers: int | None = None,
        outside: bool = False,
    ) -> None:
        """Reverse fold: turn the tip of a folded flap inside out, across ``line``.

        The tip is the paper at ``flap`` on its side of the line, taken as `valley`
        takes it. An inside reverse fold turns the tip between the flap's layers; with
        ``outside``, the tip wraps around them.
        """
        point = Point.of(flap)
        kind = "outside" if outside else "inside"
        self._act(
            "reverse",
            f"{kind} reverse fold along {line}, the paper at {point}",
            _flat(
                lambda state: moves.reverse(state, line, point, layers, outside=outside)
            ),
            line,
            point,
        )

    def petal(self, line: Line, flap: PointLike) -> None:
        """Petal fold: lift the top layer at ``flap`` over ``line``, sides folding in.

        The creases from the line's ends into the lifted region, and their partners
        in the layers joined across its sides, must be precreased first.
        """
        point = Point.of(flap)
        self._act(
            "petal",
            f"petal fold along {line}, the paper at {point}",
            _flat(lambda state: moves.petal(state, line, point)),
            line,
            point,
        )

    def squash(self, line: Line, flap: PointLike) -> None:
        """Squash fold: swing the top flap at ``flap`` over ``line``, pressing it flat.

        The flap's folded side opens flat along the line, and each of its two layers
        folds in half. The creases from where the side meets the line into both
        layers must be precreased first.
        """
        point = Point.of(flap)
        self._act(
            "squash",
            f"squash fold over {line}, the paper at {point}",
            _flat(lambda state: moves.squash(state, line, point)),
            line,
            point,
        )

    def collapse(
        self,
        *,
        valleys: Iterable[CreaseLike] = (),
        mountains: Iterable[CreaseLike] = (),
        flats: Iterable[CreaseLike] = (),
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
                    raise RefusalError(msg, rule="no-crease", state=state)
                assignments.update(dict.fromkeys(edges, kind))
            face = actions.largest(state)
            if point is not None:
                stack = actions.stack_at(state, point)
                if not stack:
                    msg = f"the fixed point {point} lies on no face's inside"
                    raise RefusalError(msg, rule="flap-off-paper", state=state)
                face = stack[0]
            return actions.collapse(state, assignments, face)

        counts = {kind: sum(k == kind for _, k in changes) for kind in "VMF"}
        self._act(
            "collapse",
            f"collapse {counts['V']} valleys, {counts['M']} mountains and "
            f"{counts['F']} flat creases",
            _flat(act),
        )

    def shape(
        self, line: Line, flap: PointLike, *, angle: Exact, layers: int | None = None
    ) -> None:
        """Turn the paper at ``flap`` about ``line`` by ``angle`` degrees, in space.

        Shaping opens folds part of the way, as the last steps of a model do. A positive
        angle turns the paper up, toward the viewer, and a negative one turns it down.
        The paper turns as one piece about its hinge, taken as `valley` takes it, and
        lines and points read in the paper as it lies flat. Once shaped, the paper
        takes no more folds; it still turns over and takes more shaping.
        """
        point = Point.of(flap)
        turn = Fraction(Number.of(angle).fraction())
        self._act(
            "shape",
            f"shape: turn the paper at {point} about {line} by {turn}°",
            lambda paper: shaping.shape(paper, line, point, layers, turn),
            line,
            point,
        )

    def flex(
        self,
        angles: Mapping[CreaseLike, Exact],
        *,
        fixed: PointLike | None = None,
    ) -> None:
        """Fold creases to angles in degrees, and let the paper follow, rigid.

        Each crease is a line of the crease pattern, or a segment of one as its two
        ends, and takes every crease of the paper along it. Each crease folds its own
        way to the angle named: 180 pressed flat, 0 open flat, and a negative angle
        the other way. A crease lying flat folds as a valley. Every other crease
        turns as little as it can while every face stays rigid. The top face at
        ``fixed`` stays where it lies; by default, the largest face stays.
        """
        targets = [
            (_crease(crease), Fraction(Number.of(angle).fraction()))
            for crease, angle in angles.items()
        ]
        point = None if fixed is None else Point.of(fixed)

        def act(paper: Shaped) -> Outcome:
            state = paper.state
            goals: dict[Edge, float] = {}
            for crease, angle in targets:
                edges = actions.along(state, crease)
                if not edges:
                    msg = f"no crease of the paper lies along {_describe(crease)}"
                    raise RefusalError(msg, rule="no-crease", state=state)
                for edge in edges:
                    sense = -1 if state.assignment(edge) == "M" else 1
                    goals[edge] = sense * radians(angle)
            face = actions.largest(state)
            if point is not None:
                stack = actions.stack_at(state, point)
                if not stack:
                    msg = f"the fixed point {point} lies on no face's inside"
                    raise RefusalError(msg, rule="flap-off-paper", state=state)
                face = stack[0]
            return flexing.flex(paper, goals, face)

        named = ", ".join(f"{_describe(c)} to {a}°" for c, a in targets)
        self._act("flex", f"flex: fold {named}", act)

    def tilt(self, line: Line, flap: PointLike, *, angle: Exact) -> None:
        """Turn the whole paper about ``line`` by ``angle`` degrees, in space.

        A tilt holds a finished model up to show it: the side of the line where
        ``flap`` lies rises when the angle is positive. The line lies on the table, in
        the coordinates of the paper as it lies flat, which shaping leaves unchanged
        for paper it has not turned. A tilted paper takes no more folds.
        """
        point = Point.of(flap)
        turn = Fraction(Number.of(angle).fraction())
        self._act(
            "tilt",
            f"tilt: turn the paper about {line}, the side of {point} up by {turn}°",
            lambda paper: shaping.tilt(paper, line, point, turn),
            line,
            point,
        )

    def turn_over(self) -> None:
        """Turn the paper over, from left to right."""
        self._act("turn-over", "turn over", shaping.turn_over)

    def layers(self, at: PointLike) -> int:
        """How many layers of paper lie at a point."""
        return len(actions.stack_at(self.state, Point.of(at)))

    def thickest(self) -> tuple[int, Point]:
        """The most layers that lie at one point, and a point where they lie."""
        return self.state.thickest

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
            faces = actions.flat_with(state, stack[:layers])
        found = []
        for edge, pair in state.creases.items():
            if not faces & set(pair):
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
            _flat(lambda state: actions.fold(state, line, point, layers, sense)),
            line,
            point,
        )

    def _act(
        self,
        action: str,
        description: str,
        act: Callable[[Shaped], Outcome],
        line: Line | None = None,
        flap: Point | None = None,
    ) -> None:
        source = capture()
        before = self.state
        diagnostics: list[Diagnostic] = []
        if line is not None and not is_referenced(before, line):
            message = f"no point or line on the paper locates the fold line {line}"
            diagnostics.append(Diagnostic.of("unreferenced-fold", message))
        try:
            outcome = act(Shaped(before, self.form))
        except RefusalError as error:
            refused = Step(
                action,
                description,
                error.state,
                frozenset(),
                line,
                flap,
                source,
                tuple(diagnostics),
                error.diagnostic,
                self.form.carried(before, error.state) if self.form else None,
            )
            raise RefusedError(self, refused, error.diagnostic) from None
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
                form=outcome.form,
                turn=outcome.turn,
                path=outcome.path,
                through=outcome.through,
            )
        )


def _flat(act: Callable[[State], Outcome]) -> Callable[[Shaped], Outcome]:
    """An action that folds flat paper, refused once shaping has turned the paper."""

    def on(paper: Shaped) -> Outcome:
        if paper.form is not None:
            msg = "shaping has turned the paper out of the plane, so it takes no fold"
            raise RefusalError(msg, rule="not-flat", state=paper.state)
        return act(paper.state)

    return on
