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
    def bottom_up(self) -> tuple[list[int], list[int]] | None: ...
    def assignments(self) -> str: ...
    def crease(
        self, outline: list[PointData], segments: list[tuple[PointData, PointData]]
    ) -> tuple[str, State | None, int | None]: ...
    def along(
        self, queries: list[tuple[PointData, PointData, bool]]
    ) -> list[list[int]]: ...
    def folded(self) -> list[list[tuple[PointData, tuple[float, float]]]]: ...
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
    fields: list[list[list[int]]],
    scale: int,
    size: int,
) -> list[tuple[float, list[int], int, int | None] | None]: ...
def read_corners(
    radicands: list[NumberData],
    segments: list[tuple[tuple[float, float], tuple[float, float]]],
    origin: tuple[float, float],
    length: float,
    tolerance: float,
    sure: float,
    fit: float,
    near: float,
    denominator: int,
    fields: list[list[list[int]]],
    scale: int,
    size: int,
) -> tuple[
    list[NumberData],
    list[PointData],
    list[Pair],
    tuple[float, float, float] | None,
]: ...
