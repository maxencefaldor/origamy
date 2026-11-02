# Copyright 2026 Maxence Faldor
"""Record the simulator's runs that the docs show, as data for the runs viewer.

Run from the repository root: ``uv run python docs/simulator.py``. Each run simulates a
scene with origamy-sim and writes its frames to ``docs/simulator/runs/<name>.json``,
which ``docs/simulator/runs/viewer.html`` plays. The frames are build output, left out
of the repository, and the docs workflow records them before it builds the site.

The configuration reads the command line through pydantic-settings, so
``--material.thickness 1e-4`` records every run with thicker paper, and
``--runs stack,strip`` records only those runs.
"""

import base64
import copy
import json
import math
import time
from collections.abc import Callable, Iterable
from dataclasses import dataclass
from pathlib import Path
from typing import ClassVar, Final, Literal

import numpy as np
from numpy.typing import NDArray
from pydantic import Field
from pydantic_settings import CliApp, SettingsConfigDict

from origamy_sim import (
    Config,
    Crease,
    Mesh,
    Report,
    Sheet,
    Simulation,
    Table,
)

RUNS: Final = Path(__file__).resolve().parent / "simulator" / "runs"

type Name = Literal["fold-over", "stack", "strip", "ramps", "accordion", "miura"]

CREASE: Final = Crease(stiffness=0.01)
"""A crease about as stiff as one folded firmly in kami, in N·m per meter per radian."""


type Stats = dict[str, float | int | bool]
"""A step's figures, by the key the viewer reads them with."""


@dataclass(frozen=True)
class Frame:
    """One recorded state of a run: where every vertex lies, and what the step did."""

    positions: NDArray[np.float64]
    """Every vertex of every sheet, in meters, of shape (n, 3)."""

    stats: Stats
    """The step's figures the viewer reads out."""


@dataclass(frozen=True)
class Run:
    """A recorded run: its scene, its frames and how to play them."""

    title: str
    triangles: list[int]
    """Every triangle's vertices, flattened, numbering the sheets' vertices in turn."""

    creases: list[tuple[int, int, int]]
    """Each crease's two vertices, and 1 for a valley or -1 for a mountain."""

    seconds_per_frame: float
    """The simulated time between frames, or how long to show each frame of a
    quasi-static run."""

    table: float | None
    """The table's height, in meters, if the run has a table."""

    readouts: list[tuple[str, str, str]]
    """Each figure the viewer reads out: its label, its key in the stats, its unit."""

    frames: list[Frame]


def report_stats(report: Report) -> Stats:
    """The figures every step reports."""
    return {
        "iterations": report.iterations,
        "converged": report.converged,
        "strain": report.greatest_strain,
        "contacts": report.contacts,
        "gap": report.least_gap if math.isfinite(report.least_gap) else -1.0,
        "time": report.time,
    }


def everything(simulation: Simulation, sheets: int) -> NDArray[np.float64]:
    """Every vertex of every sheet, in turn."""
    return np.concatenate([simulation.positions(s) for s in range(sheets)])


def triangles_of(meshes: Iterable[Mesh]) -> list[int]:
    """Every triangle of every mesh, numbering the meshes' vertices in turn."""
    triangles: list[int] = []
    offset = 0
    for mesh in meshes:
        triangles += (mesh.triangles + offset).ravel().tolist()
        offset += mesh.rest.shape[0]
    return triangles


def crease_lines(sheet: Sheet) -> list[tuple[int, int, int]]:
    """Each crease of a sheet, with the sign of its rest angle."""
    lines: list[tuple[int, int, int]] = []
    for u, v, _, _ in sheet.mesh.dihedrals.tolist():
        if sheet.crease((u, v)) is not None:
            lines.append((u, v, 1 if sheet.rest_angle((u, v)) >= 0 else -1))
    return lines


def lie_on_table(simulation: Simulation, sheet: int, height: float) -> None:
    """Moves a sheet to lie flat at a height."""
    positions = simulation.positions(sheet)
    positions[:, 2] = height
    simulation.set_positions(sheet, positions)


