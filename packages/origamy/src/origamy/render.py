# Copyright 2026 Maxence Faldor
"""Pictures of a build as PNG files, drawn in headless Chromium.

Every build writes two pictures. The first shows the paper at the end, or just before
the refused action, as a crease pattern, the folded paper and an x-ray. The second shows
every step, as diagrams do. Shaped paper shows in space, tilted, in place of from above.
"""

import os
from collections.abc import Sequence
from html import escape
from pathlib import Path

from playwright.async_api import async_playwright

from origamy.draw import (
    Look,
    axis,
    folded_box,
    folded_view,
    pattern_view,
    shaped_box,
    shaped_view,
    xray_view,
)
from origamy.paper import Step
from origamy.result import BuildResult
from origamy.space import Shaped

BROWSER = "ORIGAMY_RENDER_BROWSER"
"""The environment variable that names a Chromium executable to render with."""

_PAGE = """<!doctype html><html><head><meta charset="utf-8"><style>
body {{ margin: 0; background: #fff; font: 13px system-ui, sans-serif; color: #222; }}
main {{ display: inline-block; padding: 14px 18px; }}
h1 {{ font-size: 15px; margin: 0 0 4px; font-weight: 600; }}
p.note {{ margin: 0 0 8px; color: {note}; max-width: {width}px; }}
.row {{ display: flex; flex-wrap: wrap; gap: 6px; max-width: {width}px; }}
figure {{ margin: 0; }}
figcaption {{ font-size: 12px; color: #444; padding: 2px 4px; max-width: {cell}px;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }}
</style></head><body><main>{body}</main></body></html>"""


_NEXT: dict[str, str | float] = {
    "stroke": "#2c6fbb",
    "stroke-width": 1.4,
    "stroke-dasharray": "6 4",
}
"""How a picture of one step draws the line of the next."""


async def render(result: BuildResult, name: str, overview: Path, steps: Path) -> None:
    """Write a build's two pictures."""
    pages = [(_overview(result, name), overview), (_steps(result, name), steps)]
    executable = os.environ.get(BROWSER) or None
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(executable_path=executable)
        try:
            page = await browser.new_page(viewport={"width": 1400, "height": 800})
            for html, path in pages:
                await page.set_content(html)
                await page.locator("main").screenshot(path=path)
        finally:
            await browser.close()


def _overview(result: BuildResult, name: str) -> str:
    unit = result.paper.unit()
    colors = result.paper.front, result.paper.back
    refused = result.refused.step() if result.refused else None
    last = refused or result.steps[-1].step()
    state = last.state
    refusal = refused.refusal if refused else None
    marked = refusal.faces if refusal else ()
    box = folded_box(state)
    pattern = pattern_view(state, result.paper.size(), marked)
    look = Look(*colors, unit)
    if last.form is None:
        folded = folded_view(state, look, box, marked)
        flat = [folded]
    else:
        shaped = Shaped(state, last.form)
        folded = shaped_view(shaped, look, shaped_box(shaped, unit=unit), marked)
        flat = []
    xray = xray_view(state, box, unit, marked)
    if refused and refused.line is not None:
        for canvas in (*flat, xray):
            canvas.line(
                refused.line,
                stroke="#e0182d",
                **{"stroke-width": 2, "stroke-dasharray": "8 5"},
            )
            if refused.flap is not None:
                canvas.dot(refused.flap, 5, fill="#e0182d")
    count = len(result.steps) - 1
    if refused:
        title = f"{name}: refused after step {count}"
        message = f": {refusal.message}" if refusal else ""
        note = f"Step {count + 1}, {refused.description}{message}"
    else:
        title = f"{name}: {count} steps"
        note = ""
    body = f"<h1>{escape(title)}</h1>"
    if note:
        body += f'<p class="note">{escape(note)}</p>'
    body += (
        '<div class="row">'
        + pattern.svg("Crease pattern")
        + folded.svg("Folded, from above" if flat else "Shaped, in space")
        + xray.svg(
            "X-ray: darker where more layers lie"
            if flat
            else "X-ray, as the paper lies flat: darker where more layers lie"
        )
        + "</div>"
    )
    return _PAGE.format(body=body, note="#b0102a", width=1340, cell=440)


def _steps(result: BuildResult, name: str) -> str:
    unit = result.paper.unit()
    colors = result.paper.front, result.paper.back
    steps: Sequence[Step] = [data.step() for data in result.steps]
    look = Look(*colors, unit)
    flat = [step.state for step in steps if step.form is None]
    shaped = [Shaped(step.state, step.form) for step in steps if step.form]
    box = folded_box(*flat)
    space = shaped_box(*shaped, unit=unit) if shaped else box
    cells = []
    # As in diagrams, each picture shows the paper after one step and the next fold.
    following = [*steps[1:], *([result.refused.step()] if result.refused else [])]
    for index, step in enumerate(steps):
        after = following[index] if index < len(following) else None
        if step.form is None:
            canvas = folded_view(step.state, look, box, size=250)
            if after is not None and after.line is not None:
                canvas.line(after.line, **_NEXT)
        else:
            canvas = shaped_view(Shaped(step.state, step.form), look, space, size=250)
            turn = after.turn if after is not None and after.line is not None else None
            if turn is not None:
                reach = max(space[2] - space[0], space[3] - space[1])
                canvas.segment(*axis(turn, reach), **_NEXT)
        where = (
            f" ({step.source.site.file}:{step.source.site.line})" if step.source else ""
        )
        cells.append(canvas.svg(f"{index}. {step.action}{where}"))
    body = f"<h1>{escape(name)}: every step</h1>"
    body += '<div class="row">' + "".join(cells) + "</div>"
    return _PAGE.format(body=body, note="#222", width=1290, cell=250)
