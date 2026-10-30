# Copyright 2026 Maxence Faldor
"""The flapping bird: a bird base, its neck, tail and head reversed up, wings down."""

from origamy import Line, Paper, bisect, fold_onto, meet, midpoint, perpendicular


def fold() -> Paper:
    """Fold a bird base, reverse fold its lower flaps up, then fold the wings down."""
    paper = Paper.square(front="#5b4b9a", back="#efeaf7")
    bird_base(paper)
    tip, wing, middle = (paper.where(point) for point in ((1, 0), (0, 0), (0, "1/2")))
    left, right = paper.where(("√2/2 - 1/2", "1/2")), paper.where(("1/2", "√2/2 - 1/2"))
    # Reverse fold each lower flap up between the wings, square to a wing's edge.
    hinges = []
    for leg, other in ((left, right), (right, left)):
        line = perpendicular(Line.through(wing, other), middle)
        hinges.append(meet(line, Line.through(tip, leg)))
        flap = midpoint(tip, midpoint(middle, hinges[-1]))
        paper.reverse(line, flap=flap, layers=paper.layers(flap))
    # Mark a quarter of the neck with two precreases, then reverse fold the head there,
    # square to the wing's edge.
    neck = paper.where((0, 1))
    edge = Line.through(neck, middle)
    mark = middle
    for _ in range(2):
        crease = fold_onto(neck, mark)
        inside = midpoint(neck, midpoint(neck, midpoint(mark, hinges[0])))
        paper.precrease(crease, flap=inside, layers=paper.layers(inside))
        mark = meet(crease, edge)
    head = perpendicular(Line.through(wing, left), mark)
    top, bottom = (meet(head, Line.through(neck, end)) for end in (middle, hinges[0]))
    flap = midpoint(neck, midpoint(top, bottom))
    paper.reverse(head, flap=flap, layers=paper.layers(flap))
    # Fold each wing down, square to the body through the paper's center.
    for wing_tip, edge in (
        ((0, 0), ("√2/4 - 1/4", "1/4")),
        ((1, 1), ("3/4", "5/4 - √2/4")),
    ):
        wing, center = paper.where(wing_tip), paper.where(("1/2", "1/2"))
        line = perpendicular(Line.through(wing, paper.where((0, "1/2"))), center)
        paper.valley(line, flap=midpoint(center, paper.where(edge)), layers=1)
        paper.turn_over()
    return paper


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
