# Copyright 2026 Maxence Faldor
"""The pentagonal hypar: concentric pentagons pleated on pentagonal paper."""

from origamy import Number, Paper, Point

RINGS = 7
"""The concentric pentagons, the paper's edge the last."""
PLEAT = 100
"""How far each pentagon's pleat, and each spoke's segment, turns, in degrees."""

type Segment = tuple[Point, Point]


def fold() -> Paper:
    """Crease concentric pentagons and the spokes to the corners, then let them settle.

    The paper is the regular pentagon of side (√5 - 1)/2, its base on the x-axis. The
    pentagons alternate valley and mountain, from the center out, and each spoke's
    segment folds as the pentagon inside it does. Five corners pull, and the paper
    settles into a crown of rises and dips.
    """
    side, half = (Number.of(5).sqrt() - 1) / 2, Number.of("1/2")
    low = side * (1 - side * side / 4).sqrt()
    top = low + (side * side - half * half).sqrt()
    corners = [
        Point(half - side / 2, Number.of(0)),
        Point(half + side / 2, Number.of(0)),
        Point(Number.of(1), low),
        Point(half, top),
        Point(Number.of(0), low),
    ]
    paper = Paper.polygon(corners, front="#efe6d2", back="#8a3b2e")
    # The center lies above the base's middle by the inradius, s/2 over tan 36°.
    center = Point(half, side / 2 / (5 - 2 * Number.of(5).sqrt()).sqrt())

    def at(ring: int, k: int) -> Point:
        corner, share = corners[k % 5], Number.of(f"{ring}/{RINGS}")
        return Point(
            center.x + (corner.x - center.x) * share,
            center.y + (corner.y - center.y) * share,
        )

    pleats: list[tuple[Segment, int]] = [
        ((at(ring, k), at(ring, k + 1)), PLEAT if ring % 2 else -PLEAT)
        for ring in range(1, RINGS)
        for k in range(5)
    ]
    # The spokes inside the first pentagon fold as it does.
    pleats += [
        ((at(ring, k), at(ring + 1, k)), PLEAT if max(ring, 1) % 2 else -PLEAT)
        for ring in range(RINGS)
        for k in range(5)
    ]
    paper.crease([segment for segment, _ in pleats])
    paper.settle(dict(pleats))
    return paper
