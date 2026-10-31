# Copyright 2026 Maxence Faldor
from math import isclose

import pytest

from origamy import Line, Paper
from origamy.paper import RefusedError
from origamy.shape import fold_angles
from origamy.space import Form

SPINE = Line.through(("1/2", 0), ("1/2", 1))
MIDDLE = Line.through((0, "1/2"), (1, "1/2"))


def book() -> Paper:
    """The square folded in half, its right half over its left one."""
    paper = Paper.square()
    paper.valley(SPINE, flap=("3/4", "1/2"))
    return paper


def heights(paper: Paper, face: int) -> list[float]:
    """The height above the table of each corner of a face."""
    form = paper.form
    assert form is not None
    return [form.point(face, p)[2] for p in paper.state.folded[face]]


def top(paper: Paper) -> int:
    """The face on top of the folded book."""
    return next(f for f in range(2) if any(h > 0 for h in heights(paper, f)))


def test_flat_paper_has_flat_fold_angles() -> None:
    paper = book()
    paper.precrease(MIDDLE, flap=("1/4", "3/4"))
    state = paper.state
    angles = fold_angles(state, Form.flat(len(state.faces)))
    assigned = {"V": 180.0, "M": -180.0, "F": 0.0}
    for edge, angle in angles.items():
        assert isclose(angle, assigned[state.assignment(edge)], abs_tol=1e-9)


def test_a_shape_step_stands_a_flap_up() -> None:
    paper = book()
    paper.shape(SPINE, flap=("1/4", "1/2"), angle=90, layers=1)
    assert paper.steps[-1].turn is not None
    flap = top(paper)
    assert max(heights(paper, flap)) == pytest.approx(0.5)
    assert heights(paper, 1 - flap) == [0, 0, 0, 0]
    form = paper.form
    assert form is not None
    assert all(isclose(form.point(flap, p)[0], 0.5) for p in paper.state.folded[flap])
    assert list(fold_angles(paper.state, form).values()) == [pytest.approx(90)]


def test_shape_steps_add_up() -> None:
    paper = book()
    for _ in range(2):
        paper.shape(SPINE, flap=("1/4", "1/2"), angle=45, layers=1)
    form = paper.form
    assert form is not None
    assert list(fold_angles(paper.state, form).values()) == [pytest.approx(90)]


def test_turning_shaped_paper_over_keeps_its_form() -> None:
    paper = book()
    paper.shape(SPINE, flap=("1/4", "1/2"), angle=60, layers=1)
    paper.turn_over()
    form = paper.form
    assert form is not None
    assert list(fold_angles(paper.state, form).values()) == [pytest.approx(120)]
    flap = next(f for f in range(2) if any(h < 0 for h in heights(paper, f)))
    assert min(heights(paper, flap)) == pytest.approx(-(3**0.5) / 4)
    assert heights(paper, 1 - flap) == pytest.approx([0, 0, 0, 0])


def test_shaped_paper_takes_no_fold() -> None:
    paper = book()
    paper.shape(SPINE, flap=("1/4", "1/2"), angle=90, layers=1)
    with pytest.raises(RefusedError) as caught:
        paper.valley(MIDDLE, flap=("1/4", "3/4"))
    assert caught.value.refusal.rule == "not-flat"


def test_a_shape_step_turns_no_paper_through_paper() -> None:
    paper = book()
    with pytest.raises(RefusedError) as caught:
        paper.shape(SPINE, flap=("1/4", "1/2"), angle=-90, layers=1)
    assert caught.value.refusal.rule == "turns-through-paper"


def test_a_shape_step_needs_a_straight_hinge() -> None:
    paper = book()
    paper.shape(SPINE, flap=("1/4", "1/2"), angle=90, layers=1)
    with pytest.raises(RefusedError) as caught:
        paper.shape(MIDDLE, flap=("1/4", "3/4"), angle=90, layers=2)
    assert caught.value.refusal.rule == "hinge-bent"
