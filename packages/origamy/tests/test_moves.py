# Copyright 2026 Maxence Faldor
"""Basic moves through layers, folded on the simulator.

Each move is a short fold script of a square. It folds at kami's thickness, and with
contact that keeps a twentieth of it, nearly the mathematical sheet. A move the
simulator cannot fold yet is a strict expected failure, with what stops it. A fold at
a tipping point folds on some machines and not on others, so its expected failure may
pass.
"""

import math
from collections.abc import Callable
from typing import Final

import pytest

from origamy import Line, Paper
from origamy.physics import Physics, PhysicsError, simulate
from origamy_sim import Contact, Settings, Table

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


FULL, THIN = 1.0, 0.05
"""The shares of kami's thickness that contact keeps: all of it, and a twentieth."""

MOVES: dict[str, Callable[[], Paper]] = {
    "rolled strip": rolled_strip,
    "half twice": half_twice,
    "half three times": half_three_times,
    "top corner": top_corner,
    "pleats in half": pleats_in_half,
}

TIPPING: Final = "the second fold folds on some machines and stops short on others"
"""A fold at a tipping point, where the last bits of rounding decide its way."""

STOPS: dict[tuple[str, float], str | None] = {
    ("rolled strip", FULL): None,
    ("rolled strip", THIN): TIPPING,
    ("half twice", FULL): None,
    ("half twice", THIN): None,
    ("half three times", FULL): "the second fold stops short of its state",
    ("half three times", THIN): "the third fold turns paper both up and down",
    ("top corner", FULL): None,
    ("top corner", THIN): None,
    ("pleats in half", FULL): None,
    ("pleats in half", THIN): "the last fold stops short of its state",
}
"""Each move at each share of the thickness, with what stops it if the simulator cannot
fold it yet."""


@pytest.mark.parametrize(
    ("name", "share"),
    [
        pytest.param(
            name,
            share,
            marks=[]
            if stop is None
            else [pytest.mark.xfail(reason=stop, strict=stop is not TIPPING)],
        )
        for (name, share), stop in STOPS.items()
    ],
)
def test_a_move_folds_to_its_states(name: str, share: float) -> None:
    contact = Contact(thickness_share=share)
    physics = Physics(
        settings=Settings(time_step=math.inf, table=Table(), contact=contact)
    )
    try:
        folding = simulate(MOVES[name]().steps, physics)
    except PhysicsError as error:
        pytest.fail(str(error))
    for stage in folding.stages:
        assert stage.converged, stage.step
        assert stage.misordered == (), stage.step
        assert stage.offset < 1e-4 + folding.creep, stage.step
