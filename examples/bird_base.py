# Copyright 2026 Maxence Faldor
"""The bird base: a preliminary base with a petal fold on each side."""

from origamy import Line, Paper, bisect, meet, midpoint


def fold() -> Paper:
    """Collapse a preliminary base, then petal fold its front and its back."""
    paper = Paper.square()
    rising = Line.through((0, 0), (1, 1))
    falling = Line.through((1, 0), (0, 1))
    upright = Line.through(("1/2", 0), ("1/2", 1))
    level = Line.through((0, "1/2"), (1, "1/2"))
    paper.precrease(rising, flap=("3/4", "1/4"))
    paper.precrease(falling, flap=("7/8", "5/8"), mountain=True)
    paper.precrease(upright, flap=("3/4", "1/8"))
    paper.precrease(level, flap=("1/8", "3/4"))
    paper.collapse(valleys=[upright, level], mountains=[falling], fixed=("7/8", "5/8"))

    # Each side's top layer has its open corner and two side corners at these points
    # of the unfolded paper; the closed point is the paper's center.
    for corners in (
        ((0, 0), (0, "1/2"), ("1/2", 0)),
        ((1, 1), ("1/2", 1), (1, "1/2")),
    ):
        center = paper.where(("1/2", "1/2"))
        corner, left, right = (paper.where(point) for point in corners)
        # Fold the lower edges to the middle line and back: the kite creases.
        kite_left = bisect(left, corner, center)
        kite_right = bisect(right, corner, center)
        top_left = meet(kite_left, Line.through(center, left))
        top_right = meet(kite_right, Line.through(center, right))
        inside_left = midpoint(left, midpoint(corner, top_left))
        inside_right = midpoint(right, midpoint(corner, top_right))
        paper.precrease(kite_left, flap=inside_left, layers=1)
        paper.precrease(kite_right, flap=inside_right, layers=1)
        # Fold the top down over the kites and back, then lift the open corner up
        # along that crease, the sides folding in.
        top = Line.through(top_left, top_right)
        middle = meet(top, Line.through(center, corner))
        near_center = midpoint(midpoint(center, top_left), midpoint(center, middle))
        paper.precrease(top, flap=near_center)
        paper.petal(top, flap=midpoint(corner, midpoint(top_left, middle)))
        paper.turn_over()
    return paper
