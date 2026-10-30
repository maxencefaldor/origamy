# Copyright 2026 Maxence Faldor
"""The script lines behind each action, so that every fact points at its line."""

import inspect
import sysconfig
from functools import cache
from pathlib import Path
from types import FrameType
from typing import Final, override

from pydantic import BaseModel, ConfigDict, Field

MAX_FRAMES: Final = 4
"""Script frames kept per action, innermost first."""

_PACKAGE: Final = Path(__file__).resolve().parent


class Frame(BaseModel):
    """One line of a fold script."""

    model_config = ConfigDict(frozen=True, extra="forbid")

    file: str = Field(
        description="Script path, relative to the working directory when inside it."
    )
    line: int = Field(ge=1, description="Line number.")
    function: str = Field(
        description="Enclosing function, or '<module>' at the top level."
    )

    @override
    def __str__(self) -> str:
        """The frame as text, such as ``crane.py:12 in fold()``."""
        if self.function == "<module>":
            return f"{self.file}:{self.line} at module level"
        return f"{self.file}:{self.line} in {self.function}()"


class Source(BaseModel):
    """The script's call stack at one action, innermost frame first."""

    model_config = ConfigDict(frozen=True, extra="forbid")

    frames: tuple[Frame, ...] = Field(
        min_length=1, description="Script frames, innermost first."
    )

    @property
    def site(self) -> Frame:
        """The innermost frame: the line that called the action."""
        return self.frames[0]

    @override
    def __str__(self) -> str:
        """The innermost frame, then the frames that called it."""
        callers = "".join(f", called from {frame}" for frame in self.frames[1:])
        return f"{self.site}{callers}"

    def relative_to(self, root: Path) -> Source:
        """This source with file paths relative to ``root`` where possible."""
        return Source(
            frames=tuple(
                frame.model_copy(update={"file": relative_path(frame.file, root)})
                for frame in self.frames
            )
        )


def capture() -> Source | None:
    """The fold script's call stack here, innermost frame first, if a script called.

    The walk skips frames in origamy, the standard library and installed packages up to
    the first script frame, then stops at the next frame outside the script.
    """
    frame: FrameType | None = inspect.currentframe()
    frames: list[Frame] = []
    try:
        while frame is not None and not is_script_file(frame.f_code.co_filename):
            frame = frame.f_back
        while (
            frame is not None
            and len(frames) < MAX_FRAMES
            and is_script_file(frame.f_code.co_filename)
        ):
            frames.append(
                Frame(
                    file=frame.f_code.co_filename,
                    line=frame.f_lineno or 1,
                    function=frame.f_code.co_name,
                )
            )
            frame = frame.f_back
    finally:
        del frame  # break the reference cycle through the frame's locals
    return Source(frames=tuple(frames)) if frames else None


@cache
def is_script_file(filename: str) -> bool:
    """Whether the code in a file belongs to a fold script.

    Generated code, such as ``<string>``, and code in origamy, the standard library or
    an installed package belong to no script.
    """
    if filename.startswith("<"):
        return False
    path = Path(filename).resolve()
    return not any(path.is_relative_to(root) for root in _library_roots())


@cache
def _library_roots() -> tuple[Path, ...]:
    paths = sysconfig.get_paths()
    roots = {_PACKAGE}
    roots.update(
        Path(paths[key]).resolve()
        for key in ("stdlib", "platstdlib", "purelib", "platlib")
    )
    return tuple(sorted(roots))


def relative_path(file: str, root: Path) -> str:
    """``file`` relative to ``root`` when it lies inside it, else unchanged."""
    try:
        return Path(file).resolve().relative_to(root.resolve()).as_posix()
    except ValueError:
        return file
