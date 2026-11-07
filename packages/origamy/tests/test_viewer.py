# Copyright 2026 Maxence Faldor
import asyncio
import base64
import json
import os
import re
import shutil
from collections.abc import Callable
from importlib.resources import files
from pathlib import Path

import numpy as np
import pytest
from playwright.async_api import Route, async_playwright

from origamy import Line, Paper, service
from origamy.physics import simulate
from origamy.render import BROWSER
from origamy.result import BuildResult
from origamy.viewer import THREE_PACKAGE, film_data, write_viewer

EXAMPLES = Path(__file__).parents[3] / "examples"
THREE = Path(str(files("origamy").joinpath("assets", "three")))
GLIDE = 700
"""The milliseconds the viewer's eye takes to frame a film's next step.

The eye starts a moment after the fold's clock, so the test waits 50 more."""

type Colors = Callable[[Path], set[tuple[int, int, int]]]

HEIGHTS = """(() => {
  const heights = window.origamy.stage.faces.flatMap((face) =>
    Array.from(face.geometry.attributes.position.array).filter((_, i) => i % 3 === 2));
  return [Math.min(...heights), Math.max(...heights)];
})()"""


def viewer(tmp_path: Path, example: str) -> Path:
    script = Path(shutil.copy(EXAMPLES / f"{example}.py", tmp_path))
    request = service.BuildRequest(script=script)
    report = asyncio.run(service.build(request))
    assert report.ok, report.text
    return script.with_suffix(".html")


def test_the_viewer_holds_every_step_with_its_places_before(tmp_path: Path) -> None:
    page = viewer(tmp_path, "cup").read_text(encoding="utf-8")
    found = re.search(r'id="build">(.*?)</script>', page, re.DOTALL)
    assert found is not None
    build = json.loads(found.group(1))
    assert [step["action"] for step in build["steps"]][:3] == [
        "start",
        "valley",
        "precrease",
    ]
    fold = build["steps"][1]
    assert fold["moved"]
    assert len(fold["before"]) == len(fold["places"]) == len(fold["faces"])
    assert fold["line"] is not None
    assert THREE_PACKAGE in page


def test_the_viewer_replays_every_step_in_chromium(
    tmp_path: Path, colors: Colors
) -> None:
    page = viewer(tmp_path, "bird_base")
    shot = tmp_path / "viewer.png"
    errors, heights = asyncio.run(replay(page, shot))
    assert errors == []
    assert heights[1] - heights[0] < 0.05
    # The paper's front, lit and shaded: red well above green and blue.
    assert any(r > g + 50 and r > b + 50 for r, g, b in colors(shot))


def test_the_viewer_turns_shaped_paper_in_space(tmp_path: Path) -> None:
    page = viewer(tmp_path, "paper_plane")
    errors, (low, high) = asyncio.run(replay(page, tmp_path / "viewer.png"))
    assert errors == []
    # The plane's body hangs a quarter of its width below its level wings.
    assert low < -0.2
    assert high < 0.05


FLEXED = """from origamy import Line, Paper


def fold() -> Paper:
    paper = Paper.square()
    rising = Line.through((0, 0), (1, 1))
    falling = Line.through((1, 0), (0, 1))
    upright = Line.through(("1/2", 0), ("1/2", 1))
    level = Line.through((0, "1/2"), (1, "1/2"))
    paper.precrease(rising, flap=("3/4", "1/4"))
    paper.precrease(falling, flap=("7/8", "5/8"), mountain=True)
    paper.precrease(upright, flap=("3/4", "1/8"))
    paper.precrease(level, flap=("1/8", "3/4"))
    paper.collapse(valleys=[upright, level], mountains=[falling], fixed=("7/8", "5/8"))
    paper.flex({upright: 120, level: 120})
    return paper
"""


def test_the_viewer_follows_a_flex_through_its_path(tmp_path: Path) -> None:
    script = tmp_path / "flexed.py"
    script.write_text(FLEXED, encoding="utf-8")
    report = asyncio.run(service.build(service.BuildRequest(script=script)))
    assert report.ok, report.text
    errors, (low, high) = asyncio.run(
        replay(script.with_suffix(".html"), tmp_path / "v.png")
    )
    assert errors == []
    assert high - low > 0.3


