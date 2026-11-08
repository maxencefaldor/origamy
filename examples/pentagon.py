# Copyright 2026 Maxence Faldor
"""A regular pentagon, its base on the bottom edge and its side corners on the sides."""

from origamy import Line, Number, Paper, Point


def fold() -> Paper:
    """Fold the paper outside each side of the pentagon behind, side by side.

    A regular pentagon whose diagonal spans the square has sides s = (√5 - 1)/2, the
    golden ratio's inverse. Its base lies on the bottom edge, its two side corners on
    the square's sides, and its apex near the top.
    """
    paper = Paper.square(front="#c0392b", back="#f2ede4")
    s = (Number.of(5).sqrt() - 1) / 2
    half = Number.of("1/2")
    low = s * (1 - s * s / 4).sqrt()
    top = low + (s * s - half * half).sqrt()
    base_left, base_right = (
        Point(half - s / 2, Number.of(0)),
        Point(half + s / 2, Number.of(0)),
    )
    right, apex, left = (
        Point(Number.of(1), low),
        Point(half, top),
        Point(Number.of(0), low),
    )
    for start, end, outside in (
        (base_right, right, ("63/64", "1/64")),
        (left, base_left, ("1/64", "1/64")),
        (right, apex, ("63/64", "63/64")),
        (apex, left, ("1/64", "63/64")),
    ):
        paper.mountain(Line.through(start, end), flap=outside)
    return paper
