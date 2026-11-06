# Copyright 2026 Maxence Faldor
"""Miura-ori: a grid of parallelograms, creased open, collapsed at once, then opened."""

from fractions import Fraction
from itertools import pairwise

from origamy import Line, Paper

COLUMNS = 8
ROWS = 8
SLANT = Fraction(1, 4)
"""How far each zigzag rises across one column, as a share of a row's height."""
OPEN = 90
"""The angle, in degrees, each column's creases open to once the paper lies folded."""

type Segment = tuple[tuple[Fraction, Fraction], tuple[Fraction, Fraction]]


def fold() -> Paper:
    """Crease the zigzags and the columns, collapse them together, then open them."""
    width, height = Fraction(1, COLUMNS), Fraction(1, ROWS)

    def corner(column: int, row: int) -> tuple[Fraction, Fraction]:
        """A zigzag's corner: odd columns sit a slant above even ones."""
        rise = SLANT * height * (column % 2 - Fraction(1, 2))
        return (width * column, height * row + rise)

    valleys: list[Segment] = []
    mountains: list[Segment] = []
    # Zigzags fold mountain and valley by turns, row after row.
    for row in range(1, ROWS):
        side = mountains if row % 2 else valleys
        side += [
            (corner(column, row), corner(column + 1, row)) for column in range(COLUMNS)
        ]
    # Each column's segment between two zigzags folds the way Maekawa's theorem needs.
    for column in range(1, COLUMNS):
        ends = [(width * column, Fraction(0))]
        ends += [corner(column, row) for row in range(1, ROWS)]
        ends += [(width * column, Fraction(1))]
        for row, segment in enumerate(pairwise(ends)):
            (mountains if (column + row) % 2 == 0 else valleys).append(segment)
    paper = Paper.square(front="#2f6f8f", back="#e8f1f5")
    paper.crease([*valleys, *mountains])
    paper.collapse(valleys=valleys, mountains=mountains)
    # Rigid origami: the columns open, and the zigzags follow.
    columns = [Line.through((width * c, 0), (width * c, 1)) for c in range(1, COLUMNS)]
    paper.flex(dict.fromkeys(columns, OPEN))
    return paper
