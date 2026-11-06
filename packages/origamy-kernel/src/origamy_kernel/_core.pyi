# Copyright 2026 Maxence Faldor
# The compiled module's types; its docstrings come from the Rust doc comments.

from origamy_kernel.data import (
    ConstraintData,
    CreaseData,
    NumberData,
    Pair,
    PlaceData,
    PointData,
)

def stack(
    radicands: list[NumberData],
    vertices: list[PointData],
    corners: list[list[int]],
    places: list[PlaceData],
    creases: list[CreaseData],
    fixed: list[Pair],
    prefer: list[Pair],
    budget: int,
) -> tuple[str, list[Pair], list[ConstraintData], list[Pair]]: ...
def violations(
    radicands: list[NumberData],
    vertices: list[PointData],
    corners: list[list[int]],
    places: list[PlaceData],
    creases: list[CreaseData],
    above: list[Pair],
) -> list[ConstraintData]: ...
def crease(
    radicands: list[NumberData],
    vertices: list[PointData],
    edges: list[tuple[int, int]],
    outline: list[PointData],
    segments: list[tuple[PointData, PointData]],
) -> tuple[str, list[PointData], list[list[int]], int | None]: ...
def along(
    radicands: list[NumberData],
    vertices: list[PointData],
    edges: list[tuple[int, int]],
    queries: list[tuple[PointData, PointData, bool]],
) -> list[list[int]]: ...
def largest(
    radicands: list[NumberData],
    vertices: list[PointData],
    corners: list[list[int]],
) -> int: ...
def relations(
    value: float, root: int, scale: int, size: int
) -> list[tuple[int, int, int]]: ...
def collapse(
    radicands: list[NumberData],
    vertices: list[PointData],
    corners: list[list[int]],
    places: list[PlaceData],
    creases: list[CreaseData],
    border: list[tuple[int, int]],
    above: list[Pair],
    target: str,
    fixed: int,
    budget: int,
) -> tuple[
    str,
    list[PlaceData],
    list[Pair],
    list[int],
    tuple[int, int, int, float, float] | None,
    list[ConstraintData],
    list[Pair],
]: ...
