# Copyright 2026 Maxence Faldor
"""The newspaper hat: a sheet folded in half, its corners down, its brims up and out."""

from origamy import Line, Paper

BRIM = 20
"""How far each brim turns out off the crown, in degrees."""


def fold() -> Paper:
    """Fold the sheet in half, its top corners down and each brim up, then stand it.

    The paper is twice as tall as it is wide. Once halved, the hat's top fold runs
    along y = 1, and each brim folds up twice, to y = 1/2. Each brim then turns out off
    the crown, as a hat's brims do once it is opened to wear.
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
    # Turn each brim's two layers up off the crown, turning the paper over after each.
    for _ in range(2):
        paper.shape(high, flap=("1/2", "5/8"), angle=BRIM, layers=2)
        paper.turn_over()
    paper.tilt(high, flap=("1/2", "3/4"), angle=90)
    return paper
