# Copyright 2026 Maxence Faldor
"""What a build produced, as data: every step's exact state, its facts and its line.

The runner writes a `BuildResult` as JSON from the script's own process; the service
reads it back to report, draw and export. Numbers travel as exact data, with the
tower of square roots they live in, so a state reads back exactly as the script left it.
"""

from functools import cached_property
from pathlib import Path
from typing import Self

from pydantic import BaseModel, ConfigDict, Field

from origamy.field import Number, NumberData, radicands
from origamy.geometry import Isometry, Line, Point
from origamy.paper import Paper, Step
from origamy.provenance import Frame, Source
from origamy.rules import Diagnostic, Level
from origamy.space import Form, Turn, Vector
from origamy.state import Face, State


def _point(data: tuple[NumberData, NumberData]) -> Point:
    x, y = data
    return Point(Number.from_data(x), Number.from_data(y))


class _Data(BaseModel):
    model_config = ConfigDict(frozen=True, extra="forbid")


class StateData(_Data):
    """A state of the paper: crease pattern, places and stacking."""

    vertices: tuple[tuple[NumberData, NumberData], ...] = Field(
        description="Each vertex on the crease pattern, as exact x and y."
    )
    faces: tuple[tuple[int, ...], ...] = Field(
        description="Each face's vertex ids, counterclockwise on the crease pattern."
    )
    places: tuple[
        tuple[NumberData, NumberData, NumberData, NumberData, NumberData, NumberData],
        ...,
    ] = Field(description="Each face's isometry: xx, xy, yx, yy, tx, ty.")
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
            above=state.stacking,
        )

    def state(self) -> State:
        """The state this data describes, exactly.

        The state is built once, so the stages of a build share it and what it works
        out; its numbers live in the tower the build's data reads in.
        """
        return self._state

    @cached_property
    def _state(self) -> State:
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

    @classmethod
    def of(cls, diagnostic: Diagnostic) -> Self:
        """The data of a diagnostic."""
        return cls(
            rule=diagnostic.rule,
            level=diagnostic.level,
            message=diagnostic.message,
            faces=diagnostic.faces,
        )

    def diagnostic(self) -> Diagnostic:
        """The diagnostic this data describes."""
        return Diagnostic(self.rule, self.level, self.message, self.faces)


class TurnData(_Data):
    """A rotation in space about an axis."""

    point: Vector = Field(description="A point of the axis: the middle of the hinge.")
    direction: Vector = Field(description="The axis, as a unit vector.")
    angle: float = Field(description="The rotation in radians, counterclockwise.")


class StepData(_Data):
    """One action and the paper after it, or the paper a refused action began on."""

    action: str
    description: str
    state: StateData
    moved: tuple[int, ...] = ()
    line: tuple[NumberData, NumberData, NumberData] | None = Field(
        default=None, description="The fold line: normal x, normal y and offset."
    )
    flap: tuple[NumberData, NumberData] | None = None
    source: Source | None = None
    diagnostics: tuple[DiagnosticData, ...] = ()
    refusal: DiagnosticData | None = None
    form: tuple[tuple[float, ...], ...] | None = Field(
        default=None,
        description="Each face's rigid motion in space from where it lies flat, once "
        "shaping turns the paper: a 3 by 3 rotation, row by row, then a shift.",
    )
    turn: TurnData | None = Field(
        default=None, description="The turn in space a shape or turn over step makes."
    )
    path: tuple[tuple[tuple[float, ...], ...], ...] = Field(
        default=(), description="The forms a flex passes through on its way, in order."
    )
    through: StateData | None = Field(
        default=None,
        description="The folded paper a precrease passes through before it unfolds.",
    )
    settle: tuple[tuple[int, int, float], ...] = Field(
        default=(),
        description="The creases a settle step turns, as their two vertices, and the "
        "fold angle in radians, valleys positive, each turns toward.",
    )
    ease: float = Field(
        default=0.0,
        ge=0,
        description="The angle in radians by which a settle step opens every other "
        "fold.",
    )

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
            diagnostics=tuple(DiagnosticData.of(d) for d in step.diagnostics),
            refusal=DiagnosticData.of(step.refusal) if step.refusal else None,
            form=step.form.motions if step.form else None,
            turn=(
                TurnData(
                    point=step.turn.point,
                    direction=step.turn.direction,
                    angle=step.turn.angle,
                )
                if step.turn
                else None
            ),
            path=tuple(form.motions for form in step.path),
            through=StateData.of(step.through) if step.through else None,
            settle=tuple((u, v, angle) for (u, v), angle in step.settle),
            ease=step.ease,
        )

    def step(self) -> Step:
        """The step this data describes, built once, as `StateData.state` is."""
        return self._step

    @cached_property
    def _step(self) -> Step:
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
            self.refusal.diagnostic() if self.refusal else None,
            Form(self.form) if self.form is not None else None,
            Turn(self.turn.point, self.turn.direction, self.turn.angle)
            if self.turn
            else None,
            tuple(Form(motions) for motions in self.path),
            self.through.state() if self.through else None,
            tuple(((u, v), angle) for u, v, angle in self.settle),
            self.ease,
        )


class PaperData(_Data):
    """The paper's size and colors."""

    width: NumberData
    height: NumberData
    front: str
    back: str

    def size(self) -> tuple[Number, Number]:
        """The paper's width and height."""
        return Number.from_data(self.width), Number.from_data(self.height)

    def unit(self) -> Number:
        """The paper's longer side, the scale pictures draw to."""
        return max(self.size())


class Failure(_Data):
    """Why a fold script stopped with an error that is not a refused action."""

    error: str = Field(description="Exception type, such as 'ValueError'.")
    message: str = Field(description="Exception message.")
    frames: tuple[Frame, ...] = Field(
        default=(), description="Script frames of the traceback, innermost first."
    )


class BuildResult(_Data):
    """What a fold script folded: its paper, its steps and any refused action."""

    paper: PaperData
    steps: tuple[StepData, ...]
    refused: StepData | None = Field(
        default=None,
        description="The action the paper could not take, on the paper it began on.",
    )
    tower: tuple[NumberData, ...] = Field(
        default=(),
        description="The radicands of the build's tower of numbers, level 1 first.",
    )
    layers: int = Field(
        default=0,
        description="The most layers of paper at one point after the last step.",
    )
    thickest: tuple[NumberData, NumberData] | None = Field(
        default=None,
        description="A point where that many layers lie, as exact x and y.",
    )

    @classmethod
    def of(cls, paper: Paper, root: Path, refused: Step | None = None) -> Self:
        """The result of a script that folded ``paper``, paths relative to ``root``."""
        layers, point = paper.state.thickest
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
            layers=layers,
            thickest=(point.x.data(), point.y.data()),
        )
