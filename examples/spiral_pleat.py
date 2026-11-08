# Copyright 2026 Maxence Faldor
"""The spiral pleat: concentric dodecagons, each turned 15° on the last, pleated."""

from origamy import Number, Paper, Point

RINGS = 7
"""The concentric rings, from the center out."""
PLEAT = 110
"""How far each ring's pleat turns, in degrees."""

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
    """Crease concentric dodecagons, each turned on the last, and the slants between.

    Each ring is a dodecagon turned 15° further than the one inside it, so the
    segments joining their corners slant, and the rings alternate valley and
    mountain. The pleats shorten the rings, and the slants twist the paper as it
    buckles, so it settles into a spiral shell.
    """
    paper = Paper.square(front="#efe8d8", back="#7a2e4a")
    center = Point.of(("1/2", "1/2"))

    def at(ring: int, k: int) -> Point:
        unit, reach = direction(2 * k + ring), Number.of(f"{ring}/{2 * (RINGS + 1)}")
        return Point(center.x + unit.x * reach, center.y + unit.y * reach)

    def edge(k: int) -> Point:
        unit, half = direction(2 * k + RINGS + 1), Number.of("1/2")
        reach = half / max(abs(unit.x), abs(unit.y))
        return Point(center.x + unit.x * reach, center.y + unit.y * reach)

    pleats: list[tuple[Segment, int]] = [
        ((at(ring, k), at(ring, k + 1)), PLEAT if ring % 2 else -PLEAT)
        for ring in range(1, RINGS + 1)
        for k in range(12)
    ]
    slants: list[Segment] = [
        (at(ring, k), at(ring + 1, k) if ring < RINGS else edge(k))
        for ring in range(1, RINGS + 1)
        for k in range(12)
    ]
    paper.crease([segment for segment, _ in pleats] + slants)
    paper.settle(dict(pleats))
    return paper
