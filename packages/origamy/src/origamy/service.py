# Copyright 2026 Maxence Faldor
"""Every command, defined once: a request model and an async function.

The command line runs these functions; a server can await them. Each one returns a
model with a ``text`` that people and agents read.
"""

import asyncio
import json
import sys
import tempfile
from pathlib import Path
from typing import Self

from pydantic import BaseModel, ConfigDict, Field, TypeAdapter
from pydantic_settings import CliPositionalArg

from origamy.errors import OrigamyError
from origamy.exports import to_fold
from origamy.field import Number, tower
from origamy.provenance import Source
from origamy.render import render
from origamy.result import BuildResult, DiagnosticData, Failure, StepData
from origamy.rules import RULES, Level, rule
from origamy.viewer import write_viewer

_RUN = TypeAdapter[BuildResult | Failure](BuildResult | Failure)
"""What the runner writes: a build's result, or why its script failed."""


class _Model(BaseModel):
    model_config = ConfigDict(frozen=True, extra="forbid")


class BuildRequest(_Model):
    """Run a fold script, then report its steps and draw and export the paper."""

    script: CliPositionalArg[Path] = Field(
        description="The fold script: a Python file that defines fold(), "
        "returning a Paper."
    )
    pictures: bool = Field(default=True, description="Draw the build's pictures.")
    timeout: float = Field(
        default=300, gt=0, description="Seconds the script may run before it stops."
    )


class Fact(_Model):
    """A diagnostic, with the step and the script line it belongs to."""

    step: int = Field(description="The step's number; step 0 is the unfolded paper.")
    action: str = Field(description="The action's name, such as 'valley'.")
    description: str = Field(description="What the action does, with its line.")
    source: Source | None
    diagnostic: DiagnosticData

    @classmethod
    def of(cls, step: int, data: StepData, diagnostic: DiagnosticData) -> Self:
        """A fact a step states."""
        return cls(
            step=step,
            action=data.action,
            description=data.description,
            source=data.source,
            diagnostic=diagnostic,
        )


class BuildReport(_Model):
    """What a build did: its steps, its facts and the files it wrote."""

    script: str
    steps: int = Field(default=0, description="How many actions the paper took.")
    refused: Fact | None = Field(
        default=None, description="The action the paper could not take, if any."
    )
    facts: tuple[Fact, ...] = ()
    failure: Failure | None = None
    files: tuple[str, ...] = ()

    @property
    def ok(self) -> bool:
        """Whether the script ran to its end with no error."""
        return (
            self.refused is None
            and self.failure is None
            and all(f.diagnostic.level is not Level.ERROR for f in self.facts)
        )

    @property
    def text(self) -> str:
        """The report as lines of text, like a compiler's."""
        lines: list[str] = []
        if self.failure:
            where = f"{self.failure.frames[0]}: " if self.failure.frames else ""
            lines.append(
                f"{self.script}: the script failed: {where}"
                f"{self.failure.error}: {self.failure.message}"
            )
        elif self.refused:
            lines.append(
                f"{self.script}: {_steps(self.steps)} folded, "
                f"then step {self.refused.step} refused"
            )
        else:
            lines.append(f"{self.script}: {_steps(self.steps)} folded")
        for fact in (*self.facts, *([self.refused] if self.refused else [])):
            d = fact.diagnostic
            where = f"{fact.source}: " if fact.source else ""
            head = f"{d.level}[{d.rule}] step {fact.step}, {fact.description}"
            lines.append(f"{head}: {where}{d.message}")
            if d.faces:
                lines.append(f"  faces: {', '.join(map(str, d.faces))}")
        if self.files:
            lines.append(f"wrote {', '.join(self.files)}")
        return "\n".join(lines)


def _steps(count: int) -> str:
    return f"{count} step" if count == 1 else f"{count} steps"


async def run_script(script: Path, timeout: float) -> BuildResult | Failure:  # noqa: ASYNC109 - a script's time limit
    """Run a fold script in a process of its own; return what it folded, or why not."""
    if not await asyncio.to_thread(script.is_file):
        msg = f"no fold script at {script}"
        raise OrigamyError(msg)
    with tempfile.TemporaryDirectory() as folder:
        output = Path(folder) / "result.json"
        process = await asyncio.create_subprocess_exec(
            sys.executable,
            "-P",
            "-m",
            "origamy.runner",
            str(script),
            str(output),
            stdout=asyncio.subprocess.DEVNULL,
            stderr=asyncio.subprocess.PIPE,
        )
        try:
            _, stderr = await asyncio.wait_for(process.communicate(), timeout)
        except TimeoutError:
            process.kill()
            await process.wait()
            msg = f"{script} ran longer than {timeout:g} seconds"
            raise OrigamyError(msg) from None
        if not output.is_file():
            msg = (
                f"{script} stopped without a result: {stderr.decode(errors='replace')}"
            )
            raise OrigamyError(msg)
        return _RUN.validate_json(output.read_text(encoding="utf-8"))


async def build(request: BuildRequest) -> BuildReport:
    """Run a fold script, report its facts, and write its pictures and FOLD file."""
    script = request.script
    result = await run_script(script, request.timeout)
    if isinstance(result, Failure):
        return BuildReport(script=script.name, failure=result)
    facts = tuple(
        Fact.of(index, step, d)
        for index, step in enumerate(result.steps)
        for d in step.diagnostics
    )
    refused = None
    if result.refused:
        step = len(result.steps)
        facts += tuple(
            Fact.of(step, result.refused, d) for d in result.refused.diagnostics
        )
        if result.refused.refusal:
            refused = Fact.of(step, result.refused, result.refused.refusal)
    with tower(Number.from_data(radicand) for radicand in result.tower):
        stem = script.with_suffix("")
        fold_file = stem.with_suffix(".fold")
        fold_file.write_text(
            json.dumps(to_fold(result, script.stem), ensure_ascii=False),
            encoding="utf-8",
        )
        files = [fold_file]
        if request.pictures:
            overview = stem.with_suffix(".png")
            steps = stem.with_name(f"{stem.name}.steps.png")
            await render(result, script.stem, overview, steps)
            viewer = await asyncio.to_thread(
                write_viewer, result, script.stem, stem.with_suffix(".html")
            )
            files += [overview, steps, viewer]
    return BuildReport(
        script=script.name,
        steps=len(result.steps) - 1,
        refused=refused,
        facts=facts,
        files=tuple(str(path) for path in files),
    )


class ExplainRequest(_Model):
    """Explain the rules: each one's level and exact condition."""

    rule: CliPositionalArg[str | None] = Field(
        default=None, description="The rule to explain; every rule when omitted."
    )


class RuleInfo(_Model):
    """One rule: its name, level and exact condition."""

    name: str
    level: Level
    refusal: bool = Field(description="Whether the rule stops the action it states.")
    summary: str
    condition: str


class ExplainResult(_Model):
    """The rules asked for."""

    rules: tuple[RuleInfo, ...]

    @property
    def text(self) -> str:
        """Each rule's name, level and exact condition, as text."""
        return "\n\n".join(
            f"{r.name} ({r.level}{', refuses the action' if r.refusal else ''})\n"
            f"  {r.summary}\n  {r.condition}"
            for r in self.rules
        )


async def explain(request: ExplainRequest) -> ExplainResult:
    """Each rule's name, level and exact condition."""
    rules = list(RULES.values()) if request.rule is None else [rule(request.rule)]
    return ExplainResult(
        rules=tuple(
            RuleInfo(
                name=r.name,
                level=r.level,
                refusal=r.refusal,
                summary=r.summary,
                condition=r.condition,
            )
            for r in rules
        )
    )
