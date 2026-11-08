# Copyright 2026 Maxence Faldor
"""The circular pleat: concentric rings pleated, buckling the paper into a saddle."""

from origamy import Number, Paper, Point

RINGS = 7
"""The concentric rings, from the center out."""
SIDES = 24
"""The sides of each ring: a regular polygon close to a circle."""
PLEAT = 110
"""How far each ring's pleat turns, in degrees."""
CENTER = Point.of(("1/2", "1/2"))
"""The paper's center, which every ring surrounds."""

type Segment = tuple[Point, Point]


def corner(ring: int, k: int) -> Point:
    """The kth corner of a ring, the rings evenly spaced out to the paper's edge."""
    return CENTER + Point.polar(f"{ring}/{2 * (RINGS + 1)}", k * 360 // SIDES)


def rim(degrees: int) -> Point:
    """Where the ray from the center at an angle meets the paper's edge."""
    unit = Point.polar(1, degrees)
    return CENTER + unit * (Number.of("1/2") / max(abs(unit.x), abs(unit.y)))


def fold() -> Paper:
    """Crease concentric polygons and the radii between them, then let the rings settle.

    The rings alternate valley and mountain. The radii keep each face convex and turn
    no way, so the paper bends across them. The rings shorten as they pleat, and the
    paper buckles into a saddle.
    """
    paper = Paper.square(front="#efe8d8", back="#2f4f6f")
    pleats: list[tuple[Segment, int]] = [
        ((corner(ring, k), corner(ring, k + 1)), PLEAT if ring % 2 else -PLEAT)
        for ring in range(1, RINGS + 1)
        for k in range(SIDES)
    ]
    radii: list[Segment] = [
        (
            corner(ring, k),
            corner(ring + 1, k) if ring < RINGS else rim(k * 360 // SIDES),
        )
        for ring in range(1, RINGS + 1)
        for k in range(SIDES)
    ]
    paper.crease([segment for segment, _ in pleats] + radii)
    paper.settle(dict(pleats))
    return paper
