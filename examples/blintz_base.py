# Copyright 2026 Maxence Faldor
"""The blintz base: every corner folded to the center."""

from origamy import Paper, fold_onto


def fold() -> Paper:
    """Fold each corner of the square onto its center."""
    paper = Paper.square(front="#2c6fbb", back="#f2ede4")
    center = ("1/2", "1/2")
    for corner, flap in (
        ((0, 0), ("1/16", "1/16")),
        ((1, 0), ("15/16", "1/16")),
        ((1, 1), ("15/16", "15/16")),
        ((0, 1), ("1/16", "15/16")),
    ):
        paper.valley(fold_onto(corner, center), flap=flap)
    # Let every fold ease 20° open, as pressed paper does.
    paper.settle(ease=20)
    return paper
