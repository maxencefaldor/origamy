# Copyright 2026 Maxence Faldor
"""The hexagonal hypar: concentric hexagons pleated on hexagonal paper."""

from origamy import Paper, Point

RINGS = 6
"""The concentric hexagons, the paper's edge the last."""
PLEAT = 100
"""How far each hexagon's pleat, and each spoke's segment, turns, in degrees."""
CENTER = Point.of(("1/2", "√3/4"))
"""The hexagon's center, its lowest side on the x-axis."""

type Segment = tuple[Point, Point]


def corner(ring: int, k: int) -> Point:
    """The kth corner of a hexagon, the hexagons evenly spaced out to the edge."""
    return CENTER + Point.polar(f"{ring}/{2 * RINGS}", 60 * k)


def fold() -> Paper:
    """Crease concentric hexagons and the spokes to the corners, then let them settle.

    The hexagons alternate valley and mountain, from the center out. Each spoke's
    segment between two hexagons folds as the inner one does. Six corners pull where a
    square has four, so the paper buckles into a saddle with three rises and three
    dips.
    """
    paper = Paper.polygon(
        [corner(RINGS, k) for k in range(6)], front="#e9e1cf", back="#3e6b4f"
    )
    pleats: list[tuple[Segment, int]] = [
        ((corner(ring, k), corner(ring, k + 1)), PLEAT if ring % 2 else -PLEAT)
        for ring in range(1, RINGS)
        for k in range(6)
    ]
    # The spokes inside the first hexagon fold as it does.
    pleats += [
        ((corner(ring, k), corner(ring + 1, k)), PLEAT if max(ring, 1) % 2 else -PLEAT)
        for ring in range(RINGS)
        for k in range(6)
    ]
    paper.crease([segment for segment, _ in pleats])
    paper.settle(dict(pleats))
    return paper
