# Copyright 2026 Maxence Faldor
"""The preliminary base: four creases through the center, collapsed at once."""

from origamy import Line, Paper


def fold() -> Paper:
    """Precrease both diagonals and both middle lines, then collapse."""
    paper = Paper.square()
    rising = Line.through((0, 0), (1, 1))
    falling = Line.through((1, 0), (0, 1))
    upright = Line.through(("1/2", 0), ("1/2", 1))
    level = Line.through((0, "1/2"), (1, "1/2"))
    paper.precrease(rising, flap=("3/4", "1/4"))
    paper.precrease(falling, flap=("7/8", "5/8"), mountain=True)
    paper.precrease(upright, flap=("3/4", "1/8"))
    paper.precrease(level, flap=("1/8", "3/4"))

    # Four valleys and two mountains meet at the center; the rising diagonal stays
    # flat, inside the flaps.
    paper.collapse(valleys=[upright, level], mountains=[falling], fixed=("7/8", "5/8"))
    return paper
