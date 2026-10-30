# Copyright 2026 Maxence Faldor
from itertools import combinations

import pytest
from hypothesis import HealthCheck, assume, given, settings
from hypothesis import strategies as st

from origamy.actions import (
    Sense,
    along,
    collapse,
    fold,
    precrease,
    stack_at,
    turn_over,
    unfold,
)
from origamy.geometry import Line, Point, area, intersection, overlaps
from origamy.layers import Problem
from origamy.rules import RefusalError
from origamy.state import State

P = Point.of
SHEET = State.sheet([P((0, 0)), P((1, 0)), P((1, 1)), P((0, 1))])
SPINE = Line.through(("1/2", 0), ("1/2", 1))
MIDDLE = Line.through((0, "1/2"), (1, "1/2"))


def book() -> State:
    """The square folded in half, its right half over its left one."""
    return fold(SHEET, SPINE, P(("3/4", "1/2")), None, Sense.VALLEY).state


def faces_at(state: State, x: str, y: str) -> list[int]:
    return stack_at(state, P((x, y)))


def assert_valid(state: State) -> None:
    """The stacking orders every overlapping pair once, meeting every constraint."""
    pairs = {(min(pair), max(pair)) for pair in state.above}
    assert pairs == set(state.overlapping)
    assert len(pairs) == len(state.above)
    problem = Problem(state)
    literals = {problem.literal(*pair) for pair in state.above}
    assert all(any(x in literals for x in clause) for clause in problem.clauses)
    assert sum(area(polygon) for polygon in state.patterns) == 1


def test_a_valley_fold_turns_the_paper_over_on_top() -> None:
    state = book()
    assert faces_at(state, "1/4", "1/2") == [0, 1]
    assert state.flips(0)
    assert not state.flips(1)
    assert state.assignment((4, 5)) == "V"
    assert_valid(state)


def test_a_mountain_fold_puts_the_paper_under() -> None:
    state = fold(SHEET, SPINE, P(("3/4", "1/2")), None, Sense.MOUNTAIN).state
    assert faces_at(state, "1/4", "1/2") == [1, 0]
    assert state.assignment((4, 5)) == "M"


def test_a_fold_moves_every_layer_joined_to_the_one_taken() -> None:
    state = fold(book(), MIDDLE, P(("1/4", "3/4")), 1, Sense.VALLEY).state
    assert faces_at(state, "1/4", "1/4") == [1, 0, 2, 3]
    assert_valid(state)


def test_a_mountain_fold_of_the_top_layer_tucks_it_inside() -> None:
    folded = fold(SHEET, MIDDLE, P(("1/2", "3/4")), None, Sense.VALLEY).state
    quarter = Line.through((0, "1/4"), (1, "1/4"))
    outcome = fold(folded, quarter, P(("1/2", "1/8")), 1, Sense.MOUNTAIN)
    top, middle, bottom = faces_at(outcome.state, "1/2", "3/8")
    assert middle in outcome.moved
    assert top not in outcome.moved
    assert bottom not in outcome.moved
    assert_valid(outcome.state)


def test_unfolding_restores_the_paper() -> None:
    folded = fold(book(), MIDDLE, P(("1/4", "3/4")), None, Sense.VALLEY).state
    state = unfold(folded, MIDDLE, P(("1/4", "1/4")), None).state
    assert len(faces_at(state, "1/4", "3/4")) == 2
    assert len(faces_at(state, "1/4", "1/4")) == 2
    assert all(state.assignment(edge) in "FBV" for edge in state.edges)
    assert_valid(state)


def test_a_precrease_leaves_a_flat_crease() -> None:
    state = precrease(SHEET, MIDDLE, P(("1/2", "3/4")), None, Sense.VALLEY).state
    assert len(state.faces) == 2
    assert sorted(state.assignment(edge) for edge in state.edges).count("F") == 1
    assert not state.above


def test_turning_over_reverses_the_stack() -> None:
    state = book()
    turned = turn_over(state).state
    assert faces_at(turned, "1/4", "1/2") == [1, 0]
    assert turned.flips(1) != state.flips(1)


@pytest.mark.parametrize(
    ("line", "flap", "layers", "rule"),
    [
        (SPINE, ("1/2", "1/2"), None, "flap-on-line"),
        (SPINE, (2, 2), None, "flap-off-paper"),
        (SPINE, ("1/4", "1/2"), 3, "too-few-layers"),
        (Line.through((2, 0), (2, 1)), ("1/4", "1/2"), None, "no-hinge"),
        (SPINE, ("1/4", "1/2"), 1, "already-folded"),
    ],
)
def test_refusals(
    line: Line, flap: tuple[str, str], layers: int | None, rule: str
) -> None:
    with pytest.raises(RefusalError) as refusal:
        fold(book(), line, P(flap), layers, Sense.VALLEY)
    assert refusal.value.diagnostic.rule == rule


