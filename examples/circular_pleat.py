# Copyright 2026 Maxence Faldor
"""The circular pleat: concentric rings pleated, buckling the paper into a saddle."""

from origamy import Number, Paper, Point

RINGS = 7
"""The concentric rings, from the center out."""
SIDES = 24
"""The sides of each ring: a regular polygon close to a circle."""
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
    """Crease concentric polygons and the radii between them, then let the rings settle.

    The rings alternate valley and mountain. The radii keep each face convex and turn
    no way, so the paper bends across them. The rings shorten as they pleat, and the
    paper buckles into a saddle.
    """
    paper = Paper.square(front="#efe8d8", back="#2f4f6f")
    center = Point.of(("1/2", "1/2"))

    def at(ring: int, k: int) -> Point:
        unit, reach = direction(k), Number.of(f"{ring}/{2 * (RINGS + 1)}")
        return Point(center.x + unit.x * reach, center.y + unit.y * reach)

    def edge(k: int) -> Point:
        unit, half = direction(k), Number.of("1/2")
        reach = half / max(abs(unit.x), abs(unit.y))
        return Point(center.x + unit.x * reach, center.y + unit.y * reach)

    pleats: list[tuple[Segment, int]] = [
        ((at(ring, k), at(ring, k + 1)), PLEAT if ring % 2 else -PLEAT)
        for ring in range(1, RINGS + 1)
        for k in range(SIDES)
    ]
    radii: list[Segment] = [
        (at(ring, k), at(ring + 1, k) if ring < RINGS else edge(k))
        for ring in range(1, RINGS + 1)
        for k in range(SIDES)
    ]
    paper.crease([segment for segment, _ in pleats] + radii)
    paper.settle(dict(pleats))
    return paper
