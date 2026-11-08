# Copyright 2026 Maxence Faldor
"""A regular dodecagon, four of its sides on the square's edges."""

from origamy import Line, Number, Paper, Point, midpoint

CENTER = Point.of(("1/2", "1/2"))
"""The paper's center, the dodecagon's too."""


def fold() -> Paper:
    """Fold the paper outside each of the eight slanted sides behind.

    A regular dodecagon whose incircle touches the square's edges has its corners at
    15° plus multiples of 30°, a distance (√6 - √2)/2 from the center. Two slanted
    sides cut each corner of the square, at 30° and 60°.
    """
    paper = Paper.square(front="#1f6f8b", back="#f2ede4")
    reach = (Number.of(6).sqrt() - Number.of(2).sqrt()) / 2
    corners = [CENTER + Point.polar(reach, 15 + 30 * k) for k in range(12)]
    for k in range(12):
        start, end = corners[k], corners[(k + 1) % 12]
        # The sides square to the axes lie on the paper's edges.
        if k % 3 == 2:
            continue
        middle = midpoint(start, end)
        outside = middle + (middle - CENTER) * Number.of("1/8")
        paper.mountain(Line.through(start, end), flap=outside)
    return paper