def test_unfolding_flat_paper_is_refused() -> None:
    with pytest.raises(RefusalError) as refusal:
        unfold(SHEET, MIDDLE, P(("1/2", "3/4")), None)
    assert refusal.value.diagnostic.rule == "not-folded"


eighths = st.integers(0, 8).map(lambda k: f"{k}/8")


type Fold = tuple[Line, Point, int | None, Sense]


@st.composite
def folds(draw: st.DrawFn) -> Fold:
    p, q = draw(st.tuples(eighths, eighths)), draw(st.tuples(eighths, eighths))
    assume(p != q)
    sixteenths = st.integers(1, 15).map(lambda k: f"{k}/16")
    flap = P((draw(sixteenths), draw(sixteenths)))
    layers = draw(st.none() | st.integers(1, 3))
    sense = draw(st.sampled_from([Sense.VALLEY, Sense.MOUNTAIN]))
    return Line.through(p, q), flap, layers, sense


@settings(max_examples=40, deadline=None, suppress_health_check=[HealthCheck.too_slow])
@given(st.lists(folds(), min_size=1, max_size=4))
def test_every_fold_leaves_a_valid_state(sequence: list[Fold]) -> None:
    state = SHEET
    for line, flap, layers, sense in sequence:
        try:
            state = fold(state, line, flap, layers, sense).state
        except RefusalError:
            continue
        assert_valid(state)
        for f, g in combinations(range(len(state.faces)), 2):
            assert overlaps(state.folded[f], state.folded[g]) == (
                (f, g) in state.overlapping
            )
        common = [
            intersection(state.folded[f], state.folded[g]) for f, g in state.overlapping
        ]
        assert all(area(c).sign() > 0 for c in common)


def precreased(*lines: Line) -> State:
    """The square, precreased along lines through its center."""
    state = SHEET
    for line in lines:
        flap = next(
            P(point)
            for point in (("7/8", "1/16"), ("1/16", "7/8"), ("15/16", "5/8"))
            if line.side(P(point))
            and not any(p == P(point) for polygon in state.patterns for p in polygon)
        )
        state = precrease(state, line, flap, None, Sense.VALLEY).state
    return state


RISING = Line.through((0, 0), (1, 1))
FALLING = Line.through((1, 0), (0, 1))


CENTER = P(("1/2", "1/2"))

type Crease = Line | tuple[Point, Point]


def creases_along(state: State, *creases: Crease) -> list[tuple[int, int]]:
    return [edge for crease in creases for edge in along(state, crease)]


def test_a_collapse_folds_the_preliminary_base() -> None:
    state = precreased(RISING, FALLING, SPINE, MIDDLE)
    changes = dict.fromkeys(creases_along(state, SPINE, MIDDLE), "V")
    changes |= dict.fromkeys(creases_along(state, FALLING), "M")
    outcome = collapse(state, changes, stack_at(state, P(("15/16", "5/8")))[0])
    folded = outcome.state
    assert len(faces_at(folded, "7/8", "5/8")) == 4
    assert {folded.assignment(edge) for edge in changes} == {"V", "M"}
    assert_valid(folded)


def test_a_collapse_unfolds_too() -> None:
    state = book()
    outcome = collapse(state, dict.fromkeys(along(state, SPINE), "F"), 1)
    assert not outcome.state.above
    assert outcome.moved == {0}


@pytest.mark.parametrize(
    ("lines", "valleys", "mountains", "rule"),
    [
        (
            (RISING, FALLING, SPINE, MIDDLE),
            (RISING, FALLING, SPINE, MIDDLE),
            (),
            "mountain-valley-count",
        ),
        (
            (RISING, SPINE, MIDDLE),
            (SPINE, (CENTER, P((0, 0)))),
            ((CENTER, P((1, "1/2"))),),
            "unbalanced-angles",
        ),
    ],
)
def test_collapse_refusals(
    lines: tuple[Line, ...],
    valleys: tuple[Crease, ...],
    mountains: tuple[Crease, ...],
    rule: str,
) -> None:
    state = precreased(*lines)
    changes = dict.fromkeys(creases_along(state, *valleys), "V")
    changes |= dict.fromkeys(creases_along(state, *mountains), "M")
    with pytest.raises(RefusalError) as refusal:
        collapse(state, changes, 0)
    assert refusal.value.diagnostic.rule == rule
