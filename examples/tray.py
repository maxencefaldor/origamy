# Copyright 2026 Maxence Faldor
"""A tray: walls turned up round a square floor, each corner pinched into a fin."""

from fractions import Fraction

from origamy import Paper

WALL, FIN = 90, -170
"""How far the walls and the corners' diagonals turn, in degrees."""

type Corner = tuple[Fraction, Fraction]


def fold() -> Paper:
    """Crease the floor's edges, their extensions and the corner diagonals, then settle.

    The floor is the middle square, from 1/4 to 3/4. Its edges, carried on to the
    paper's edges, fold up as valleys. Each corner square folds in half along its
    diagonal as a mountain, so the walls meet and the corner pinches into a fin.
    """
    paper = Paper.square(front="#b5523b", back="#f2e8d5")
    low, high = Fraction(1, 4), Fraction(3, 4)
    corners: list[Corner] = [(low, low), (high, low), (high, high), (low, high)]
    walls: list[tuple[Corner, Corner]] = []
    fins: list[tuple[Corner, Corner]] = []
    for (x, y), (nx, ny) in zip(corners, corners[1:] + corners[:1], strict=True):
        walls.append(((x, y), (nx, ny)))
        # Carry the floor's corner out to the paper's edges, and to its corner.
        edge_x = Fraction(0) if x == low else Fraction(1)
        edge_y = Fraction(0) if y == low else Fraction(1)
        walls += [((x, edge_y), (x, y)), ((edge_x, y), (x, y))]
        fins.append(((edge_x, edge_y), (x, y)))
    paper.crease(walls + fins)
    paper.settle({**dict.fromkeys(walls, WALL), **dict.fromkeys(fins, FIN)})
    return paper
