# Copyright 2026 Maxence Faldor
"""The frog base, from its crease pattern: creased open, then collapsed at once."""

from origamy import Paper, Point

CENTER = Point.of(("1/2", "1/2"))


def fold() -> Paper:
    """Crease the frog base's pattern on the open square, then collapse it."""
    valleys, mountains = [], []
    # Each side folds the same way, a quarter turn round the center from the last.
    side = {
        "middle": Point.of(("1/2", 0)),
        "point": Point.of(("1/2", "1/2 - √2/4")),
        "left": Point.of(("√2/4", "1/2 - √2/4")),
        "right": Point.of(("1 - √2/4", "1/2 - √2/4")),
        "left corner": Point.of((0, 0)),
        "right corner": Point.of((1, 0)),
    }
    for _ in range(4):
        middle, point, left, right = (
            side[k] for k in ("middle", "point", "left", "right")
        )
        mountains += [(CENTER, point), (point, left), (point, right)]
        valleys += [(point, middle), (CENTER, left), (CENTER, right)]
        valleys += [(left, middle), (right, middle)]
        valleys += [(left, side["left corner"]), (right, side["right corner"])]
        side = {key: quarter_turn(p) for key, p in side.items()}
    # One diagonal folds; the other stays flat inside the flaps.
    mountains += [(CENTER, Point.of((0, 0))), (CENTER, Point.of((1, 1)))]
    flats = [(CENTER, Point.of((1, 0))), (CENTER, Point.of((0, 1)))]
    paper = Paper.square()
    paper.crease([*valleys, *mountains, *flats])
    paper.collapse(valleys=valleys, mountains=mountains)
    return paper


def quarter_turn(p: Point) -> Point:
    """A point turned a quarter turn counterclockwise about the square's center."""
    return Point(1 - p.y, p.x)
