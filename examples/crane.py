# Copyright 2026 Maxence Faldor
"""The traditional crane, orizuru: a bird base, narrowed, with a neck, tail and head."""

from origamy import Line, Paper, bisect, meet, midpoint, perpendicular


def fold() -> Paper:
    """Fold a bird base, narrow its legs, reverse fold them up, then fold the wings."""
    paper = Paper.square()
    bird_base(paper)

    # Narrow each leg on the front, then on the back: fold its outer edge onto the
    # center line. The creases bisect the bird base's 22.5° points: 11.25° geometry.
    corners = []
    for outer in (
        (("√2/2 - 1/2", "1/2"), ("1/2", "√2/2 - 1/2")),
        (("1/2", "3/2 - √2/2"), ("3/2 - √2/2", "1/2")),
    ):
        tip, middle = paper.where((1, 0)), paper.where((0, "1/2"))
        for side in (paper.where(point) for point in outer):
            crease = bisect(side, tip, middle)
            end = meet(crease, Line.through(side, middle))
            corners.append(meet(crease, Line.through(side, paper.where((0, 0)))))
            paper.valley(crease, flap=midpoint(side, midpoint(tip, end)), layers=2)
        paper.turn_over()

    # Reverse fold each leg up between the wings: the neck square to one wing's edge,
    # the tail square to the other's.
    tip, wing, middle = (paper.where(point) for point in ((1, 0), (0, 0), (0, "1/2")))
    left, right = corners[:2]
    hinges = []
    for leg, other in ((left, right), (right, left)):
        line = perpendicular(Line.through(wing, other), middle)
        hinges.append(meet(line, Line.through(tip, leg)))
        flap = midpoint(tip, midpoint(middle, hinges[-1]))
        paper.reverse(line, flap=flap, layers=paper.layers(flap))

    # Reverse fold the head, square to the wing's edge through its corner.
    neck = paper.where((0, 1))
    head = perpendicular(Line.through(wing, left), left)
    top, bottom = (meet(head, Line.through(neck, end)) for end in (middle, hinges[0]))
    flap = midpoint(neck, midpoint(top, bottom))
    paper.reverse(head, flap=flap, layers=paper.layers(flap))

    # Fold each wing down at the shoulders, then raise it 30° above level. Last, hold
    # the bird up by its shoulders, its neck and tail rising.
    wings = (((0, 0), ("1/8", "1/16")), ((1, 1), ("7/8", "15/16")))
    for tip, inside in wings:
        paper.valley(shoulders(paper, tip), flap=paper.where(inside), layers=1)
        paper.turn_over()
    for tip, inside in wings:
        paper.shape(
            shoulders(paper, tip), flap=paper.where(inside), angle=120, layers=1
        )
        paper.turn_over()
    paper.tilt(shoulders(paper, (0, 0)), flap=paper.where((1, 0)), angle=90)
    return paper


def shoulders(paper: Paper, tip: tuple[int, int]) -> Line:
    """Where a wing folds down: square to the line from its tip, through the center."""
    axis = Line.through(paper.where(tip), paper.where((0, "1/2")))
    return perpendicular(axis, paper.where(("1/2", "1/2")))


def bird_base(paper: Paper) -> None:
    """Collapse a preliminary base, then petal fold its front and its back."""
    rising = Line.through((0, 0), (1, 1))
    falling = Line.through((1, 0), (0, 1))
    upright = Line.through(("1/2", 0), ("1/2", 1))
    level = Line.through((0, "1/2"), (1, "1/2"))
    paper.precrease(rising, flap=("3/4", "1/4"))
    paper.precrease(falling, flap=("7/8", "5/8"), mountain=True)
    paper.precrease(upright, flap=("3/4", "1/8"))
    paper.precrease(level, flap=("1/8", "3/4"))
    paper.collapse(valleys=[upright, level], mountains=[falling], fixed=("7/8", "5/8"))
    for corners in (
        ((0, 0), (0, "1/2"), ("1/2", 0)),
        ((1, 1), ("1/2", 1), (1, "1/2")),
    ):
        center = paper.where(("1/2", "1/2"))
        corner, left, right = (paper.where(point) for point in corners)
        kite_left, kite_right = (
            bisect(left, corner, center),
            bisect(right, corner, center),
        )
        top_left = meet(kite_left, Line.through(center, left))
        top_right = meet(kite_right, Line.through(center, right))
        paper.precrease(
            kite_left, flap=midpoint(left, midpoint(corner, top_left)), layers=1
        )
        paper.precrease(
            kite_right, flap=midpoint(right, midpoint(corner, top_right)), layers=1
        )
        top = Line.through(top_left, top_right)
        middle = meet(top, Line.through(center, corner))
        paper.precrease(
            top, flap=midpoint(midpoint(center, top_left), midpoint(center, middle))
        )
        paper.petal(top, flap=midpoint(corner, midpoint(top_left, middle)))
        paper.turn_over()
