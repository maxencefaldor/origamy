# Copyright 2026 Maxence Faldor
"""The spiral pleat: concentric dodecagons, each turned 15° on the last, pleated."""

from origamy import Number, Paper, Point

RINGS = 7
"""The concentric rings, from the center out."""
PLEAT = 110
"""How far each ring's pleat turns, in degrees."""
CENTER = Point.of(("1/2", "1/2"))
"""The paper's center, which every ring turns about."""

type Segment = tuple[Point, Point]


def corner(ring: int, k: int) -> Point:
    """The kth corner of a ring, each ring turned 15° on the one inside it."""
    return CENTER + Point.polar(f"{ring}/{2 * (RINGS + 1)}", 15 * (2 * k + ring))


def rim(degrees: int) -> Point:
    """Where the ray from the center at an angle meets the paper's edge."""
    unit = Point.polar(1, degrees)
    return CENTER + unit * (Number.of("1/2") / max(abs(unit.x), abs(unit.y)))


def fold() -> Paper:
    """Crease concentric dodecagons, each turned on the last, and the slants between.

    Each ring is a dodecagon turned 15° further than the one inside it, so the
    segments joining their corners slant, and the rings alternate valley and
    mountain. The pleats shorten the rings, and the slants twist the paper as it
    buckles, so it settles into a spiral shell.
    """
    paper = Paper.square(front="#efe8d8", back="#7a2e4a")
    pleats: list[tuple[Segment, int]] = [
        ((corner(ring, k), corner(ring, k + 1)), PLEAT if ring % 2 else -PLEAT)
        for ring in range(1, RINGS + 1)
        for k in range(12)
    ]
    slants: list[Segment] = [
        (
            corner(ring, k),
            corner(ring + 1, k) if ring < RINGS else rim(15 * (2 * k + RINGS + 1)),
        )
        for ring in range(1, RINGS + 1)
        for k in range(12)
    ]
    paper.crease([segment for segment, _ in pleats] + slants)
    paper.settle(dict(pleats))
    return paper
