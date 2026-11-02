# Copyright 2026 Maxence Faldor
"""The layer solver: how faces stack wherever they overlap, with no paper crossing.

The solver has one variable for each pair of faces that overlap: which one is above.
After Akitaya, Demaine and Ku's Flat-Folder, four kinds of constraint say exactly when a
stacking is valid:

- **taco-taco**: two folds along one stretch of line, their faces on the same side, nest
  or stack but never interleave;
- **taco-tortilla**: paper that runs on across a fold is not inside the fold;
- **tortilla-tortilla**: two pieces of paper creased flat along one stretch of line keep
  the same order on both sides of it;
- **transitivity**: three faces that share area stack with no cycle.

The solver propagates these constraints and searches, keeping the reason for each
deduction, so that when no stacking exists it names the constraints that conflict.
"""

from collections import defaultdict
from collections.abc import Iterable, Sequence
from dataclasses import dataclass
from itertools import combinations, product

from origamy.errors import OrigamyError
from origamy.geometry import (
    Line,
    Point,
    bounds,
    box,
    collinear_overlap,
    intersection,
    overlaps,
    through_inside,
)
from origamy.state import Pair, State

type Clause = tuple[int, ...]
"""Literals: ``+v`` puts the lesser face of pair ``v`` above, and ``-v`` below."""


@dataclass(frozen=True, slots=True)
class Constraint:
    """One geometric reason that limits how some faces stack."""

    kind: str
    """``taco-taco``, ``taco-tortilla``, ``tortilla-tortilla`` or ``transitivity``."""
    faces: tuple[int, ...]
    where: tuple[Point, Point] | None
    """The stretch of crease the constraint is about, once folded, if any."""


@dataclass(frozen=True, slots=True)
class Crease:
    """A crease as the solver sees it: its faces and where it lies once folded."""

    faces: tuple[int, int]
    """A fold's two faces, or a flat crease's two faces, the positive side's first."""
    ends: tuple[Point, Point]
    line: Line
    side: int
    """For a fold, the side of its line that both faces lie on; 0 for a flat crease."""


class NoStackingError(OrigamyError):
    """No stacking of the faces passes no paper through paper."""

    def __init__(
        self, constraints: Sequence[Constraint], fixed: Sequence[Pair]
    ) -> None:
        """Keep the constraints and the required pairs that conflict."""
        super().__init__("no stacking of the faces keeps paper from crossing paper")
        self.constraints = tuple(constraints)
        self.fixed = tuple(fixed)


class BudgetError(OrigamyError):
    """The search for a stacking stopped before finding one or proving none exists."""