def fold_over(config: Config) -> Run:
    """A square folded along its diagonal crease onto itself, on the table."""
    cells, side = 24, 0.15
    mesh = Mesh.grid(cells, cells, side, side)
    sheet = Sheet(mesh, config.material)
    diagonal = [
        (k * (cells + 1) + k, (k + 1) * (cells + 1) + k + 1) for k in range(cells)
    ]
    for edge in diagonal:
        sheet.set_crease(edge, CREASE)
    settings = copy.replace(config.settings, table=Table(height=0.0))
    simulation = Simulation([sheet], settings)
    h, gap = config.material.thickness, config.settings.contact.gap
    lie_on_table(simulation, 0, 0.5 * h + 0.5 * gap)
    below = np.flatnonzero(mesh.rest[:, 1] <= mesh.rest[:, 0] + 1e-12)
    simulation.hold(0, below)
    target, ramp = math.pi - 0.01, 90
    index = {tuple(d[:2]): i for i, d in enumerate(mesh.dihedrals.tolist())}

    def fold(step: int) -> Stats:
        rest = target * min(1.0, step / ramp)
        for edge in diagonal:
            simulation.set_rest_angle(0, edge, rest)
        report = simulation.step()
        angles = simulation.fold_angles(0)
        folded = float(np.mean([angles[index[e]] for e in diagonal]))
        return {
            **report_stats(report),
            "rest": math.degrees(rest),
            "fold": math.degrees(folded),
        }

    frames = record(simulation, 1, (fold(step) for step in range(1, 151)))
    return Run(
        title="A square folded over itself",
        triangles=triangles_of([mesh]),
        creases=crease_lines(simulation.sheet(0)),
        seconds_per_frame=simulation.settings.time_step,
        table=0.0,
        readouts=[
            ("Crease rest angle", "rest", "°"),
            ("Fold angle", "fold", "°"),
            ("Least gap", "gap", "m"),
            ("Newton iterations", "iterations", ""),
        ],
        frames=frames,
    )


def stack(config: Config) -> Run:
    """Four squares dropped onto the table, each turned from the one below."""
    layers, cells, side = 4, 8, 0.05
    meshes = [Mesh.grid(cells, cells, side, side) for _ in range(layers)]
    sheets = [Sheet(mesh, config.material) for mesh in meshes]
    settings = copy.replace(config.settings, time_step=1 / 120, table=Table(height=0.0))
    simulation = Simulation(sheets, settings)
    for layer, mesh in enumerate(meshes):
        turn = math.radians(15 * layer)
        rotation = np.array(
            [[math.cos(turn), -math.sin(turn)], [math.sin(turn), math.cos(turn)]]
        )
        centered = mesh.rest - 0.5 * side
        placed = np.column_stack(
            [centered @ rotation.T, np.full(len(centered), 0.002 * (layer + 1))]
        )
        simulation.set_positions(layer, placed)
    frames = record(
        simulation, layers, (report_stats(simulation.step()) for _ in range(90))
    )
    return Run(
        title="Four sheets dropped on the table",
        triangles=triangles_of(meshes),
        creases=[],
        seconds_per_frame=settings.time_step,
        table=0.0,
        readouts=[
            ("Time", "time", "s"),
            ("Pairs in contact", "contacts", ""),
            ("Least gap", "gap", "m"),
            ("Newton iterations", "iterations", ""),
        ],
        frames=frames,
    )


def strip(config: Config) -> Run:
    """A strip clamped 6 cm over the table, let go: it swings down onto the table."""
    columns, rows, length, width, height = 48, 8, 0.12, 0.02, 0.06
    mesh = Mesh.grid(columns, rows, length, width)
    settings = copy.replace(config.settings, time_step=1 / 120, table=Table(height=0.0))
    simulation = Simulation([Sheet(mesh, config.material)], settings)
    lie_on_table(simulation, 0, height)
    simulation.hold(0, np.flatnonzero(mesh.rest[:, 0] < 2 * length / columns + 1e-9))
    tip = mesh.rest[:, 0] > length - 1e-9

    def swing() -> Stats:
        report = simulation.step()
        drop = height - float(simulation.positions(0)[tip, 2].mean())
        return {**report_stats(report), "drop": drop}

    frames = record(simulation, 1, (swing() for _ in range(240)))
    return Run(
        title="A strip swinging onto the table",
        triangles=triangles_of([mesh]),
        creases=[],
        seconds_per_frame=settings.time_step,
        table=0.0,
        readouts=[
            ("Time", "time", "s"),
            ("Tip drop", "drop", "m"),
            ("Least gap", "gap", "m"),
            ("Newton iterations", "iterations", ""),
        ],
        frames=frames,
    )


