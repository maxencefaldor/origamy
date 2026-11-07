# Copyright 2026 Maxence Faldor
import asyncio
import json
import shutil
from fractions import Fraction
from pathlib import Path

import pytest

from origamy import CreasePattern, Line, Number, Paper, Point, RefusedError, service
from origamy.geometry import PointLike
from origamy.pattern import SURE, snap

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
    assert snap(1 / 3) == Number.of(Fraction(1, 3))
    assert snap(81 / 1040) == Number.of(Fraction(81, 1040))
    assert snap((1 + 5**0.5) / 4) == Number.of("1/4 + √5/4")
    assert snap((3 - 6**0.5) / 7) == Number.of("3/7 - √6/7")
    assert snap(1 - 2**0.5 / 2) == Number.of("1 - √2/2")
    assert snap(3**0.5 / 4) == Number.of("√3/4")
    # cos 22.5° / 2 needs √2, then √(2 + √2) above it.
    assert snap((2 + 2**0.5) ** 0.5 / 4) == (2 + Number.of("√2")).sqrt() / 4
    with pytest.raises(ValueError, match="no exact number"):
        snap(0.123456789)
    # A file's rounding: the number still explains the float far better than chance,
    # though not surely.
    rounded = (8 + 2**0.5) / 1024 + 1e-14
    assert snap(rounded) == Number.of("1/128 + √2/1024")
    with pytest.raises(ValueError, match="no exact number"):
        snap(rounded, fit=SURE)


def test_a_corner_no_number_explains_lies_where_creases_cross(tmp_path: Path) -> None:
    # Two creases from exact corners cross at a point of Q(√2, √3), which no single
    # square root's numbers explain.
    rising = Line.through((0, 0), (1, "√2/2"))
    falling = Line.through((0, 1), (1, "√3/3"))
    crossing = rising.meet(falling)
    assert crossing is not None
    corners = [(0, 0), (1, 0), (1, 3**0.5 / 3), (1, 2**0.5 / 2), (1, 1), (0, 1)]
    border = [(k, (k + 1) % len(corners)) for k in range(len(corners))]
    document = {
        "vertices_coords": [*corners, list(crossing.floats())],
        "edges_vertices": [*border, (0, 6), (6, 3), (5, 6), (6, 2)],
        "edges_assignment": ["B"] * len(border) + ["F"] * 4,
    }
    path = tmp_path / "crossing.fold"
    path.write_text(json.dumps(document), encoding="utf-8")
    with pytest.raises(ValueError, match="no exact number"):
        snap(crossing.floats()[0])
    pattern = CreasePattern.read(path)
    assert {end for crease in pattern.flats for end in crease} == {
        Point.of((0, 0)),
        Point.of((0, 1)),
        Point.of((1, "√2/2")),
        Point.of((1, "√3/3")),
        crossing,
    }


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


def test_a_square_turned_by_a_sixteenth_turn_reads_and_collapses(
    tmp_path: Path,
) -> None:
    # The preliminary base on a square turned by 22.5°, as the traditional crane's
    # pattern draws it: its corners hold √2 and √(2 + √2).
    root = (2 + Number.of("√2")).sqrt()
    cosine, sine = root / 2, (Number.of("√2") - 1) * root / 2
    reach = (cosine + sine) / 2

    def turned(x: Fraction, y: Fraction) -> Point:
        """A point of the centered square, turned and moved into the box of side 1."""
        return Point(
            (cosine * x - sine * y + reach) / (2 * reach),
            (sine * x + cosine * y + reach) / (2 * reach),
        )

    half, zero = Fraction(1, 2), Fraction(0)
    corners = [turned(x, y) for x, y in ((-half, -half), (half, -half), (half, half))]
    corners.append(turned(-half, half))
    sides = ((zero, -half), (half, zero), (zero, half), (-half, zero))
    middles = [turned(x, y) for x, y in sides]
    lines = [(1, corners[k], corners[(k + 1) % 4]) for k in range(4)]
    lines += [(2, corners[1], corners[3]), (3, middles[0], middles[2])]
    lines.append((3, middles[1], middles[3]))
    path = tmp_path / "turned.cp"
    floats = [
        (kind, [200 * float(c) for p in ends for c in (p.x, p.y)])
        for kind, *ends in lines
    ]
    path.write_text(
        "".join(f"{kind} {' '.join(map(str, ends))}\n" for kind, ends in floats),
        encoding="utf-8",
    )
    pattern = CreasePattern.read(path)
    assert set(pattern.outline) == set(corners)
    paper = Paper.polygon(pattern.outline)
    paper.crease(pattern.creases)
    paper.collapse(valleys=pattern.valleys, mountains=pattern.mountains)
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


def test_a_pattern_on_hexagonal_paper_folds_on_its_outline(tmp_path: Path) -> None:
    # A hexagon folded in half along its long diagonal, written in ORIPA's format.
    height = 3**0.5
    corners = [
        (1, 0),
        (3, 0),
        (4, height),
        (3, 2 * height),
        (1, 2 * height),
        (0, height),
    ]
    rows = [
        f"1 {x} {y} {u} {v}"
        for (x, y), (u, v) in zip(corners, [*corners[1:], corners[0]], strict=True)
    ]
    path = tmp_path / "hexagon.cp"
    path.write_text("\n".join([*rows, f"3 0 {height} 4 {height}"]), encoding="utf-8")
    pattern = CreasePattern.read(path)
    assert pattern.outline == tuple(
        Point.of((Fraction(x, 4), y))
        for x, y in [
            (0, "√3/4"),
            (1, 0),
            (3, 0),
            (4, "√3/4"),
            (3, "√3/2"),
            (1, "√3/2"),
        ]
    )
    paper = Paper.polygon(pattern.outline)
    paper.crease(pattern.creases)
    paper.collapse(valleys=pattern.valleys)
    assert paper.thickest()[0] == 2


@pytest.mark.parametrize(
    "corners",
    [
        [(0, 0), (1, 1), (1, 0)],
        [(0, 0), (2, 0), (1, "1/2"), (2, 1), (0, 1)],
        [(1, 1), (2, 1), (2, 2)],
    ],
    ids=["clockwise", "concave", "off-axes"],
)
def test_paper_must_be_convex_and_counterclockwise_on_the_axes(
    corners: list[tuple[int | str, int | str]],
) -> None:
    with pytest.raises(ValueError, match="paper"):
        Paper.polygon(corners)
