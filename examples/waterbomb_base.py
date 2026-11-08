# Copyright 2026 Maxence Faldor
"""The waterbomb base: diagonals valley, a midline mountain, collapsed in one step."""

from origamy import Line, Paper


def fold() -> Paper:
    """Precrease the diagonals and a midline, then collapse them into a triangle."""
    paper = Paper.square(front="#2c6fbb", back="#f2ede4")
    rising = Line.through((0, 0), (1, 1))
    falling = Line.through((1, 0), (0, 1))
    level = Line.through((0, "1/2"), (1, "1/2"))
    paper.precrease(rising, flap=("3/4", "1/4"))
    paper.precrease(falling, flap=("1/4", "1/8"))
    paper.precrease(level, flap=("1/2", "3/4"), mountain=True)
    paper.collapse(valleys=[rising, falling], mountains=[level], fixed=("1/2", "1/8"))
    # Let every fold ease 20° open, as pressed paper does.
    paper.settle(ease=20)
    return paper
