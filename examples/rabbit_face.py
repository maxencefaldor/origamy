# Copyright 2026 Maxence Faldor
"""A rabbit face: a triangle whose corners fold up past its top as long ears."""

from origamy import Line, Paper, Point, bisect, fold_onto


def fold() -> Paper:
    """Fold a triangle, raise its corners as ears, tuck the chin, then stand it.

    The triangle's right angle at (0, 0) is the chin, and its long folded edge the top
    of the head.
    """
    paper = Paper.square(front="#e9a7b5", back="#f4f1ec")
    chin, right, left = (0, 0), (1, 0), (0, 1)
    paper.valley(Line.through(right, left), flap=("3/4", "3/4"))
    for corner, inside in ((right, ("7/8", "1/16")), (left, ("1/16", "7/8"))):
        paper.precrease(fold_onto(corner, chin), flap=inside)
    # Each ear turns up about a point near the chin, so it rises far past the head.
    ears = []
    for corner, middle, inside in (
        (right, ("3/10", 0), ("7/8", "1/16")),
        (left, (0, "3/10"), ("1/16", "7/8")),
    ):
        pivot = Point.of(middle)
        ear = bisect(corner, pivot, pivot + Point.of((1, 1)))
        paper.valley(ear, flap=inside)
        ears.append((ear, inside))
    # Fold the chin's tip behind.
    paper.mountain(fold_onto(chin, ("1/16", "1/16")), flap=("1/64", "1/64"))
    # Tip the ears forward, and stand the face on its chin.
    for ear, tip in ears:
        paper.shape(ear, flap=paper.where(tip), angle=15, layers=2)
    paper.tilt(Line.through(("1/32", 0), (0, "1/32")), flap=("1/2", "1/2"), angle=80)
    return paper
