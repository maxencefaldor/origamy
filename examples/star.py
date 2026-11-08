# Copyright 2026 Maxence Faldor
"""A four-pointed star: eight rays from the center, alternately valley and mountain."""

from origamy import Paper, Point

RAY = 120
"""How far each ray turns, in degrees."""
CENTER = Point.of(("1/2", "1/2"))
"""The paper's center, where the rays meet."""
ENDS = tuple(
    Point.of(end)
    for end in (
        (1, "1/2"),
        (1, 1),
        ("1/2", 1),
        (0, 1),
        (0, "1/2"),
        (0, 0),
        ("1/2", 0),
        (1, 0),
    )
)
"""Where the rays meet the paper's edge, every 45° counterclockwise from the x-axis."""

type Segment = tuple[Point, Point]


def fold() -> Paper:
    """Crease eight rays from the center to the edges, then let them settle.

    The rays run every 45°, to the corners and to the middles of the edges, and
    alternate valley and mountain. Four valleys gather the paper between four
    mountains, which rise as the star's points.
    """
    paper = Paper.square(front="#5b3f8c", back="#efe9f6")
    rays: list[tuple[Segment, int]] = [
        ((CENTER, end), RAY if k % 2 else -RAY) for k, end in enumerate(ENDS)
    ]
    paper.crease([segment for segment, _ in rays])
    paper.settle(dict(rays))
    return paper
