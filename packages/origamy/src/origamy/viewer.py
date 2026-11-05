# Copyright 2026 Maxence Faldor
"""A standalone viewer: one HTML file that replays a fold step by step, in 3D.

The page holds the build as data, the viewer's module and its styles, and loads
three.js from jsDelivr, so it stays small and needs a network connection when opened.
Each step carries every face's place before and after it, so the viewer animates the
move: a fold turns its paper about its line, and a collapse follows its creases as
rigid origami. Once shaping turns the paper out of the plane, each step also carries
every face's motion in space, and the turn its moving faces make. The same page, in
headless Chromium, draws any step from any direction.
"""

import base64
import json
import os
import tempfile
from collections.abc import Sequence
from dataclasses import dataclass
from functools import cache
from html import escape
from importlib.resources import files
from math import atan2, cos, degrees, hypot, radians, sin
from pathlib import Path
from string import Template
from typing import Any, Final

from playwright.async_api import Route, async_playwright

from origamy import flex
from origamy.draw import GAP, Eye, across, folded_box, layers
from origamy.errors import OrigamyError
from origamy.geometry import Isometry, Line
from origamy.paper import Step
from origamy.render import BROWSER
from origamy.result import BuildResult
from origamy.space import Shaped
from origamy.state import State

THREE_VERSION: Final = "0.186.1"
"""The three.js version the viewer loads, and the one vendored for its tests."""

THREE_PACKAGE: Final = f"https://cdn.jsdelivr.net/npm/three@{THREE_VERSION}/"
"""The three.js package on jsDelivr, which the viewer loads."""

type Data = dict[str, Any]

_COLLAPSES: Final = frozenset({"collapse", "reverse", "petal", "squash"})
"""The actions that turn many creases at once, which the viewer follows as a flex."""

_FRAMES: Final = 12
"""How many parts the viewer divides a collapse's path into."""

_TILT: Final = (0.35, -0.75, 0.75)
"""Toward the eye the viewer opens flat paper with: in front of it, and above."""

_STEEPEST: Final = 89.99
"""The steepest elevation an eye takes, so that it still knows which way is up."""

_THREE: Final = Path(str(files("origamy").joinpath("assets", "three")))
"""The copy of three.js that origamy vendors."""

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
    path.write_text(_page(build_data(result, name), name), encoding="utf-8")
    return path


@dataclass(frozen=True, slots=True)
class Direction:
    """Where an eye looks at the paper from, far away, in degrees.

    The azimuth runs round the paper on the table: 0 in front of its bottom edge, 90 at
    its right, 180 behind it and 270 at its left. The elevation rises from 0, level
    with the table, to 90, straight above; below the table it is negative.
    """

    azimuth: float
    elevation: float

    @classmethod
    def of(cls, toward: Sequence[float]) -> Direction:
        """The direction of a vector toward the eye."""
        x, y, z = toward
        level = hypot(x, y)
        return cls(degrees(atan2(x, -y)) % 360, degrees(atan2(z, level)))

    @property
    def toward(self) -> tuple[float, float, float]:
        """A unit vector toward the eye; straight above or below tilts a hair."""
        a = radians(self.azimuth)
        e = radians(max(-_STEEPEST, min(_STEEPEST, self.elevation)))
        return sin(a) * cos(e), -cos(a) * cos(e), sin(e)


async def draw_view(  # noqa: PLR0913 - a picture of one step, from one direction
    result: BuildResult,
    name: str,
    path: Path,
    *,
    step: int | None,
    azimuth: float | None,
    elevation: float | None,
    xray: bool,
) -> tuple[int, Direction]:
    """Draw one step of a build in 3D into a PNG, as the viewer shows it.

    The step defaults to the last; the direction, to the one the viewer opens with,
    which an azimuth or an elevation given replaces. Returns the step and direction.
    """
    data = build_data(result, name, motion=False)
    count = len(data["steps"])
    index = count - 1 if step is None else step
    if not 0 <= index < count:
        msg = f"the build has steps 0 to {count - 1}, not {index}"
        raise OrigamyError(msg)
    opening = Direction.of(data["eye"] or _TILT)
    direction = Direction(
        opening.azimuth if azimuth is None else azimuth % 360,
        opening.elevation if elevation is None else elevation,
    )
    with tempfile.TemporaryDirectory() as folder:
        page = Path(folder) / "view.html"
        page.write_text(_page(data, name), encoding="utf-8")
        async with async_playwright() as playwright:
            browser = await playwright.chromium.launch(
                executable_path=os.environ.get(BROWSER) or None
            )
            try:
                tab = await browser.new_page(viewport={"width": 960, "height": 760})
                await tab.route(f"{THREE_PACKAGE}**", _vendored)
                await tab.goto(page.as_uri())
                await tab.wait_for_selector('#origamy[data-state="ready"]')
                toward = list(direction.toward)
                await tab.evaluate(
                    "([i, t, x]) => window.origamy.view(i, t, x)", [index, toward, xray]
                )
                await tab.wait_for_timeout(300)
                await tab.locator(".stage").screenshot(path=path)
            finally:
                await browser.close()
    return index, direction


