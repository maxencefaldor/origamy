# Copyright 2026 Maxence Faldor
"""The traditional swan: a kite folded narrow and halved, its point reversed up."""

from origamy import Line, Paper, Point, bisect, midpoint

NECK = 72
"""The angle, in degrees, the neck rises off the spine."""


def fold() -> Paper:
    """Fold a kite, turn over, fold it narrower, halve it, and reverse the neck up.

    The paper's diagonal from (0, 0) to (1, 1) is the swan's spine: (0, 0) becomes the
    head and (1, 1) the tail.
    """
    paper = Paper.square(front="#1f2a44", back="#f7f4ec")
    point, tail = (0, 0), (1, 1)
    spine = Line.through(point, tail)
    paper.precrease(spine, flap=("3/4", "1/4"))
    for side, flap in (((1, 0), ("15/16", "1/4")), ((0, 1), ("1/4", "15/16"))):
        paper.valley(bisect(side, point, tail), flap=flap)
    # Turn over: the point lies at (1, 0) and the tail at (0, 1). Fold the kite's long
    # edges onto the spine again.
    paper.turn_over()
    tip, back = paper.where(point), paper.where(tail)
    for corner in ((1, "√2 - 1"), ("√2 - 1", 1)):
        side = paper.where(corner)
        paper.valley(
            bisect(side, tip, back),
            flap=midpoint(side, midpoint(side, midpoint(tip, back))),
        )
    # Fold in half behind along the spine, which runs to the tip at -45°. Reverse the
    # point up as the neck, then its tip forward as the head, square to the neck.
    paper.mountain(Line.through(tip, back), flap=("7/10", "2/5"))
    rise = -45 - NECK
    neck = paper.where(("7/16", "7/16"))
    paper.reverse(
        bisect(tip, neck, neck + Point.polar(1, rise)), flap=("23/25", "7/100")
    )
    head = paper.where(("3/32", "3/32"))
    paper.reverse(
        bisect(paper.where(point), head, head + Point.polar(1, rise + 90)),
        flap=paper.where(("3/100", "27/1000")),
        layers=2,
    )
    # Stand the swan on its belly, along the spine.
    paper.tilt(Line.through(tip, back), flap=("1/4", "1/2"), angle=90)
    return paper
