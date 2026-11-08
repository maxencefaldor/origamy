# Copyright 2026 Maxence Faldor
"""The hyperbolic paraboloid: concentric squares pleated, twisting into a saddle."""

from fractions import Fraction

from origamy import Paper

RINGS = 8
"""The concentric squares, the paper's edge the last."""
PLEAT = 100
"""How far each square's pleat, and each diagonal's segment, turns, in degrees."""

type Corner = tuple[Fraction, Fraction]


def fold() -> Paper:
    """Crease concentric squares and both diagonals, then let the pleats settle.

    The squares alternate valley and mountain, from the center out. Each diagonal
    segment between two squares folds as the inner square does, so every corner
    turns with its square. No flat state holds these pleats together: the paper
    bends, and the sheet twists into a saddle.
    """
    paper = Paper.square(front="#e8e2d0", back="#b5523b")
    corners = [(-1, -1), (1, -1), (1, 1), (-1, 1)]

    def at(ring: int, corner: tuple[int, int]) -> Corner:
        reach = Fraction(ring, 2 * RINGS)
        return Fraction(1, 2) + corner[0] * reach, Fraction(1, 2) + corner[1] * reach

    sides = list(zip(corners, corners[1:] + corners[:1], strict=True))
    pleats: list[tuple[tuple[Corner, Corner], int]] = []
    for ring in range(1, RINGS):
        for a, b in sides:
            pleats.append(((at(ring, a), at(ring, b)), PLEAT if ring % 2 else -PLEAT))
    # The segments inside the first square fold as it does.
    for ring in range(RINGS):
        for corner in corners:
            sense = PLEAT if max(ring, 1) % 2 else -PLEAT
            pleats.append(((at(ring, corner), at(ring + 1, corner)), sense))
    paper.crease([segment for segment, _ in pleats])
    paper.settle(dict(pleats))
    return paper
