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
import gzip
import itertools
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

import numpy as np
from numpy.typing import NDArray
from playwright.async_api import Route, async_playwright

from origamy import flex
from origamy.draw import GAP, Eye, across, folded_box, layers
from origamy.errors import OrigamyError
from origamy.geometry import Isometry, Line
from origamy.paper import Step
from origamy.physics import Folding, Stage
from origamy.render import BROWSER
from origamy.result import BuildResult
from origamy.space import Shaped, placements
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

_FLEX_FACES: Final = 128
"""The most faces a collapse may hold for the viewer to follow it as rigid origami.

The flex solver's work grows with the cube of the paper's creases, and a crease
pattern of hundreds of folds rarely moves as rigid origami at all. A larger collapse
moves each face straight to its place."""

_TILT: Final = (0.35, -0.75, 0.75)
"""Toward the eye the viewer opens flat paper with: in front of it, and above."""

_STEEPEST: Final = 89.99
"""The steepest elevation an eye takes, so that it still knows which way is up."""

_SHOTS: Final = 12
"""The most frames a film keeps of one simulated fold."""

_SHARE: Final = 1e-9
"""How far short of a share of a fold's way a frame may lie and still reach it."""

_STEPS: Final = 32000
"""How many steps a film's frames take from the middle of the table to their reach."""

_STILL_ANGLE: Final = 1e-9
"""The least angle, in radians, between two poses that a turn tells apart."""

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
  window.origamy = await start(root, build);
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


def write_viewer(
    result: BuildResult,
    name: str,
    path: Path,
    *,
    studio: bool = False,
    folding: Folding | None = None,
) -> Path:
    """Write the viewer of a build to ``path`` and return the path.

    With ``studio``, the viewer opens showing paper as paper: lit by a soft room and
    casting shadows on a table, without outlines. Otherwise it opens as a diagram.
    With ``folding``, the build as the simulator folded it, the viewer plays the
    simulated paper's frames in place of rigid faces.
    """
    # A film shows each step's motion, so the rigid faces need none.
    data = build_data(result, name, motion=folding is None) | {"studio": studio}
    if folding is not None:
        data["film"] = film_data(result, folding)
    path.write_text(_page(data, name), encoding="utf-8")
    return path


