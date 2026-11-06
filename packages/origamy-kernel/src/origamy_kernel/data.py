# Copyright 2026 Maxence Faldor
"""The data the kernel reads and returns: numbers, points, places, creases."""

type NumberData = tuple[list[int], int]
"""A number as its numerators and its denominator."""

type PointData = tuple[NumberData, NumberData]
"""A point as its two coordinates."""

type PlaceData = tuple[
    NumberData, NumberData, NumberData, NumberData, NumberData, NumberData
]
"""An isometry as its matrix row by row, then its translation."""

type Pair = tuple[int, int]
"""Two faces: the upper first, in an order; the lesser first, in a pair."""

type CreaseData = tuple[tuple[int, int], tuple[int, int]]
"""A crease as its two vertices and its two faces."""

type ConstraintData = tuple[str, list[int], int | None]
"""A constraint as its kind, its faces, and its crease's index if any."""