async def replay(page: Path, shot: Path) -> tuple[list[str], tuple[float, float]]:
    """Open the viewer, step through every step with its motion, and screenshot it.

    Returns the page's errors, and the paper's lowest and highest heights at the end.
    """
    errors: list[str] = []

    async def serve(route: Route) -> None:
        path = route.request.url.removeprefix(THREE_PACKAGE).replace("build/", "")
        await route.fulfill(path=THREE / path, content_type="text/javascript")

    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=os.environ.get(BROWSER) or None
        )
        tab = await browser.new_page(viewport={"width": 1200, "height": 800})
        tab.on("pageerror", lambda error: errors.append(str(error)))
        await tab.route(THREE_PACKAGE + "**", serve)
        await tab.goto(page.as_uri())
        await tab.wait_for_selector('#origamy[data-state="ready"]')
        count = await tab.evaluate("JSON.parse(build.textContent).steps.length")
        for index in range(count):
            await tab.evaluate(f"window.origamy.go({index}, true)")
            await tab.wait_for_timeout(120)
        await tab.wait_for_timeout(1500)
        await tab.screenshot(path=shot)
        heights = await tab.evaluate(HEIGHTS)
        await browser.close()
    return errors, (float(heights[0]), float(heights[1]))


BOOK = """from origamy import Line, Paper


def fold() -> Paper:
    paper = Paper.square()
    paper.valley(Line.through(("1/2", 0), ("1/2", 1)), flap=("3/4", "1/2"))
    return paper
"""


PIXELS = """(() => {
  const stage = window.origamy.stage;
  stage.renderer.render(stage.scene, stage.camera);
  const gl = stage.renderer.getContext();
  const [w, h] = [gl.drawingBufferWidth, gl.drawingBufferHeight];
  const data = new Uint8Array(w * h * 4);
  gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, data);
  let shade = 0;
  let outline = 0;
  for (let i = 0; i < data.length; i += 4) {
    const [r, g, b, a] = data.slice(i, i + 4);
    if (a > 8 && a < 200 && r < 60 && g < 60 && b < 60) shade += 1;
    const gray = [r, g, b].every((c) => Math.abs(c - 58) < 12);
    if (a === 255 && gray) outline += 1;
  }
  return [shade, outline];
})()"""
"""Count the stage's pixels that a translucent dark shadow covers, and the outline's.

The canvas holds the pixels, so the count reads real pixels, alpha included.
"""


async def studio_pixels(page: Path) -> tuple[list[str], dict[str, list[int]]]:
    """Open the viewer, and count its pixels in three views.

    The views are the studio, the studio without its table, and the diagram.
    """
    errors: list[str] = []

    async def serve(route: Route) -> None:
        path = route.request.url.removeprefix(THREE_PACKAGE).replace("build/", "")
        await route.fulfill(path=THREE / path, content_type="text/javascript")

    counts: dict[str, list[int]] = {}
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=os.environ.get(BROWSER) or None
        )
        tab = await browser.new_page(viewport={"width": 900, "height": 700})
        tab.on("pageerror", lambda error: errors.append(str(error)))
        await tab.route(THREE_PACKAGE + "**", serve)
        await tab.goto(page.as_uri())
        await tab.wait_for_selector('#origamy[data-state="ready"]')
        await tab.click('[data-do="studio"]')
        for name, change in (
            ("studio", ""),
            ("bare", "window.origamy.stage.table.visible = false"),
            ("diagram", "window.origamy.stage.studio(false)"),
        ):
            await tab.evaluate(change or "null")
            await tab.wait_for_timeout(300)
            counts[name] = await tab.evaluate(PIXELS)
        await browser.close()
    return errors, counts


def test_the_studio_shows_paper_without_outlines_and_shades_the_table(
    tmp_path: Path,
) -> None:
    script = tmp_path / "flexed.py"
    script.write_text(FLEXED, encoding="utf-8")
    report = asyncio.run(service.build(service.BuildRequest(script=script)))
    assert report.ok, report.text
    errors, counts = asyncio.run(studio_pixels(script.with_suffix(".html")))
    assert errors == []
    # The paper's shadow darkens the table, and goes with it.
    assert counts["studio"][0] > counts["bare"][0] + 500
    # The diagram outlines every face; the studio draws none.
    assert counts["diagram"][1] > 50
    assert counts["studio"][1] < counts["diagram"][1] / 10


