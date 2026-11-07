# Copyright 2026 Maxence Faldor
"""The glider: a paper plane whose nose folds back, so its wings run wide."""

from origamy import Line, Paper, Point, bisect, fold_onto, meet, midpoint


def fold() -> Paper:
    """Fold the top corners in, the nose down, the corners in again, then halve it.

    The paper is A-series, √2 times as tall as wide; its keel runs up x = 1/2.
    """
    paper = Paper(1, "√2", front="#e2b33c", back="#fbf4df")
    nose, tail = ("1/2", "√2"), ("1/2", 0)
    keel = Line.through(nose, tail)
    paper.precrease(keel, flap=("3/4", "1/2"))
    # Fold each top corner onto the keel, the fold through the nose.
    for corner, inside in (
        ((0, "√2"), ("1/16", "√2 - 1/8")),
        ((1, "√2"), ("15/16", "√2 - 1/8")),
    ):
        paper.valley(bisect(corner, nose, tail), flap=inside)
    # Fold the nose down onto the corners' lowest points.
    level = "√2 - 1/4"
    paper.valley(Line.through((0, level), (1, level)), flap=("17/32", "√2 - 1/16"))
    # Fold the new top corners onto the keel, the folds through the top's middle.
    top = ("1/2", level)
    for corner, inside in (
        (("1/4", level), ("5/16", "√2 - 9/32")),
        (("3/4", level), ("11/16", "√2 - 9/32")),
    ):
        paper.valley(bisect(corner, top, tail), flap=inside)
    # Fold in half behind, then fold each wing down along the body.
    paper.mountain(keel, flap=("3/4", "1/4"))
    for corner in ((0, 0), (1, 0)):
        tail_corner, keel_foot = paper.where(corner), paper.where(tail)
        wing = fold_onto(tail_corner, keel_foot)
        flap = midpoint(tail_corner, meet(wing, Line.through(tail_corner, keel_foot)))
        flap += Point.of((0, "1/16"))
        paper.valley(wing, flap=flap, layers=1)
        paper.turn_over()
    # Spread the wings: the body turns down to a right angle, then the other wing
    # turns level.
    paper.turn_over()
    for crease, flap, layers in (
        ("1/4", ("3/8", "1/8"), 3),
        ("3/4", ("7/8", "1/8"), 1),
    ):
        line = Line.through(paper.where((crease, 0)), paper.where((crease, "1/2")))
        paper.shape(line, flap=paper.where(flap), angle=90, layers=layers)
    paper.turn_over()
    return paper
