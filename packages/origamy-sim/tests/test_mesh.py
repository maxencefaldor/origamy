# Copyright 2026 Maxence Faldor
import ast
import itertools
import re
from pathlib import Path

import numpy as np
import pytest
from hypothesis import given
from hypothesis import strategies as st
from numpy.typing import ArrayLike, NDArray

import origamy_sim
from origamy_sim import Mesh, _core

SQUARE = [[0, 0], [1, 0], [1, 1], [0, 1]]
HALVES = [[0, 1, 2], [0, 2, 3]]


def grid(columns: int, rows: int) -> Mesh:
    """A rectangle of unit squares, each split along its rising diagonal."""
    x, y = np.meshgrid(np.arange(columns + 1), np.arange(rows + 1))
    corner = np.arange(x.size).reshape(x.shape)
    low_left, low_right = corner[:-1, :-1].ravel(), corner[:-1, 1:].ravel()
    up_left, up_right = corner[1:, :-1].ravel(), corner[1:, 1:].ravel()
    triangles = np.concatenate(
        [
            np.column_stack([low_left, low_right, up_right]),
            np.column_stack([low_left, up_right, up_left]),
        ]
    )
    return Mesh(np.column_stack([x.ravel(), y.ravel()]), triangles)


def test_a_square_has_one_dihedral_along_its_diagonal() -> None:
    mesh = Mesh(SQUARE, HALVES)
    assert mesh.edges.tolist() == [[0, 1], [0, 2], [0, 3], [1, 2], [2, 3]]
    assert mesh.dihedrals.tolist() == [[0, 2, 3, 1]]
    assert mesh.boundary.tolist() == [[0, 1], [3, 0], [1, 2], [2, 3]]
    assert mesh.area == 1
    assert mesh.rest.dtype == np.float64
    assert mesh.triangles.dtype == np.int64
    assert repr(mesh) == "<Mesh of 4 vertices and 2 triangles>"


@given(st.integers(1, 10), st.integers(1, 10))
def test_a_grid_has_the_topology_of_a_disc(columns: int, rows: int) -> None:
    mesh = grid(columns, rows)
    vertices, edges, triangles = len(mesh.rest), len(mesh.edges), len(mesh.triangles)
    assert vertices - edges + triangles == 1
    assert len(mesh.boundary) == 2 * (columns + rows)
    assert len(mesh.dihedrals) == edges - len(mesh.boundary)
    assert mesh.area == pytest.approx(columns * rows)


def test_each_dihedral_has_its_left_opposite_vertex_on_its_left() -> None:
    mesh = grid(3, 2)
    u, v, left, right = (mesh.rest[mesh.dihedrals[:, k]] for k in range(4))

    def turn(p: NDArray[np.float64]) -> NDArray[np.float64]:
        """Twice the signed area of each triangle from u to v to p."""
        d, e = v - u, p - u
        return d[:, 0] * e[:, 1] - d[:, 1] * e[:, 0]

    assert (turn(left) > 0).all()
    assert (turn(right) < 0).all()


@pytest.mark.parametrize("dtype", [np.int8, np.uint16, np.int32, np.uint32, np.int64])
def test_vertices_come_in_any_integer_type(dtype: type[np.integer]) -> None:
    rest = np.array(SQUARE, dtype=np.float32)
    assert Mesh(rest, np.array(HALVES, dtype=dtype)) == Mesh(SQUARE, HALVES)


def test_faces_mesh_into_small_triangles_whose_edges_follow_theirs() -> None:
    # A unit square folded along its diagonal: two faces, meshed finely.
    points = [[0.0, 0.0], [0.1, 0.0], [0.1, 0.1], [0.0, 0.1]]
    refinement = origamy_sim.Refinement(max_area=1e-4)
    mesh, faces, lines = Mesh.of_faces(points, [[0, 1, 2], [0, 2, 3]], refinement)
    assert lines == []
    assert mesh.rest[:4] == pytest.approx(np.array(points))
    corners = mesh.rest[mesh.triangles]
    sides = corners[:, [1, 2], :] - corners[:, [0, 0], :]
    areas = 0.5 * (sides[:, 0, 0] * sides[:, 1, 1] - sides[:, 0, 1] * sides[:, 1, 0])
    assert (areas > 0).all()
    assert areas.max() <= refinement.max_area * (1 + 1e-9)
    assert [areas[faces == f].sum() for f in (0, 1)] == pytest.approx([5e-3, 5e-3])
    # The mesh edges between the faces run along the diagonal.
    triangle = {frozenset(t): i for i, t in enumerate(mesh.triangles.tolist())}
    for u, v, a, b in mesh.dihedrals.tolist():
        left, right = (faces[triangle[frozenset((u, v, w))]] for w in (a, b))
        if left != right:
            assert mesh.rest[[u, v], 0] == pytest.approx(mesh.rest[[u, v], 1])
    with pytest.raises(ValueError, match="face 1 runs clockwise or overlaps another"):
        Mesh.of_faces(points, [[0, 1, 2], [0, 3, 2]], refinement)


