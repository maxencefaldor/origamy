# Copyright 2026 Maxence Faldor
"""An original: the classic dart, its wingtips turned up as winglets."""

from origamy import Line, Paper, Point, bisect, fold_onto, meet, midpoint


def fold() -> Paper:
    """Fold a dart, spread its wings, then turn a strip of each wingtip up square."""
    paper = Paper(1, "√2", front="#d9822b", back="#fbf3e8")
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
    # Hold the plane by one wing and spread it: the body turns down to a right angle,
    # then the other wing turns level. The wing creases halve the paper between each
    # tail corner and the keel's foot.
    paper.turn_over()
    for crease, flap, layers in (
        ("1/4", ("3/8", "1/8"), 3),
        ("3/4", ("7/8", "1/8"), 1),
    ):
        line = Line.through(paper.where((crease, 0)), paper.where((crease, "1/2")))
        paper.shape(line, flap=paper.where(flap), angle=90, layers=layers)
    paper.turn_over()
    # Each winglet turns up square to the wing along the line halfway from the paper's
    # edge to the wing's crease, where the edge folds onto the crease.
    for edge, tip in (("1/8", ("1/16", "1/8")), ("7/8", ("15/16", "1/8"))):
        line = Line.through(paper.where((edge, 0)), paper.where((edge, "1/2")))
        paper.shape(line, flap=paper.where(tip), angle=90, layers=1)
    # Let every fold ease 20° open, as pressed paper does.
    paper.settle(ease=20)
    return paper
