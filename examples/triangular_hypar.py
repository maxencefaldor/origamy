# Copyright 2026 Maxence Faldor
"""The triangular hypar: concentric triangles pleated on equilateral paper."""

from origamy import Number, Paper, Point

RINGS = 7
"""The concentric triangles, the paper's edge the last."""
PLEAT = 100
"""How far each triangle's pleat, and each spoke's segment, turns, in degrees."""

type Segment = tuple[Point, Point]


def fold() -> Paper:
    """Crease concentric triangles and the spokes to the corners, then let them settle.

    The triangles alternate valley and mountain, from the center out. Each spoke's
    segment between two triangles folds as the inner one does. The pleats shorten
    the triangles, and the paper curls its three corners into a twisted star.
    """
    corners = [Point.of((0, 0)), Point.of((1, 0)), Point.of(("1/2", "√3/2"))]
    paper = Paper.polygon(corners, front="#e9e1cf", back="#2c5d8a")
    center = Point.of(("1/2", "√3/6"))

    def at(ring: int, k: int) -> Point:
        corner, share = corners[k % 3], Number.of(f"{ring}/{RINGS}")
        return Point(
            center.x + (corner.x - center.x) * share,
            center.y + (corner.y - center.y) * share,
        )

    pleats: list[tuple[Segment, int]] = [
        ((at(ring, k), at(ring, k + 1)), PLEAT if ring % 2 else -PLEAT)
        for ring in range(1, RINGS)
        for k in range(3)
    ]
    # The spokes inside the first triangle fold as it does.
    pleats += [
        ((at(ring, k), at(ring + 1, k)), PLEAT if max(ring, 1) % 2 else -PLEAT)
        for ring in range(RINGS)
        for k in range(3)
    ]
    paper.crease([segment for segment, _ in pleats])
    paper.settle(dict(pleats))
    return paper
