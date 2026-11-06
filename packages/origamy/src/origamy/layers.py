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

The exact kernel, `origamy_kernel`, builds these constraints and searches. It
propagates them and keeps the reason for each deduction, so that when no stacking
exists it names the constraints that conflict.
"""

from collections.abc import Iterable, Sequence
from dataclasses import dataclass
from typing import Final

from origamy.errors import OrigamyError
from origamy.geometry import Point
from origamy.state import Pair, State
from origamy_kernel.data import ConstraintData

BUDGET: Final = 100_000
"""The dead ends the search for a stacking may meet before it gives up."""


@dataclass(frozen=True, slots=True)
class Constraint:
    """One geometric reason that limits how some faces stack."""

    kind: str
    """``taco-taco``, ``taco-tortilla``, ``tortilla-tortilla`` or ``transitivity``."""
    faces: tuple[int, ...]
    where: tuple[Point, Point] | None
    """The stretch of crease the constraint is about, once folded, if any."""


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


def solve(
    state: State,
    fixed: Iterable[Pair] = (),
    prefer: Iterable[Pair] = (),
    budget: int = BUDGET,
) -> frozenset[Pair]:
    """Find a stacking: one ``(upper, lower)`` for every pair of overlapping faces.

    ``fixed`` lists the pairs whose order is required, as ``(upper, lower)``. The search
    tries the orders in ``prefer`` first, so among valid stackings it finds one that
    keeps them where it can. It raises `NoStackingError` when no stacking exists, and
    `BudgetError` after ``budget`` dead ends. The state's own stacking plays no part.
    """
    outcome, pairs, constraints, conflict = state.kernel.stack(
        list(fixed), list(prefer), budget
    )
    if outcome != "stacked":
        raise unstacked(state, outcome, constraints, conflict)
    return frozenset(pairs)


def violations(state: State) -> list[Constraint]:
    """The constraints the state's stacking breaks.

    Each clause it meets with no order counts, and so do three faces that share area
    and that it stacks in a cycle, one such three for each region where the most faces
    meet. A valid stacking breaks none.
    """
    found = state.kernel.violations(sorted(state.above))
    return constraints(state, found)


def constraints(state: State, found: Iterable[ConstraintData]) -> list[Constraint]:
    """The constraints the kernel names, each with its stretch of crease once folded."""
    creases = list(state.creases.items())

    def where(index: int | None) -> tuple[Point, Point] | None:
        if index is None:
            return None
        (u, v), (f, _) = creases[index]
        return state.folded_point(f, u), state.folded_point(f, v)

    return [
        Constraint(kind, tuple(faces), where(index)) for kind, faces, index in found
    ]


def unstacked(
    state: State,
    outcome: str,
    found: Iterable[ConstraintData],
    conflict: Sequence[Pair],
) -> NoStackingError | BudgetError:
    """The error a kernel's outcome names when it found no stacking."""
    if outcome == "budget":
        return BudgetError("the search for a stacking ran out of budget")
    return NoStackingError(constraints(state, found), conflict)
