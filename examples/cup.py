# Copyright 2026 Maxence Faldor
"""A traditional paper cup, folded from a square with simple folds only."""

from origamy import Line, Paper, bisect, fold_onto, meet, midpoint


def fold() -> Paper:
    """Fold a triangle, bring each side corner across, then fold the top flaps down."""
    paper = Paper.square()
    a, b, c = (0, 0), (1, 0), (0, 1)

    # Fold the top right half down: a triangle whose long folded edge runs from b to c,
    # with two loose layers at the right angle a.
    paper.valley(Line.through(b, c), flap=("3/4", "3/4"))

    # Fold corner b to the side from a to c, so that the flap's edge from b toward a
    # lands parallel to the long edge: b moves along the bisector of its angle, which
    # a precrease marks where it meets that side.
    paper.precrease(bisect(a, b, c), flap=("1/2", "1/32"))
    to_side = meet(Line.through(a, c), bisect(a, b, c))
    paper.valley(fold_onto(b, to_side), flap=("15/16", "1/32"))

    # The same for corner c, onto the side from a to b.
    paper.precrease(bisect(a, c, b), flap=("1/32", "1/2"))
    to_bottom = meet(Line.through(a, b), bisect(a, c, b))
    paper.valley(fold_onto(c, to_bottom), flap=("1/32", "15/16"))

    # Fold the front flap at a down over the side flaps' edges, then turn the paper
    # over and fold the back flap the same way.
    for _ in range(2):
        corner, left, right = (paper.where(p) for p in (a, b, c))
        rim = Line.through(left, right)
        paper.valley(rim, flap=midpoint(corner, midpoint(left, right)), layers=1)
        paper.turn_over()
    return paper
