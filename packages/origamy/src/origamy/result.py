# Copyright 2026 Maxence Faldor
"""What a build produced, as data: every step's exact state, its facts and its line.

The runner writes a `BuildResult` as JSON from the script's own process; the service
reads it back to report, draw and export. Numbers travel as exact text, such as
``"1 - √2/2"``, so a state reads back exactly as the script left it.
"""

from collections.abc import Sequence
from pathlib import Path
from typing import Self

from pydantic import BaseModel, ConfigDict, Field, JsonValue

from origamy.field import Number, radicands
from origamy.geometry import Isometry, Line, Point
from origamy.paper import Paper, Step
from origamy.provenance import Frame, Source
from origamy.rules import Diagnostic, Level
from origamy.state import Face, State

type Exact = JsonValue
"""A number as data, as `Number.data` writes it."""


def _point(data: Sequence[JsonValue]) -> Point:
    x, y = data
    return Point(Number.from_data(x), Number.from_data(y))


class _Data(BaseModel):
    model_config = ConfigDict(frozen=True, extra="forbid")


class StateData(_Data):
    """A state of the paper: crease pattern, places and stacking."""

    vertices: tuple[tuple[Exact, Exact], ...] = Field(
        description="Each vertex on the crease pattern, as exact x and y."
    )
    faces: tuple[tuple[int, ...], ...] = Field(
        description="Each face's vertex ids, counterclockwise on the crease pattern."
    )
    places: tuple[tuple[Exact, Exact, Exact, Exact, Exact, Exact], ...] = Field(
        description="Each face's isometry: xx, xy, yx, yy, tx, ty."
    )
    above: tuple[tuple[int, int], ...] = Field(
        description="Each pair of overlapping faces, the upper first."
    )

    @classmethod
    def of(cls, state: State) -> Self:
        """The data of a state."""
        return cls(
            vertices=tuple((p.x.data(), p.y.data()) for p in state.vertices),
            faces=tuple(f.corners for f in state.faces),
            places=tuple(
                (
                    f.place.xx.data(),
                    f.place.xy.data(),
                    f.place.yx.data(),
                    f.place.yy.data(),
                    f.place.tx.data(),
                    f.place.ty.data(),
                )
                for f in state.faces
            ),
            above=tuple(sorted(state.above)),
        )

    def state(self) -> State:
        """The state this data describes, exactly."""
        return State(
            tuple(_point(v) for v in self.vertices),
            tuple(
                Face(corners, Isometry(*(Number.from_data(x) for x in place)))
                for corners, place in zip(self.faces, self.places, strict=True)
            ),
            frozenset(self.above),
        )


class DiagnosticData(_Data):
    """One statement of a rule's fact."""

    rule: str
    level: Level
    message: str
    faces: tuple[int, ...] = ()

    def diagnostic(self) -> Diagnostic:
        """The diagnostic this data describes."""
        return Diagnostic(self.rule, self.level, self.message, self.faces)


class StepData(_Data):
    """One action and the state after it, or before it when the action was refused."""

    action: str
    description: str
    state: StateData
    moved: tuple[int, ...] = ()
    line: tuple[Exact, Exact, Exact] | None = Field(
        default=None, description="The fold line: normal x, normal y and offset."
    )
    flap: tuple[Exact, Exact] | None = None
    source: Source | None = None
    diagnostics: tuple[DiagnosticData, ...] = ()

    @classmethod
    def of(cls, step: Step, root: Path) -> Self:
        """The data of a step, its script paths relative to ``root``."""
        line = step.line
        return cls(
            action=step.action,
            description=step.description,
            state=StateData.of(step.state),
            moved=tuple(sorted(step.moved)),
            line=(
                (line.normal.x.data(), line.normal.y.data(), line.offset.data())
                if line
                else None
            ),
            flap=(step.flap.x.data(), step.flap.y.data()) if step.flap else None,
            source=step.source.relative_to(root) if step.source else None,
            diagnostics=tuple(
                DiagnosticData(
                    rule=d.rule, level=d.level, message=d.message, faces=d.faces
                )
                for d in step.diagnostics
            ),
        )

    def step(self) -> Step:
        """The step this data describes."""
        line = (
            Line.make(_point(self.line[:2]), Number.from_data(self.line[2]))
            if self.line
            else None
        )
        return Step(
            self.action,
            self.description,
            self.state.state(),
            frozenset(self.moved),
            line,
            _point(self.flap) if self.flap else None,
            self.source,
            tuple(d.diagnostic() for d in self.diagnostics),
        )


class PaperData(_Data):
    """The paper's size and colors."""

    width: Exact
    height: Exact
    front: str
    back: str


class Failure(_Data):
    """Why a fold script stopped with an error that is not a refused action."""

    error: str = Field(description="Exception type, such as 'ValueError'.")
    message: str = Field(description="Exception message.")
    frames: tuple[Frame, ...] = Field(
        default=(), description="Script frames of the traceback, innermost first."
    )


class BuildResult(_Data):
    """What a fold script produced: its steps, any refused action, or a failure."""

    paper: PaperData | None = None
    steps: tuple[StepData, ...] = ()
    refused: StepData | None = Field(
        default=None,
        description="The action the paper could not take, with the state before it.",
    )
    failure: Failure | None = None
    tower: tuple[JsonValue, ...] = Field(
        default=("2",),
        description="The radicands of the build's tower of numbers, level 1 first.",
    )

    @classmethod
    def of(cls, paper: Paper, root: Path, refused: Step | None = None) -> Self:
        """The result of a script that folded ``paper``, paths relative to ``root``."""
        return cls(
            paper=PaperData(
                width=paper.width.data(),
                height=paper.height.data(),
                front=paper.front,
                back=paper.back,
            ),
            steps=tuple(StepData.of(step, root) for step in paper.steps),
            refused=StepData.of(refused, root) if refused else None,
            tower=tuple(radicand.data() for radicand in radicands()),
        )
