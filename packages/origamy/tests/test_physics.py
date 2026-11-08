# Copyright 2026 Maxence Faldor
import itertools
import math
import runpy
from fractions import Fraction
from pathlib import Path

import numpy as np
import pytest
from hypothesis import given, settings
from hypothesis import strategies as st
from pydantic import ValidationError

from origamy import Line, Paper
from origamy.geometry import HALF
from origamy.physics import (
    Folding,
    Physics,
    _mesh,
    _widths,
    crossing,
    simulate,
)
from origamy.state import Edge, State

EXAMPLES = Path(__file__).parents[3] / "examples"
UPRIGHT = Line.through(("1/2", 0), ("1/2", 1))
LEVEL = Line.through((0, "1/2"), (1, "1/2"))
PHYSICS = Physics()
# The heights at which paper lies on the table: its half thickness, and a layer.
LYING = PHYSICS.material.thickness / 2
LAYER = PHYSICS.material.thickness + PHYSICS.settings.contact.gap
# The deepest simulated paper may pass through paper: a few sheets of kami.
CROSSING = 1e-4


def example(name: str) -> Paper:
    """An example's paper, folded."""
    paper = runpy.run_path(str(EXAMPLES / f"{name}.py"))["fold"]()
    assert isinstance(paper, Paper)
    return paper


def assert_folds_as_its_states(folding: Folding) -> None:
    """Every fold converges and ends in its state.

    Each corner lies at its place, every pair stacks right, and no paper passes through
    paper. A corner may creep back where a crease wraps layers. No paper passes below
    the table on the way.
    """
    for stage in folding.stages:
        assert stage.converged, stage.step
        assert stage.misordered == (), stage.step
        assert stage.crossing < CROSSING, stage.step
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


def test_crossing_measures_paper_through_paper() -> None:
    # A triangle on the table, and another standing up through it, 0.1 below it.
    lying = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]
    standing = [(0.2, 0.2, -0.1), (0.2, 0.2, 0.3), (0.6, 0.6, 0.3)]
    points = np.array(lying + standing, dtype=np.float64)
    triangles = np.array([[0, 1, 2], [3, 4, 5]], dtype=np.int64)
    assert crossing(points, triangles) == pytest.approx(0.1)
    # Laid on the first, the second only touches it.
    points[3:] = [(0.2, 0.2, 1e-9), (0.6, 0.2, 1e-9), (0.2, 0.6, 1e-9)]
    assert crossing(points, triangles) == 0.0


def test_triangles_that_share_a_corner_never_cross() -> None:
    # Two triangles folded flat onto each other about their shared edge.
    points = np.array([(0, 0, 0), (1, 0, 0), (0, 1, 0), (0, 1, 1e-3)], dtype=float)
    triangles = np.array([[0, 1, 2], [0, 1, 3]], dtype=np.int64)
    assert crossing(points, triangles) == 0.0


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


def test_without_contact_between_layers_creases_fold_sharp_through_the_layers() -> None:
    # The strip folds up twice, as in the band test, on paper without thickness.
    paper = Paper.square()
    paper.valley(Line.through((0, "1/8"), (1, "1/8")), flap=("1/2", "1/16"))
    paper.valley(Line.through((0, "1/4"), (1, "1/4")), flap=("1/2", "3/16"))
    contact = PHYSICS.settings.contact.model_copy(update={"layers": False})
    settings = PHYSICS.settings.model_copy(update={"contact": contact})
    physics = Physics(settings=settings)
    folding = simulate(paper.steps, physics)
    assert_folds_as_its_states(folding)
    # No crease bends round a layer, so no corner creeps back.
    assert folding.creep == 0
    crease = physics.size / 4
    assert np.isclose(folding.mesh.rest[:, 1], crease, atol=1e-9).sum() >= 2
    beside = np.abs(folding.mesh.rest[:, 1] - crease)
    assert not ((beside > 1e-9) & (beside < 1e-3)).any()


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


def test_a_band_meets_at_its_end_where_a_sharp_crease_meets_it() -> None:
    # The middle lines cross at the square's center, and only the level one's left half
    # has a band.
    paper = Paper.square()
    paper.precrease(UPRIGHT, flap=("3/4", "1/2"))
    paper.precrease(LEVEL, flap=("1/4", "3/4"))
    last = paper.steps[-1].state
    left = {edge: math.pi * LAYER for edge in last.creases if _level(last, edge, "1/4")}
    mesh, _, lines = _mesh(last, PHYSICS.size, left, PHYSICS)
    # Every line reaches the center, and no other point lies near the sharp creases
    # there.
    middle = np.array([PHYSICS.size / 2] * 2)
    distances = np.linalg.norm(mesh.rest - middle, axis=1)
    assert all(distances[line.chain].min() < 1e-9 for line in lines)
    assert np.count_nonzero(distances < PHYSICS.bands.edge / 2) == 1


def test_a_band_runs_on_through_a_sharp_crease_that_alone_crosses_it() -> None:
    # Both halves of the level line share a band, and only the sharp upright crosses it.
    paper = Paper.square()
    paper.precrease(UPRIGHT, flap=("3/4", "1/2"))
    paper.precrease(LEVEL, flap=("1/4", "3/4"))
    last = paper.steps[-1].state
    width = math.pi * LAYER
    level = {edge: width for edge in last.creases if _level(last, edge)}
    mesh, _, _ = _mesh(last, PHYSICS.size, level, PHYSICS)
    # The band's lines cross the upright crease, which they put points on.
    middle = PHYSICS.size / 2
    count = PHYSICS.bands.lines
    for rank in (*range(-count, 0), *range(1, count + 1)):
        point = (middle, middle + rank * width / (2 * count))
        assert np.isclose(mesh.rest, point, atol=1e-9).all(axis=1).any(), point


