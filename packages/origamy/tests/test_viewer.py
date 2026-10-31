# Copyright 2026 Maxence Faldor
import asyncio
import json
import os
import re
import shutil
from collections.abc import Callable
from importlib.resources import files
from pathlib import Path

from playwright.async_api import Route, async_playwright

from origamy import service
from origamy.render import BROWSER
from origamy.viewer import THREE_PACKAGE

EXAMPLES = Path(__file__).parents[3] / "examples"
THREE = Path(str(files("origamy").joinpath("assets", "three")))

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