class Problem:
    """The stacking constraints of a state, as clauses over pairs of faces."""

    def __init__(self, state: State) -> None:
        """Build every constraint of a state's geometry."""
        self.state = state
        self.pairs = sorted(state.overlapping)
        self.variables = {pair: index + 1 for index, pair in enumerate(self.pairs)}
        self.clauses: list[Clause] = []
        self.origins: list[Constraint] = []
        creases = list(_creases(state))
        self._folds_and_flats(creases)
        self._crossings(creases)
        self._transitivity()

    def literal(self, upper: int, lower: int) -> int | None:
        """The literal that says ``upper`` is above ``lower``, if the two overlap."""
        if upper < lower:
            return self.variables.get((upper, lower))
        variable = self.variables.get((lower, upper))
        return None if variable is None else -variable

    def _add(self, constraint: Constraint, clauses: Iterable[Clause]) -> None:
        for clause in clauses:
            self.clauses.append(clause)
            self.origins.append(constraint)

    def _equal(self, constraint: Constraint, x: int | None, y: int | None) -> None:
        if x is not None and y is not None and x != y:
            self._add(constraint, ((-x, y), (x, -y)))

    def _folds_and_flats(self, creases: list[Crease]) -> None:
        on_line: dict[Line, list[Crease]] = defaultdict(list)
        for crease in creases:
            on_line[crease.line].append(crease)
        for group in on_line.values():
            for first, second in combinations(group, 2):
                if not collinear_overlap(first.ends, second.ends):
                    continue
                if first.side and second.side:
                    if first.side == second.side:
                        self._taco_taco(first, second)
                elif first.side or second.side:
                    taco, flat = (first, second) if first.side else (second, first)
                    tortilla = flat.faces[0] if taco.side > 0 else flat.faces[1]
                    self._taco_tortilla(taco, tortilla)
                else:
                    self._tortilla_tortilla(first, second)

    def _taco_taco(self, first: Crease, second: Crease) -> None:
        (a, b), (c, d) = first.faces, second.faces
        if len({a, b, c, d}) < 4:  # noqa: PLR2004 - two folds of four faces
            return
        # c lies between a and b when exactly one of a and b is below it; so does d.
        # Nesting or stacking puts both or neither between: the parity is even.
        literals = [
            self.literal(c, a),
            self.literal(c, b),
            self.literal(d, a),
            self.literal(d, b),
        ]
        if any(literal is None for literal in literals):
            return
        known = [literal for literal in literals if literal is not None]
        constraint = Constraint("taco-taco", (a, b, c, d), first.ends)
        self._add(
            constraint,
            [
                tuple(-x if odd else x for x, odd in zip(known, values, strict=True))
                for values in product((False, True), repeat=4)
                if sum(values) % 2
            ],
        )

    def _taco_tortilla(self, taco: Crease, tortilla: int) -> None:
        a, b = taco.faces
        if tortilla in (a, b):
            return
        constraint = Constraint("taco-tortilla", (a, b, tortilla), taco.ends)
        self._equal(constraint, self.literal(tortilla, a), self.literal(tortilla, b))

    def _tortilla_tortilla(self, first: Crease, second: Crease) -> None:
        (p1, n1), (p2, n2) = first.faces, second.faces
        if len({p1, n1, p2, n2}) < 4:  # noqa: PLR2004 - two creases of four faces
            return
        constraint = Constraint("tortilla-tortilla", (p1, n1, p2, n2), first.ends)
        self._equal(constraint, self.literal(p1, p2), self.literal(n1, n2))

    def _crossings(self, creases: list[Crease]) -> None:
        folded = self.state.folded
        boxes = [box(polygon) for polygon in folded]
        for crease in creases:
            low_x, low_y, high_x, high_y = bounds(crease.ends)
            for face, (x0, y0, x1, y1) in enumerate(boxes):
                if face in crease.faces or not (
                    x0 <= high_x and low_x <= x1 and y0 <= high_y and low_y <= y1
                ):
                    continue
                if through_inside(folded[face], *crease.ends):
                    a, b = crease.faces
                    kind = "taco-tortilla" if crease.side else "tortilla-tortilla"
                    constraint = Constraint(kind, (a, b, face), crease.ends)
                    self._equal(
                        constraint, self.literal(face, a), self.literal(face, b)
                    )

    def _transitivity(self) -> None:
        folded = self.state.folded
        overlapping = self.state.overlapping
        neighbors: dict[int, set[int]] = defaultdict(set)
        for f, g in overlapping:
            neighbors[f].add(g)
            neighbors[g].add(f)
        for f, g in self.pairs:
            common = intersection(folded[f], folded[g])
            for h in sorted(neighbors[f] & neighbors[g]):
                if h <= g or not overlaps(common, folded[h]):
                    continue
                x, y, z = self.literal(f, g), self.literal(g, h), self.literal(h, f)
                if x is None or y is None or z is None:
                    continue
                constraint = Constraint("transitivity", (f, g, h), None)
                self._add(constraint, ((-x, -y, -z), (x, y, z)))


def _creases(state: State) -> Iterable[Crease]:
    for edge, (f, g) in state.creases.items():
        ends = state.folded_point(f, edge[0]), state.folded_point(f, edge[1])
        line = Line.through(*ends)
        side_f = _side(state, f, line)
        if state.faces[f].place != state.faces[g].place:
            yield Crease((f, g), ends, line, side_f)
        else:
            yield Crease((f, g) if side_f > 0 else (g, f), ends, line, 0)


def _side(state: State, face: int, line: Line) -> int:
    for point in state.folded[face]:
        if side := line.side(point):
            return side
    return 0


