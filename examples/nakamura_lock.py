# Copyright 2026 Maxence Faldor
"""The Nakamura lock: a paper plane whose nose tip folds up and locks its corners."""

from origamy import Line, Paper, bisect, fold_onto


def fold() -> Paper:
    """Fold the corners in, the nose down, the corners in again, then lock them.

    The paper is A-series, √2 times as tall as wide; its keel runs up x = 1/2.
    The plane stays open, its wings raised in a shallow V.
    """
    paper = Paper(1, "√2", front="#3a7d44", back="#f1f6ee")
    nose, tail = ("1/2", "√2"), ("1/2", 0)
    keel = Line.through(nose, tail)
    paper.precrease(keel, flap=("3/4", "1/2"))
    for corner, inside in (
        ((0, "√2"), ("1/16", "√2 - 1/8")),
        ((1, "√2"), ("15/16", "√2 - 1/8")),
    ):
        paper.valley(bisect(corner, nose, tail), flap=inside)
    # Fold the nose down along the corners' feet, its tip on the keel below them.
    level = "√2 - 1/2"
    paper.valley(Line.through((0, level), (1, level)), flap=("17/32", "√2 - 1/16"))
    # Fold the new top corners to the keel, meeting an eighth above the nose's tip.
    meeting = ("1/2", "√2 - 7/8")
    for corner, inside in (
        ((0, level), ("1/16", "√2 - 17/32")),
        ((1, level), ("15/16", "√2 - 17/32")),
    ):
        paper.valley(fold_onto(corner, meeting), flap=inside)
    # Fold the nose's tip up over the corners: the lock.
    lock = "√2 - 15/16"
    paper.valley(
        Line.through((0, lock), (1, lock)), flap=("65/128", "√2 - 31/32"), layers=1
    )
    # Raise the wings to a shallow V about the keel, the lock on top.
    paper.shape(keel, flap=("3/4", "1/4"), angle=36)
    paper.tilt(keel, flap=("3/4", "1/4"), angle=-18)
    return paper
