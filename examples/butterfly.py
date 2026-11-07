# Copyright 2026 Maxence Faldor
"""The traditional butterfly: a waterbomb base whose tip folds down and locks."""

from origamy import Line, Paper, fold_onto


def fold() -> Paper:
    """Fold the front flaps up, turn over, fold the tip down and back, then raise wings.

    The waterbomb base's apex lies at (1/2, 1/2) and its base along the bottom edge.
    """
    paper = Paper.square(front="#f5c542", back="#2f6db5")
    rising = Line.through((0, 0), (1, 1))
    falling = Line.through((1, 0), (0, 1))
    level = Line.through((0, "1/2"), (1, "1/2"))
    paper.precrease(rising, flap=("3/4", "1/4"))
    paper.precrease(falling, flap=("1/4", "1/8"))
    paper.precrease(level, flap=("1/2", "3/4"), mountain=True)
    paper.collapse(valleys=[rising, falling], mountains=[level], fixed=("1/2", "1/8"))
    # Fold the front flaps' corners up to the apex: a diamond.
    apex = ("1/2", "1/2")
    paper.valley(fold_onto((0, 0), apex), flap=("1/16", "1/32"), layers=2)
    paper.valley(fold_onto((1, 0), apex), flap=("15/16", "1/32"), layers=2)
    # Turn over, fold the apex down past the bottom edge, and its tip back up over it.
    paper.turn_over()
    paper.valley(Line.through((0, "3/16"), (1, "3/16")), flap=("1/2", "7/16"))
    paper.valley(Line.through((0, 0), (1, 0)), flap=("1/2", "-1/16"))
    # Turn over, and raise the wings in a V about the body.
    paper.turn_over()
    body = Line.through(("1/2", 0), ("1/2", 1))
    paper.shape(body, flap=("3/4", "1/8"), angle=50)
    paper.tilt(body, flap=("1/4", "1/8"), angle=25)
    return paper
