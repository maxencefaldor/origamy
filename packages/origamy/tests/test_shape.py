# Copyright 2026 Maxence Faldor
import asyncio
import json
import re
import runpy
import shutil
from collections.abc import Callable
from math import isclose
from pathlib import Path

import pytest

from origamy import Line, Paper, Point, service
from origamy.draw import Eye, layers
from origamy.paper import RefusedError
from origamy.space import Form, Shaped, apply, fold_angles, same

EXAMPLES = Path(__file__).parents[3] / "examples"
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


def test_a_tilt_turns_the_whole_paper() -> None:
    paper = Paper.square()
    paper.tilt(Line.through((0, 0), (1, 0)), flap=("1/2", "1/2"), angle=90)
    form = paper.form
    assert form is not None
    assert form.point(0, Point.of((1, 1))) == pytest.approx((1, 0, 1))
    assert paper.steps[-1].moved == {0}
    with pytest.raises(RefusedError) as caught:
        paper.tilt(Line.through((0, 0), (1, 0)), flap=("1/2", 0), angle=90)
    assert caught.value.refusal.rule == "flap-on-line"


def test_a_turned_flap_meets_the_layer_it_turned_from_in_pictures() -> None:
    folded = runpy.run_path(str(EXAMPLES / "flapping_bird.py"))["fold"]()
    state, form = folded.state, folded.form
    parted = layers(Shaped(state, form))

    def where(face: int, vertex: int) -> tuple[float, ...]:
        flat = (*state.folded_point(face, vertex).floats(), parted.heights[face])
        lifted = apply(form.motions[face], flat)
        return tuple(c + s for c, s in zip(lifted, parted.shifts[face], strict=True))

    # Heights count in gaps between layers here, so a gap is one unit.
    apart = [
        max(abs(a - b) for a, b in zip(where(f, v), where(g, v), strict=True))
        for edge, (f, g) in state.creases.items()
        if not same(form.motions[f], form.motions[g])
        for v in edge
    ]
    assert apart
    assert max(apart) < 3
    assert max(parted.heights) > 5


@pytest.fixture(scope="module")
def plane(tmp_path_factory: pytest.TempPathFactory) -> Path:
    """The paper plane built, its last steps spreading its wings: its script."""
    folder = tmp_path_factory.mktemp("plane")
    script = Path(shutil.copy(EXAMPLES / "paper_plane.py", folder))
    report = asyncio.run(service.build(service.BuildRequest(script=script)))
    assert report.ok, report.text
    return script


def test_a_build_exports_shaped_paper_in_3d(plane: Path) -> None:
    document = json.loads(plane.with_suffix(".fold").read_text(encoding="utf-8"))
    last = document["file_frames"][-1]
    assert last["frame_attributes"] == ["3D"]
    assert min(z for _, _, z in last["vertices_coords"]) < -0.2
    assert {round(a) for a in last["edges_foldAngle"]} >= {90, -90}
    assert document["file_frames"][0]["frame_attributes"] == ["2D"]
    page = plane.with_suffix(".html").read_text(encoding="utf-8")
    found = re.search(r'id="build">(.*?)</script>', page, re.DOTALL)
    assert found is not None
    shaped = [s for s in json.loads(found.group(1))["steps"] if s["action"] == "shape"]
    assert len(shaped) == 2
    assert all(s["turn"] is not None and s["motions"] for s in shaped)


def test_pictures_draw_shaped_paper_in_space(
    plane: Path, colors: Callable[[Path], set[tuple[int, int, int]]]
) -> None:
    folded = runpy.run_path(str(EXAMPLES / "paper_plane.py"))["fold"]()
    eye = Eye.facing(Shaped(folded.state, folded.form))
    # The body stands upright, showing the eye its blue front, shaded as it turns away.
    share = 1 - 0.3 * (1 - abs(eye.toward[0]))
    shade = [c * share for c in (44, 111, 187)]
    assert any(
        all(abs(a - b) <= 2 for a, b in zip(color, shade, strict=True))
        for color in colors(plane.with_suffix(".png"))
    )
