# Copyright 2026 Maxence Faldor
"""origamy: a compiler for origami, built for AI agents.

A fold script defines ``fold()``, which folds a `Paper` step by step and returns it.
Each action either happens or is refused with the facts of why, at its script line.
"""

from origamy.constructions import bisect, fold_onto, meet, midpoint, perpendicular
from origamy.errors import OrigamyError
from origamy.field import Number
from origamy.geometry import Line, Point
from origamy.paper import Paper, RefusedError

__all__ = [
    "Line",
    "Number",
    "OrigamyError",
    "Paper",
    "Point",
    "RefusedError",
    "bisect",
    "fold_onto",
    "meet",
    "midpoint",
    "perpendicular",
]
