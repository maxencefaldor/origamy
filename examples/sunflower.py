# Copyright 2026 Maxence Faldor
"""A sunflower: a 24-sided disc that domes up, pleats radiating from it as petals."""

from origamy import Number, Paper, Point

DISC, PETAL = 60, 80
"""How far the disc's edge and each radiating pleat turn, in degrees."""
SIDES = 24
"""The disc's sides, and the pleats that radiate from its corners."""

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
    """Crease a disc and the pleats from its corners to the edges, then let them settle.

    The disc's edge turns as a valley, and the pleats alternate valley and mountain.
    The pleats gather the paper round the disc, which domes up as the heart of the
    flower, while the pleated paper spreads round it as petals.
    """
    paper = Paper.square(front="#e7b93a", back="#f7f0dc")
    center = Point.of(("1/2", "1/2"))

    def at(k: int) -> Point:
        unit, reach = direction(k), Number.of("1/4")
        return Point(center.x + unit.x * reach, center.y + unit.y * reach)

    def edge(k: int) -> Point:
        unit, half = direction(k), Number.of("1/2")
        reach = half / max(abs(unit.x), abs(unit.y))
        return Point(center.x + unit.x * reach, center.y + unit.y * reach)

    disc: list[tuple[Segment, int]] = [((at(k), at(k + 1)), DISC) for k in range(SIDES)]
    petals: list[tuple[Segment, int]] = [
        ((at(k), edge(k)), PETAL if k % 2 else -PETAL) for k in range(SIDES)
    ]
    paper.crease([segment for segment, _ in disc + petals])
    paper.settle(dict(disc + petals))
    return paper
