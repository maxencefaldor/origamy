# Copyright 2026 Maxence Faldor
"""A regular hexagon: an equilateral triangle, its corners folded to its center."""

from origamy import Line, Paper, fold_onto


def fold() -> Paper:
    """Fold the equilateral triangle, then fold each of its corners onto its center.

    Each fold cuts a third from both sides at its corner, so the six sides left are
    equal, and the hexagon is regular.
    """
    paper = Paper.square(front="#7b4fa3", back="#f2ede4")
    middle = Line.through(("1/2", 0), ("1/2", 1))
    paper.precrease(middle, flap=("3/4", "1/2"))
    apex = ("1/2", "√3/2")
    paper.precrease(fold_onto((1, 0), apex), flap=("15/16", "1/8"))
    # The folds through each bottom corner and the apex.
    for start, flap in (((0, 0), ("1/32", "15/16")), ((1, 0), ("31/32", "15/16"))):
        paper.mountain(Line.through(start, apex), flap=flap)
    # The second fold carries the top edge's middle past the first; fold it back.
    paper.mountain(Line.through((0, 0), apex), flap=paper.where(("1/2", 1)))
    center = ("1/2", "√3/6")
    for corner, flap in (
        ((0, 0), ("1/32", "1/64")),
        ((1, 0), ("31/32", "1/64")),
        (apex, ("1/2", "√3/2 - 1/32")),
    ):
        paper.valley(fold_onto(corner, center), flap=flap)
    # Let every fold ease 20° open, as pressed paper does.
    paper.settle(ease=20)
    return paper
