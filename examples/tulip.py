# Copyright 2026 Maxence Faldor
"""The traditional tulip: a triangle whose bottom corners rise as petals."""

from origamy import Line, Paper, bisect, meet, midpoint


def fold() -> Paper:
    """Fold a triangle, then raise each bottom corner past the top as a petal.

    The triangle's long edge, the paper's diagonal from (0, 0) to (1, 1), is the
    flower's base, and its right angle at (0, 1) the middle petal. Each side petal
    rises 22.5° off the flower's axis.
    """
    paper = Paper.square(front="#f6c1cf", back="#d6335f")
    left, right, top = (0, 0), (1, 1), (0, 1)
    paper.valley(Line.through(left, right), flap=("3/4", "1/4"))
    base = midpoint(left, right)
    for corner, edge_end, inside in (
        (left, (0, "1/2"), ("1/16", "1/8")),
        (right, ("1/2", 1), ("7/8", "15/16")),
    ):
        # The petal's tip goes halfway between the axis and the edge from the top.
        side = Line.through(top, edge_end)
        tip = meet(bisect(top, base, edge_end), side)
        paper.valley(bisect(corner, base, tip), flap=inside)
    return paper
