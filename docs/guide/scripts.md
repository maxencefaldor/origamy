# Fold scripts

A fold script describes a model as ordinary Python, so loops, functions and data can fold it.

## The script

A fold script is a Python file that defines `fold()`, which takes no argument and returns a `Paper`.
Each call on the paper asks for one action, and each action the paper takes becomes a step.
Python supplies what a model needs to repeat itself: loops for the sides of a model, and functions for steps that recur.
The [bird base](https://github.com/maxencefaldor/origamy/blob/main/examples/bird_base.py) folds its front and its back in one loop, turning the paper over after each side:

```python
for corners in (
    ((0, 0), (0, "1/2"), ("1/2", 0)),
    ((1, 1), ("1/2", 1), (1, "1/2")),
):
    center = paper.where(("1/2", "1/2"))
    corner, left, right = (paper.where(point) for point in corners)
    # Precrease the kite creases and the top, then lift the open corner.
    paper.petal(top, flap=midpoint(corner, midpoint(top_left, middle)))
    paper.turn_over()
```

The [crane](https://github.com/maxencefaldor/origamy/blob/main/examples/crane.py) calls a function `bird_base(paper)` first, then narrows the legs, folds the neck, head and wings, and spreads the wings.

## Running apart

The build runs the script in a process of its own, with a time limit of 300 seconds that `--timeout` changes.
A script that hangs or crashes therefore leaves the command line and the MCP server running.
The build discards whatever the script prints, so a script reports only through its paper.
The script's folder comes first on the import path, so a script can import modules that sit beside it.
A fold script still runs as ordinary Python with your permissions, so build only scripts you trust.

An exception stops the build, and the report names the exception and the script line that raised it:

```text
failed.py: the script failed: failed.py:6 in fold(): TypeError: numbers are exact: got the float 0.75
```

A script without `fold()`, or whose `fold()` returns anything but a `Paper`, fails the same way with a `ScriptError`.
A script that runs past its time limit stops, and the command exits with status 2.

## Script lines

Each action records the script lines that asked for it: the line that called the action, then up to three lines that called that one.
Every fact in the report names these lines, and every step in the pictures and the viewer names the first of them.
A path is relative to the working directory when the script lies inside it.
The fold script below folds two hems in a function of its own:

```python title="models/hem.py"
from origamy import Line, Paper


def hem(paper: Paper, edge: Line, flap: tuple[str, str]) -> None:
    paper.valley(edge, flap=flap)


def fold() -> Paper:
    paper = Paper.square()
    hem(paper, Line.through(("1/8", 0), ("1/8", 1)), flap=("1/16", "1/2"))
    hem(paper, Line.through(("7/8", 0), ("7/8", 1)), flap=("15/16", "1/2"))
    return paper
```

Nothing on a fresh square marks x = 1/8, so each fold draws a warning.
Each warning names the line in `hem()`, then the line in `fold()` that called it:

```text
hem.py: 2 steps folded
warn[unreferenced-fold] step 1, valley fold along x = 1/8, the paper at (1/16, 1/2): models/hem.py:5 in hem(), called from models/hem.py:10 in fold(): no point or line on the paper locates the fold line x = 1/8
warn[unreferenced-fold] step 2, valley fold along x = 7/8, the paper at (15/16, 1/2): models/hem.py:5 in hem(), called from models/hem.py:11 in fold(): no point or line on the paper locates the fold line x = 7/8
wrote models/hem.fold, models/hem.png, models/hem.steps.png, models/hem.html
```

## What a build writes

A build of `models/name.py` writes four files beside the script, even when the paper refuses an action:

| File | Holds |
| --- | --- |
| `name.fold` | The [FOLD file](exports.md): the last crease pattern, and the folded paper at every step |
| `name.png` | The paper at the end, or just before a refused action: crease pattern, folded paper and x-ray |
| `name.steps.png` | Every step, each with the next fold line dashed |
| `name.html` | The [3D viewer](pictures.md#the-3d-viewer), which replays the fold step by step |

The option `--no-pictures` writes the FOLD file alone.
The option `--output-format json` prints the report as JSON for programs.
The [Pictures](pictures.md) page shows what each picture holds.
