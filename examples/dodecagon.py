# Copyright 2026 Maxence Faldor
"""A regular dodecagon, four of its sides on the square's edges."""

from origamy import Line, Number, Paper, Point

COSINES = ("1", "√6/4 + √2/4", "√3/2", "√2/2", "1/2", "√6/4 - √2/4", "0")
"""The cosines of each multiple of 15° up to 90°, exact in the tower of square roots."""


def direction(k: int) -> Point:
    """The unit vector k times 15° counterclockwise from the x-axis."""
    quarter, step = divmod(k % 24, 6)
    c, s = Number.of(COSINES[step]), Number.of(COSINES[6 - step])
    for _ in range(quarter):
        c, s = -s, c
    return Point(c, s)


def fold() -> Paper:
    """Fold the paper outside each of the eight slanted sides behind.

    A regular dodecagon whose incircle touches the square's edges has its corners at
    15° plus multiples of 30°, a distance (√6 - √2)/2 from the center. Two slanted
    sides cut each corner of the square, at 30° and 60°.
    """
    paper = Paper.square(front="#1f6f8b", back="#f2ede4")
    center = Point.of(("1/2", "1/2"))
    reach = (Number.of(6).sqrt() - Number.of(2).sqrt()) / 2
    corners = [
        Point(center.x + d.x * reach, center.y + d.y * reach)
        for d in (direction(1 + 2 * k) for k in range(12))
    ]
    for k in range(12):
        start, end = corners[k], corners[(k + 1) % 12]
        # The sides square to the axes lie on the paper's edges.
        if k % 3 == 2:
            continue
        middle = Point((start.x + end.x) / 2, (start.y + end.y) / 2)
        outward = Point(middle.x - center.x, middle.y - center.y)
        outside = Point(middle.x + outward.x / 8, middle.y + outward.y / 8)
        paper.mountain(Line.through(start, end), flap=outside)
    return paper
