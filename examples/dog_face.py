# Copyright 2026 Maxence Faldor
"""The traditional dog face: a triangle whose corners fold down as ears."""

from origamy import Line, Paper, fold_onto


def fold() -> Paper:
    """Fold a triangle, fold the ears down and the nose up, then stand the face up.

    The triangle's long folded edge is the top of the head, and its right angle at
    (0, 0) the chin.
    """
    paper = Paper.square(front="#2a211c", back="#a8733f")
    right, left = (1, 0), (0, 1)
    paper.valley(Line.through(right, left), flap=("3/4", "3/4"))
    # Each ear folds down from the head's top, its tip past the side of the face.
    ears = (
        (Line.through(("3/4", "1/4"), ("5/8", 0)), ("15/16", "1/32")),
        (Line.through(("1/4", "3/4"), (0, "5/8")), ("1/32", "15/16")),
    )
    for ear, tip in ears:
        paper.valley(ear, flap=tip)
    # The chin's tip turns up, on the front layer only: the nose.
    paper.valley(fold_onto((0, 0), ("1/8", "1/8")), flap=("1/32", "1/32"), layers=1)
    # Lift the ears off the face, and stand the face on its chin.
    for ear, tip in reversed(ears):
        paper.shape(ear, flap=paper.where(tip), angle=35, layers=2)
    paper.tilt(Line.through(("1/8", 0), (0, "1/8")), flap=("1/2", "1/2"), angle=80)
    return paper
