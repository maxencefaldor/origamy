# Copyright 2026 Maxence Faldor
"""A bell: a dodecagon that rises as the crown, its pleats falling round it."""

from origamy import Number, Paper, Point

RIM, PLEAT = 90, 120
"""How far the rim's edge and each radiating pleat turn, in degrees."""
SIDES = 12
"""The dodecagon's sides, and the pleats that radiate from its corners."""

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
    """Crease a dodecagon and the pleats from its corners to the edges, then settle.

    The dodecagon's edge turns as a valley, and the pleats alternate valley and
    mountain, folding toward 120°. The dodecagon rises as the bell's crown, and the
    pleated paper falls round it.
    """
    paper = Paper.square(front="#3b6ea8", back="#eef3f9")
    center = Point.of(("1/2", "1/2"))

    def at(k: int) -> Point:
        unit, reach = direction(2 * k), Number.of("1/6")
        return Point(center.x + unit.x * reach, center.y + unit.y * reach)

    def edge(k: int) -> Point:
        unit, half = direction(2 * k), Number.of("1/2")
        reach = half / max(abs(unit.x), abs(unit.y))
        return Point(center.x + unit.x * reach, center.y + unit.y * reach)

    rim: list[tuple[Segment, int]] = [((at(k), at(k + 1)), RIM) for k in range(SIDES)]
    pleats: list[tuple[Segment, int]] = [
        ((at(k), edge(k)), PLEAT if k % 2 else -PLEAT) for k in range(SIDES)
    ]
    paper.crease([segment for segment, _ in rim + pleats])
    paper.settle(dict(rim + pleats))
    return paper
