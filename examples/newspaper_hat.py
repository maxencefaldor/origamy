# Copyright 2026 Maxence Faldor
"""The newspaper hat: a sheet folded in half, its corners down, its brims up."""

from origamy import Line, Paper


def fold() -> Paper:
    """Fold the sheet in half, its top corners down and each brim up, then stand it.

    The paper is twice as tall as it is wide. Once halved, the hat's top fold runs
    along y = 1, and each brim folds up twice, to y = 1/2.
    """
    paper = Paper(1, 2, front="#d9d3c4", back="#efe9dc")
    paper.valley(Line.through((0, 1), (1, 1)), flap=("1/2", "7/4"))
    # Fold the top corners down to the middle, both layers together.
    top = ("1/2", 1)
    paper.valley(Line.through(top, (0, "1/2")), flap=("1/16", "15/16"))
    paper.valley(Line.through(top, (1, "1/2")), flap=("15/16", "15/16"))
    # Roll the front brim up twice over the corners, then the back brim behind.
    low, high = (Line.through((0, y), (1, y)) for y in ("1/4", "1/2"))
    paper.valley(low, flap=("1/2", "1/8"), layers=1)
    paper.valley(high, flap=("1/2", "3/8"), layers=2)
    paper.mountain(low, flap=("1/2", "1/8"))
    paper.mountain(high, flap=("1/2", "3/8"), layers=2)
    paper.tilt(high, flap=("1/2", "3/4"), angle=90)
    # Let every fold ease 15° open, as pressed paper does.
    paper.settle(ease=15)
    return paper
