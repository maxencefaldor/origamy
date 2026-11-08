# Copyright 2026 Maxence Faldor
"""The traditional whale: a kite base whose blunt end folds over as the head."""

from origamy import Line, Paper, bisect


def fold() -> Paper:
    """Fold a kite, fold its blunt corner over, halve it, and reverse the tail up.

    The diagonal from (0, 0) to (1, 1) is the whale's spine: (1, 1) the head and
    (0, 0) the tail.
    """
    paper = Paper.square(front="#2f5d8a", back="#e3ebf2")
    tail, head = (0, 0), (1, 1)
    spine = Line.through(tail, head)
    paper.precrease(spine, flap=("3/4", "1/4"))
    paper.turn_over()
    # Fold the edges at the tail onto the spine: a kite, its flaps the whale's color.
    paper.valley(bisect((1, 0), tail, head), flap=("15/16", "1/4"))
    paper.valley(bisect((0, 1), tail, head), flap=("1/4", "15/16"))
    # Fold the blunt corner over along the kite's widest line: the head.
    paper.valley(Line.through((1, "√2 - 1"), ("√2 - 1", 1)), flap=("15/16", "15/16"))
    # Fold the whale in half along its spine, the flaps inside.
    paper.valley(spine, flap=("3/5", "9/20"))
    # Reverse the tail up, between the body's layers.
    paper.reverse(
        Line.through(("5/16", "5/16"), ("1/16", "1/4")), flap=("1/20", "1/12")
    )
    # Stand the whale on its belly, the fold along the spine.
    paper.tilt(spine, flap=("1/2", "3/4"), angle=90)
    return paper
