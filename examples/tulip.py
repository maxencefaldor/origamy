# Copyright 2026 Maxence Faldor
"""The traditional tulip: a triangle whose bottom corners rise as petals, then open."""

from origamy import Line, Paper, Point, bisect, meet, midpoint

PETAL = 120
"""The angle, in degrees, each side petal opens to from pressed flat."""
CUP = 150
"""The angle, in degrees, the flower's two layers open to, as a cup."""


def fold() -> Paper:
    """Fold a triangle, then raise each bottom corner past the top as a petal.

    The triangle's long edge, the paper's diagonal from (0, 0) to (1, 1), is the
    flower's base, and its right angle at (0, 1) the middle petal. Each side petal
    rises 22.5° off the flower's axis. Last, the flower opens: the petals lift off the
    middle one and its layers part, the paper bending.
    """
    paper = Paper.square(front="#f6c1cf", back="#d6335f")
    left, right, top = (0, 0), (1, 1), (0, 1)
    paper.valley(Line.through(left, right), flap=("3/4", "1/4"))
    base = midpoint(left, right)
    petals: list[Line] = []
    for corner, edge_end, inside in (
        (left, (0, "1/2"), ("1/16", "1/8")),
        (right, ("1/2", 1), ("7/8", "15/16")),
    ):
        # The petal's tip goes halfway between the axis and the edge from the top.
        side = Line.through(top, edge_end)
        tip = meet(bisect(top, base, edge_end), side)
        petal = bisect(corner, base, tip)
        paper.valley(petal, flap=inside)
        # The petal's crease runs on in the layer beneath, mirrored in the diagonal.
        petals += [petal, Line.at(mirror(petal.point), mirror(petal.direction))]
    paper.settle({**dict.fromkeys(petals, PETAL), Line.through(left, right): CUP})
    return paper


def mirror(point: Point) -> Point:
    """A point mirrored in the paper's diagonal from (0, 0) to (1, 1)."""
    return Point(point.y, point.x)
