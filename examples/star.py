# Copyright 2026 Maxence Faldor
"""A four-pointed star: eight rays from the center, alternately valley and mountain."""

from origamy import Number, Paper, Point

RAY = 120
"""How far each ray turns, in degrees."""

COSINES = ("1", "√6/4 + √2/4", "√3/2", "√2/2", "1/2", "√6/4 - √2/4", "0")
"""The cosines of each multiple of 15° up to 90°, exact in the tower of square roots."""

type Segment = tuple[Point, Point]


def direction(k: int) -> Point:
    """The unit vector k times 15° counterclockwise from the x-axis."""
    quarter, step = divmod(k % 24, 6)
    c, s = Number.of(COSINES[step]), Number.of(COSINES[6 - step])
    for _ in range(quarter):
        c, s = -s, c
    return Point(c, s)


def fold() -> Paper:
    """Crease eight rays from the center to the edges, then let them settle.

    The rays run every 45°, to the corners and to the middles of the edges, and
    alternate valley and mountain. Four valleys gather the paper between four
    mountains, which rise as the star's points.
    """
    paper = Paper.square(front="#5b3f8c", back="#efe9f6")
    center = Point.of(("1/2", "1/2"))

    def edge(k: int) -> Point:
        unit, half = direction(k), Number.of("1/2")
        reach = half / max(abs(unit.x), abs(unit.y))
        return Point(center.x + unit.x * reach, center.y + unit.y * reach)

    rays: list[tuple[Segment, int]] = [
        ((center, edge(3 * k)), RAY if k % 2 else -RAY) for k in range(8)
    ]
    paper.crease([segment for segment, _ in rays])
    paper.settle(dict(rays))
    return paper
