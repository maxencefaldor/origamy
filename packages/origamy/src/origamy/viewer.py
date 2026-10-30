# Copyright 2026 Maxence Faldor
"""A standalone viewer: one HTML file that replays a fold step by step, in 3D.

The page holds the build as data, the viewer's module and its styles, and loads
three.js from jsDelivr, so it stays small and needs a network connection when opened.
Each step carries every face's place before and after it, so the viewer animates the
move: a face that turns over makes a half turn about its axis, and the rest glide.
"""

import base64
import json
from fractions import Fraction
from functools import cache
from html import escape
from importlib.resources import files
from pathlib import Path
from string import Template
from typing import Any, Final

from origamy.draw import across, stacking
from origamy.field import Number
from origamy.geometry import Isometry, Line, Point, bounds, covers
from origamy.paper import Step
from origamy.result import BuildResult
from origamy.state import State

THREE_VERSION: Final = "0.186.1"
"""The three.js version the viewer loads, and the one vendored for its tests."""

THREE_PACKAGE: Final = f"https://cdn.jsdelivr.net/npm/three@{THREE_VERSION}/"
"""The three.js package on jsDelivr, which the viewer loads."""

type Data = dict[str, Any]

_PAGE: Final = Template("""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>$title</title>
<style>$style</style>
<script type="importmap">$imports</script>
</head>
<body>
<div id="origamy" data-state="loading"><p class="splash">Loading the 3D view…</p></div>
<script type="application/json" id="build">$build</script>
<script type="module">
const root = document.getElementById("origamy");
try {
  const { start } = await import("origamy/viewer.js");
  const build = JSON.parse(document.getElementById("build").textContent);
  window.origamy = start(root, build);
} catch (error) {
  console.error(error);
  root.dataset.state = "failed";
  root.querySelector(".splash").textContent = /import|fetch|module/i.test(String(error))
    ? "The 3D view needs three.js from cdn.jsdelivr.net, which did not load."
    : "The 3D view could not start: " + error.message;
}
</script>
</body>
</html>
""")


def write_viewer(result: BuildResult, name: str, path: Path) -> Path:
    """Write the viewer of a build to ``path`` and return the path."""
    imports = {
        "three": THREE_PACKAGE + "build/three.module.js",
        "three/addons/": THREE_PACKAGE + "examples/jsm/",
        "origamy/viewer.js": "data:text/javascript;base64,"
        + base64.b64encode(_asset("viewer.js").encode()).decode("ascii"),
    }
    page = _PAGE.substitute(
        title=escape(f"{name} · origamy"),
        style=_asset("viewer.css"),
        imports=json.dumps({"imports": imports}),
        build=json.dumps(build_data(result, name)).replace("</", "<\\/"),
    )
    path.write_text(page, encoding="utf-8")
    return path


def build_data(result: BuildResult, name: str) -> Data:
    """The build as the viewer reads it: floats, with each step's places before it."""
    assert result.paper is not None  # noqa: S101 - only results with paper have views
    steps = [data.step() for data in result.steps]
    refused = result.refused.step() if result.refused else None
    views = [
        _step(step, steps[index - 1] if index else None)
        for index, step in enumerate(steps)
    ]
    if refused is not None:
        views.append({**_step(refused, None), "refused": True})
    return {
        "name": name,
        "paper": result.paper.model_dump(),
        "size": float(
            max(Number.of(result.paper.width), Number.of(result.paper.height))
        ),
        "steps": views,
    }


def _step(step: Step, previous: Step | None) -> Data:
    state = step.state
    levels = _levels(state)
    before, before_levels = [_floats(f.place) for f in state.faces], levels
    if previous is not None:
        parents = [_parent(previous.state, polygon) for polygon in state.patterns]
        old = _levels(previous.state)
        before = [_floats(previous.state.faces[p].place) for p in parents]
        before_levels = [old[p] for p in parents]
    source = str(step.source.site) if step.source else None
    return {
        "action": step.action,
        "description": step.description,
        "source": source,
        "vertices": [p.floats() for p in state.vertices],
        "faces": [list(f.corners) for f in state.faces],
        "places": [_floats(f.place) for f in state.faces],
        "levels": levels,
        "before": before,
        "beforeLevels": before_levels,
        "edges": [[u, v, state.assignment((u, v))] for u, v in sorted(state.edges)],
        "moved": sorted(step.moved),
        "line": _segment(step.line, state) if step.line else None,
        "flap": step.flap.floats() if step.flap else None,
        "facts": [
            {
                "rule": d.rule,
                "level": str(d.level),
                "message": d.message,
                "faces": list(d.faces),
            }
            for d in step.diagnostics
        ],
    }


def _levels(state: State) -> list[int]:
    order = stacking(state)
    levels = [0] * len(state.faces)
    for level, face in enumerate(order):
        levels[face] = level
    return levels


def _parent(state: State, polygon: tuple[Point, ...]) -> int:
    """The face of an earlier state that holds a polygon of the crease pattern."""
    center = sum(polygon[1:], polygon[0]) * Number(Fraction(1, len(polygon)))
    return next(
        f for f, pattern in enumerate(state.patterns) if covers(pattern, center)
    )


def _segment(line: Line, state: State) -> tuple[tuple[float, float], ...]:
    """The fold line across the folded paper, and as far again beyond it."""
    x0, y0, x1, y1 = bounds(p for polygon in state.folded for p in polygon)
    start, end = across(line, (float(x0), float(y0), float(x1), float(y1)))
    middle = ((start[0] + end[0]) / 2, (start[1] + end[1]) / 2)
    return tuple(
        (middle[0] + (p[0] - middle[0]) / 2, middle[1] + (p[1] - middle[1]) / 2)
        for p in (start, end)
    )


def _floats(place: Isometry) -> list[float]:
    entries = (place.xx, place.xy, place.yx, place.yy, place.tx, place.ty)
    return [float(x) for x in entries]


@cache
def _asset(name: str) -> str:
    return files("origamy").joinpath("assets", name).read_text(encoding="utf-8")
