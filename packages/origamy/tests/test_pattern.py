# Copyright 2026 Maxence Faldor
import asyncio
import json
import shutil
from fractions import Fraction
from pathlib import Path

import pytest

from origamy import CreasePattern, Line, Number, Paper, Point, RefusedError, service
from origamy.geometry import PointLike
from origamy.pattern import snap

EXAMPLES = Path(__file__).parents[3] / "examples"

PRELIMINARY = """\
1 -200 -200 200 -200
1 200 -200 200 200
1 200 200 -200 200
1 -200 200 -200 -200
2 200 -200 -200 200
3 0 -200 0 200
3 -200 0 200 0
4 -100 -200 -100 200
"""
"""A preliminary base in ORIPA's format: its edges, a mountain diagonal, two valley
middle lines and an aid line. The other diagonal stays flat inside the flaps."""


def test_coordinates_snap_to_the_simplest_exact_number() -> None:
    assert snap(0.5) == Number.of("1/2")
    assert snap(0.3333333333) == Number.of(Fraction(1, 3))
    assert snap(1 - 2**0.5 / 2) == Number.of("1 - √2/2")
    assert snap(3**0.5 / 4) == Number.of("√3/4")
    with pytest.raises(ValueError, match="no exact number"):
        snap(0.123456789)


def test_an_oripa_pattern_collapses_into_a_preliminary_base(tmp_path: Path) -> None:
    path = tmp_path / "preliminary.cp"
    path.write_text(PRELIMINARY, encoding="utf-8")
    pattern = CreasePattern.read(path)
    assert (pattern.width, pattern.height) == (Number.of(1), Number.of(1))
    assert (len(pattern.valleys), len(pattern.mountains)) == (2, 1)
    paper = Paper.square()
    paper.crease(pattern.creases)
    paper.collapse(valleys=pattern.valleys, mountains=pattern.mountains)
    assert len(paper.state.faces) == 6
    assert paper.thickest()[0] == 4


def test_a_build_folds_again_from_its_own_crease_pattern(tmp_path: Path) -> None:
    # The bird base, read back from the crease pattern its build writes, collapses in
    # one step to the same creases.
    script = Path(shutil.copy(EXAMPLES / "bird_base.py", tmp_path))
    request = service.BuildRequest(script=script, pictures=False)
    assert asyncio.run(service.build(request)).ok
    path = tmp_path / "bird_base.fold"
    document = json.loads(path.read_text(encoding="utf-8"))
    pattern = CreasePattern.read(path)
    paper = Paper.square()
    paper.crease(pattern.creases)
    paper.collapse(valleys=pattern.valleys, mountains=pattern.mountains)
    kinds = [paper.state.assignment(edge) for edge in paper.state.creases]
    expected = document["edges_assignment"]
    assert (kinds.count("M"), kinds.count("V")) == (
        expected.count("M"),
        expected.count("V"),
    )


def test_creasing_keeps_the_paper_open_and_its_creases() -> None:
    paper = Paper.square()
    paper.precrease(Line.through(("1/2", 0), ("1/2", 1)), flap=("3/4", "1/2"))
    paper.crease([((0, "1/2"), ("1/2", "1/2")), (("1/2", "1/2"), (1, "1/2"))])
    state = paper.state
    assert len(state.faces) == 4
    assert {state.assignment(edge) for edge in state.creases} == {"F"}
    assert len({face.place for face in state.faces}) == 1


def test_creasing_folded_paper_is_refused() -> None:
    paper = Paper.square()
    paper.valley(Line.through(("1/2", 0), ("1/2", 1)), flap=("3/4", "1/2"))
    with pytest.raises(RefusedError) as refused:
        paper.crease([((0, "1/4"), ("1/2", "1/4"))])
    assert refused.value.refusal.rule == "paper-folded"


def test_a_crease_off_the_paper_is_refused() -> None:
    paper = Paper.square()
    with pytest.raises(RefusedError) as refused:
        paper.crease([((0, "1/2"), (2, "1/2"))])
    assert refused.value.refusal.rule == "crease-off-paper"


@pytest.mark.parametrize(
    "creases",
    [
        [((0, "1/2"), ("1/2", "1/2"))],
        [((0, 0), ("1/2", "1/2")), (("1/2", "1/2"), (1, 0))],
    ],
    ids=["dangling", "bent"],
)
def test_a_crease_that_ends_inside_the_paper_is_refused(
    creases: list[tuple[PointLike, PointLike]],
) -> None:
    paper = Paper.square()
    with pytest.raises(RefusedError) as refused:
        paper.crease(creases)
    assert refused.value.refusal.rule == "loose-crease"


def test_a_crease_through_a_vertex_meets_the_creases_there() -> None:
    # Two diagonals cross at the center, which splits each into two creases.
    paper = Paper.square()
    paper.crease([((0, 0), (1, 1)), ((1, 0), (0, 1))])
    state = paper.state
    assert len(state.faces) == 4
    center = Point.of(("1/2", "1/2"))
    assert (
        sum(center in (state.vertices[u], state.vertices[v]) for u, v in state.creases)
        == 4
    )
