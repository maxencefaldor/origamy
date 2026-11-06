# Copyright 2026 Maxence Faldor
import ast
import inspect
from pathlib import Path

import pytest

import origamy_kernel
from origamy_kernel import _core

ONE = ([1], 1)


def point(x: int, y: int) -> tuple[tuple[list[int], int], tuple[list[int], int]]:
    return ([x], 1), ([y], 1)


def test_the_stub_types_everything_the_compiled_core_defines() -> None:
    stub = ast.parse(Path(origamy_kernel.__file__).with_name("_core.pyi").read_text())
    functions = {
        node.name: node for node in stub.body if isinstance(node, ast.FunctionDef)
    }
    assert set(functions) == {name for name in dir(_core) if not name.startswith("_")}
    for name, node in functions.items():
        typed = [argument.arg for argument in node.args.args]
        assert typed == list(inspect.signature(getattr(_core, name)).parameters)
    assert sorted(origamy_kernel.__all__) == sorted(functions)


def test_a_diagonal_creases_a_square_into_two_triangles() -> None:
    square = [point(0, 0), point(1, 0), point(1, 1), point(0, 1)]
    edges = [(0, 1), (1, 2), (2, 3), (0, 3)]
    outcome, added, faces, index = origamy_kernel.crease(
        [], square, edges, square, [(point(0, 0), point(1, 1))]
    )
    assert (outcome, added, index) == ("creased", [], None)
    assert sorted(sorted(face) for face in faces) == [[0, 1, 2], [0, 2, 3]]


def test_a_crease_off_the_paper_names_its_segment() -> None:
    square = [point(0, 0), point(1, 0), point(1, 1), point(0, 1)]
    edges = [(0, 1), (1, 2), (2, 3), (0, 3)]
    segments = [(point(0, 0), point(1, 1)), (point(0, 0), point(2, 1))]
    outcome, _, _, index = origamy_kernel.crease([], square, edges, square, segments)
    assert (outcome, index) == ("off-paper", 1)


def test_malformed_numbers_and_indices_raise_value_errors() -> None:
    square = [point(0, 0), point(1, 0), point(1, 1), point(0, 1)]
    with pytest.raises(ValueError, match="denominator"):
        origamy_kernel.crease([], [(([1], 0), ONE)], [], [], [])
    with pytest.raises(ValueError, match="power of two"):
        origamy_kernel.crease([], [(([1, 2, 3], 1), ONE)], [], [], [])
    with pytest.raises(ValueError, match="edge"):
        origamy_kernel.crease([], square, [(0, 9)], square, [])
    with pytest.raises(ValueError, match="M, V or F"):
        origamy_kernel.collapse(
            [],
            square,
            [[0, 1, 2, 3]],
            [(ONE, ([0], 1), ([0], 1), ONE, ([0], 1), ([0], 1))],
            [],
            [],
            [],
            "X",
            0,
            10,
        )
