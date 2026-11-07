# Copyright 2026 Maxence Faldor
"""An original: a square pleated into eight strips, folded in half, then opened."""

from origamy import Line, Paper


def fold() -> Paper:
    """Precrease seven lines, collapse them into pleats, fold the stack, open the fan.

    Opening the pleats while the stack stays folded in half bends the paper, so the
    simulator settles it: the pleats spread from the fold as a hand fan does.
    """
    paper = Paper.square(front="#d4a017", back="#fbf4dc")
    lines = [Line.through((f"{k}/8", 0), (f"{k}/8", 1)) for k in range(1, 8)]
    # Halve, then halve each half, then each quarter: each crease lands between two.
    for k in (4, 2, 6, 1, 3, 5, 7):
        paper.precrease(lines[k - 1], flap=(f"{k}/16", "1/2"))
    paper.collapse(valleys=lines[0::2], mountains=lines[1::2], fixed=("1/16", "1/2"))
    paper.valley(Line.through((0, "1/2"), (1, "1/2")), flap=("1/16", "3/4"))
    paper.settle(dict.fromkeys(lines, 90))
    return paper
