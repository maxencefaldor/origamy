# Copyright 2026 Maxence Faldor
"""The traditional swan: a kite folded narrow and halved, its point reversed up."""

from origamy import Line, Paper, bisect, midpoint


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
    # Fold in half behind along the spine, then reverse the point up as the neck.
    paper.mountain(Line.through(tip, back), flap=("7/10", "2/5"))
    paper.reverse(Line.through(("9/16", "7/16"), ("5/8", 0)), flap=("23/25", "7/100"))
    # Reverse the neck's tip forward and down as the head.
    head = paper.where(("3/100", "27/1000"))
    paper.reverse(
        Line.through(("21/64", "1/100"), ("41/96", "-33/100")), flap=head, layers=2
    )
    # Stand the swan on its belly, along the spine.
    paper.tilt(Line.through(tip, back), flap=("1/4", "1/2"), angle=90)
    # Let every fold ease 15° open, as pressed paper does.
    paper.settle(ease=15)
    return paper
