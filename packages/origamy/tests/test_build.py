# Copyright 2026 Maxence Faldor
import asyncio
import json
import shutil
from collections.abc import Callable
from pathlib import Path

import pytest

from origamy import (
    Line,
    Paper,
    Point,
    RefusedError,
    actions,
    bisect,
    fold_onto,
    service,
)
from origamy.geometry import PointLike
from origamy.layers import BudgetError, NoStackingError, Problem, solve
from origamy.state import State

EXAMPLES = Path(__file__).parents[3] / "examples"

type Colors = Callable[[Path], set[tuple[int, int, int]]]

REFUSED = """\
from origamy import Line, Paper


def fold():
    paper = Paper.square()
    paper.valley(Line.through(("1/2", 0), ("1/2", 1)), flap=("3/4", "1/2"))
    paper.valley(Line.through((2, 0), (2, 1)), flap=("1/4", "1/2"))
    return paper
"""


def build(script: Path, *, pictures: bool = False) -> service.BuildReport:
    request = service.BuildRequest(script=script, pictures=pictures)
    return asyncio.run(service.build(request))


def test_the_paper_records_each_step_and_its_line() -> None:
    paper = Paper.square()
    paper.valley(Line.through(("1/2", 0), ("1/2", 1)), flap=("3/4", "1/2"))
    paper.turn_over()
    assert [step.action for step in paper.steps] == ["start", "valley", "turn-over"]
    assert paper.steps[1].source is not None
    assert paper.steps[1].source.site.function == (
        "test_the_paper_records_each_step_and_its_line"
    )
    assert paper.where((1, 1)) == paper.where((0, 1))


def test_a_refused_action_keeps_the_steps_before_it() -> None:
    paper = Paper.square()
    with pytest.raises(RefusedError) as refused:
        paper.valley(Line.through((2, 0), (2, 1)), flap=("1/2", "1/2"))
    assert refused.value.paper is paper
    assert refused.value.refusal.rule == "no-hinge"
    assert len(paper.steps) == 1


def test_an_unreferenced_fold_is_a_warning() -> None:
    paper = Paper.square()
    paper.valley(Line.through(("3/10", 0), ("3/10", 1)), flap=("1/10", "1/2"))
    assert [d.rule for d in paper.steps[-1].diagnostics] == ["unreferenced-fold"]
    paper.valley(fold_onto((1, 0), ("3/10", 0)), flap=("9/10", "1/2"))
    assert not paper.steps[-1].diagnostics


def test_build_reports_steps_and_writes_a_fold_file(tmp_path: Path) -> None:
    script = Path(shutil.copy(EXAMPLES / "cup.py", tmp_path))
    report = build(script)
    assert report.ok, report.text
    assert report.steps == 9
    assert report.text.startswith("cup.py: 9 steps folded")
    document = json.loads((tmp_path / "cup.fold").read_text(encoding="utf-8"))
    assert document["file_spec"] == 1.2
    assert len(document["file_frames"]) == 10
    assert set(document["edges_assignment"]) <= {"M", "V", "F", "B"}


def test_build_reports_a_refusal_at_its_line(tmp_path: Path) -> None:
    script = tmp_path / "broken.py"
    script.write_text(REFUSED, encoding="utf-8")
    report = build(script)
    assert not report.ok
    assert report.refused is not None
    assert report.refused.step == 2
    assert report.refused.source is not None
    assert report.refused.source.site.line == 7
    assert "error[no-hinge] step 2" in report.text


def test_build_reports_a_failing_script(tmp_path: Path) -> None:
    script = tmp_path / "failing.py"
    script.write_text("def fold():\n    return 1 / 0\n", encoding="utf-8")
    report = build(script)
    assert report.failure is not None
    assert report.failure.error == "ZeroDivisionError"
    assert report.failure.frames[0].line == 2


def test_explain_lists_every_rule() -> None:
    text = asyncio.run(service.explain(service.ExplainRequest())).text
    assert "layer-crossing (error, refuses the action)" in text
    assert "unreferenced-fold (warn)" in text


def test_the_solver_names_the_constraints_that_conflict() -> None:
    paper = Paper.square()
    paper.valley(Line.through(("1/2", 0), ("1/2", 1)), flap=("3/4", "1/2"))
    paper.valley(Line.through((0, "1/2"), (1, "1/2")), flap=("1/4", "3/4"))
    state = paper.state
    upper, lower = next(iter(state.above))
    with pytest.raises(NoStackingError) as error:
        solve(Problem(state), [(upper, lower), (lower, upper)])
    assert error.value.fixed
    folded_in_four = State(state.vertices, state.faces)
    stacks = solve(Problem(folded_in_four), sorted(state.above))
    assert stacks == state.above


def test_pictures_have_the_paper_in_them(tmp_path: Path, colors: Colors) -> None:
    script = Path(shutil.copy(EXAMPLES / "cup.py", tmp_path))
    report = build(script, pictures=True)
    assert report.ok, report.text
    assert (200, 85, 61) in colors(tmp_path / "cup.png")  # the front, folded
    assert (200, 85, 61) in colors(tmp_path / "cup.steps.png")