def film_data(result: BuildResult, folding: Folding) -> Data:
    """The simulated paper as the viewer plays it: its mesh, and each step's frames.

    Each frame is where every vertex of the mesh lay, in whole steps of ``step`` of the
    paper's units, less where it lay the frame before, gzipped. The paper
    rests exactly where each state puts it, and moves between rests as the simulator
    folded it. Each fold names, for each face's copy of a vertex, how many layers lie
    under it, and for each face whether it shows its back. The viewer thus draws
    touching layers in their stacking.
    """
    steps = [data.step() for data in result.steps]
    last = steps[-1].state
    # The viewer works in the paper's units; the simulator, in meters.
    xs = [p.floats()[0] for p in last.vertices]
    rest = folding.mesh.rest
    scale = float(rest[:, 0].max() - rest[:, 0].min()) / (max(xs) - min(xs))
    films: list[Data] = [{"frames": "", "folds": []} for _ in steps]
    frames: list[list[NDArray[np.float64]]] = [[] for _ in steps]
    # Each vertex goes with one face of the last crease pattern that it bounds.
    owner = np.zeros(len(rest), dtype=np.int64)
    owner[folding.mesh.triangles.ravel()] = np.repeat(folding.faces, 3)
    flat = np.column_stack([rest[:, :2] / scale, np.zeros(len(rest))])
    before = np.column_stack([rest[:, :2], np.zeros(len(rest))])
    correction = np.zeros_like(before)
    copies = _copies(folding.mesh.triangles, folding.faces)
    posed = _Pose(np.eye(3), np.zeros(3))
    for stage in folding.stages:
        parents = stage.state.parents(last)
        if stage.settled:
            # A settle stage bends the paper away from any state: its frames keep the
            # simulator's paper, posed as the last rest was, the last mismatch fading.
            shares, kept = _shots(before, stage.frames)
            for index in kept:
                shaped = stage.frames[index] + (1 - shares[index]) * correction
                frames[stage.step].append(posed.moved(shaped))
            films[stage.step]["folds"].append(
                {
                    "count": len(kept),
                    "lifts": _packed(_lifts(stage, parents, copies)),
                    "flips": [stage.state.faces[p].place.flips for p in parents],
                }
            )
            before, correction = stage.frames[-1], np.zeros_like(correction)
            continue
        # The paper rests exactly where its state puts it. Between rests it keeps the
        # simulator's motion, eased from one rest's mismatch to the next by how far it
        # has come, and turned as a whole from one rest's pose to the next.
        placed = np.array(placements(stage.state, stage.form))[np.array(parents)][owner]
        state = np.einsum("nij,nj->ni", placed[:, :9].reshape(-1, 3, 3), flat)
        state = (state + placed[:, 9:]) * scale
        found = stage.frames[-1]
        reached = _fitted(state, found, flat=stage.form is None)
        shares, kept = _shots(before, stage.frames)
        after = reached - found
        pose = _Pose.between(reached, state)
        for index in kept:
            share = shares[index]
            shaped = stage.frames[index] + (1 - share) * correction + share * after
            frames[stage.step].append(posed.toward(pose, share).moved(shaped))
        before, correction, posed = found, after, pose
        films[stage.step]["folds"].append(
            {
                "count": len(kept),
                "lifts": _packed(_lifts(stage, parents, copies)),
                "flips": [stage.state.faces[p].place.flips for p in parents],
            }
        )
    # Frames keep whole steps, a thirty-thousandth of the paper's reach. Each frame
    # stores how far it moved from the one before, and gzip packs the moves.
    reach = max((np.abs(np.stack(own)).max() for own in frames if own), default=1.0)
    step = reach / scale / _STEPS
    for film, own in zip(films, frames, strict=True):
        if own:
            steps = np.rint(np.stack(own) / scale / step).astype(np.int64)
            moves = np.diff(steps, axis=0, prepend=np.zeros_like(steps[:1]))
            raw = np.ascontiguousarray(moves.astype("<i4")).tobytes()
            film["frames"] = base64.b64encode(gzip.compress(raw, mtime=0)).decode()
    return {
        "step": step,
        "rest": _packed((rest / scale).astype(np.float32)),
        "triangles": _packed(folding.mesh.triangles.astype(np.int32)),
        "faces": _packed(folding.faces.astype(np.int32)),
        "vertices": len(folding.mesh.rest),
        "steps": films,
    }


def _shots(
    before: NDArray[np.float64], frames: Sequence[NDArray[np.float64]]
) -> tuple[NDArray[np.float64], list[int]]:
    """How far along its way each frame of a fold lies, and the frames a film keeps.

    The way is the paper's greatest move from frame to frame, from where it lay
    before. A film keeps a few frames, evenly spread along the way, and the last.
    """
    lengths = np.array(
        [np.abs(b - a).max() for a, b in itertools.pairwise([before, *frames])]
    )
    total = lengths.sum()
    shares = np.cumsum(lengths) / total if total > 0 else np.ones(len(lengths))
    marks = np.arange(1, _SHOTS + 1) / _SHOTS - _SHARE
    return shares, sorted({*np.searchsorted(shares, marks).tolist(), len(shares) - 1})


@dataclass(frozen=True)
class _Pose:
    """A turn of paper about its middle, then a shift: how a frame comes to its rest.

    The turn acts on rows of points, and the middle is the points' mean.
    """

    turn: NDArray[np.float64]
    shift: NDArray[np.float64]

    @classmethod
    def between(cls, points: NDArray[np.float64], target: NDArray[np.float64]) -> _Pose:
        """The pose that takes points, rigidly, nearest a target, by Kabsch."""
        middle, aim = points.mean(axis=0), target.mean(axis=0)
        u, _, vt = np.linalg.svd((points - middle).T @ (target - aim))
        sign = np.sign(np.linalg.det(u @ vt)) or 1.0
        return cls(u @ np.diag([1.0, 1.0, sign]) @ vt, aim - middle)

    def moved(self, points: NDArray[np.float64]) -> NDArray[np.float64]:
        """Points in this pose."""
        middle = points.mean(axis=0)
        return (points - middle) @ self.turn + middle + self.shift

    def toward(self, other: _Pose, share: float) -> _Pose:
        """The pose a share of the way to another, turning about one axis."""
        start, end = _quaternion(self.turn), _quaternion(other.turn)
        if start @ end < 0:
            end = -end
        angle = np.arccos(np.clip(start @ end, -1.0, 1.0))
        if angle < _STILL_ANGLE:
            mixed = start
        else:
            mixed = np.sin((1 - share) * angle) * start + np.sin(share * angle) * end
            mixed /= np.sin(angle)
        return _Pose(_turn(mixed), (1 - share) * self.shift + share * other.shift)