def ramps(config: Config) -> Run:
    """Two squares on ramps of paper: friction holds one, and the other slides off."""
    length, width, side, lane = 0.16, 0.05, 0.03, 0.08
    tilts = (math.radians(15), math.radians(30))
    ramp, square = Mesh.grid(16, 5, length, width), Mesh.grid(6, 6, side, side)
    meshes = [ramp, ramp, square, square]
    settings = copy.replace(config.settings, time_step=1 / 240, table=Table(height=0.0))
    simulation = Simulation([Sheet(mesh, config.material) for mesh in meshes], settings)
    h, gap = config.material.thickness, config.settings.contact.gap

    def on_ramp(
        k: int, points: NDArray[np.float64], lift: float
    ) -> NDArray[np.float64]:
        """Points in ramp k's plane, lifted along its normal."""
        tilt = tilts[k]
        x, y = points[:, 0], points[:, 1]
        return np.column_stack(
            [
                x * math.cos(tilt) - lift * math.sin(tilt),
                y + k * lane,
                0.5 * h + 0.5 * gap + x * math.sin(tilt) + lift * math.cos(tilt),
            ]
        )

    # Every sheet starts flat at z = 0, on the others. The squares move to their places
    # first, then the far ramp, then the near one, so no sheet moves into another.
    top = square.rest + np.array([length - side - 0.01, 0.5 * (width - side)])
    for k in (0, 1):
        simulation.set_positions(2 + k, on_ramp(k, top, h + 0.5 * gap))
    # Mirrored across its width, each ramp lies white side up under its colored square.
    turned = ramp.rest * np.array([1.0, -1.0]) + np.array([0.0, width])
    for k in (1, 0):
        simulation.set_positions(k, on_ramp(k, turned, 0.0))
        simulation.hold(k, np.arange(len(ramp.rest)))
    downhill = np.array([-math.cos(tilts[0]), 0.0, -math.sin(tilts[0])])
    start = simulation.positions(2).mean(axis=0)

    def slide() -> Stats:
        report = simulation.step()
        slip = float((simulation.positions(2).mean(axis=0) - start) @ downhill)
        speed = float(np.linalg.norm(simulation.velocities(3).mean(axis=0)))
        return {**report_stats(report), "slip": slip, "speed": speed}

    frames = record(simulation, len(meshes), (slide() for _ in range(144)))
    return Run(
        title="Squares on two ramps",
        triangles=triangles_of(meshes),
        creases=[],
        seconds_per_frame=settings.time_step,
        table=0.0,
        readouts=[
            ("Time", "time", "s"),
            ("Slip at 15°", "slip", "m"),
            ("Speed at 30°", "speed", "m/s"),
            ("Newton iterations", "iterations", ""),
        ],
        frames=frames,
    )


def accordion(config: Config) -> Run:
    """A strip pleated by seven creases, valley and mountain in turn, on the table."""
    columns, rows, length, width, panels = 32, 8, 0.16, 0.04, 8
    mesh = Mesh.grid(columns, rows, length, width)
    sheet = Sheet(mesh, config.material)
    lines: list[tuple[list[tuple[int, int]], int]] = []
    for k in range(1, panels):
        i = k * columns // panels
        edges = [
            (j * (columns + 1) + i, (j + 1) * (columns + 1) + i) for j in range(rows)
        ]
        for edge in edges:
            sheet.set_crease(edge, CREASE)
        lines.append((edges, 1 if k % 2 else -1))
    settings = copy.replace(
        config.settings,
        time_step=math.inf,
        table=Table(height=0.0),
        solver=copy.replace(config.settings.solver, iteration_limit=1000),
    )
    simulation = Simulation([sheet], settings)
    h, gap = config.material.thickness, config.settings.contact.gap
    lie_on_table(simulation, 0, 0.5 * h + 0.5 * gap)
    middle = np.flatnonzero(
        np.abs(mesh.rest[:, 0] - 0.5 * length) <= 0.5 * length / panels
    )
    simulation.hold(0, middle)
    target, count = math.radians(150), 70

    def pleat(step: int) -> Stats:
        angle = target * step / count
        for edges, sign in lines:
            for edge in edges:
                simulation.set_rest_angle(0, edge, sign * angle)
        return {**report_stats(simulation.step()), "rest": math.degrees(angle)}

    frames = record(simulation, 1, (pleat(step) for step in range(1, count + 1)))
    return Run(
        title="Accordion pleats",
        triangles=triangles_of([mesh]),
        creases=crease_lines(simulation.sheet(0)),
        seconds_per_frame=1 / 20,
        table=0.0,
        readouts=[
            ("Crease rest angles", "rest", "°"),
            ("Greatest strain", "strain", ""),
            ("Least gap", "gap", "m"),
            ("Newton iterations", "iterations", ""),
        ],
        frames=frames,
    )


