# Copyright 2026 Maxence Faldor
"""A lotus: an octagonal cup whose pleats flare out from its rim as petals."""

from origamy import Number, Paper, Point

RIM, PETAL = 70, 110
"""How far the rim's edge and each radiating pleat turn, in degrees."""
SIDES = 8
"""The octagon's sides, and the pleats that radiate from its corners."""

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
    """Crease an octagon and the pleats from its corners to the edges, then settle.

    The octagon's edge turns up as a valley, and the pleats alternate valley and
    mountain, folding toward 110°. The paper gathers into a cup round the octagon,
    and the pleats between its corners flare out as petals.
    """
    paper = Paper.square(front="#e88aa8", back="#f8e8ee")
    center = Point.of(("1/2", "1/2"))

    def at(k: int) -> Point:
        unit, reach = direction(3 * k), Number.of("1/5")
        return Point(center.x + unit.x * reach, center.y + unit.y * reach)

    def edge(k: int) -> Point:
        unit, half = direction(3 * k), Number.of("1/2")
        reach = half / max(abs(unit.x), abs(unit.y))
        return Point(center.x + unit.x * reach, center.y + unit.y * reach)

    rim: list[tuple[Segment, int]] = [((at(k), at(k + 1)), RIM) for k in range(SIDES)]
    petals: list[tuple[Segment, int]] = [
        ((at(k), edge(k)), PETAL if k % 2 else -PETAL) for k in range(SIDES)
    ]
    paper.crease([segment for segment, _ in rim + petals])
    paper.settle(dict(rim + petals))
    return paper
