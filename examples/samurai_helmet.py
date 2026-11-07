# Copyright 2026 Maxence Faldor
"""The traditional samurai helmet, kabuto: a triangle whose corners become horns."""

from origamy import Line, Paper, fold_onto, midpoint


def fold() -> Paper:
    """Fold a triangle, bring its corners down, then up as horns, and fold the brim.

    The paper stands on its corner (0, 0), the helmet's lowest point; its long fold
    from (0, 1) to (1, 0) is the top.
    """
    low, left, right = (0, 0), (0, 1), (1, 0)
    paper = Paper.square(front="#c8a24a", back="#7a1f1f")
    paper.valley(Line.through(left, right), flap=("3/4", "3/4"))
    # Fold each side corner down to the lowest point.
    for corner, inside in ((right, ("7/8", "1/16")), (left, ("1/16", "7/8"))):
        paper.valley(fold_onto(corner, low), flap=inside)
    # Fold each flap's tip back up to the top: the horns' bases.
    top = midpoint(left, right)
    for inside in (("1/4", "1/16"), ("1/16", "1/4")):
        paper.valley(fold_onto(low, top), flap=inside, layers=2)
    # Fold each tip out and up: the horns.
    root = ("1/4", "1/4")
    for side, inside in (
        (("1/2", "3/8"), ("9/20", "2/5")),
        (("3/8", "1/2"), ("2/5", "9/20")),
    ):
        paper.valley(Line.through(root, side), flap=inside, layers=2)
    # The brim: fold the front layer's lowest point up to the horns' roots, then fold
    # the band up again along the horns' bases.
    paper.valley(fold_onto(low, root), flap=("1/16", "1/32"), layers=1)
    base = Line.through(("1/2", 0), (0, "1/2"))
    paper.valley(base, flap=("3/16", "3/16"), layers=2)
    # Fold the back layer's point up behind, inside the helmet, and stand it up.
    paper.mountain(base, flap=("1/16", "1/32"))
    paper.tilt(base, flap=("1/2", "1/2"), angle=85)
    return paper
