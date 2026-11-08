# Copyright 2026 Maxence Faldor
"""A lotus: an octagonal cup whose pleats flare out from its rim as petals."""

from origamy import Number, Paper, Point

OCTAGON, PETAL = 70, 110
"""How far the octagon's sides and each radiating pleat turn, in degrees."""
SIDES = 8
"""The octagon's sides, and the pleats that radiate from its corners."""
CENTER = Point.of(("1/2", "1/2"))
"""The paper's center, where the octagon lies."""

type Segment = tuple[Point, Point]


def rim(degrees: int) -> Point:
    """Where the ray from the center at an angle meets the paper's edge."""
    unit = Point.polar(1, degrees)
    return CENTER + unit * (Number.of("1/2") / max(abs(unit.x), abs(unit.y)))


def fold() -> Paper:
    """Crease an octagon and the pleats from its corners to the edges, then settle.

    The octagon's sides turn up as valleys, and the pleats alternate valley and
    mountain, folding toward 110°. The paper gathers into a cup round the octagon,
    and the pleats between its corners flare out as petals.
    """
    paper = Paper.square(front="#e88aa8", back="#f8e8ee")
    angles = [k * 360 // SIDES for k in range(SIDES + 1)]
    corners = [CENTER + Point.polar("1/5", angle) for angle in angles]
    octagon: list[tuple[Segment, int]] = [
        ((corners[k], corners[k + 1]), OCTAGON) for k in range(SIDES)
    ]
    petals: list[tuple[Segment, int]] = [
        ((corners[k], rim(angles[k])), PETAL if k % 2 else -PETAL) for k in range(SIDES)
    ]
    paper.crease([segment for segment, _ in octagon + petals])
    paper.settle(dict(octagon + petals))
    return paper
