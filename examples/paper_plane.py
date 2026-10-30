# Copyright 2026 Maxence Faldor
"""The classic dart: a paper plane from A-series paper, √2 times as tall as wide."""

from origamy import Line, Paper, Point, bisect, fold_onto, meet, midpoint


def fold() -> Paper:
    """Fold the top corners to the keel twice, halve it, then fold both wings down."""
    paper = Paper(1, "√2", front="#2c6fbb", back="#eef3fa")
    nose, tail = ("1/2", "√2"), ("1/2", 0)
    keel = Line.through(nose, tail)
    paper.precrease(keel, flap=("3/4", "1/2"))
    # Fold each top corner onto the keel, the fold through the nose.
    for corner, inside in (
        ((0, "√2"), ("1/16", "√2 - 1/8")),
        ((1, "√2"), ("15/16", "√2 - 1/8")),
    ):
        paper.valley(bisect(corner, nose, tail), flap=inside)
    # Fold each new edge onto the keel again.
    for side in ((0, "√2 - 1/2"), (1, "√2 - 1/2")):
        edge = paper.where(side)
        line = bisect(edge, nose, tail)
        level = meet(line, Line.through(edge, ("1/2", side[1])))
        paper.valley(line, flap=midpoint(edge, level))
    # Fold in half behind, then fold each wing down along the body.
    paper.mountain(keel, flap=("3/4", "1/2"))
    for corner in ((0, 0), (1, 0)):
        tail_corner, keel_foot = paper.where(corner), paper.where(tail)
        wing = fold_onto(tail_corner, keel_foot)
        flap = midpoint(tail_corner, meet(wing, Line.through(tail_corner, keel_foot)))
        flap += Point.of((0, "1/16"))
        paper.valley(wing, flap=flap, layers=1)
        paper.turn_over()
    return paper