async def _vendored(route: Route) -> None:
    """Answer a request for three.js with the copy origamy vendors."""
    name = route.request.url.removeprefix(THREE_PACKAGE).replace("build/", "")
    await route.fulfill(path=_THREE / name, content_type="text/javascript")


def _page(data: Data, name: str) -> str:
    """The viewer's page, holding a build's data."""
    imports = {
        "three": THREE_PACKAGE + "build/three.module.js",
        "three/addons/": THREE_PACKAGE + "examples/jsm/",
        "origamy/viewer.js": "data:text/javascript;base64,"
        + base64.b64encode(_asset("viewer.js").encode()).decode("ascii"),
    }
    return _PAGE.substitute(
        title=escape(f"{name} · origamy"),
        style=_asset("viewer.css"),
        imports=json.dumps({"imports": imports}),
        build=json.dumps(data).replace("</", "<\\/"),
    )


def build_data(result: BuildResult, name: str, *, motion: bool = True) -> Data:
    """The build as the viewer reads it: floats, with each step's places before it."""
    steps = [data.step() for data in result.steps]
    refused = result.refused.step() if result.refused else None
    views = [
        _step(step, steps[index - 1] if index else None, motion=motion)
        for index, step in enumerate(steps)
    ]
    if refused is not None:
        views.append({**_step(refused, None, motion=motion), "refused": True})
    shaped = [Shaped(s.state, s.form) for s in (*steps, refused) if s and s.form]
    return {
        "name": name,
        "paper": result.paper.model_dump(),
        "size": float(result.paper.unit()),
        "gap": GAP,
        "eye": Eye.facing(shaped[-1]).toward if shaped else None,
        "steps": views,
    }


def _step(step: Step, previous: Step | None, *, motion: bool) -> Data:
    state = step.state
    parted = layers(Shaped(state, step.form))
    before = [_floats(f.place) for f in state.faces]
    heights, shifts = parted.heights, parted.shifts
    form_before = step.form
    if previous is not None:
        parents = previous.state.parents(state)
        old = layers(Shaped(previous.state, previous.form))
        before = [_floats(previous.state.faces[p].place) for p in parents]
        heights = tuple(old.heights[p] for p in parents)
        shifts = tuple(old.shifts[p] for p in parents)
        form_before = previous.form and previous.form.carried(previous.state, state)
    path = [form.motions for form in step.path]
    if motion and previous and step.action in _COLLAPSES and not form_before:
        # A collapse turns many creases at once: follow them as rigid origami, so the
        # paper stays whole on its way, where a rigid motion takes it there. A face
        # that turns over on the way starts its layer gap on its other side.
        forms = flex.path(previous.state, state, [k / _FRAMES for k in range(_FRAMES)])
        if forms is not None:
            form_before, path = forms[0], [form.motions for form in forms[1:]]
            turned = [
                previous.state.flips(p) != state.flips(f)
                for f, p in enumerate(previous.state.parents(state))
            ]
            heights = tuple(
                -h if t else h for h, t in zip(heights, turned, strict=True)
            )
    turn = step.turn
    source = str(step.source.site) if step.source else None
    return {
        "action": step.action,
        "description": step.description,
        "source": source,
        "vertices": [p.floats() for p in state.vertices],
        "faces": [list(f.corners) for f in state.faces],
        "places": [_floats(f.place) for f in state.faces],
        "height": parted.heights,
        "shift": parted.shifts,
        "before": before,
        "heightBefore": heights,
        "shiftBefore": shifts,
        "motions": step.form.motions if step.form else None,
        "motionsBefore": form_before.motions if form_before else None,
        "path": path,
        "turn": (
            {"point": turn.point, "direction": turn.direction, "angle": turn.angle}
            if turn
            else None
        ),
        "edges": [[u, v, state.assignment((u, v))] for u, v in sorted(state.edges)],
        "creases": [[u, w, f, g] for (u, w), (f, g) in sorted(state.creases.items())],
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
            for d in (*([step.refusal] if step.refusal else []), *step.diagnostics)
        ],
    }


def _segment(line: Line, state: State) -> tuple[tuple[float, float], ...]:
    """The fold line across the folded paper, and as far again beyond it."""
    start, end = across(line, folded_box(state))
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
