# Copyright 2026 Maxence Faldor
"""Run one fold script in a process of its own.

The service starts ``python -P -m origamy.runner SCRIPT RESULT`` for every build, so a
script's prints, crashes and endless loops stay out of the command line and of any
server. The runner writes a `BuildResult` as JSON to ``RESULT``.
"""

import runpy
import sys
import traceback
from collections.abc import Generator
from contextlib import contextmanager
from pathlib import Path

from origamy.paper import Paper, RefusedError
from origamy.provenance import Frame, is_script_file, relative_path
from origamy.result import BuildResult, Failure

ENTRY = "fold"
"""The function a fold script defines: it takes no argument and returns a `Paper`."""


def run(script: Path) -> BuildResult:
    """Run a fold script in this process and return what it produced."""
    root = Path.cwd()
    try:
        with _importable(script.parent):
            namespace = runpy.run_path(str(script), run_name="__origamy__")
            entry = namespace.get(ENTRY)
            if not callable(entry):
                message = f"{script} must define {ENTRY}(), returning a Paper"
                return BuildResult(
                    failure=Failure(error="ScriptError", message=message)
                )
            paper = entry()
    except RefusedError as error:
        return BuildResult.of(error.paper, root, error.refused)
    except SystemExit as error:
        return BuildResult(failure=_failure(error, root, f"exited with {error.code!r}"))
    except BaseException as error:  # noqa: BLE001 - a fold script may raise anything
        return BuildResult(failure=_failure(error, root, str(error)))
    if not isinstance(paper, Paper):
        message = f"{ENTRY}() returned {type(paper).__name__}, not a Paper"
        return BuildResult(failure=Failure(error="ScriptError", message=message))
    return BuildResult.of(paper, root)


@contextmanager
def _importable(folder: Path) -> Generator[None]:
    """Put a folder first on the import path while a script runs."""
    sys.path.insert(0, str(folder.resolve()))
    try:
        yield
    finally:
        sys.path.remove(str(folder.resolve()))


def _failure(error: BaseException, root: Path, message: str) -> Failure:
    frames = [
        Frame(
            file=relative_path(frame.filename, root),
            line=frame.lineno or 1,
            function=frame.name,
        )
        for frame in reversed(traceback.extract_tb(error.__traceback__))
        if is_script_file(frame.filename)
    ]
    return Failure(error=type(error).__name__, message=message, frames=tuple(frames))


def main() -> None:
    """Run the script named on the command line and write its result."""
    script, result = Path(sys.argv[1]), Path(sys.argv[2])
    result.write_text(run(script).model_dump_json(), encoding="utf-8")


if __name__ == "__main__":
    main()