def reverse_fold(towards: PointLike) -> Paper:
    """A triangle whose tip at (1, 1) turns inside, its fold line found by bisecting."""
    paper = Paper.square()
    spine = Line.through((0, 0), (1, 1))
    paper.valley(spine, flap=("1/4", "3/4"))
    line = bisect((1, 1), ("3/4", "3/4"), towards)
    paper.precrease(line, flap=("31/32", "29/32"))
    tip = [crease for crease in paper.creases(spine) if Point.of((1, 1)) in crease]
    paper.collapse(valleys=paper.creases(line), mountains=tip, fixed=("1/2", "1/4"))
    return paper


def test_an_inside_reverse_fold_is_a_collapse() -> None:
    paper = reverse_fold(("3/4", 0))
    assert paper.steps[-1].action == "collapse"
    assert paper.layers(("13/16", "5/8")) == 4
    assert paper.where((1, 1)) == Point.of(("3/4", "3/4 - √2/4"))


def test_a_tip_turned_through_the_spine_is_refused() -> None:
    with pytest.raises(RefusedError) as refused:
        reverse_fold((0, "3/4"))
    diagnostic = refused.value.refusal
    assert diagnostic.rule == "layer-crossing"
    assert "taco-tortilla" in diagnostic.message


def test_a_collapse_along_no_crease_is_refused() -> None:
    paper = Paper.square()
    with pytest.raises(RefusedError) as refused:
        paper.collapse(valleys=[Line.through(("1/2", 0), ("1/2", 1))])
    assert refused.value.refusal.rule == "no-crease"


def triangle() -> Paper:
    """The square folded along its diagonal, its tip at (1, 1)."""
    paper = Paper.square()
    paper.valley(Line.through((0, 0), (1, 1)), flap=("1/4", "3/4"))
    return paper


@pytest.mark.parametrize(
    ("towards", "outside", "tip"),
    [
        (("3/4", 0), False, ("3/4", "3/4 - √2/4")),
        ((0, "3/4"), True, ("3/4 - √2/4", "3/4")),
    ],
)
def test_reverse_folds_turn_the_tip_inside_out(
    towards: PointLike, *, outside: bool, tip: PointLike
) -> None:
    paper = triangle()
    line = bisect((1, 1), ("3/4", "3/4"), towards)
    paper.reverse(line, flap=("63/64", "61/64"), outside=outside)
    assert paper.steps[-1].action == "reverse"
    assert paper.where((1, 1)) == Point.of(tip)


def test_an_inside_reverse_fold_out_through_the_spine_is_refused() -> None:
    paper = triangle()
    line = bisect((1, 1), ("3/4", "3/4"), (0, "3/4"))
    with pytest.raises(RefusedError) as refused:
        paper.reverse(line, flap=("63/64", "61/64"))
    step = refused.value.refused
    assert refused.value.refusal.rule == "layer-crossing"
    assert max(refused.value.refusal.faces) < len(step.state.faces)


def test_a_reverse_fold_that_could_open_in_two_places_is_refused() -> None:
    paper = Paper.square()
    paper.valley(Line.through(("1/3", 0), ("1/3", 1)), flap=("1/6", "1/2"))
    paper.mountain(Line.through(("2/3", 0), ("2/3", 1)), flap=("5/6", "1/2"))
    with pytest.raises(RefusedError) as refused:
        paper.reverse(Line.through((0, "3/4"), (1, "3/4")), flap=("1/2", "7/8"))
    assert refused.value.refusal.rule == "several-spines"


def test_a_reverse_fold_needs_a_spine() -> None:
    paper = Paper.square()
    with pytest.raises(RefusedError) as refused:
        paper.reverse(Line.through(("1/2", 0), ("1/2", 1)), flap=("3/4", "1/2"))
    step = refused.value.refused
    assert refused.value.refusal.rule == "no-spine"
    assert len(step.state.faces) == 2  # the paper as the move cut it


def test_petal_folds_make_the_bird_base(tmp_path: Path) -> None:
    script = Path(shutil.copy(EXAMPLES / "bird_base.py", tmp_path))
    report = build(script)
    assert report.ok, report.text
    assert report.steps == 15
    document = json.loads((tmp_path / "bird_base.fold").read_text(encoding="utf-8"))
    kinds = document["edges_assignment"]
    assert (kinds.count("M"), kinds.count("V")) == (16, 4)


def test_a_petal_fold_needs_folded_sides() -> None:
    paper = Paper.square()
    with pytest.raises(RefusedError) as refused:
        paper.petal(Line.through((0, "1/2"), (1, "1/2")), flap=("1/2", "1/4"))
    assert refused.value.refusal.rule == "no-crease"


def test_a_petal_fold_needs_a_folded_side() -> None:
    paper = Paper.square()
    middle = Line.through((0, "1/2"), (1, "1/2"))
    paper.precrease(middle, flap=("1/2", "3/4"))
    with pytest.raises(RefusedError) as refused:
        paper.petal(middle, flap=("1/2", "1/4"))
    assert refused.value.refusal.rule == "no-side"


def test_a_search_out_of_budget_is_refused(monkeypatch: pytest.MonkeyPatch) -> None:
    def exhausted(*_: object, **__: object) -> frozenset[tuple[int, int]]:
        msg = "the search reached its budget of 0 dead ends"
        raise BudgetError(msg)

    monkeypatch.setattr(actions, "solve", exhausted)
    paper = Paper.square()
    with pytest.raises(RefusedError) as refused:
        paper.valley(Line.through(("1/2", 0), ("1/2", 1)), flap=("3/4", "1/2"))
    assert refused.value.refusal.rule == "layer-order-unknown"
