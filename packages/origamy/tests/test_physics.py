# Copyright 2026 Maxence Faldor
import itertools
import math
import runpy
from pathlib import Path

import numpy as np
import pytest
from hypothesis import given, settings
from hypothesis import strategies as st
from pydantic import ValidationError

from origamy import Line, Paper
from origamy.physics import Folding, Physics, PhysicsError, _mesh, simulate

EXAMPLES = Path(__file__).parents[3] / "examples"
UPRIGHT = Line.through(("1/2", 0), ("1/2", 1))
LEVEL = Line.through((0, "1/2"), (1, "1/2"))
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

    A corner may creep back where a crease wraps layers. No paper passes below the
    table on the way.
    """
    for stage in folding.stages:
        assert stage.converged, stage.step
        assert stage.misordered == (), stage.step
        assert stage.offset < 1e-4 + folding.creep, stage.step
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


def test_a_crease_bends_round_the_layer_it_wraps() -> None:
    # The bottom strip folds up, then the doubled strip folds up again. The second
    # crease wraps the strip, whose raw edge lies along it.
    paper = Paper.square()
    paper.valley(Line.through((0, "1/8"), (1, "1/8")), flap=("1/2", "1/16"))
    paper.valley(Line.through((0, "1/4"), (1, "1/4")), flap=("1/2", "3/16"))
    physics = Physics(increments=60)
    folding = simulate(paper.steps, physics)
    assert_folds_as_its_states(folding)
    # The crease bends over a band half a circle round the strip and itself.
    width = math.pi * LAYER
    assert folding.creep == pytest.approx(width)
    crease = physics.size / 4
    lines = physics.bands.lines
    for rank in range(-lines, lines + 1):
        offset = crease + rank * width / (2 * lines)
        assert np.isclose(folding.mesh.rest[:, 1], offset, atol=1e-9).sum() >= 2


def test_bands_cross_where_their_creases_cross() -> None:
    # Both middle lines of the square cross at its center, each with a band.
    paper = Paper.square()
    paper.precrease(UPRIGHT, flap=("3/4", "1/2"))
    paper.precrease(LEVEL, flap=("1/4", "3/4"))
    last = paper.steps[-1].state
    width = math.pi * LAYER
    mesh, _, lines = _mesh(
        last, PHYSICS.size, dict.fromkeys(last.creases, width), PHYSICS
    )
    # Each band's lines run on to the other band's crease, and the two bands' lines
    # meet wherever they cross.
    middle = PHYSICS.size / 2
    count = PHYSICS.bands.lines
    offsets = [rank * width / (2 * count) for rank in range(-count, count + 1) if rank]
    for across, up in itertools.product([0.0, *offsets], offsets):
        for point in ((middle + across, middle + up), (middle + up, middle + across)):
            assert np.isclose(mesh.rest, point, atol=1e-9).all(axis=1).any(), point
    assert len(lines) == 4 * 2 * count


def test_a_band_runs_to_the_paper_edge_however_sharp_the_corner() -> None:
    # The diagonal meets the square's edges at 45 degrees, at two corners.
    paper = Paper.square()
    paper.precrease(Line.through((0, 0), (1, 1)), flap=("3/4", "1/4"))
    last = paper.steps[-1].state
    width = math.pi * LAYER
    mesh, _, _ = _mesh(last, PHYSICS.size, dict.fromkeys(last.creases, width), PHYSICS)
    # Each line ends on the edge it meets, its offset times root two from the corner.
    count = PHYSICS.bands.lines
    size = PHYSICS.size
    for rank in range(1, count + 1):
        away = math.sqrt(2) * rank * width / (2 * count)
        for point in ((away, 0), (0, away), (size - away, size), (size, size - away)):
            assert np.isclose(mesh.rest, point, atol=1e-9).all(axis=1).any(), point


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
    [
        {"size": 0.0},
        {"increments": 0},
        {"margin": 0.0},
        {"margin": 4.0},
        {"bands": {"lines": 0}},
        {"bands": {"edge": 0.0}},
        {"bands": {"growth": 1.0}},
    ],
)
def test_physics_refuses_values_outside_its_range(change: dict[str, object]) -> None:
    with pytest.raises(ValidationError):
        Physics(**change)  # ty: ignore[invalid-argument-type]
