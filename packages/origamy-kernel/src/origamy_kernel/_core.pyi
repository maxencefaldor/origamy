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
        creases: list[CreaseData],
    ) -> Self: ...
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
        self,
        border: list[tuple[int, int]],
        above: list[Pair],
        target: str,
        fixed: int,
        budget: int,
    ) -> tuple[
        str,
        State | None,
        list[PlaceData],
        list[Pair],
        list[int],
        tuple[int, int, int, float, float] | None,
        list[ConstraintData],
        list[Pair],
    ]: ...

def crease(
    radicands: list[NumberData],
    vertices: list[PointData],
    edges: list[tuple[int, int]],
    outline: list[PointData],
    segments: list[tuple[PointData, PointData]],
) -> tuple[str, list[PointData], list[list[int]], int | None]: ...
def relations(
    value: float, root: int, scale: int, size: int
) -> list[tuple[int, int, int]]: ...
