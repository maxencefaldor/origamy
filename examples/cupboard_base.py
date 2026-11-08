# Copyright 2026 Maxence Faldor
"""The cupboard base: two opposite edges folded to the middle, as doors."""

from origamy import Line, Paper


def fold() -> Paper:
    """Fold the left and right edges onto the vertical midline."""
    paper = Paper.square(front="#2c6fbb", back="#f2ede4")
    paper.valley(Line.through(("1/4", 0), ("1/4", 1)), flap=("1/16", "1/2"))
    paper.valley(Line.through(("3/4", 0), ("3/4", 1)), flap=("15/16", "1/2"))
    # Let every fold ease 20° open, as pressed paper does.
    paper.settle(ease=20)
    return paper
