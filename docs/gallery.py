# Copyright 2026 Maxence Faldor
"""Write a standalone 3D viewer of every example the docs' gallery shows.

Run from the repository root: ``uv run python docs/gallery.py``, or name examples, such
as ``crane``, to write only those. The gallery page's links to ``gallery/<name>.html``
choose the examples, and the first link's text names each model, such as ``Crane``.
Each script in ``examples`` folds in its own process, and its viewer goes to
``docs/gallery/<name>.html``, named after the script with hyphens. Each viewer loads
three.js from jsDelivr when opened.

Each viewer plays the model's film: the model as origamy's simulator folds it, from the
flat sheet, in the viewer's studio. The simulation keeps no thickness, and orders hold
its layers in the stacking. A model whose simulation strays from its states shows as
rigid faces.
Worker processes simulate the models in parallel, the longest first.

Writing a viewer needs no browser, so the docs workflow runs this script on a plain
runner. The viewers are build output, left out of the repository. With
``--thumbnails``, the script also draws each model's card image into ``docs/images``
from its viewer, in Chromium; those images are committed.
"""

import asyncio
import math
import os
import re
import sys
import time
from concurrent.futures import ProcessPoolExecutor
from importlib.resources import files
from pathlib import Path
from typing import Final

from playwright.async_api import Route, async_playwright

from origamy import Number
from origamy.field import tower
from origamy.physics import Physics, simulate
from origamy.render import BROWSER
from origamy.result import Failure
from origamy.service import run_script
from origamy.viewer import THREE_PACKAGE, write_viewer
from origamy_sim import Contact, Settings, Table

ROOT: Final = Path(__file__).resolve().parent.parent
EXAMPLES: Final = ROOT / "examples"
GALLERY: Final = ROOT / "docs" / "gallery"
IMAGES: Final = ROOT / "docs" / "images"
GALLERY_PAGE: Final = ROOT / "docs" / "gallery.md"
THREE: Final = Path(str(files("origamy").joinpath("assets", "three")))
VIEWER_LINK: Final = re.compile(r"\[([^\]]+)\]\(gallery/([a-z0-9-]+)\.html\)")
"""A link to a viewer, such as ``[Crane](gallery/crane.html)``."""
PHYSICS: Final = Physics(
    settings=Settings(time_step=math.inf, table=Table(), contact=Contact(layers=False))
)
"""How the gallery simulates each model: quasi-static, on a table, without thickness."""
STRAY: Final = 0.03
"""The farthest, in meters, a simulated fold may leave paper from its state for the
viewer to play the simulation."""
LONGEST: Final = ("crane", "flapping-bird", "frog-base", "glider")
"""The models whose simulations take longest, which start first."""


def gallery_headings() -> dict[str, str]:
    """Each viewer's name and its model's name, from the gallery page, in order."""
    headings: dict[str, str] = {}
    for text, name in VIEWER_LINK.findall(GALLERY_PAGE.read_text(encoding="utf-8")):
        headings.setdefault(name, text)
    return headings


def write_gallery_viewer(name: str, heading: str) -> tuple[Path, str]:
    """Fold an example in its own process, simulate it, and write its viewer.

    Returns the viewer's path and what the simulation found.
    """
    script = EXAMPLES / (name.replace("-", "_") + ".py")
    result = asyncio.run(run_script(script, timeout=600))
    if isinstance(result, Failure):
        msg = f"{script.name} failed: {result.error}: {result.message}"
        raise SystemExit(msg)
    path = GALLERY / f"{name}.html"
    path.parent.mkdir(parents=True, exist_ok=True)
    with tower(Number.from_data(radicand) for radicand in result.tower):
        folding = simulate([data.step() for data in result.steps], PHYSICS)
        stray = max(stage.offset for stage in folding.stages)
        faithful = stray <= STRAY
        write_viewer(
            result, heading, path, studio=True, folding=folding if faithful else None
        )
    found = f"{len(folding.stages)} simulated folds, straying {stray * 1000:.1f} mm"
    return path, found if faithful else found + ", shown rigid"


async def draw_thumbnails(names: list[str]) -> None:
    """Draw each viewer's view of the finished model into a card image.

    Each model shows in the viewer's studio, from the eye the viewer opens with.
    """

    async def serve(route: Route) -> None:
        path = route.request.url.removeprefix(THREE_PACKAGE).replace("build/", "")
        await route.fulfill(path=THREE / path, content_type="text/javascript")

    async with async_playwright() as playwright:
        executable = os.environ.get(BROWSER)
        browser = await playwright.chromium.launch(executable_path=executable)
        page = await browser.new_page(viewport={"width": 900, "height": 700})
        await page.route(f"{THREE_PACKAGE}**", serve)
        for name in names:
            await page.goto((GALLERY / f"{name}.html").as_uri())
            await page.wait_for_selector('#origamy[data-state="ready"]')
            await page.add_style_tag(content="nav.views { display: none; }")
            # The card shows the finished model alone, without its last fold line.
            await page.evaluate(
                "const stage = window.origamy.stage;"
                "stage.look('tilt', false, null, true);"
                "if (stage.line) stage.line.visible = false;"
            )
            await page.wait_for_timeout(900)
            image = IMAGES / f"gallery-{name}.png"
            await page.locator(".stage").screenshot(path=image)
            print(f"drew {image.relative_to(ROOT)}")
        await browser.close()


async def main() -> None:
    """Write the viewer of every example, or of the examples the command line names."""
    wanted = {arg for arg in sys.argv[1:] if not arg.startswith("--")}
    headings = {
        name: heading
        for name, heading in gallery_headings().items()
        if not wanted or name in wanted or name.replace("-", "_") in wanted
    }
    started = time.perf_counter()
    order = sorted(headings, key=lambda name: name not in LONGEST)
    with ProcessPoolExecutor() as pool:
        loop = asyncio.get_running_loop()
        jobs = [
            loop.run_in_executor(pool, write_gallery_viewer, name, headings[name])
            for name in order
        ]
        for job in asyncio.as_completed(jobs):
            path, found = await job
            size = path.stat().st_size / 1000
            print(f"wrote {path.relative_to(ROOT)}: {size:,.0f} kB, {found}")
    print(f"wrote {len(headings)} viewers in {time.perf_counter() - started:.1f} s")
    if "--thumbnails" in sys.argv:
        await draw_thumbnails(list(headings))


if __name__ == "__main__":
    asyncio.run(main())
