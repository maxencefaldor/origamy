# Copyright 2026 Maxence Faldor
"""The traditional heart, from a square folded on its diagonal."""

from origamy import Line, Paper, Point, fold_onto, midpoint


def fold() -> Paper:
    """Fold the top and bottom corners in, the bottom edges up, then round the lobes.

    The paper's diagonal from (0, 0) to (1, 1) is the heart's axis: (0, 0) its point
    and (1, 1) its top.
    """
    paper = Paper.square(front="#c81d3a", back="#f6e3e6")
    bottom, top, center = (0, 0), (1, 1), ("1/2", "1/2")
    paper.precrease(Line.through(bottom, top), flap=("3/4", "1/4"))
    paper.precrease(Line.through((1, 0), (0, 1)), flap=("7/8", "5/8"))
    paper.turn_over()

    # Fold the top corner down to the center, then the bottom corner up to the top edge.
    peak, point, middle = (paper.where(p) for p in (top, bottom, center))
    paper.valley(fold_onto(peak, middle), flap=peak + Point.of(("1/16", "-1/8")))
    edge = midpoint(peak, middle)
    paper.valley(fold_onto(point, edge), flap=point + Point.of(("-1/16", "1/8")))

    # Fold each half of the bottom edge up onto the axis: the heart's point.
    low = midpoint(point, edge)
    for along, flap in (((0, 1), ("3/32", "5/32")), ((-1, 0), ("-5/32", "-3/32"))):
        paper.valley(
            Line.through(low, low + Point.of(along)), flap=low + Point.of(flap)
        )

    # Round the lobes: fold their corners behind. One fold takes both top corners,
    # which lie on one line.
    for ends, flap in (
        (((0, "5/8"), ("1/8", "3/4")), ("1/32", "11/16")),
        (((0, "1/2"), ("1/8", "3/8")), ("1/32", "13/32")),
        ((("1/2", 1), ("5/8", "7/8")), ("19/32", "31/32")),
    ):
        paper.mountain(Line.through(*ends), flap=flap)
    # Let every fold ease open a little, as pressed paper does: the heart puffs up.
    paper.settle(ease=15)
    return paper