def test_a_band_meets_at_its_end_where_two_sharp_creases_cross_it() -> None:
    # Both diagonals cross the level band at the center, and stay sharp.
    paper = Paper.square()
    paper.precrease(Line.through((0, 0), (1, 1)), flap=("3/4", "1/4"))
    paper.precrease(Line.through((1, 0), (0, 1)), flap=("1/4", "1/8"))
    paper.precrease(LEVEL, flap=("1/8", "3/4"))
    last = paper.steps[-1].state
    level = {edge: math.pi * LAYER for edge in last.creases if _level(last, edge)}
    mesh, _, lines = _mesh(last, PHYSICS.size, level, PHYSICS)
    # Points the band put beside one diagonal would stop it folding flat, so every
    # line meets the center.
    middle = np.array([PHYSICS.size / 2] * 2)
    distances = np.linalg.norm(mesh.rest - middle, axis=1)
    assert all(distances[line.chain].min() < 1e-9 for line in lines)


def _level(state: State, edge: Edge, near: str | None = None) -> bool:
    """Whether a crease of a state runs level, across ``x = near`` if given."""
    a, b = (state.vertices[v] for v in edge)
    return a.y == b.y and (
        near is None or min(a.x, b.x) < Fraction(near) < max(a.x, b.x)
    )


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


def test_creases_a_fold_turns_in_one_line_share_one_band() -> None:
    # A strip folds up over a precrease, then the paper folds in half across both.
    paper = Paper.square()
    paper.precrease(Line.through((0, "1/4"), (1, "1/4")), flap=("1/2", "1/8"))
    paper.valley(Line.through((0, "1/8"), (1, "1/8")), flap=("1/2", "1/16"))
    paper.valley(UPRIGHT, flap=("3/4", "1/2"))
    folds = [
        (before, after)
        for before, step in itertools.pairwise(paper.steps)
        for before, after in itertools.pairwise(
            (before.state, *((step.through,) if step.through else ()), step.state)
        )
    ]
    last = paper.steps[-1].state
    widths = _widths(folds, last, LAYER)
    on = {
        tuple(sorted(float(last.vertices[v].y) for v in edge)): edge
        for edge in last.creases
        if all(last.vertices[v].x == HALF for v in edge)
    }
    # The sheet's crease wraps the strip where the strip lies over it, and turns on as
    # one crease past the precrease. The strip's own crease lies inside, sharp.
    wrapping = 1.5 * math.pi * LAYER
    assert widths[on[0.125, 0.25]] == pytest.approx(wrapping)
    assert widths[on[0.25, 1.0]] == pytest.approx(wrapping)
    assert on[0.0, 0.125] not in widths


def test_paper_turned_both_up_and_down_folds_lifted_off_the_table() -> None:
    # The middle strip stays; the left strip folds up over it and the right one under.
    paper = Paper.square()
    third, two_thirds = (Line.through((x, 0), (x, 1)) for x in ("1/3", "2/3"))
    paper.precrease(third, flap=("1/6", "1/2"))
    paper.precrease(two_thirds, flap=("5/6", "1/2"))
    paper.collapse(valleys=[third], mountains=[two_thirds], fixed=("1/2", "1/2"))
    folding = simulate(paper.steps)
    assert_folds_as_its_states(folding)
    # The hands lift the model by the paper's size, then lay it back on the table.
    stage = folding.stages[-1]
    assert stage.frames[0][:, 2].min() > LYING + PHYSICS.size - 1e-6
    heights = stage.frames[-1][:, 2]
    assert heights.min() < LYING + PHYSICS.settings.contact.gap
    assert heights.max() < LYING + 3 * LAYER + 1e-3


def test_shaping_stands_a_flap_up_in_space() -> None:
    # The square folds in half, then its top layer turns up square to the table.
    paper = Paper.square()
    paper.valley(UPRIGHT, flap=("3/4", "1/2"))
    paper.shape(UPRIGHT, flap=("1/4", "1/2"), angle=90, layers=1)
    folding = simulate(paper.steps)
    shaped = folding.stages[-1]
    assert shaped.form is not None
    assert shaped.converged
    # The flap stands half the square's side high, and keeps its shape within 1 mm.
    top = shaped.frames[-1][:, 2].max()
    assert top == pytest.approx(PHYSICS.size / 2, abs=2e-3)
    assert shaped.offset < 1e-3


def test_a_tilt_stands_the_whole_paper_up_on_the_table() -> None:
    paper = Paper.square()
    paper.valley(UPRIGHT, flap=("3/4", "1/2"))
    paper.tilt(Line.through((0, 0), (1, 0)), flap=("1/4", "1/2"), angle=90)
    stage = simulate(paper.steps).stages[-1]
    assert stage.form is not None
    heights = stage.frames[-1][:, 2]
    assert heights.min() == pytest.approx(LYING, abs=PHYSICS.settings.contact.gap)
    assert heights.max() == pytest.approx(LYING + PHYSICS.size, abs=1e-3)
    assert stage.offset < 1e-4


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
