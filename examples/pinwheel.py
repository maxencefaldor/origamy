# Copyright 2026 Maxence Faldor
"""The traditional pinwheel: a square whose corners swing out as four vanes."""

from fractions import Fraction

from origamy import Line, Paper

type Corner = tuple[Fraction, Fraction]


def turned(point: Corner) -> Corner:
    """A point of the square, turned a quarter clockwise about its center."""
    x, y = point
    return y, 1 - x


def fold() -> Paper:
    """Crease the middle square, the corners' diagonals and the slants, then collapse.

    The pattern turns onto itself a quarter turn about the center, as a pinwheel does.
    The middle square's sides and the corners' diagonals fold as valleys, and each
    slant from a corner of the middle square to the middle of an edge as a mountain.
    """
    paper = Paper.square(front="#f2ede4", back="#d6453d")
    q, t, h = Fraction(1, 4), Fraction(3, 4), Fraction(1, 2)
    one = {
        "valleys": [((q, t), (t, t)), ((0, 1), (q, t))],
        "mountains": [((q, t), (h, 1))],
    }
    valleys, mountains = [], []
    for kind, segments in one.items():
        for start, end in segments:
            a, b = (
                (Fraction(start[0]), Fraction(start[1])),
                (Fraction(end[0]), Fraction(end[1])),
            )
            for _ in range(4):
                (valleys if kind == "valleys" else mountains).append((a, b))
                a, b = turned(a), turned(b)
    paper.crease(valleys + mountains)
    paper.collapse(valleys=valleys, mountains=mountains, fixed=("1/2", "1/2"))
    # Stand the pinwheel up, as on its stick.
    paper.tilt(Line.through((0, 0), (1, 0)), flap=("1/2", "1/2"), angle=80)
    return paper
