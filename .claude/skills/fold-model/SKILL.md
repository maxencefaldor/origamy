---
name: fold-model
description: Fold an origami model with origamy. Use when asked to fold, design or diagram origami, or to fix an origamy fold script.
---

# Fold a model with origamy

## The goal

The goal is a paper model that looks like its subject, such as a crane or a dragon, from every side.
Every action must be one the paper takes, and the finished model must read as its subject in 3D.
A build with no refusals is necessary, and far from enough: judge the model by how it looks.

Folds and angles both make the look.
Folds decide which flaps exist, how long and how thick they are, and where they hinge.
Angles decide how far each crease opens in space, so flaps spread, stand or curve.
Work in any order, and change either at any time.

## What you have

- **A fold script:** `models/<name>.py`, defining `fold()` that returns an `origamy.Paper`.
- **Builds:** `uv run origamy build models/<name>.py`, or the MCP tool `fold_model`.
  A build reports each refusal and warning with its rule, faces and script line.
  The build draws the crease pattern, the folded paper and an x-ray in `<name>.png`, and every step in `<name>.steps.png`.
  A build also writes `<name>.html`, a 3D viewer that replays every step.
- **Views:** `uv run origamy view models/<name>.py --azimuth A --elevation E`, or the MCP tool `view_model`.
  A view draws one step in 3D from one direction, as often and from as many sides as you ask.
  The azimuth runs round the paper: 0 in front of its bottom edge, 90 at its right, 180 behind, 270 at its left.
  The elevation runs from 90, straight above, through 0, level with the table, to below it.
  `--step` picks a step, and `--xray` sees through every layer.
- **Rules:** `uv run origamy explain` gives every rule's exact condition.

## Writing the script

```python
from origamy import Line, Paper


def fold() -> Paper:
    paper = Paper.square()
    spine = Line.through(("1/2", 0), ("1/2", 1))
    paper.valley(spine, flap=("3/4", "1/2"))
    paper.shape(spine, flap=("1/4", "1/2"), angle=60, layers=1)
    return paper
```

- Coordinates are where the paper lies flat now, seen from above: x to the right, y up.
  Unfolded, the paper covers (0, 0) to (width, height), its front up.
- Numbers are exact: an int, a `Fraction`, or text such as `"1/2"` or `"√2 - 1"`.
  Floats are refused.
- Lines come from `Line.through(p, q)` and the constructions `fold_onto`, `bisect`, `perpendicular`, `meet` and `midpoint`.
- A fold takes the paper at its flap point and everything joined to it on that side of the line.
  `layers=n` takes the top n layers there instead of all the paper on that side.
- The folds are `valley`, `mountain`, `unfold`, `precrease`, `collapse`, `reverse`, `petal`, `squash` and `turn_over`.
- `crease(segments)` creases the open paper along a whole crease pattern, and one `collapse` then folds it.
  `CreasePattern.read(path)` reads a pattern from a FOLD or ORIPA file, and `Paper.polygon(pattern.outline)` makes its paper.
- The angles are set by `shape`, `flex` and `tilt`.
  `shape(line, flap, angle, layers)` turns a flap about its hinge by an angle, up when positive.
  `flex({crease: angle})` folds creases to angles, from 0 open to 180 pressed flat, and the creases around each vertex follow together.
  `tilt(line, flap, angle)` turns the whole model in space.
  `settle({crease: angle})` turns creases toward angles and lets the simulator bend the paper: fans, opened cups, puffed forms.
  A settle step ends the build.
- Within one script, a fold after `shape`, `flex` or `tilt` is refused (`not-flat`).
- The queries are `where` (unfolded point to where it lies now), `origin` (back), `creases` (crease pattern segments along a line), `layers` (layers at a point) and `thickest` (the most layers at one point).

## When the paper refuses

- `layer-crossing` on an inside reverse fold: the tip would pass between layers that folds already wrap.
  The same line with `outside=True` wraps the tip round them instead, and often folds.
- `hinge-bent` on a shape step: its line crosses faces that earlier shaping turned apart.
  Draw the line so it crosses only the flap you turn, starting from that flap's own edge.
- `flap-off-paper`: the flap point lies on a crease or an edge, or where no paper lies now.
  Move it a little into the face you mean, and remember that a turn over mirrors every coordinate.
- `flap-on-line` on a tilt: the point names no side of the line.
  Name a point of the body that no later fold moves, not the tip of a flap.

## When the pictures disagree

A build with no refusals can still be wrong: check every model from several sides.
A tilt raises the side of its flap point, so a point that a fold moved across the line turns the model upside down.
Points that stick up near the head read as ears; turn or droop them until the silhouette reads as the subject.
