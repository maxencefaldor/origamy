# Copyright 2026 Maxence Faldor
"""A house with two doors: a folded sheet, its roof behind and its doors ajar."""

from origamy import Line, Paper


def fold() -> Paper:
    """Fold the sheet in half, fold the roof's corners behind, then fold the doors in.

    Once halved, the house's top fold runs along y = 1/2, and its roof rises from the
    sides at y = 1/4 to the middle of the top. The doors hinge at x = 1/4 and 3/4.
    """
    paper = Paper.square(front="#f4ead6", back="#c0392b")
    paper.valley(Line.through((0, "1/2"), (1, "1/2")), flap=("1/2", "7/8"))
    peak = ("1/2", "1/2")
    paper.mountain(Line.through(peak, (0, "1/4")), flap=("1/16", "15/32"))
    paper.mountain(Line.through(peak, (1, "1/4")), flap=("15/16", "15/32"))
    doors = (
        (Line.through(("1/4", 0), ("1/4", "1/2")), ("1/16", "1/8")),
        (Line.through(("3/4", 0), ("3/4", "1/2")), ("15/16", "1/8")),
    )
    for door, outside in doors:
        paper.valley(door, flap=outside)
    # Open the doors part way, and stand the house up.
    for door, outside in doors:
        inside = paper.where(outside)
        paper.shape(door, flap=inside, angle=70, layers=2)
    paper.tilt(Line.through((0, 0), (1, 0)), flap=("1/2", "1/4"), angle=90)
    return paper