def solve(
    problem: Problem,
    fixed: Iterable[Pair] = (),
    prefer: Iterable[Pair] = (),
    budget: int = 100_000,
) -> frozenset[Pair]:
    """Find a stacking: one ``(upper, lower)`` for every pair of overlapping faces.

    ``fixed`` lists the pairs whose order is required, as ``(upper, lower)``. The search
    tries the orders in ``prefer`` first, so among valid stackings it finds one that
    keeps them where it can. It raises `NoStackingError` when no stacking exists, and
    `BudgetError` after ``budget`` dead ends.
    """
    solver = _Solver(problem)
    required = [lit for pair in fixed if (lit := problem.literal(*pair)) is not None]
    if contradiction := {lit for lit in required if -lit in required}:
        raise NoStackingError(
            (), [_pair(problem, lit) for lit in sorted(contradiction)]
        )
    for literal in dict.fromkeys(required):
        solver.assign(literal, None)
    if (conflict := solver.propagate(0)) is not None:
        clauses, literals = solver.explain(conflict)
        raise NoStackingError(
            [problem.origins[index] for index in clauses],
            [_pair(problem, literal) for literal in literals],
        )
    order = [lit for pair in prefer if (lit := problem.literal(*pair)) is not None]
    if not solver.search(order, budget):
        raise NoStackingError((), ())
    return frozenset(
        _pair(problem, variable if solver.values[variable] > 0 else -variable)
        for variable in range(1, len(problem.pairs) + 1)
    )


def _pair(problem: Problem, literal: int) -> Pair:
    f, g = problem.pairs[abs(literal) - 1]
    return (f, g) if literal > 0 else (g, f)


class _Solver:
    """Unit propagation and chronological backtracking over a problem's clauses."""

    def __init__(self, problem: Problem) -> None:
        self.clauses = problem.clauses
        self.values = [0] * (len(problem.pairs) + 1)
        self.reasons: list[int | None] = [None] * len(self.values)
        self.trail: list[int] = []
        self.containing: dict[int, list[int]] = defaultdict(list)
        for index, clause in enumerate(self.clauses):
            for literal in clause:
                self.containing[literal].append(index)

    def value(self, literal: int) -> int:
        """1 when the literal holds, -1 when it fails, 0 when it is open."""
        value = self.values[abs(literal)]
        return value if literal > 0 else -value

    def assign(self, literal: int, reason: int | None) -> None:
        """Make a literal hold, because of a clause or as a choice."""
        self.values[abs(literal)] = 1 if literal > 0 else -1
        self.reasons[abs(literal)] = reason
        self.trail.append(literal)

    def propagate(self, start: int) -> int | None:
        """Deduce from the trail after ``start``; return a clause that fails, if any."""
        head = start
        while head < len(self.trail):
            falsified = -self.trail[head]
            head += 1
            for index in self.containing[falsified]:
                open_literal = None
                for literal in self.clauses[index]:
                    value = self.value(literal)
                    if value > 0 or (not value and open_literal is not None):
                        break  # satisfied, or two literals still open
                    if not value:
                        open_literal = literal
                else:
                    if open_literal is None:
                        return index
                    self.assign(open_literal, index)
        return None

    def explain(self, conflict: int) -> tuple[list[int], list[int]]:
        """The clauses and required literals behind a conflict at the root."""
        clauses: set[int] = set()
        required: set[int] = set()
        pending, seen = [conflict], set[int]()
        while pending:
            index = pending.pop()
            clauses.add(index)
            for literal in self.clauses[index]:
                variable = abs(literal)
                if variable in seen:
                    continue
                seen.add(variable)
                reason = self.reasons[variable]
                if reason is None:
                    required.add(variable * self.values[variable])
                else:
                    pending.append(reason)
        return sorted(clauses), sorted(required)

    def search(self, order: Sequence[int], budget: int) -> bool:
        """Complete the assignment, choosing preferred literals first.

        Returns False when no completion exists.
        """
        preferred = {abs(literal): literal for literal in order}
        queue = list(preferred) + [
            v for v in range(1, len(self.values)) if v not in preferred
        ]
        decisions: list[tuple[int, int, bool]] = []
        cursor = 0
        while True:
            while cursor < len(queue) and self.values[queue[cursor]]:
                cursor += 1
            if cursor == len(queue):
                return True
            literal = preferred.get(queue[cursor], queue[cursor])
            decisions.append((len(self.trail), literal, False))
            self.assign(literal, None)
            while self.propagate(decisions[-1][0]) is not None:
                budget -= 1
                if budget < 0:
                    msg = "the search for a stacking ran out of budget"
                    raise BudgetError(msg)
                while decisions and decisions[-1][2]:
                    self._undo(decisions.pop()[0])
                if not decisions:
                    return False
                mark, literal, _ = decisions.pop()
                self._undo(mark)
                decisions.append((mark, -literal, True))
                self.assign(-literal, None)
                cursor = 0

    def _undo(self, mark: int) -> None:
        for literal in self.trail[mark:]:
            self.values[abs(literal)] = 0
            self.reasons[abs(literal)] = None
        del self.trail[mark:]
