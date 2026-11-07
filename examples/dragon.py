# Copyright 2026 Maxence Faldor
"""An original: a wyvern, with a curled tail, a hooked snout and arched bat wings."""

from origamy import Line, Paper, Point, bisect, meet, midpoint, perpendicular

DROOP = 30
"""How far, in degrees, the membrane turns down at each finger of a wing."""
RISE = 150
"""How far, in degrees, each wing rises from folded down: high above level."""
FINGERS = (
    (("-83/1000", "-207/1000"), ("-77/1000", "-151/1000")),
    (("-47/1000", "-118/1000"), ("-44/1000", "-69/1000")),
    (("-18/1000", "-44/1000"), ("-20/1000", "-9/1000")),
)
"""Each finger's end on the trailing edge, and a point of the membrane beyond it, as
offsets from the wing's tip once the wings rise."""
WRIST = ("-197/1000", "-77/1000")
"""Where the fingers start, on the leading edge, as an offset from the wing's tip."""


def fold() -> Paper:
    """Fold a bird base, narrow its legs, and make a wyvern of its four flaps.

    The legs reverse fold into a rising neck and a level tail. The head reverse folds
    forward, its snout hooks down, and the tail's tip curls up. The wings fold down,
    rise high, and arch down at three fingers each, as a bat's do. Last, the wyvern
    stands up.
    """
    paper = Paper.square(front="#2f6b3a", back="#d9c27a")
    bird_base(paper)

    # Narrow each leg on the front, then on the back, as the crane does.
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

    # The neck rises as the crane's does, square to one wing's edge. The tail reaches
    # straight back, level with the belly.
    tip, wing, middle = (paper.where(point) for point in ((1, 0), (0, 0), (0, "1/2")))
    left, right = corners[:2]
    neck = perpendicular(Line.through(wing, right), middle)
    shoulder = meet(neck, Line.through(tip, left))
    flap = midpoint(tip, midpoint(middle, shoulder))
    paper.reverse(neck, flap=flap, layers=paper.layers(flap))
    tail = Line.through(middle, middle + Point.of((1, 0)))
    hip = meet(tail, Line.through(tip, right))
    flap = midpoint(tip, midpoint(middle, hip))
    paper.reverse(tail, flap=flap, layers=paper.layers(flap))

    # The head reaches forward from the top of the neck, and the last quarter of the
    # head hooks down, wrapped round the outside.
    neck_tip = paper.where((0, 1))
    head = perpendicular(Line.through(wing, left), left)
    top, bottom = (
        meet(head, Line.through(neck_tip, end)) for end in (middle, shoulder)
    )
    flap = midpoint(neck_tip, midpoint(top, bottom))
    paper.reverse(head, flap=flap, layers=paper.layers(flap))
    head_tip = paper.where((0, 1))
    snout = midpoint(midpoint(midpoint(top, bottom), head_tip), head_tip)
    along = head_tip - snout
    line = Line.through(snout, snout + along - Point(-along.y, along.x))
    flap = midpoint(snout, head_tip)
    paper.reverse(line, flap=flap, layers=paper.layers(flap), outside=True)

    # The tail's last eighth curls up, wrapped round the outside.
    tail_tip, kink = paper.where((1, 0)), paper.where(("7/8", "1/8"))
    along = tail_tip - kink
    line = Line.through(kink, kink + along - Point(-along.y, along.x))
    flap = paper.where(("47/50", "1/20"))
    paper.reverse(line, flap=flap, layers=paper.layers(flap), outside=True)

    # Fold each wing down at the shoulders, then raise it high above level.
    wings = (((0, 0), ("1/8", "1/16")), ((1, 1), ("7/8", "15/16")))
    for corner, inside in wings:
        paper.valley(shoulders(paper, corner), flap=paper.where(inside), layers=1)
        paper.turn_over()
    for corner, inside in wings:
        paper.shape(
            shoulders(paper, corner), flap=paper.where(inside), angle=RISE, layers=1
        )
        paper.turn_over()

    # Arch each wing: fingers run from a wrist on the leading edge to the trailing
    # edge, and the membrane turns down at each. The second wing's points mirror the
    # first's, as the turn over between them mirrors the paper.
    for corner, side in (((0, 0), 1), ((1, 1), -1)):
        tip = paper.where(corner)

        def near(offset: tuple[str, str], tip: Point = tip, side: int = side) -> Point:
            shift = Point.of(offset)
            return Point(tip.x + side * shift.x, tip.y + shift.y)

        for end, inside in FINGERS:
            line = Line.through(near(WRIST), near(end))
            paper.shape(line, flap=near(inside), angle=-DROOP, layers=1)
        paper.turn_over()

    # Stand the wyvern up on its belly, its back to the sky.
    back = paper.where(("1/2", "1/2")) - Point.of(("1/16", "1/16"))
    paper.tilt(shoulders(paper, (0, 0)), flap=back, angle=90)
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
