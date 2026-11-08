# Copyright 2026 Maxence Faldor
"""A penguin: a kite whose sides stop short of its spine, so a white belly shows."""

from origamy import Line, Paper, fold_onto


def fold() -> Paper:
    """Fold the sides in short of the spine, the feet up and the head down, then stand.

    The paper's diagonal from (0, 0) to (1, 1) is the penguin's spine: (1, 1) the head
    and (0, 0) the feet. Its white side up, the folds show its black front.
    """
    paper = Paper.square(front="#1d1f24", back="#f7f5ef")
    paper.precrease(Line.through((0, 0), (1, 1)), flap=("3/4", "1/4"))
    paper.turn_over()
    # After the turn, the head lies at (0, 1) and the feet at (1, 0).
    head, feet = (0, 1), (1, 0)
    wings = (
        (Line.through(head, ("5/16", 0)), ("1/32", "1/16")),
        (Line.through(head, (1, "11/16")), ("15/16", "31/32")),
    )
    for wing, outside in wings:
        paper.valley(wing, flap=outside)
    # Fold the feet up, and the head down: both show the black front.
    paper.valley(fold_onto(feet, ("3/4", "1/4")), flap=("31/32", "1/64"))
    paper.valley(fold_onto(head, ("1/4", "3/4")), flap=("1/64", "31/32"), layers=1)
    # Stand the penguin on its feet.
    paper.tilt(Line.through(("7/8", 0), (1, "1/8")), flap=("1/2", "1/2"), angle=85)
    return paper
