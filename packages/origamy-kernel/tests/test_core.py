# Copyright 2026 Maxence Faldor
import ast
import inspect
from pathlib import Path

import pytest

import origamy_kernel
from origamy_kernel import _core
from origamy_kernel.data import NumberData, PlaceData, PointData

ONE: NumberData = ([1], 1)
ZERO: NumberData = ([0], 1)


def point(x: int, y: int) -> PointData:
    return ([x], 1), ([y], 1)


def test_the_stub_types_everything_the_compiled_core_defines() -> None:
    stub = ast.parse(Path(origamy_kernel.__file__).with_name("_core.pyi").read_text())
    functions = {
        node.name: node for node in stub.body if isinstance(node, ast.FunctionDef)
    }
    classes = {node.name: node for node in stub.body if isinstance(node, ast.ClassDef)}
    defined = {name for name in dir(_core) if not name.startswith("_")}
    assert set(functions) | set(classes) == defined
    for name, node in functions.items():
        typed = [argument.arg for argument in node.args.args]
        assert typed == list(inspect.signature(getattr(_core, name)).parameters)
    for name, node in classes.items():
        methods = {
            item.name: item
            for item in node.body
            if isinstance(item, ast.FunctionDef) and not item.name.startswith("_")
        }
        cls = getattr(_core, name)
        assert set(methods) == {m for m in vars(cls) if not m.startswith("_")}
        for method, item in methods.items():
            typed = [argument.arg for argument in item.args.args]
            assert typed == list(inspect.signature(getattr(cls, method)).parameters)
    assert sorted(origamy_kernel.__all__) == sorted(defined)


FLAT: PlaceData = (ONE, ZERO, ZERO, ONE, ZERO, ZERO)
SQUARE = [point(0, 0), point(1, 0), point(1, 1), point(0, 1)]


def test_a_diagonal_creases_a_square_into_two_triangles() -> None:
    paper = origamy_kernel.State([], SQUARE, [[0, 1, 2, 3]], [FLAT], [])
    outcome, creased, index = paper.crease(SQUARE, [(point(0, 0), point(1, 1))])
    assert (outcome, index) == ("creased", None)
    assert creased is not None
    assert creased.vertices() == SQUARE
    assert sorted(sorted(face) for face in creased.corners()) == [[0, 1, 2], [0, 2, 3]]
    assert creased.creases() == [((0, 2), (0, 1))]
    assert creased.stacking() == []


def test_a_crease_off_the_paper_names_its_segment() -> None:
    paper = origamy_kernel.State([], SQUARE, [[0, 1, 2, 3]], [FLAT], [])
    segments = [(point(0, 0), point(1, 1)), (point(0, 0), point(2, 1))]
    outcome, creased, index = paper.crease(SQUARE, segments)
    assert (outcome, creased, index) == ("off-paper", None, 1)


def test_malformed_numbers_and_faces_raise_value_errors() -> None:
    with pytest.raises(ValueError, match="denominator"):
        origamy_kernel.State(
            [], [(([1], 0), ONE), *SQUARE[1:]], [[0, 1, 2]], [FLAT], []
        )
    with pytest.raises(ValueError, match="power of two"):
        origamy_kernel.State([], [(([1, 2, 3], 1), ONE)], [[0, 0, 0]], [FLAT], [])
    with pytest.raises(ValueError, match="face"):
        origamy_kernel.State([], SQUARE, [[0, 1, 9]], [FLAT], [])
    held = origamy_kernel.State([], SQUARE, [[0, 1, 2, 3]], [FLAT], [])
    with pytest.raises(ValueError, match="M, V or F"):
        held.collapse("X", 0, 10)
    with pytest.raises(ValueError, match="kind"):
        held.collapse("", 3, 10)


def test_a_held_state_folds_and_stacks() -> None:
    vertices = [point(0, 0), point(1, 0), point(2, 0), point(2, 1), point(1, 1)]
    vertices.append(point(0, 1))
    corners = [[0, 1, 4, 5], [1, 2, 3, 4]]
    held = origamy_kernel.State([], vertices, corners, [FLAT] * 2, [])
    assert held.creases() == [((1, 4), (0, 1))]
    assert held.edges()[:2] == [((0, 1), [0]), ((1, 4), [0, 1])]
    assert held.stacking() == []
    assert held.assignments() == "F"
    assert held.largest() == 0
    assert held.overlapping() == []
    assert held.along([(point(1, 0), point(1, 1), False)]) == [[0]]
    segments = [(point(1, 0), point(1, 1), True), (point(2, 0), point(2, 1), True)]
    assert held.along(segments) == [[0], []]
    outcome, after, moved, *_ = held.collapse("V", 0, 10)
    assert (outcome, moved) == ("collapsed", [1])
    assert after is not None
    assert after.stacking() == [(1, 0)]
    assert after.assignments() == "V"
    assert after.overlapping() == [(0, 1)]
    assert after.thickest()[0] == 2
    assert after.stack([(1, 0)], [], 10)[:2] == ("stacked", [(1, 0)])
    assert after.violations([(1, 0)]) == []
