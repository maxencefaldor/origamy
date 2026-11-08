# Copyright 2026 Maxence Faldor
"""An original: the corners folded to the center, then each tip folded back out."""

from origamy import Line, Number, Paper, Point, fold_onto, midpoint

QUARTER, EIGHTH = Number.of("1/4"), Number.of("1/8")


def fold() -> Paper:
    """Fold the corners to the center, then fold each corner back to its flap's edge."""
    paper = Paper.square(front="#b23a48", back="#fcefd9")
    paper.precrease(Line.through((0, 0), (1, 1)), flap=("3/4", "1/4"))
    paper.precrease(Line.through((1, 0), (0, 1)), flap=("1/4", "1/8"))
    corners = ((0, 0), (1, 0), (1, 1), (0, 1))
    for corner in corners:
        tip, center = paper.where(corner), paper.where(("1/2", "1/2"))
        inward = center - tip
        flap = tip + inward * QUARTER + Point(-inward.y, inward.x) * EIGHTH
        paper.valley(fold_onto(tip, center), flap=flap)
    # Fold each tip back out to the middle of its flap's edge.
    for corner in corners:
        tip, base = paper.where(corner), paper.where(midpoint(corner, ("1/2", "1/2")))
        outward = base - tip
        flap = tip + outward * QUARTER + Point(-outward.y, outward.x) * EIGHTH
        paper.valley(fold_onto(tip, base), flap=flap, layers=1)
    # Let every fold ease 20° open, as pressed paper does.
    paper.settle(ease=20)
    return paper
