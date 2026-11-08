# Copyright 2026 Maxence Faldor
"""The hexagonal hypar: concentric hexagons pleated on hexagonal paper."""

from origamy import Number, Paper, Point

RINGS = 6
"""The concentric hexagons, the paper's edge the last."""
PLEAT = 100
"""How far each hexagon's pleat, and each spoke's segment, turns, in degrees."""

COSINES = ("1", "1/2", "-1/2", "-1", "-1/2", "1/2")
SINES = ("0", "√3/2", "√3/2", "0", "-√3/2", "-√3/2")
"""Each corner's direction from the hexagon's center, a sixth of a turn apart."""

type Segment = tuple[Point, Point]


def fold() -> Paper:
    """Crease concentric hexagons and the spokes to the corners, then let them settle.

    The hexagons alternate valley and mountain, from the center out. Each spoke's
    segment between two hexagons folds as the inner one does. Six corners pull where a
    square has four, so the paper buckles into a saddle with three rises and three
    dips.
    """
    center = Point.of(("1/2", "√3/4"))
    half = Number.of("1/2")
    corners = [
        Point(center.x + Number.of(c) * half, center.y + Number.of(s) * half)
        for c, s in zip(COSINES, SINES, strict=True)
    ]
    paper = Paper.polygon(corners, front="#e9e1cf", back="#3e6b4f")

    def at(ring: int, k: int) -> Point:
        corner, share = corners[k % 6], Number.of(f"{ring}/{RINGS}")
        return Point(
            center.x + (corner.x - center.x) * share,
            center.y + (corner.y - center.y) * share,
        )

    pleats: list[tuple[Segment, int]] = [
        ((at(ring, k), at(ring, k + 1)), PLEAT if ring % 2 else -PLEAT)
        for ring in range(1, RINGS)
        for k in range(6)
    ]
    # The spokes inside the first hexagon fold as it does.
    pleats += [
        ((at(ring, k), at(ring + 1, k)), PLEAT if max(ring, 1) % 2 else -PLEAT)
        for ring in range(RINGS)
        for k in range(6)
    ]
    paper.crease([segment for segment, _ in pleats])
    paper.settle(dict(pleats))
    return paper
