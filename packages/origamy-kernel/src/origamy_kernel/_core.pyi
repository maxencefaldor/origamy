# Copyright 2026 Maxence Faldor
# The compiled module's types; its docstrings come from the Rust doc comments.

from typing import Self

from origamy_kernel.data import (
    ConstraintData,
    CreaseData,
    NumberData,
    Pair,
    PlaceData,
    PointData,
)

class State:
    def __new__(
        cls,
        radicands: list[NumberData],
        vertices: list[PointData],
        corners: list[list[int]],
        places: list[PlaceData],
        stacking: list[Pair],
    ) -> Self: ...
    def vertices(self) -> list[PointData]: ...
    def corners(self) -> list[list[int]]: ...
    def places(self) -> list[PlaceData]: ...
    def creases(self) -> list[CreaseData]: ...
    def edges(self) -> list[tuple[Pair, list[int]]]: ...
    def stacking(self) -> list[Pair]: ...
    def assignments(self) -> str: ...
    def crease(
        self, outline: list[PointData], segments: list[tuple[PointData, PointData]]
    ) -> tuple[str, State | None, int | None]: ...
    def along(
        self, queries: list[tuple[PointData, PointData, bool]]
    ) -> list[list[int]]: ...
    def largest(self) -> int: ...
    def overlapping(self) -> list[Pair]: ...
    def thickest(self) -> tuple[int, PointData]: ...
    def stack(
        self, fixed: list[Pair], prefer: list[Pair], budget: int
    ) -> tuple[str, list[Pair], list[ConstraintData], list[Pair]]: ...
    def violations(self, above: list[Pair]) -> list[ConstraintData]: ...
    def collapse(
        self, target: str, fixed: int, budget: int
    ) -> tuple[
        str,
        State | None,
        list[int],
        tuple[int, int, int, float, float] | None,
        list[ConstraintData],
        list[Pair],
    ]: ...

def explain(
    values: list[float],
    tolerance: float,
    sure: float,
    denominator: int,
    roots: list[int],
    scale: int,
    size: int,
) -> list[tuple[float, tuple[int, int], tuple[int, int], int] | None]: ...
def read_corners(
    radicands: list[NumberData],
    keys: list[tuple[float, float]],
    floats: list[tuple[float, float]],
    edges: list[tuple[int, int]],
    tolerance: float,
    sure: float,
    fit: float,
    near: float,
    denominator: int,
    roots: list[int],
    scale: int,
    size: int,
) -> tuple[list[NumberData], list[PointData], tuple[float, float, float] | None]: ...
