# Copyright 2026 Maxence Faldor
"""Fold a folder of published crease patterns, and tally what origamy makes of them.

Run from the repository root: ``uv run python docs/patterns.py --folder <path>``. The
script reads each FOLD and ORIPA file under the folder, creases its paper along the
pattern, and collapses it, each in its own process. It prints one line for each
pattern, then the tally: patterns collapsed, refused by each rule, unread, with no
fold assigned, out of time, and crashed. Flat-Folder's examples, at
https://github.com/origamimagiro/flat-folder, hold 755 published patterns.

The configuration reads the command line through pydantic-settings, so ``--jobs 1``
folds one pattern at a time, and ``--timeout 600`` gives each ten minutes.
"""

import asyncio
import sys
import time
from collections import Counter
from pathlib import Path
from typing import ClassVar, Final

from pydantic import BaseModel, Field
from pydantic_settings import BaseSettings, CliApp, SettingsConfigDict

from origamy import CreasePattern, Paper, RefusedError

SUFFIXES: Final = (".fold", ".cp")
"""The files the script reads: FOLD and ORIPA."""


class Folded(BaseModel):
    """What one pattern came to."""

    path: str
    outcome: str = Field(
        description="collapsed, refused, unread, unassigned, out of time, or crashed"
    )
    rule: str | None = None
    message: str | None = None
    folds: int = 0
    faces: int = 0
    layers: int = 0
    seconds: float = 0.0


class Patterns(BaseSettings):
    """How to fold a folder of crease patterns."""

    model_config: ClassVar[SettingsConfigDict] = SettingsConfigDict(
        cli_prog_name="docs/patterns.py", use_attribute_docstrings=True
    )

    folder: Path | None = None
    """The folder whose FOLD and ORIPA files to fold, every subfolder included."""
    pattern: Path | None = None
    """One pattern to fold in this process, printing what it came to as JSON."""
    jobs: int = Field(default=2, gt=0)
    """How many patterns fold at once, each in its own process."""
    timeout: float = Field(default=1200.0, gt=0)
    """How long each pattern may take, in seconds."""

    def cli_cmd(self) -> None:
        """Fold one pattern, or every pattern of the folder."""
        if self.pattern is not None:
            print(fold(self.pattern).model_dump_json())
        elif self.folder is not None:
            paths = sorted(p for p in self.folder.rglob("*") if p.suffix in SUFFIXES)
            asyncio.run(self.tally(self.folder, paths))
        else:
            msg = "name a --folder of patterns, or one --pattern"
            raise SystemExit(msg)

    async def tally(self, folder: Path, paths: list[Path]) -> None:
        """Fold the patterns under a folder, then print the tally."""
        gate = asyncio.Semaphore(self.jobs)

        async def one(path: Path) -> Folded:
            async with gate:
                folded = await self.apart(path)
            print(
                f"{folded.outcome:12} {folded.folds:6} folds {folded.seconds:7.1f} s  "
                f"{path.relative_to(folder)}",
                flush=True,
            )
            return folded

        results = await asyncio.gather(*(one(path) for path in paths))
        outcomes = Counter(r.outcome for r in results)
        rules = Counter(r.rule for r in results if r.outcome == "refused")
        print(
            f"\n{len(results)} patterns: "
            + ", ".join(
                f"{count} {outcome}" for outcome, count in outcomes.most_common()
            )
        )
        for rule, count in rules.most_common():
            print(f"  refused by {rule}: {count}")
        if collapsed := [r for r in results if r.outcome == "collapsed"]:
            largest = max(collapsed, key=lambda r: r.folds)
            print(
                f"largest collapsed: {Path(largest.path).name}, {largest.folds} folds, "
                f"{largest.faces} faces, {largest.layers} layers, "
                f"{largest.seconds:.0f} s"
            )

    async def apart(self, path: Path) -> Folded:
        """Fold one pattern in its own process, within the time limit."""
        process = await asyncio.create_subprocess_exec(
            sys.executable,
            __file__,
            "--pattern",
            str(path),
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        try:
            out, err = await asyncio.wait_for(process.communicate(), self.timeout)
        except TimeoutError:
            process.kill()
            await process.wait()
            return Folded(path=str(path), outcome="out of time", seconds=self.timeout)
        if process.returncode != 0:
            message = err.decode().strip().splitlines()[-1:] or ["killed"]
            return Folded(path=str(path), outcome="crashed", message=message[0])
        return Folded.model_validate_json(out.decode().strip().splitlines()[-1])


def fold(path: Path) -> Folded:
    """Read a pattern, crease its paper along it, and collapse it."""
    start = time.perf_counter()
    try:
        pattern = CreasePattern.read(path)
    except ValueError as error:
        return Folded(path=str(path), outcome="unread", message=str(error))
    folds = len(pattern.valleys) + len(pattern.mountains)
    if not folds:
        return Folded(path=str(path), outcome="unassigned")
    paper = Paper.polygon(pattern.outline)
    try:
        paper.crease(pattern.creases)
        paper.collapse(valleys=pattern.valleys, mountains=pattern.mountains)
    except RefusedError as error:
        return Folded(
            path=str(path),
            outcome="refused",
            rule=error.refusal.rule,
            message=error.refusal.message,
            folds=folds,
            seconds=time.perf_counter() - start,
        )
    return Folded(
        path=str(path),
        outcome="collapsed",
        folds=folds,
        faces=len(paper.state.faces),
        layers=paper.thickest()[0],
        seconds=time.perf_counter() - start,
    )


if __name__ == "__main__":
    CliApp.run(Patterns)
