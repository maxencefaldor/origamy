# Copyright 2026 Maxence Faldor
"""A regular octagon, folded from a square as Sundara Row's exercises fold it."""

from origamy import Line, Paper


def fold() -> Paper:
    """Fold each corner behind along a line that cuts 1 - √2/2 from both its edges.

    A regular octagon in the unit square has sides √2 - 1, so each corner's cut
    leaves 1 - √2/2 of each edge beside it.
    """
    paper = Paper.square(front="#2a7f62", back="#f2ede4")
    a, b = "1 - √2/2", "√2/2"
    for start, end, corner in (
        ((a, 0), (0, a), ("1/32", "1/32")),
        ((b, 0), (1, a), ("31/32", "1/32")),
        ((1, b), (b, 1), ("31/32", "31/32")),
        ((0, b), (a, 1), ("1/32", "31/32")),
    ):
        paper.mountain(Line.through(start, end), flap=corner)
    return paper
