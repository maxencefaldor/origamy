# Copyright 2026 Maxence Faldor
"""Basic moves through layers, folded on the simulator.

Each move is a short fold script of a square. It folds at kami's thickness, and with
contact that keeps a twentieth of it, nearly the mathematical sheet. A move the
simulator cannot fold yet is a strict expected failure, with what stops it, and a slow
test. A fold at a tipping point folds on some machines and not on others, so its
expected failure may pass.
"""

from collections.abc import Callable
from typing import Final

import pytest

from origamy import Line, Paper
from origamy.physics import Physics, PhysicsError, simulate
from origamy_sim import Contact

UPRIGHT = Line.through(("1/2", 0), ("1/2", 1))
LEVEL = Line.through((0, "1/2"), (1, "1/2"))


def rolled_strip() -> Paper:
    """A strip folded up twice: the second crease wraps the strip's raw edge."""
    paper = Paper.square()
    paper.valley(Line.through((0, "1/8"), (1, "1/8")), flap=("1/2", "1/16"))
    paper.valley(Line.through((0, "1/4"), (1, "1/4")), flap=("1/2", "3/16"))
    return paper


def half_twice() -> Paper:
    """A square folded in half, then in half again through both layers."""
    paper = Paper.square()
    paper.valley(UPRIGHT, flap=("3/4", "1/2"))
    paper.valley(LEVEL, flap=("1/4", "3/4"))
    return paper


def half_three_times() -> Paper:
    """A square folded in half three times, through two layers, then four."""
    paper = half_twice()
    paper.valley(Line.through(("1/4", 0), ("1/4", 1)), flap=("1/8", "1/4"))
    return paper


def top_corner() -> Paper:
    """A square folded in half, then the top layer's corner folded down alone."""
    paper = Paper.square()
    paper.valley(UPRIGHT, flap=("3/4", "1/2"))
    paper.valley(
        Line.through(("1/4", 1), ("1/2", "3/4")), flap=("3/8", "15/16"), layers=1
    )
    return paper


def pleats_in_half() -> Paper:
    """Four strips pleated together, then folded in half across the pleats."""
    paper = Paper.square()
    lines = [Line.through((f"{k}/4", 0), (f"{k}/4", 1)) for k in range(1, 4)]
    for k in (2, 1, 3):
        paper.precrease(lines[k - 1], flap=(f"{k}/8", "1/2"))
    paper.collapse(valleys=lines[0::2], mountains=lines[1::2], fixed=("1/8", "1/2"))
    paper.valley(LEVEL, flap=("1/8", "3/4"))
    return paper


def inside_reverse() -> Paper:
    """A square folded on its diagonal, its corner reversed between the layers."""
    paper = Paper.square()
    paper.valley(Line.through((0, 0), (1, 1)), flap=("3/4", "1/4"))
    paper.reverse(Line.through(("5/8", 1), ("3/4", "3/4")), flap=("15/16", "31/32"))
    return paper


FULL, THIN, NONE = 1.0, 0.05, 0.0
"""The shares of kami's thickness that contact keeps: all of it, a twentieth, and none.

With none, paper passes through paper, as a sheet without thickness does.
"""

MOVES: dict[str, Callable[[], Paper]] = {
    "rolled strip": rolled_strip,
    "half twice": half_twice,
    "half three times": half_three_times,
    "top corner": top_corner,
    "pleats in half": pleats_in_half,
    "inside reverse": inside_reverse,
}

TIPPING: Final = "the second fold folds on some machines and stops short on others"
"""A fold at a tipping point, where the last bits of rounding decide its way."""
MACHINES: Final = (
    "the second fold lands in its state here and strays on some CI runners"
)
"""A fold that some machines' arithmetic leads astray, though nanometers here do not."""

STOPS: dict[tuple[str, float], str | None] = {
    ("rolled strip", FULL): None,
    ("rolled strip", THIN): TIPPING,
    ("half twice", FULL): None,
    ("half twice", THIN): "the second fold stretches narrow bands to the cap",
    ("half three times", FULL): None,
    ("half three times", THIN): "the second fold does not settle",
    ("top corner", FULL): None,
    ("top corner", THIN): None,
    ("pleats in half", FULL): None,
    ("pleats in half", THIN): None,
    ("inside reverse", FULL): MACHINES,
    ("inside reverse", THIN): "a time step of the reverse fold does not converge",
    **{(name, NONE): None for name in MOVES},
    ("inside reverse", NONE): "a time step of the reverse fold does not converge",
}
"""Each move at each share of the thickness, with what stops it if the simulator cannot
fold it yet."""


def _marks(stop: str | None) -> list[pytest.MarkDecorator]:
    """No marks for a move that folds, and an expected failure for one that stops."""
    if stop is None:
        return []
    if stop in {TIPPING, MACHINES}:
        return [pytest.mark.xfail(reason=stop, strict=False)]
    return [pytest.mark.xfail(reason=stop, strict=True), pytest.mark.slow]


@pytest.mark.parametrize(
    ("name", "share"),
    [
        pytest.param(
            name,
            share,
            marks=_marks(stop),
        )
        for (name, share), stop in STOPS.items()
    ],
)
def test_a_move_folds_to_its_states(name: str, share: float) -> None:
    contact = Contact(thickness_share=share) if share else Contact(layers=False)
    settings = Physics().settings.model_copy(update={"contact": contact})
    physics = Physics(settings=settings)
    try:
        folding = simulate(MOVES[name]().steps, physics)
    except PhysicsError as error:
        pytest.fail(str(error))
    for stage in folding.stages:
        assert stage.converged, stage.step
        assert stage.misordered == (), stage.step
        assert stage.offset < 1e-4 + folding.creep, (stage.step, stage.offset)
