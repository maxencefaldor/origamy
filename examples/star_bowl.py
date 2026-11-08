# Copyright 2026 Maxence Faldor
"""A star bowl: a dodecagon sunk in deep pleats, which gather into points."""

from origamy import Number, Paper, Point

FLOOR, PLEAT = -70, 100
"""How far the floor's edge and each radiating pleat turn, in degrees."""
SIDES = 12
"""The floor's sides, and the pleats that radiate from its corners."""

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
    """Crease a floor and the pleats from its corners to the edges, then settle.

    The floor's edge turns as a mountain, so the floor sinks, and the pleats alternate
    valley and mountain, folding toward 100°. The pleats gather the paper into walls
    that close round the floor, and the paper's corners stand out as points.
    """
    paper = Paper.square(front="#2f8a83", back="#e8f4f2")
    center = Point.of(("1/2", "1/2"))

    def at(k: int) -> Point:
        unit, reach = direction(2 * k), Number.of("1/4")
        return Point(center.x + unit.x * reach, center.y + unit.y * reach)

    def edge(k: int) -> Point:
        unit, half = direction(2 * k), Number.of("1/2")
        reach = half / max(abs(unit.x), abs(unit.y))
        return Point(center.x + unit.x * reach, center.y + unit.y * reach)

    floor: list[tuple[Segment, int]] = [
        ((at(k), at(k + 1)), FLOOR) for k in range(SIDES)
    ]
    pleats: list[tuple[Segment, int]] = [
        ((at(k), edge(k)), PLEAT if k % 2 else -PLEAT) for k in range(SIDES)
    ]
    paper.crease([segment for segment, _ in floor + pleats])
    paper.settle(dict(floor + pleats))
    return paper
