# origamy

A compiler for origami, built for AI agents.

![The bird base folded with origamy: its crease pattern, the folded paper, and an x-ray of its layers](docs/images/bird-base.png)

An AI agent writes an origami model as a Python fold script, and origamy folds it step by step on a simulated sheet.
Each action either happens or is refused with the facts of why, at the script line that asked for it.
Every build draws the crease pattern, the folded paper and an x-ray of its layers, and exports a FOLD file.

The tool is a simulator and a compiler at once.
No list of legal moves exists: the agent proposes any fold, and origamy says exactly what happens.
Coordinates are exact numbers of the form a + b√2, so every check is exact, down to the 22.5° angles of the bird base.

## Install

Python 3.14 or newer runs origamy, and a Chromium draws its pictures.

```sh
uv init --bare --pin-python --python 3.14 my-models  # a project for your fold scripts
cd my-models
uv add git+https://github.com/maxencefaldor/origamy#subdirectory=packages/origamy
uv run playwright install chromium                   # the browser origamy draws in, once per machine
```

## Fold a model

A fold script defines `fold()`, which folds a `Paper` and returns it.
Coordinates are where the paper lies now, seen from above, with the unfolded square from (0, 0) to (1, 1).
Each fold takes a line and a flap point: the paper at that point, and everything joined to it on its side of the line, turns over the line.

```python
from origamy import Line, Paper


def fold() -> Paper:
    paper = Paper.square()
    paper.valley(Line.through(("1/2", 0), ("1/2", 1)), flap=("3/4", "1/2"))
    paper.valley(Line.through((0, "1/2"), ("1/2", "1/2")), flap=("1/4", "3/4"))
    return paper
```

Build it with `uv run origamy build square.py`:

```text
square.py: 2 steps folded
wrote square.fold, square.png, square.steps.png, square.html
```

The picture `square.png` shows the paper at the end, and `square.steps.png` shows every step with the next fold line dashed, as diagrams do.
The [cup example](examples/cup.py) folds a traditional paper cup with exact constructions.

## Viewer

Every build also writes a standalone page, such as `square.html`, that replays the fold in 3D.
It shows the crease pattern beside the folded paper, animates each step, and lets people orbit, view from above or below, and see through the layers.
Hovering a face lights it in both views, and each step names its script line and its facts.

![The viewer replaying the bird base, its crease pattern beside the folded paper in 3D](docs/images/viewer.png)

## Collapse

A collapse folds, unfolds or reverses many creases at once, along creases already in the paper.
Creases read as on the crease pattern: whole lines, or segments given by their two ends, with mountains and valleys seen from the paper's front.
The [preliminary base](examples/preliminary_base.py) precreases four lines through the center, then collapses them in one step:

```python
paper.collapse(valleys=[upright, level], mountains=[falling])
```

Two queries find the creases to name: `paper.creases(line)` gives the creases lying along a line now, as crease pattern segments, and `paper.origin(point)` gives where a point of the top layer lies on the unfolded paper.
Before folding, a collapse checks every vertex with Maekawa's and Kawasaki's theorems, and the layer solver checks every stacking.

## Named moves

Named moves compute their creases and collapse in one step, judged like any collapse.

| Move | What it does |
| --- | --- |
| `paper.reverse(line, flap)` | Turns the tip of a folded flap inside out, between its layers; `outside=True` wraps it around them |
| `paper.petal(line, flap)` | Lifts the top layer over a line while its sides fold in along precreased creases |

The [bird base](examples/bird_base.py) collapses a preliminary base, then petal folds each side:

![Every step of the bird base, each next fold dashed](docs/images/bird-base-steps.png)

## Connect an agent

The package `origamy-mcp` serves origamy over the Model Context Protocol, so any agent harness can fold.
From a clone of this repository, Claude Code adds it with one command:

```sh
claude mcp add origamy -- uv run --directory /path/to/origamy origamy-mcp
```

The tool `fold_model` runs a fold script and answers with the report and both pictures, and `explain_rule` gives a rule's exact condition.
The skill in [.claude/skills/fold-model](.claude/skills/fold-model/SKILL.md) teaches the loop, the coordinates and the actions, and no designs.

## Feedback is facts

A refused action stops the build at its line, and the picture shows the paper just before it, with the fold line and the faces involved.

```text
broken.py: 1 step folded, then step 2 refused
warn[unreferenced-fold] step 2, valley: broken.py:7 in fold(): no point or line on the paper locates the fold line x = 2
error[no-hinge] step 2, valley fold along x = 2, the paper at (1/4, 1/2): broken.py:7 in fold(): the paper to move meets no other paper along x = 2
  faces: 0, 1
```

The command `uv run origamy explain` lists every rule with its level and exact condition.

| Rule | Level | Fact |
| --- | --- | --- |
| `flap-off-paper`, `flap-on-line` | error | The flap point names no paper to move |
| `too-few-layers` | error | Fewer layers lie at the flap point than the action takes |
| `no-hinge` | error | The paper to move is joined to no other paper along the fold line |
| `already-folded`, `not-folded` | error | A fold runs along a crease already folded, or an unfold along flat paper |
| `no-crease` | error | A collapse names a line with no crease of the paper along it |
| `no-spine` | error | A reverse fold meets a layer with no fold to turn inside out |
| `mountain-valley-count` | error | Mountains and valleys at a vertex do not differ by two |
| `unbalanced-angles` | error | The angles between folds at a vertex do not alternate to 180° |
| `layer-crossing` | error | No stacking of the faces keeps paper from passing through paper |
| `layer-order-unknown` | error | The layer solver stopped before finding a stacking |
| `unreferenced-fold` | warn | No point or line already on the paper locates the fold line |

## How it works

The state of the paper is its crease pattern, where each face lies, and which face is above wherever two overlap.
A fold reflects the faces it moves across its line, exactly, and a collapse places each face by the reflections across the folds between it and a fixed face.
The layer solver then finds how the faces stack, after the constraints of [Flat-Folder](https://erikdemaine.org/papers/FlatFolder_OSME2024/paper.pdf): taco-taco, taco-tortilla, tortilla-tortilla and transitivity.
The paper that stays keeps its order, the paper that moves reverses its own, and a valley fold lands it on top or a mountain fold underneath.
When no stacking exists, origamy names the constraints that conflict.

## Status

The project is in its first phase: exact folds, unfolds, precreases, collapses, reverse and petal folds, with the layer solver, pictures, a 3D viewer, an MCP server and FOLD export.
Squash and sink folds, and 3D shaping in the viewer, come next.

## License

Apache-2.0.
