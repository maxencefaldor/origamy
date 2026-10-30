# Copyright 2026 Maxence Faldor
"""The fortune teller: corners to the center on both sides, then folded in half."""

from origamy import Line, Number, Paper, Point, fold_onto

QUARTER, EIGHTH = Number.of("1/4"), Number.of("1/8")


def fold() -> Paper:
    """Fold the four corners to the center, turn over, fold the new four, then halve."""
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
    paper.valley(
        Line.at(center, (1, 0)), flap=center - Line.at(center, (1, 0)).normal * EIGHTH
    )
    return paper
