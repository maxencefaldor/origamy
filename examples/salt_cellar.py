# Copyright 2026 Maxence Faldor
"""The salt cellar: the fortune teller opened, its four pockets standing up."""

from origamy import Line, Number, Paper, Point, fold_onto

QUARTER, EIGHTH = Number.of("1/4"), Number.of("1/8")


def fold() -> Paper:
    """Fold the corners to the center on both sides, crease the middles, then open.

    A settle step turns both middle lines toward 100° as mountains and eases the
    other folds 40° open. The pockets rise, as fingers open a fortune teller.
    """
    paper = Paper.square(front="#3a7d44", back="#f4efe1")
    # Crease both diagonals to find the center.
    paper.precrease(Line.through((0, 0), (1, 1)), flap=("3/4", "1/4"))
    paper.precrease(Line.through((1, 0), (0, 1)), flap=("1/4", "1/8"))
    for ring in (
        ((0, 0), (1, 0), (1, 1), (0, 1)),
        (("1/2", 0), (1, "1/2"), ("1/2", 1), (0, "1/2")),
    ):
        for corner in ring:
            tip, center = paper.where(corner), paper.where(("1/2", "1/2"))
            inward = center - tip
            flap = tip + inward * QUARTER + Point(-inward.y, inward.x) * EIGHTH
            paper.valley(fold_onto(tip, center), flap=flap)
        paper.turn_over()
    center = paper.where(("1/2", "1/2"))
    across, up = Line.at(center, (1, 0)), Line.at(center, (0, 1))
    # Crease both middle lines, each flap point off the other line.
    nudge = EIGHTH * EIGHTH
    paper.precrease(across, flap=center - across.normal * EIGHTH + up.normal * nudge)
    paper.precrease(up, flap=center - up.normal * EIGHTH + across.normal * nudge)
    paper.settle({across: -100, up: -100}, ease=40)
    return paper
