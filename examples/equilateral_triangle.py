# Copyright 2026 Maxence Faldor
"""The largest equilateral triangle a square holds, folded with exact 60° angles."""

from origamy import Line, Paper, fold_onto


def fold() -> Paper:
    """Find 60° by folding a corner onto the midline, then fold the rest behind.

    Folding the corner (1, 0) onto the vertical midline through (0, 0) puts it at
    (1/2, √3/2): the triangle's apex, with its base the bottom edge.
    """
    paper = Paper.square(front="#d9822b", back="#f2ede4")
    middle = Line.through(("1/2", 0), ("1/2", 1))
    paper.precrease(middle, flap=("3/4", "1/2"))
    apex = ("1/2", "√3/2")
    paper.precrease(fold_onto((1, 0), apex), flap=("15/16", "1/8"))
    # The folds through each bottom corner and the apex.
    for start, flap in (((0, 0), ("1/32", "15/16")), ((1, 0), ("31/32", "15/16"))):
        paper.mountain(Line.through(start, apex), flap=flap)
    # The second fold carries the top edge's middle past the first; fold it back.
    paper.mountain(Line.through((0, 0), apex), flap=paper.where(("1/2", 1)))
    return paper
