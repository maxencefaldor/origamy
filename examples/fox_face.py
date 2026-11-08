# Copyright 2026 Maxence Faldor
"""The traditional fox face: a triangle whose corners fold down, then up as ears."""

from origamy import Line, Paper, fold_onto, midpoint


def fold() -> Paper:
    """Fold a triangle, its corners down to the chin, then up past the top as ears.

    The triangle's right angle at (0, 0) is the chin, and the middle of its long fold,
    at (1/2, 1/2), the top of the head.
    """
    paper = Paper.square(front="#f6efe2", back="#d9692a")
    chin, right, left = (0, 0), (1, 0), (0, 1)
    paper.valley(Line.through(right, left), flap=("3/4", "3/4"))
    # Fold each side corner down onto the chin: a square, two flaps on its front.
    paper.valley(fold_onto(right, chin), flap=("15/16", "1/32"))
    paper.valley(fold_onto(left, chin), flap=("1/32", "15/16"))
    # Fold each flap's tip back up past the top, a little outward: the ears.
    ears = []
    for tip, inside in (
        (("25/32", "11/32"), ("1/16", "1/32")),
        (("11/32", "25/32"), ("1/32", "1/16")),
    ):
        ear = fold_onto(chin, tip)
        paper.valley(ear, flap=inside, layers=2)
        ears.append((ear, tip))
    center = ("1/4", "1/4")
    # Tip the ears forward, and stand the face on its chin.
    for ear, tip in ears:
        paper.shape(
            ear,
            flap=midpoint(tip, midpoint(tip, midpoint(tip, center))),
            angle=25,
            layers=2,
        )
    paper.tilt(Line.through(("1/16", 0), (0, "1/16")), flap=("1/4", "1/4"), angle=80)
    # Let every fold ease 15° open, as pressed paper does.
    paper.settle(ease=15)
    return paper
