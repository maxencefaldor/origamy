# Copyright 2026 Maxence Faldor
import runpy
from pathlib import Path

import pytest
from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st

from origamy import Line, Paper, Point
from origamy.actions import stack_at
from origamy.flex import path
from origamy.paper import RefusedError
from origamy.space import STILL, Form, fold_angles
from origamy.state import State

EXAMPLES = Path(__file__).parents[3] / "examples"
UPRIGHT = Line.through(("1/2", 0), ("1/2", 1))
LEVEL = Line.through((0, "1/2"), (1, "1/2"))


def example(name: str) -> Paper:
    """An example's paper, folded."""
    paper = runpy.run_path(str(EXAMPLES / f"{name}.py"))["fold"]()
    assert isinstance(paper, Paper)
    return paper


def assert_whole(paper: Paper) -> None:
    """Every face that holds a vertex puts it at one point in space."""
    assert paper.form is not None
    assert_form_whole(paper.state, paper.form)


def assert_form_whole(state: State, form: Form) -> None:
    """Every face that holds a vertex puts it at one point in space, in a form."""
    seen: dict[int, tuple[float, ...]] = {}
    for f, face in enumerate(state.faces):
        for v in face.corners:
            where = form.point(f, state.folded_point(f, v))
            assert where == pytest.approx(seen.setdefault(v, where), abs=1e-8)


@settings(max_examples=12, deadline=None, suppress_health_check=[HealthCheck.too_slow])
@given(st.integers(min_value=100, max_value=179))
def test_a_flex_opens_a_preliminary_base_whole(angle: int) -> None:
    paper = example("preliminary_base")
    paper.flex({UPRIGHT: angle, LEVEL: angle})
    assert paper.steps[-1].action == "flex"
    assert_whole(paper)
    angles = fold_angles(paper.state, paper.form)
    for line in (UPRIGHT, LEVEL):
        for edge in paper.state.creases:
            a, b = (paper.state.vertices[v] for v in edge)
            if line.contains(a) and line.contains(b):
                assert angles[edge] == pytest.approx(angle)


def test_a_flex_keeps_the_fixed_face_still() -> None:
    paper = example("preliminary_base")
    point = Point.of(("7/8", "5/8"))
    paper.flex({UPRIGHT: 150, LEVEL: 150}, fixed=point)
    face = stack_at(paper.state, point)[0]
    form = paper.form
    assert form is not None
    assert form.motions[face] == pytest.approx(STILL)
    assert face not in paper.steps[-1].moved
    assert paper.steps[-1].moved
    assert len(paper.steps[-1].path) > 1


def test_a_flex_that_no_rigid_motion_reaches_is_refused() -> None:
    paper = example("pleated_fan")
    with pytest.raises(RefusedError) as caught:
        paper.flex(
            {Line.through((f"{k}/8", 0), (f"{k}/8", 1)): 120 for k in range(1, 8)}
        )
    assert caught.value.refusal.rule == "no-flex"


def test_a_flex_folds_no_crease_past_flat() -> None:
    paper = Paper.square()
    paper.valley(UPRIGHT, flap=("3/4", "1/2"))
    with pytest.raises(RefusedError) as caught:
        paper.flex({UPRIGHT: 190})
    assert caught.value.refusal.rule == "turns-through-paper"


def test_a_flex_turns_no_layer_into_the_layers_it_lies_on() -> None:
    # Fold in thirds: the left third over the middle, then the right third over both.
    paper = Paper.square()
    left = Line.through(("1/3", 0), ("1/3", 1))
    right = Line.through(("2/3", 0), ("2/3", 1))
    paper.valley(left, flap=("1/6", "1/2"))
    paper.valley(right, flap=("5/6", "1/2"))
    with pytest.raises(RefusedError) as caught:
        paper.flex({left: 90}, fixed=("1/2", "1/2"))
    assert caught.value.refusal.rule == "turns-through-paper"
    paper.flex({right: 90}, fixed=("1/2", "1/2"))
    assert paper.steps[-1].action == "flex"


def test_a_flex_needs_a_crease() -> None:
    paper = example("preliminary_base")
    with pytest.raises(RefusedError) as caught:
        paper.flex({Line.through(("1/3", 0), ("1/3", 1)): 90})
    assert caught.value.refusal.rule == "no-crease"


def test_faces_that_keep_their_places_stay_still_on_the_way() -> None:
    # The preliminary base's collapse keeps two faces in place, joined by a flat crease.
    paper = example("preliminary_base")
    before, after = paper.steps[-2].state, paper.steps[-1].state
    parents = before.parents(after)
    kept = [
        f
        for f, face in enumerate(after.faces)
        if face.place == before.faces[parents[f]].place
    ]
    assert len(kept) == 2
    forms = path(before, after, [k / 8 for k in range(8)])
    assert forms is not None
    for form in forms:
        for f in kept:
            assert form.motions[f] == pytest.approx(STILL, abs=1e-12)


def test_a_collapse_follows_a_whole_path_from_flat_paper() -> None:
    paper = example("preliminary_base")
    before, after = paper.steps[-2].state, paper.steps[-1].state
    forms = path(before, after, [k / 6 for k in range(6)])
    assert forms is not None
    assert len(forms) == 6
    for form in forms:
        assert_form_whole(after, form)
    # The path starts where the paper lay, flat on the table.
    start = forms[0]
    heights = [
        start.point(f, after.folded_point(f, v))[2]
        for f, face in enumerate(after.faces)
        for v in face.corners
    ]
    assert heights == pytest.approx([0] * len(heights), abs=1e-9)