def _quaternion(turn: NDArray[np.float64]) -> NDArray[np.float64]:
    """The unit quaternion (w, x, y, z) of a turn that acts on rows of points."""
    m = turn.T
    w = np.sqrt(max(0.0, 1 + m[0, 0] + m[1, 1] + m[2, 2])) / 2
    x = np.copysign(
        np.sqrt(max(0.0, 1 + m[0, 0] - m[1, 1] - m[2, 2])) / 2, m[2, 1] - m[1, 2]
    )
    y = np.copysign(
        np.sqrt(max(0.0, 1 - m[0, 0] + m[1, 1] - m[2, 2])) / 2, m[0, 2] - m[2, 0]
    )
    z = np.copysign(
        np.sqrt(max(0.0, 1 - m[0, 0] - m[1, 1] + m[2, 2])) / 2, m[1, 0] - m[0, 1]
    )
    q = np.array([w, x, y, z])
    return q / np.linalg.norm(q)


def _turn(q: NDArray[np.float64]) -> NDArray[np.float64]:
    """The turn, acting on rows of points, of a unit quaternion (w, x, y, z)."""
    w, x, y, z = q
    m = np.array(
        [
            [1 - 2 * (y * y + z * z), 2 * (x * y - w * z), 2 * (x * z + w * y)],
            [2 * (x * y + w * z), 1 - 2 * (x * x + z * z), 2 * (y * z - w * x)],
            [2 * (x * z - w * y), 2 * (y * z + w * x), 1 - 2 * (x * x + y * y)],
        ]
    )
    return m.T


def _copies(
    triangles: NDArray[np.int64], faces: NDArray[np.int64]
) -> NDArray[np.int64]:
    """Each face's own copy of its vertices, face by face, as the viewer draws them.

    Each row holds a face and a vertex. A face's vertices come in the order its
    triangles first name them.
    """
    copies: list[tuple[int, int]] = []
    for face in range(int(faces.max()) + 1):
        own = dict.fromkeys(int(v) for v in triangles[faces == face].ravel())
        copies += [(face, v) for v in own]
    return np.array(copies, dtype=np.int64)


def _lifts(
    stage: Stage, parents: Sequence[int], copies: NDArray[np.int64]
) -> NDArray[np.uint8]:
    """How many layers lie under each copy of a vertex at a fold's end.

    Each face sits only as high as the faces it lies over need, so a stack thickens
    only as far as its own layers reach.
    """
    heights = np.array(layers(Shaped(stage.state, stage.form), tight=True).heights)
    return np.minimum(heights[np.array(parents)[copies[:, 0]]], 255).astype(np.uint8)


def _fitted(
    points: NDArray[np.float64], target: NDArray[np.float64], *, flat: bool
) -> NDArray[np.float64]:
    """Points moved by the rigid motion that brings them nearest a target, by Kabsch.

    Each point pairs with the target's point of the same row, and the motion never
    mirrors them. Flat points only turn and slide on the table, and lie at its height.
    """
    if flat:
        middle, aim = points[:, :2].mean(axis=0), target[:, :2].mean(axis=0)
        u, _, vt = np.linalg.svd((points[:, :2] - middle).T @ (target[:, :2] - aim))
        turn = np.diag([1.0, np.sign(np.linalg.det(u @ vt))])
        moved = (points[:, :2] - middle) @ (u @ turn @ vt) + aim
        return np.column_stack([moved, np.full(len(points), target[:, 2].min())])
    middle, aim = points.mean(axis=0), target.mean(axis=0)
    u, _, vt = np.linalg.svd((points - middle).T @ (target - aim))
    turn = np.diag([1.0, 1.0, np.sign(np.linalg.det(u @ vt))])
    return (points - middle) @ (u @ turn @ vt) + aim


def _packed(array: NDArray[Any]) -> str:
    """An array's bytes, little-endian, in base64."""
    return base64.b64encode(np.ascontiguousarray(array).tobytes()).decode("ascii")


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
    rigid = len(state.faces) <= _FLEX_FACES
    if motion and previous and step.action in _COLLAPSES and not form_before and rigid:
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
