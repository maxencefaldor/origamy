# Copyright 2026 Maxence Faldor
"""A crown: a 24-sided floor whose radiating pleats fold far and close into points."""

from origamy import Number, Paper, Point

FLOOR, PLEAT = 80, 150
"""How far the floor's edge and each radiating pleat turn, in degrees."""
SIDES = 24
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

    The floor's edge turns up as a valley, and the pleats alternate valley and
    mountain, folding far. They gather the paper into a band round the floor, and the
    paper's corners rise from it as the crown's points.
    """
    paper = Paper.square(front="#d4a62a", back="#f5e6b8")
    center = Point.of(("1/2", "1/2"))

    def at(k: int) -> Point:
        unit, reach = direction(k), Number.of("1/4")
        return Point(center.x + unit.x * reach, center.y + unit.y * reach)

    def edge(k: int) -> Point:
        unit, half = direction(k), Number.of("1/2")
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
