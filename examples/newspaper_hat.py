# Copyright 2026 Maxence Faldor
"""The newspaper hat: a sheet halved, its corners down, its brims up, then opened."""

from origamy import Line, Paper


def fold() -> Paper:
    """Fold the hat flat, then open its crown into a pyramid and stand it up.

    The paper is twice as tall as it is wide. Once halved, the hat's top fold runs
    along y = 1, and each brim folds up twice, to y = 1/2. A crease down the middle
    lets each face of the crown fold in two, so the crown opens as rigid origami.
    """
    paper = Paper(1, 2, front="#d9d3c4", back="#efe9dc")
    # Crease the middle, then halve the sheet.
    paper.precrease(Line.through(("1/2", 0), ("1/2", 2)), flap=("3/4", 1))
    paper.valley(Line.through((0, 1), (1, 1)), flap=("3/8", "7/4"))
    # Fold the top corners down to the middle, both layers together.
    top = ("1/2", 1)
    paper.valley(Line.through(top, (0, "1/2")), flap=("1/16", "15/16"))
    paper.valley(Line.through(top, (1, "1/2")), flap=("15/16", "15/16"))
    # Roll the front brim up twice over the corners, then the back brim behind.
    low, high = (Line.through((0, y), (1, y)) for y in ("1/4", "1/2"))
    paper.valley(low, flap=("3/8", "1/8"), layers=1)
    paper.valley(high, flap=("3/8", "3/8"), layers=2)
    paper.mountain(low, flap=("3/8", "1/8"))
    paper.mountain(high, flap=("3/8", "3/8"), layers=2)
    # Fold each brim's corners in over it, along the crown's sides, so the brims
    # never cross as the crown opens.
    for _ in range(2):
        for side, flap in (
            ((0, "1/2"), ("1/32", "5/8")),
            ((1, "1/2"), ("31/32", "5/8")),
        ):
            paper.valley(Line.through(top, side), flap=flap, layers=2)
        paper.turn_over()
    # Open the crown: the front corners' creases turn to a right angle at the sides,
    # and the corners stay pressed on the back half.
    pressed = [(top, (0, "3/2")), (top, (1, "3/2")), ((0, 1), top), (top, (1, 1))]
    paper.flex(
        {(top, (0, "1/2")): 90, (top, (1, "1/2")): 90, **dict.fromkeys(pressed, 180)}
    )
    # Stand the hat on its opening: its axis leans 66° off upright, about (3, 1).
    paper.tilt(Line.through(top, ("7/2", 2)), flap=("2/5", "13/10"), angle=66)
    return paper
