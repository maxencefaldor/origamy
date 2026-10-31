---
name: fold-model
description: Fold an origami model with origamy. Use when asked to fold, design or diagram origami, or to fix an origamy fold script.
---

# Fold a model with origamy

Work in a loop: write a fold script, build it, read the report and look at the pictures, change the script, and build again.
Stop when the paper takes every action and the pictures show what you meant.
The report alone cannot tell you the last part.

## Loop

1. Write `models/<name>.py`, defining `fold()` that returns an `origamy.Paper`.
1. Run `uv run origamy build models/<name>.py`, or call the MCP tool `fold_model`.
1. Read the report.
   A refused action stops the build: its line names the rule, the faces involved and the script line.
1. Look at `<name>.png`: the crease pattern, the folded paper and an x-ray of the layers, at the end or just before the refused action, with its fold line and faces marked.
   Look at `<name>.steps.png` for every step, each next fold line dashed.
1. Change the script and build again.

## Writing the script

```python
from origamy import Line, Paper


def fold() -> Paper:
    paper = Paper.square()
    paper.valley(Line.through(("1/2", 0), ("1/2", 1)), flap=("3/4", "1/2"))
    return paper
```

- Coordinates are where the paper lies now, seen from above: x to the right, y up.
  Unfolded, the paper covers (0, 0) to (width, height), its front up.
- Numbers are exact: an int, a `Fraction`, or text such as `"1/2"` or `"√2 - 1"`.
  Floats are refused.
- Lines come from `Line.through(p, q)` and the constructions `fold_onto`, `bisect`, `perpendicular`, `meet` and `midpoint`.
- A fold takes the paper at its flap point and everything joined to it on that side of the line.
  `layers=n` takes the top n layers there instead of all the paper on that side.
- The actions are `valley`, `mountain`, `unfold`, `precrease`, `collapse`, `reverse`, `petal` and `turn_over`.
- Shaping comes last: `shape(line, flap, angle=90, layers=n)` opens a fold part of the way, and `tilt(line, flap, angle=90)` holds the whole model up.
  Shaped paper takes no more folds, so finish folding first.
- The queries are `where` (unfolded point to where it lies now), `origin` (back), `creases` (crease pattern segments along a line) and `layers` (layers at a point).
- `uv run origamy explain` gives every rule's exact condition.
