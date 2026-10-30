# Copyright 2026 Maxence Faldor
"""Pictures of a build as PNG files, drawn in headless Chromium.

Every build writes two pictures. The first shows the paper at the end, or just before
the refused action, as a crease pattern, the folded paper and an x-ray. The second shows
every step, as diagrams do.
"""

import os
from collections.abc import Sequence
from html import escape
from pathlib import Path

from playwright.async_api import async_playwright

from origamy.draw import Look, folded_box, folded_view, pattern_view, xray_view
from origamy.field import Number
from origamy.paper import Step
from origamy.result import BuildResult

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
    assert result.paper is not None  # noqa: S101 - only results with paper render
    unit = max(Number.of(result.paper.width), Number.of(result.paper.height))
    colors = result.paper.front, result.paper.back
    refused = result.refused.step() if result.refused else None
    last = refused or result.steps[-1].step()
    state = last.state
    marked = refused.diagnostics[0].faces if refused else ()
    box = folded_box(state)
    pattern = pattern_view(
        state, (Number.of(result.paper.width), Number.of(result.paper.height)), marked
    )
    folded = folded_view(state, Look(*colors, unit), box, marked)
    xray = xray_view(state, box, unit, marked)
    if refused and refused.line is not None:
        for canvas in (folded, xray):
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
        note = (
            f"Step {count + 1}, {refused.description}: {refused.diagnostics[0].message}"
        )
    else:
        title = f"{name}: {count} steps"
        note = ""
    body = f"<h1>{escape(title)}</h1>"
    if note:
        body += f'<p class="note">{escape(note)}</p>'
    body += (
        '<div class="row">'
        + pattern.svg("Crease pattern")
        + folded.svg("Folded, from above")
        + xray.svg("X-ray: darker where more layers lie")
        + "</div>"
    )
    return _PAGE.format(body=body, note="#b0102a", width=1340, cell=440)


def _steps(result: BuildResult, name: str) -> str:
    assert result.paper is not None  # noqa: S101 - only results with paper render
    unit = max(Number.of(result.paper.width), Number.of(result.paper.height))
    colors = result.paper.front, result.paper.back
    steps: Sequence[Step] = [data.step() for data in result.steps]
    box = folded_box(*(step.state for step in steps))
    cells = []
    # As in diagrams, each picture shows the paper after one step and the next fold.
    following = [*steps[1:], *([result.refused.step()] if result.refused else [])]
    for index, step in enumerate(steps):
        canvas = folded_view(step.state, Look(*colors, unit), box, size=250)
        after = following[index] if index < len(following) else None
        if after is not None and after.line is not None:
            canvas.line(
                after.line,
                stroke="#2c6fbb",
                **{"stroke-width": 1.4, "stroke-dasharray": "6 4"},
            )
        where = (
            f" ({step.source.site.file}:{step.source.site.line})" if step.source else ""
        )
        cells.append(canvas.svg(f"{index}. {step.action}{where}"))
    body = f"<h1>{escape(name)}: every step</h1>"
    body += '<div class="row">' + "".join(cells) + "</div>"
    return _PAGE.format(body=body, note="#222", width=1290, cell=250)
