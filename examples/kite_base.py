# Copyright 2026 Maxence Faldor
"""The kite base: two edges folded onto a diagonal."""

from origamy import Line, Paper, bisect


def fold() -> Paper:
    """Crease a diagonal, then fold the two edges at one end onto it.

    The diagonal from (0, 0) to (1, 1) is the kite's axis, and (0, 0) its sharp point.
    """
    paper = Paper.square(front="#2c6fbb", back="#f2ede4")
    point, blunt = (0, 0), (1, 1)
    paper.precrease(Line.through(point, blunt), flap=("3/4", "1/4"))
    paper.valley(bisect((1, 0), point, blunt), flap=("15/16", "1/4"))
    paper.valley(bisect((0, 1), point, blunt), flap=("1/4", "15/16"))
    # Let every fold ease 20° open, as pressed paper does.
    paper.settle(ease=20)
    return paper
