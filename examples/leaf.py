# Copyright 2026 Maxence Faldor
"""A pleated leaf: pleats across a square's diagonal, which settle about a midrib."""

from fractions import Fraction

from origamy import Paper

PLEATS = 12
"""The pleats on each side of the midrib, from its middle out."""
PLEAT, RIB = 120, 40
"""How far each pleat and the midrib turn, in degrees."""

type Corner = tuple[Fraction, Fraction]


def fold() -> Paper:
    """Crease pleats square to one diagonal, and the other diagonal as the midrib.

    The pleats alternate valley and mountain, and each one crosses the midrib. The
    midrib folds a little, and the pleats fold far, so the leaf's halves curl.
    """
    paper = Paper.square(front="#5f8f3a", back="#a9c46b")
    creases: list[tuple[tuple[Corner, Corner], int]] = []
    rib = ((Fraction(0), Fraction(0)), (Fraction(1), Fraction(1)))
    creases.append((rib, RIB))
    # Each pleat runs along x + y = c, square to the midrib, from edge to edge.
    for k in range(1, 2 * PLEATS):
        c = Fraction(k, PLEATS)
        if c <= 1:
            ends = ((c, Fraction(0)), (Fraction(0), c))
        else:
            ends = ((Fraction(1), c - 1), (c - 1, Fraction(1)))
        creases.append((ends, PLEAT if k % 2 else -PLEAT))
    paper.crease([segment for segment, _ in creases])
    paper.settle(dict(creases))
    return paper