def miura(config: Config) -> Run:
    """A Miura-ori folded rigidly by actuating its creases, in the air."""
    columns, rows, side, angle = 8, 6, 0.015, 1.2

    def rigid(fold: float) -> NDArray[np.float64]:
        across = side * math.cos(angle) / math.cos(fold)
        along = math.sqrt(side * side - across * across)
        i, j = np.meshgrid(np.arange(columns + 1), np.arange(rows + 1))
        return np.column_stack(
            [
                (i * side * math.cos(fold) + (j % 2) * across).ravel(),
                (j * along).ravel(),
                ((i % 2) * side * math.sin(fold)).ravel(),
            ]
        )

    at = lambda i, j: j * (columns + 1) + i  # noqa: E731
    triangles = [
        corners
        for j in range(rows)
        for i in range(columns)
        for corners in (
            (at(i, j), at(i + 1, j), at(i + 1, j + 1)),
            (at(i, j), at(i + 1, j + 1), at(i, j + 1)),
        )
    ]
    mesh = Mesh(rigid(0.0)[:, :2], triangles)
    sheet = Sheet(mesh, config.material)
    creases = [
        (u, v)
        for u, v, _, _ in mesh.dihedrals.tolist()
        if u % (columns + 1) == v % (columns + 1)
        or u // (columns + 1) == v // (columns + 1)
    ]
    for edge in creases:
        sheet.set_crease(edge, CREASE)
    settings = copy.replace(
        config.settings, gravity=(0.0, 0.0, 0.0), time_step=math.inf, table=None
    )
    exact = Simulation([Sheet(mesh, config.material)], settings)
    simulation = Simulation([sheet], settings)
    simulation.hold(0, [at(0, 0), at(1, 0), at(1, 1)])
    index = {tuple(d[:2]): i for i, d in enumerate(mesh.dihedrals.tolist())}
    count = 60

    def fold(step: int) -> Stats:
        target = 0.85 * angle * step / count
        exact.set_positions(0, rigid(target))
        angles = exact.fold_angles(0)
        for edge in creases:
            simulation.set_rest_angle(0, edge, float(angles[index[edge]]))
        report = simulation.step()
        folded = simulation.fold_angles(0)
        return {
            **report_stats(report),
            "fold": math.degrees(max(abs(folded[index[e]]) for e in creases)),
            "error": float(np.abs(folded - angles).max()),
        }

    frames = record(simulation, 1, (fold(step) for step in range(1, count + 1)))
    return Run(
        title="Miura-ori",
        triangles=triangles_of([mesh]),
        creases=crease_lines(simulation.sheet(0)),
        seconds_per_frame=1 / 20,
        table=None,
        readouts=[
            ("Largest fold", "fold", "°"),
            ("Gap to rigid folding", "error", "rad"),
            ("Greatest strain", "strain", ""),
            ("Newton iterations", "iterations", ""),
        ],
        frames=frames,
    )


def record(
    simulation: Simulation,
    sheets: int,
    steps: Iterable[Stats],
) -> list[Frame]:
    """The first state of a run, then the state after each of its steps."""
    first = Frame(
        positions=everything(simulation, sheets),
        stats={"iterations": 0, "converged": True, "time": 0.0},
    )
    return [
        first,
        *(Frame(positions=everything(simulation, sheets), stats=s) for s in steps),
    ]


NAMES: Final[tuple[Name, ...]] = (
    "fold-over",
    "stack",
    "strip",
    "ramps",
    "accordion",
    "miura",
)

SCENES: Final[dict[Name, Callable[[Config], Run]]] = {
    "fold-over": fold_over,
    "stack": stack,
    "strip": strip,
    "ramps": ramps,
    "accordion": accordion,
    "miura": miura,
}


def write(name: str, run: Run) -> Path:
    """Writes a run's data for the viewer: positions as base64 float32, in meters."""
    positions = np.stack([frame.positions for frame in run.frames]).astype(np.float32)
    data = {
        "title": run.title,
        "triangles": run.triangles,
        "creases": run.creases,
        "secondsPerFrame": run.seconds_per_frame,
        "table": run.table,
        "readouts": run.readouts,
        "vertices": positions.shape[1],
        "frames": positions.shape[0],
        "positions": base64.b64encode(positions.tobytes()).decode(),
        "stats": [frame.stats for frame in run.frames],
    }
    path = RUNS / f"{name}.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, separators=(",", ":")), encoding="utf-8")
    return path


class Runs(Config):
    """Record the simulator's runs for the docs."""

    model_config: ClassVar[SettingsConfigDict] = SettingsConfigDict(
        cli_prog_name="docs/simulator.py"
    )

    runs: list[Name] = Field(default_factory=lambda: list(NAMES))
    """The runs to record."""

    def cli_cmd(self) -> None:
        """Record each run and write its data."""
        for name in self.runs:
            begun = time.perf_counter()
            run = SCENES[name](self)
            path = write(name, run)
            converged = all(frame.stats["converged"] for frame in run.frames)
            print(
                f"wrote {path.name}: {len(run.frames)} frames in "
                f"{time.perf_counter() - begun:.1f} s, "
                f"{'every step converged' if converged else 'NOT EVERY STEP CONVERGED'}"
            )
            if not converged:
                raise SystemExit(1)


if __name__ == "__main__":
    CliApp.run(Runs)
