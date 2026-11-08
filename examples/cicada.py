# Copyright 2026 Maxence Faldor
"""The traditional cicada, semi: a triangle whose corners fold up, then down."""

from origamy import Line, Paper, fold_onto


def fold() -> Paper:
    """Fold a triangle, raise its corners, fold them down as wings, then the head.

    The triangle's right angle at (1, 1) is the head, and its long folded edge runs
    from (1, 0) to (0, 1). The body's axis runs from (1/2, 1/2) to the head.
    """
    paper = Paper.square(front="#efe6cc", back="#4f7a34")
    head, right, left = (1, 1), (1, 0), (0, 1)
    paper.valley(Line.through(right, left), flap=("1/4", "1/4"))
    # Fold each side corner up onto the head: a square, two flaps on its front.
    paper.valley(fold_onto(right, head), flap=("15/16", "1/4"))
    paper.valley(fold_onto(left, head), flap=("1/4", "15/16"))
    # Fold each flap's tip down past the tail, a little outward: the wings.
    for corner, inside in (
        ((1, "1/2"), ("31/32", "7/8")),
        (("1/2", 1), ("7/8", "31/32")),
    ):
        paper.valley(Line.through(corner, ("23/32", "23/32")), flap=inside, layers=2)
    # Fold the head's top layer down, then the next a little higher: two stripes.
    paper.valley(fold_onto(head, ("3/4", "3/4")), flap=("31/32", "31/32"), layers=1)
    paper.valley(fold_onto(head, ("13/16", "13/16")), flap=("31/32", "31/32"), layers=1)
    # Turn the sides down: the body rounds, and the cicada stands on its wings' edges.
    sides = (
        (Line.through(("3/4", "1/2"), (1, "3/4")), ("15/16", "9/16")),
        (Line.through(("1/2", "3/4"), ("3/4", 1)), ("9/16", "15/16")),
    )
    for side, outside in sides:
        paper.shape(side, flap=outside, angle=-75)
    return paper
