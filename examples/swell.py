# Copyright 2026 Maxence Faldor
"""An original: a Miura-ori whose cells swell from one edge to the other."""

from fractions import Fraction
from itertools import pairwise

from origamy import Line, Paper

COLUMNS = 8
ROWS = 6
GROWTH = Fraction(5, 4)
"""How much wider each column is than the one before it."""
SLANT = Fraction(1, 2)
"""How far each zigzag rises across a column, as a share of the column's width."""
OPEN = 60
"""The angle, in degrees, each column's creases open to once the paper lies folded."""

type Point = tuple[Fraction, Fraction]
type Segment = tuple[Point, Point]


def fold() -> Paper:
    """Crease swelling zigzags and columns, collapse them, then open them.

    Each column is wider than the one before it, and its zigzag offset grows with it.
    Every zigzag segment then keeps one slope, and every vertex the Miura-ori's angles.
    The paper still opens as rigid origami, its cells swelling as they go.
    """
    widths = [GROWTH**column for column in range(COLUMNS)]
    total = sum(widths)
    edges = [sum(widths[:column], Fraction(0)) / total for column in range(COLUMNS + 1)]
    # Offsets grow by the columns' ratio, so two neighbors sum to a slant of a width.
    offsets = [SLANT * GROWTH**k / ((1 + GROWTH) * total) for k in range(COLUMNS + 1)]
    height = Fraction(1, ROWS)

    def corner(column: int, row: int) -> Point:
        """A zigzag's corner, up on odd column lines and down on even ones."""
        sign = 1 if column % 2 else -1
        return (edges[column], height * row + sign * offsets[column])

    valleys: list[Segment] = []
    mountains: list[Segment] = []
    for row in range(1, ROWS):
        side = mountains if row % 2 else valleys
        side += [(corner(c, row), corner(c + 1, row)) for c in range(COLUMNS)]
    for column in range(1, COLUMNS):
        ends = [(edges[column], Fraction(0))]
        ends += [corner(column, row) for row in range(1, ROWS)]
        ends += [(edges[column], Fraction(1))]
        for row, segment in enumerate(pairwise(ends)):
            (mountains if (column + row) % 2 == 0 else valleys).append(segment)
    paper = Paper.square(front="#16796f", back="#eef6f3")
    paper.crease([*valleys, *mountains])
    paper.collapse(valleys=valleys, mountains=mountains)
    columns = [Line.through((x, 0), (x, 1)) for x in edges[1:-1]]
    paper.flex(dict.fromkeys(columns, OPEN))
    return paper
