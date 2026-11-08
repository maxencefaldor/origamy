# Copyright 2026 Maxence Faldor
"""A sunflower: a 24-sided disc that domes up, pleats radiating from it as petals."""

from origamy import Number, Paper, Point

DISC, PETAL = 60, 80
"""How far the disc's edge and each radiating pleat turn, in degrees."""
SIDES = 24
"""The disc's sides, and the pleats that radiate from its corners."""
CENTER = Point.of(("1/2", "1/2"))
"""The paper's center, where the disc lies."""

type Segment = tuple[Point, Point]


def rim(degrees: int) -> Point:
    """Where the ray from the center at an angle meets the paper's edge."""
    unit = Point.polar(1, degrees)
    return CENTER + unit * (Number.of("1/2") / max(abs(unit.x), abs(unit.y)))


def fold() -> Paper:
    """Crease a disc and the pleats from its corners to the edges, then let them settle.

    The disc's edge turns as a valley, and the pleats alternate valley and mountain.
    The pleats gather the paper round the disc, which domes up as the heart of the
    flower, while the pleated paper spreads round it as petals.
    """
    paper = Paper.square(front="#e7b93a", back="#f7f0dc")
    angles = [k * 360 // SIDES for k in range(SIDES + 1)]
    corners = [CENTER + Point.polar("1/4", angle) for angle in angles]
    disc: list[tuple[Segment, int]] = [
        ((corners[k], corners[k + 1]), DISC) for k in range(SIDES)
    ]
    petals: list[tuple[Segment, int]] = [
        ((corners[k], rim(angles[k])), PETAL if k % 2 else -PETAL) for k in range(SIDES)
    ]
    paper.crease([segment for segment, _ in disc + petals])
    paper.settle(dict(disc + petals))
    return paper