def test_a_view_draws_a_step_from_the_direction_asked(
    tmp_path: Path, colors: Colors
) -> None:
    script = tmp_path / "book.py"
    script.write_text(BOOK, encoding="utf-8")
    seen = {}
    for name, elevation in (("above", 90), ("below", -90)):
        request = service.ViewRequest(script=script, elevation=elevation)
        report = asyncio.run(service.view(request))
        assert report.step == 1
        assert report.elevation == elevation
        assert "seen from azimuth" in report.text
        seen[name] = colors(Path(report.files[0]))
    # Folded in half, the book shows the back of its top layer above and the back of
    # its bottom layer below: cream both ways, and no vermilion front at all.
    for found in seen.values():
        assert any(r > 200 and g > 190 and b > 170 and r - b < 45 for r, g, b in found)
        assert not any(r > 150 and r > g + 60 for r, g, b in found)
    unfolded = asyncio.run(service.view(service.ViewRequest(script=script, step=0)))
    front = colors(Path(unfolded.files[0]))
    assert any(r > 150 and r > g + 60 for r, g, b in front)


def filmed(tmp_path: Path) -> tuple[BuildResult, Path]:
    """Fold a book in half and turn it over, simulate it, and write its viewer."""
    paper = Paper.square()
    paper.valley(Line.through(("1/2", 0), ("1/2", 1)), flap=("3/4", "1/2"))
    paper.turn_over()
    result = BuildResult.of(paper, tmp_path)
    page = tmp_path / "book.html"
    write_viewer(result, "Book", page, studio=True, folding=simulate(paper.steps))
    return result, page


def test_a_film_rests_exactly_where_each_state_puts_the_paper(
    tmp_path: Path,
) -> None:
    paper = Paper.square()
    paper.valley(Line.through(("1/2", 0), ("1/2", 1)), flap=("3/4", "1/2"))
    folding = simulate(paper.steps)
    film = film_data(BuildResult.of(paper, tmp_path), folding)
    frames = np.frombuffer(base64.b64decode(film["steps"][1]["frames"]), "<i2")
    frames = frames * film["step"]
    last = frames.reshape(-1, film["vertices"], 3)[-1]
    rest = np.frombuffer(base64.b64decode(film["rest"]), np.float32).reshape(-1, 2)
    # The right half lies folded onto the left, every vertex mirrored in the middle.
    for (x, y), (fx, fy, fz) in zip(rest, last, strict=True):
        assert fx == pytest.approx(min(x, 1 - x), abs=1e-4)
        assert fy == pytest.approx(y, abs=1e-4)
        assert fz == pytest.approx(0, abs=1e-4)


EYE = """(() => {
  const stage = window.origamy.stage;
  window.origamy.go(1, true);
  const [start, span] = [stage.started, stage.duration()];
  const seen = [];
  for (let k = 0; k <= 40; k++) {
    const now = start + (k / 40) * span;
    stage.steer(now);
    stage.animate(now);
    const paper = stage.film.positions.reduce((sum, x, i) => sum + x * (i % 7), 0);
    seen.push([now - start, ...stage.camera.position.toArray(), paper]);
  }
  return seen;
})()"""
"""Play the fold on a clock the test keeps, and note the eye and the paper as it plays.

Each note holds the time since the fold began, where the eye stands, and a sum of
where the paper lies.
"""


def test_the_eye_holds_still_while_a_film_folds(tmp_path: Path) -> None:
    _, page = filmed(tmp_path)

    async def watch() -> tuple[list[str], list[list[str]]]:
        errors: list[str] = []

        async def serve(route: Route) -> None:
            path = route.request.url.removeprefix(THREE_PACKAGE).replace("build/", "")
            await route.fulfill(path=THREE / path, content_type="text/javascript")

        async with async_playwright() as playwright:
            browser = await playwright.chromium.launch(
                executable_path=os.environ.get(BROWSER) or None
            )
            tab = await browser.new_page(viewport={"width": 900, "height": 700})
            tab.on("pageerror", lambda error: errors.append(str(error)))
            await tab.route(THREE_PACKAGE + "**", serve)
            await tab.goto(page.as_uri())
            await tab.wait_for_selector('#origamy[data-state="ready"]')
            await tab.evaluate("window.origamy.go(0, false)")
            seen = await tab.evaluate(EYE)
            await browser.close()
        return errors, [[str(value) for value in shot] for shot in seen]

    errors, seen = asyncio.run(watch())
    assert errors == []
    # The eye frames the step first, then stands still while the paper moves.
    notes = np.array(seen, dtype=float)
    folding = notes[notes[:, 0] >= GLIDE + 50]
    assert np.abs(notes[0, 1:4] - folding[-1, 1:4]).max() > 1e-6
    assert np.abs(folding[:, 1:4] - folding[-1, 1:4]).max() < 1e-9
    assert len(set(folding[:, 4])) > 5
