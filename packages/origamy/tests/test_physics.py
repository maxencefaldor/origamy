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
    _extents,
    _Folder,
    _mesh,
    crossing,
    simulate,
)
from origamy.state import State

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

    Every vertex beside the bands lies at its place, every pair stacks right, and no
    paper passes through paper. A vertex may creep back by twice the widest band, all
    its paper folded back. No paper passes below the table on the way.
    """
    for stage in folding.stages:
        assert stage.converged, stage.step
        assert stage.misordered == (), stage.step
        assert stage.crossing < CROSSING, stage.step
        assert stage.offset < 1e-4 + folding.creep, stage.step
        lowest = min(frame[:, 2].min() for frame in stage.frames)
        assert lowest > LYING - 1e-6, stage.step


# The collapses land within half a millimeter of each state, but some of their
# increments stop at the iteration limit, minutes into the fold.
COLLAPSING = "some increments of the collapse stop at the iteration limit"


@pytest.mark.parametrize(
    ("name", "folds", "layers"),
    [
        ("kite_base", 4, 2),
        ("blintz_base", 4, 2),
        pytest.param(
            "preliminary_base",
            9,
            4,
            marks=[pytest.mark.slow, pytest.mark.xfail(reason=COLLAPSING)],
        ),
        pytest.param(
            "waterbomb_base",
            7,
            4,
            marks=[pytest.mark.slow, pytest.mark.xfail(reason=COLLAPSING)],
        ),
    ],
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
    # The crease bends over a band twice half a circle round the strip and itself. The
    # band reaches below the crease, into the doubled strip the fold turns up.
    width = physics.bands.slack * math.pi * LAYER
    assert folding.creep == pytest.approx(2 * width)
    crease = physics.size / 4
    count = 2 * physics.bands.lines
    for rank in range(count + 1):
        offset = crease - rank * width / count
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


def test_bands_stop_round_a_vertex_where_the_paper_is_cut_away() -> None:
    # Both middle lines of the square cross at its center, each folded in turn.
    paper = Paper.square()
    paper.precrease(UPRIGHT, flap=("3/4", "1/2"))
    paper.precrease(LEVEL, flap=("1/4", "3/4"))
    last = paper.steps[-1].state
    extents = _extents(folds(paper), last, LAYER, PHYSICS.bands.slack)
    mesh, _, lines = _mesh(last, PHYSICS.size, extents, PHYSICS)
    # No paper lies at the center, where the bands meet.
    middle = np.array([PHYSICS.size / 2] * 2)
    corners = mesh.rest[mesh.triangles]
    assert not any(_holds(triangle, middle) for triangle in corners)
    # Each band's lines stop short of the center, and no line crosses another band.
    width = max(extents.values())
    for line in lines:
        ends = mesh.rest[[line.chain[0], line.chain[-1]]]
        assert np.linalg.norm(ends - middle, axis=1).min() > width - 1e-9
    # The hole is no wider than the bands that meet there.
    distances = np.linalg.norm(mesh.rest - middle, axis=1)
    assert distances.min() < 2 * width


def _holds(triangle: np.ndarray, point: np.ndarray) -> bool:
    """Whether a triangle of the plane holds a point inside it."""
    a, b, c = triangle
    sides = [
        (q[0] - p[0]) * (point[1] - p[1]) - (q[1] - p[1]) * (point[0] - p[0])
        for p, q in ((a, b), (b, c), (c, a))
    ]
    return all(side > 0 for side in sides) or all(side < 0 for side in sides)


def test_a_band_reaches_into_the_face_its_fold_turns_over() -> None:
    # The right half folds over the left one.
    paper = Paper.square()
    paper.valley(UPRIGHT, flap=("3/4", "1/2"))
    last = paper.steps[-1].state
    extents = _extents(folds(paper), last, LAYER, PHYSICS.bands.slack)
    ((edge, face), width) = next(iter(extents.items()))
    assert len(extents) == 1
    # The band reaches into the right half alone, twice half a circle a layer across.
    corners = [last.vertices[c] for c in last.faces[face].corners]
    assert all(corner.x >= HALF for corner in corners)
    assert width == pytest.approx(PHYSICS.bands.slack * math.pi / 2 * LAYER)
    assert face in last.creases[edge]


def test_each_layer_of_a_fold_curls_inside_the_next() -> None:
    # The square folds in half, then in half again through both layers.
    paper = Paper.square()
    paper.valley(UPRIGHT, flap=("3/4", "1/2"))
    paper.valley(LEVEL, flap=("1/4", "3/4"))
    last = paper.steps[-1].state
    extents = _extents(folds(paper), last, LAYER, PHYSICS.bands.slack)
    # The second fold wraps the inner layer's crease in the outer one's, round two
    # layers, so the outer layer's band is three times as wide.
    level = sorted(
        extent
        for (edge, _), extent in extents.items()
        if all(last.vertices[v].y == HALF for v in edge)
    )
    half_circle = PHYSICS.bands.slack * math.pi / 2 * LAYER
    assert level == pytest.approx([half_circle, 3 * half_circle])


def test_a_diagonal_band_stays_on_the_paper_into_its_sharp_corners() -> None:
    # The diagonal meets the square's edges at 45 degrees, at two corners.
    paper = Paper.square()
    paper.precrease(Line.through((0, 0), (1, 1)), flap=("3/4", "1/4"))
    last = paper.steps[-1].state
    extents = _extents(folds(paper), last, LAYER, PHYSICS.bands.slack)
    mesh, _, lines = _mesh(last, PHYSICS.size, extents, PHYSICS)
    # Every point of the mesh lies on the square, and the band's lines all run between
    # the holes in its corners.
    assert (mesh.rest >= -1e-12).all()
    assert (mesh.rest <= PHYSICS.size + 1e-12).all()
    assert len(lines) == 2 * PHYSICS.bands.lines


def test_a_crease_wraps_the_most_layers_anywhere_along_it() -> None:
    # The newspaper hat's front brim rolls up twice over its corners, folded down.
    paper = Paper(1, 2)
    paper.valley(Line.through((0, 1), (1, 1)), flap=("1/2", "7/4"))
    top = ("1/2", 1)
    paper.valley(Line.through(top, (0, "1/2")), flap=("1/16", "15/16"))
    paper.valley(Line.through(top, (1, "1/2")), flap=("15/16", "15/16"))
    low, high = (Line.through((0, y), (1, y)) for y in ("1/4", "1/2"))
    paper.valley(low, flap=("1/2", "1/8"), layers=1)
    paper.valley(high, flap=("1/2", "3/8"), layers=2)
    last = paper.steps[-1].state
    extents = _extents(folds(paper), last, LAYER, PHYSICS.bands.slack)
    (brim,) = (
        edge
        for edge in last.creases
        if all(last.vertices[v].y == Fraction(3, 2) for v in edge)
    )
    # The corners meet over the middle of the brim's second crease. Beside it, the
    # crease wraps both layers of a corner and the strip the first roll turned in.
    width = sum(extent for (edge, _), extent in extents.items() if edge == brim)
    assert width == pytest.approx(PHYSICS.bands.slack * 2 * math.pi * LAYER)


def test_every_span_turns_the_paper_across_its_band() -> None:
    # The swan's neck folds where bands meet at a point, whose far lines there lie
    # beside the near ones.
    paper = example("swan")
    folder = _Folder(paper.steps[-1].state, folds(paper), PHYSICS.size, PHYSICS)
    # The core takes each span only with a direction across the band from its edge.
    assert len(folder.spans) > 0


def folds(paper: Paper) -> list[tuple[State, State]]:
    """Each fold of a paper's steps: from one state to the next it passes through."""
    return [
        (before, after)
        for before, step in itertools.pairwise(paper.steps)
        for before, after in itertools.pairwise(
            (before.state, *((step.through,) if step.through else ()), step.state)
        )
    ]


@pytest.mark.xfail(
    reason="each crease rests 0.01 rad short of flat, and the strips stand 1.6 mm high"
)
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
    folding = simulate(paper.steps)
    stage = folding.stages[-1]
    assert stage.form is not None
    heights = stage.frames[-1][:, 2]
    assert heights.min() == pytest.approx(LYING, abs=PHYSICS.settings.contact.gap)
    assert heights.max() == pytest.approx(LYING + PHYSICS.size, abs=1e-3)
    # The tilt carries the folded paper whole, as far off its state as the fold left it.
    assert stage.offset < 1e-4 + folding.creep


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
