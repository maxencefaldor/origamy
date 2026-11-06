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
    ) -> Self: ...
    def vertices(self) -> list[PointData]: ...
    def corners(self) -> list[list[int]]: ...
    def places(self) -> list[PlaceData]: ...
    def creases(self) -> list[CreaseData]: ...
    def stacking(self) -> list[Pair] | None: ...
    def crease(
        self, outline: list[PointData], segments: list[tuple[PointData, PointData]]
    ) -> tuple[str, State | None, list[PointData], list[list[int]], int | None]: ...
    def along(
        self, queries: list[tuple[int | PointData, int | PointData, bool]]
    ) -> list[list[int]]: ...
    def largest(self) -> int: ...
    def overlapping(self) -> list[Pair]: ...
    def thickest(self) -> tuple[int, PointData]: ...
    def stack(
        self, fixed: list[Pair], prefer: list[Pair], budget: int
    ) -> tuple[str, list[Pair], list[ConstraintData], list[Pair]]: ...
    def violations(self, above: list[Pair]) -> list[ConstraintData]: ...
    def collapse(
        self, above: list[Pair], target: str, fixed: int, budget: int
    ) -> tuple[
        str,
        State | None,
        list[int],
        tuple[int, int, int, float, float] | None,
        list[ConstraintData],
        list[Pair],
    ]: ...

def relations(
    value: float, root: int, scale: int, size: int
) -> list[tuple[int, int, int]]: ...
