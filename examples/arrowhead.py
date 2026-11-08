# Copyright 2026 Maxence Faldor
"""An original: a preliminary base narrowed front and back into an arrowhead."""

from origamy import Line, Paper, bisect, meet, midpoint


def fold() -> Paper:
    """Collapse a preliminary base, kite fold both sides, then fold the tops down."""
    paper = Paper.square(front="#1f6f78", back="#e8f3f4")
    rising = Line.through((0, 0), (1, 1))
    falling = Line.through((1, 0), (0, 1))
    upright = Line.through(("1/2", 0), ("1/2", 1))
    level = Line.through((0, "1/2"), (1, "1/2"))
    paper.precrease(rising, flap=("3/4", "1/4"))
    paper.precrease(falling, flap=("7/8", "5/8"), mountain=True)
    paper.precrease(upright, flap=("3/4", "1/8"))
    paper.precrease(level, flap=("1/8", "3/4"))
    paper.collapse(valleys=[upright, level], mountains=[falling], fixed=("7/8", "5/8"))
    for corners in (
        ((0, 0), (0, "1/2"), ("1/2", 0)),
        ((1, 1), ("1/2", 1), (1, "1/2")),
    ):
        center = paper.where(("1/2", "1/2"))
        corner, left, right = (paper.where(point) for point in corners)
        for side in (left, right):
            kite = bisect(side, corner, center)
            top = meet(kite, Line.through(center, side))
            paper.valley(kite, flap=midpoint(side, midpoint(corner, top)), layers=1)
        paper.turn_over()
    # Let every fold ease 20° open, as pressed paper does.
    paper.settle(ease=20)
    return paper
