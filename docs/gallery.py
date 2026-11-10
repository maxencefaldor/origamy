# Copyright 2026 Maxence Faldor
"""Write a standalone 3D viewer of every example the docs' gallery shows.

Run from the repository root: ``uv run python docs/gallery.py``, or name examples, such
as ``crane``, to write only those. The gallery page's cards choose the examples, each
titled by a link to ``gallery/<name>.html`` that names its model, then its step count.
A card whose count differs from its build's stops the script.
Each script in ``examples`` folds in its own process, and its viewer goes to
``docs/gallery/<name>.html``, named after the script with hyphens. Each viewer loads
three.js from jsDelivr when opened.

Each viewer plays the model's film: the model as origamy's simulator folds it, from the
flat sheet, in the viewer's studio. The paper keeps its thickness, and contact keeps
its layers apart, so the film plays exactly as the simulator folded it. A model whose
simulation strays from its states, or settles with paper through paper, stops the
script once every other viewer is written. Worker processes simulate the models in
parallel, the longest first.

Writing a viewer needs no browser, so the docs workflow runs this script on a plain
runner. The viewers are build output, left out of the repository. With
``--thumbnails``, the script also draws each model's card image into ``docs/images``
from its viewer, in Chromium; those images are committed.
"""

import asyncio
import os
import re
import sys
import time
from concurrent.futures import ProcessPoolExecutor
from importlib.resources import files
from pathlib import Path
from typing import Final

from playwright.async_api import Route, ViewportSize, async_playwright

from origamy import Number
from origamy.field import tower
from origamy.physics import Physics, simulate
from origamy.render import BROWSER
from origamy.result import Failure
from origamy.service import run_script
from origamy.viewer import THREE_PACKAGE, write_viewer

ROOT: Final = Path(__file__).resolve().parent.parent
EXAMPLES: Final = ROOT / "examples"
GALLERY: Final = ROOT / "docs" / "gallery"
IMAGES: Final = ROOT / "docs" / "images"
GALLERY_PAGE: Final = ROOT / "docs" / "gallery.md"
THREE: Final = Path(str(files("origamy").joinpath("assets", "three")))
CARD: Final = re.compile(
    r"\*\*\[([^\]]+)\]\(gallery/([a-z0-9-]+)\.html\)\*\* · (\d+) steps?"
)
"""A card's title, such as ``**[Cup](gallery/cup.html)** · 9 steps``."""
PHYSICS: Final = Physics()
"""How the gallery simulates each model: as origamy folds kami, with its thickness."""
STRAY: Final = 0.005
"""The farthest, in meters, a simulated fold may leave paper from its state."""
CROSSING: Final = 1e-4
"""The deepest, in meters, a settle step may leave paper through paper: a few sheets
of kami."""
THUMBNAIL: Final = ViewportSize(width=800, height=600)
"""The size of every card image, in pixels: the 4:3 of a card's frame."""
BARE: Final = "body, #origamy .stage.studio { background: none; }"
"""The style that leaves a card image without a backdrop."""
LONGEST: Final = ("fortune-teller", "cup", "window", "arrowhead")
"""The models whose simulations take longest, which start first."""


def gallery_cards() -> dict[str, tuple[str, int]]:
    """Each card's viewer name, with its model's name and step count, in page order."""
    page = GALLERY_PAGE.read_text(encoding="utf-8")
    return {name: (heading, int(steps)) for heading, name, steps in CARD.findall(page)}


def write_gallery_viewer(name: str, heading: str, steps: int) -> tuple[Path, str]:
    """Fold an example in its own process, simulate it, and write its viewer.

    Returns the viewer's path and what the simulation found.
    """
    script = EXAMPLES / (name.replace("-", "_") + ".py")
    result = asyncio.run(run_script(script, timeout=600))
    if isinstance(result, Failure):
        msg = f"{script.name} failed: {result.error}: {result.message}"
        raise SystemExit(msg)
    taken = len(result.steps) - 1
    if taken != steps:
        msg = f"the {heading} card counts {steps} steps; {script.name} takes {taken}"
        raise SystemExit(msg)
    path = GALLERY / f"{name}.html"
    path.parent.mkdir(parents=True, exist_ok=True)
    with tower(Number.from_data(radicand) for radicand in result.tower):
        folding = simulate([data.step() for data in result.steps], PHYSICS)
        stray = max(stage.offset for stage in folding.stages)
        settled = [stage.crossing for stage in folding.stages if stage.settled]
        crossed = max(settled, default=0.0)
        found = f"{len(folding.stages)} simulated folds, straying {stray * 1000:.1f} mm"
        if crossed:
            found += f", settling {crossed * 1000:.2f} mm through itself"
        if stray > STRAY or crossed > CROSSING:
            msg = f"the simulator cannot fold the {heading} yet: {found}"
            raise SystemExit(msg)
        write_viewer(result, heading, path, studio=True, folding=folding)
    return path, found


async def draw_thumbnails(names: list[str]) -> None:
    """Draw each viewer's view of the finished model into a card image.

    Each model shows in the viewer's studio, from the eye the viewer opens with. Every
    image has the card frame's size, and no backdrop: the frame paints the studio's.
    """

    async def serve(route: Route) -> None:
        path = route.request.url.removeprefix(THREE_PACKAGE).replace("build/", "")
        await route.fulfill(path=THREE / path, content_type="text/javascript")

    async with async_playwright() as playwright:
        executable = os.environ.get(BROWSER)
        browser = await playwright.chromium.launch(executable_path=executable)
        page = await browser.new_page(viewport=THUMBNAIL)
        await page.route(f"{THREE_PACKAGE}**", serve)
        for name in names:
            await page.goto((GALLERY / f"{name}.html").as_uri())
            await page.wait_for_selector('#origamy[data-state="ready"]')
            await page.add_style_tag(content=BARE)
            # The stage fills the page and shows the step the viewer opens on, the last.
            await page.evaluate("window.origamy.view(window.origamy.index)")
            await page.wait_for_timeout(900)
            image = IMAGES / f"gallery-{name}.png"
            await page.screenshot(path=image, omit_background=True)
            print(f"drew {image.relative_to(ROOT)}")
        await browser.close()


async def main() -> None:
    """Write the viewer of every example, or of the examples the command line names."""
    wanted = {arg for arg in sys.argv[1:] if not arg.startswith("--")}
    cards = {
        name: card
        for name, card in gallery_cards().items()
        if not wanted or name in wanted or name.replace("-", "_") in wanted
    }
    started = time.perf_counter()
    order = sorted(cards, key=lambda name: name not in LONGEST)
    with ProcessPoolExecutor() as pool:
        loop = asyncio.get_running_loop()
        jobs = [
            loop.run_in_executor(pool, write_gallery_viewer, name, *cards[name])
            for name in order
        ]
        strays: list[str] = []
        for job in asyncio.as_completed(jobs):
            try:
                path, found = await job
            except SystemExit as stray:
                strays.append(str(stray))
                print(stray)
                continue
            size = path.stat().st_size / 1000
            print(f"wrote {path.relative_to(ROOT)}: {size:,.0f} kB, {found}")
    took = time.perf_counter() - started
    print(f"wrote {len(cards) - len(strays)} viewers in {took:.1f} s")
    if strays:
        raise SystemExit("\n".join(strays))
    if "--thumbnails" in sys.argv:
        await draw_thumbnails(list(cards))


if __name__ == "__main__":
    asyncio.run(main())
