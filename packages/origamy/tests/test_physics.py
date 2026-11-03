# Copyright 2026 Maxence Faldor
import runpy
from pathlib import Path

import numpy as np
import pytest
from hypothesis import given, settings
from hypothesis import strategies as st
from pydantic import ValidationError

from origamy import Line, Paper
from origamy.physics import Folding, Physics, PhysicsError, simulate

EXAMPLES = Path(__file__).parents[3] / "examples"
UPRIGHT = Line.through(("1/2", 0), ("1/2", 1))
PHYSICS = Physics()
# The heights at which paper lies on the table: its half thickness, and a layer.
LYING = PHYSICS.material.thickness / 2
LAYER = PHYSICS.material.thickness + PHYSICS.settings.contact.gap


def example(name: str) -> Paper:
    """An example's paper, folded."""
    paper = runpy.run_path(str(EXAMPLES / f"{name}.py"))["fold"]()
    assert isinstance(paper, Paper)
    return paper


def assert_folds_as_its_states(folding: Folding) -> None:
    """Every fold converges, puts each corner at its place and stacks every pair right.

    No paper passes below the table on the way.
    """
    for stage in folding.stages:
        assert stage.converged, stage.step
        assert stage.misordered == (), stage.step
        assert stage.offset < 1e-4, stage.step
        lowest = min(frame[:, 2].min() for frame in stage.frames)
        assert lowest > LYING - 1e-6, stage.step


@pytest.mark.parametrize(
    ("name", "folds", "layers"),
    [("preliminary_base", 9, 4), ("waterbomb_base", 7, 4), ("window", 12, 3)],
)
def test_a_model_folds_from_the_flat_sheet_in_its_stacking(
    name: str, folds: int, layers: int
) -> None:
    # Each precrease folds and unfolds, and each other step folds once.
    folding = simulate(example(name).steps)
    assert len(folding.stages) == folds
    assert_folds_as_its_states(folding)
    # The model lies flat on the table, its layers stacked one thickness apart.
    heights = folding.stages[-1].frames[-1][:, 2]
    assert heights.max() < LYING + layers * LAYER + 1e-3


def test_a_mountain_fold_folds_up_from_the_turned_over_sheet() -> None:
    paper = Paper.square()
    paper.mountain(UPRIGHT, flap=("3/4", "1/2"))
    folding = simulate(paper.steps)
    assert_folds_as_its_states(folding)
    # The sheet turns over first, so its front faces the table.
    (stage,) = folding.stages
    turned = stage.frames[0]
    corners = turned[folding.mesh.triangles[0]]
    normal = np.cross(corners[1] - corners[0], corners[2] - corners[0])
    assert normal[2] < 0


def test_turning_over_turns_the_simulated_paper_over() -> None:
    paper = Paper.square()
    paper.valley(UPRIGHT, flap=("3/4", "1/2"))
    paper.turn_over()
    folding = simulate(paper.steps)
    assert_folds_as_its_states(folding)
    folded, turned = (stage.frames[-1] for stage in folding.stages)
    assert len(folding.stages[-1].frames) == 1
    assert turned[:, 2].min() == pytest.approx(folded[:, 2].min())
    assert turned[:, 2].max() == pytest.approx(folded[:, 2].max())


@given(
    st.integers(2, 14),
    st.integers(2, 14),
    st.booleans(),
)
@settings(max_examples=6, deadline=None)
def test_a_fold_across_the_square_folds_physically(
    left: int,
    right: int,
    mountain: bool,  # noqa: FBT001 - drawn by Hypothesis
) -> None:
    # The line runs across the square, under the middle of its top edge.
    paper = Paper.square()
    line = Line.through((0, f"{left}/16"), (1, f"{right}/16"))
    if mountain:
        paper.mountain(line, flap=("1/2", "15/16"))
    else:
        paper.valley(line, flap=("1/2", "15/16"))
    assert_folds_as_its_states(simulate(paper.steps))


def test_paper_turned_both_up_and_down_from_held_paper_is_refused() -> None:
    # The middle strip stays; the left strip folds up over it and the right one under.
    paper = Paper.square()
    third, two_thirds = (Line.through((x, 0), (x, 1)) for x in ("1/3", "2/3"))
    paper.precrease(third, flap=("1/6", "1/2"))
    paper.precrease(two_thirds, flap=("5/6", "1/2"))
    paper.collapse(valleys=[third], mountains=[two_thirds], fixed=("1/2", "1/2"))
    with pytest.raises(PhysicsError, match="step 3 turns paper both up"):
        simulate(paper.steps)


def test_paper_shaped_in_space_is_refused() -> None:
    paper = Paper.square()
    paper.shape(UPRIGHT, flap=("3/4", "1/2"), angle=90)
    with pytest.raises(PhysicsError, match="step 1 shapes the paper in space"):
        simulate(paper.steps)


@pytest.mark.parametrize(
    "change",
    [{"size": 0.0}, {"increments": 0}, {"margin": 0.0}, {"margin": 4.0}],
)
def test_physics_refuses_values_outside_its_range(change: dict[str, float]) -> None:
    with pytest.raises(ValidationError):
        Physics(**change)  # ty: ignore[invalid-argument-type]