def test_lines_join_faces_meshed_as_chains_of_edges() -> None:
    # A line a tenth of a millimeter inside the square's left side, between points on
    # its bottom and top sides, joins the mesh after the refinement.
    points = [[0.0, 0.0], [1e-4, 0.0], [0.1, 0.0], [0.1, 0.1], [1e-4, 0.1], [0.0, 0.1]]
    refinement = origamy_sim.Refinement(max_area=1e-4)
    mesh, _, (line,) = Mesh.of_faces(
        points, [[0, 1, 2, 3, 4, 5]], refinement, lines=[(1, 4)]
    )
    assert (line[0], line[-1]) == (1, 4)
    assert mesh.rest[line, 0] == pytest.approx(1e-4)
    assert np.diff(mesh.rest[line, 1]) == pytest.approx(0.1 / (len(line) - 1))
    edges = {tuple(e) for e in mesh.edges.tolist()}
    assert all(tuple(sorted(pair)) in edges for pair in itertools.pairwise(line))
    with pytest.raises(ValueError, match="line 0 needs two distinct points"):
        Mesh.of_faces(points, [[0, 1, 2, 3, 4, 5]], refinement, lines=[(1, 1)])


def test_points_only_lines_name_join_with_their_lines() -> None:
    # A line between two points inside the square, which no face names.
    points = [
        [0.0, 0.0],
        [0.1, 0.0],
        [0.1, 0.1],
        [0.0, 0.1],
        [0.02, 0.05],
        [0.08, 0.05],
    ]
    refinement = origamy_sim.Refinement(max_area=1e-4)
    mesh, _, (line,) = Mesh.of_faces(points, [[0, 1, 2, 3]], refinement, lines=[(4, 5)])
    # The points keep their order, before every vertex the refinement adds.
    assert mesh.rest[:6] == pytest.approx(np.array(points))
    assert (line[0], line[-1]) == (4, 5)
    assert mesh.rest[line, 1] == pytest.approx(0.05)


def test_a_grid_is_a_rectangle_of_cells() -> None:
    grid = Mesh.grid(3, 2, 0.3, 0.1)
    assert grid.rest.shape == (12, 2)
    assert grid.triangles.shape == (12, 3)
    assert grid.rest[11].tolist() == [0.3, 0.1]
    assert grid.area == pytest.approx(0.03)
    with pytest.raises(ValueError, match="no triangles"):
        Mesh.grid(0, 2, 1.0, 1.0)
    with pytest.raises(ValueError, match="counterclockwise"):
        Mesh.grid(1, 1, -1.0, 1.0)


def test_vertices_are_integers() -> None:
    with pytest.raises(
        TypeError, match="triangles must hold integers, but it holds float64"
    ):
        Mesh(SQUARE, [[0, 1, 2.0], [0, 2, 3]])
    with pytest.raises(
        TypeError, match="triangles must hold integers, but it holds bool"
    ):
        Mesh(SQUARE, [[False, True, True]])
    with pytest.raises(TypeError, match="safe"):
        Mesh(SQUARE, np.array(HALVES, dtype=np.uint64))


@pytest.mark.parametrize(
    ("rest", "triangles", "message"),
    [
        ([[0, 0, 0]], HALVES, "rest has shape (1, 3), but it must have shape (n, 2)"),
        (SQUARE, [0, 1, 2], "triangles has shape (3,), but it must have shape (n, 3)"),
        (SQUARE, np.empty((0, 3), dtype=int), "the mesh has no triangles"),
        (
            SQUARE,
            [[0, 1, 4], [0, 2, 3]],
            "triangle 0 names vertex 4, but the mesh has 4",
        ),
        (
            SQUARE,
            [[0, 1, 2], [0, -1, 3]],
            "triangle 1 names vertex -1, but the mesh has 4",
        ),
        (SQUARE, [[0, 1, 1], [0, 2, 3]], "triangle 0 names one vertex twice"),
        (SQUARE, [[0, 2, 1], [0, 2, 3]], "triangle 0 has area -0.5 m²"),
        (SQUARE, [[0, 1, 2]], "vertex 3 belongs to no triangle"),
        ([[0, 0], [1, 0], [np.nan, 1]], [[0, 1, 2]], "vertex 2 lies at a coordinate"),
        (
            [[0, 0], [1, 0], [0.5, 1], [0.5, 2]],
            [[0, 1, 2], [0, 1, 3]],
            "both triangles along edge (0, 1) run it in the same direction",
        ),
    ],
)
def test_a_mesh_that_makes_no_sheet_says_why(
    rest: ArrayLike, triangles: ArrayLike, message: str
) -> None:
    with pytest.raises(ValueError, match=re.escape(message)):
        Mesh(rest, triangles)


def test_the_api_page_documents_every_export() -> None:
    page = Path(__file__).parents[3] / "docs" / "simulator" / "api.md"
    documented = re.findall(r"^::: origamy_sim\.(\w+)$", page.read_text(), re.MULTILINE)
    assert sorted(documented) == sorted(origamy_sim.__all__)


def test_the_stub_types_everything_the_compiled_core_defines() -> None:
    stub = ast.parse(Path(origamy_sim.__file__).with_name("_core.pyi").read_text())
    classes = {node.name: node for node in stub.body if isinstance(node, ast.ClassDef)}
    assert set(classes) == {name for name in dir(_core) if not name.startswith("_")}
    for name, node in classes.items():
        typed = {
            item.name
            for item in node.body
            if isinstance(item, ast.FunctionDef) and not item.name.startswith("_")
        }
        defined = {
            member
            for member in vars(getattr(_core, name))
            if not member.startswith("_")
        }
        assert typed == defined
