# Copyright 2026 Maxence Faldor
"""The square envelope: a square on its corner, its corners folded over to close it."""

from origamy import Paper, fold_onto


def fold() -> Paper:
    """Fold the side corners to the center, the bottom corner up past it, then the flap.

    The paper lies on its corner (1/2, 0) along the diagonal from (0, 0) to (1, 1):
    the corners (1, 0) and (0, 1) fold to the center as the envelope's sides.
    """
    paper = Paper.square(front="#f6f1e4", back="#c9a66b")
    center = ("1/2", "1/2")
    paper.valley(fold_onto((1, 0), center), flap=("15/16", "1/16"))
    paper.valley(fold_onto((0, 1), center), flap=("1/16", "15/16"))
    # The bottom corner folds a little past the center, closing the pocket.
    paper.valley(fold_onto((0, 0), ("5/8", "5/8")), flap=("1/32", "1/32"))
    # The top corner folds down over it as the flap.
    paper.valley(fold_onto((1, 1), ("1/2", "1/2")), flap=("31/32", "31/32"))
    paper.settle(ease=10)
    return paper
