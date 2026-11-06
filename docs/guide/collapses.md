# Collapses

A collapse folds, unfolds or reverses many creases at once, and the paper follows.

## Collapse

The action `collapse(valleys=..., mountains=..., flats=..., fixed=...)` sets creases already in the paper.
Each crease is a whole line of the crease pattern, or a segment of one given by its two ends.
A line or segment takes every crease of the paper along it.
The paper refuses a collapse that names a line or segment with no crease along it (`no-crease`).
Valleys and mountains read as on the crease pattern, from the paper's front.
Creases in `flats` lie flat afterward, and every other crease keeps its state.

The top face at the point `fixed` stays where it lies, and the largest face stays when the script names no `fixed` point.
Each other face lies where the creases between it and the fixed face put it.
The [preliminary base](https://github.com/maxencefaldor/origamy/blob/main/examples/preliminary_base.py) precreases four lines through the center, then collapses them in one step:

```python title="examples/preliminary_base.py"
# Copyright 2026 Maxence Faldor
"""The preliminary base: four creases through the center, collapsed at once."""

from origamy import Line, Paper


def fold() -> Paper:
    """Precrease both diagonals and both middle lines, then collapse."""
    paper = Paper.square()
    rising = Line.through((0, 0), (1, 1))
    falling = Line.through((1, 0), (0, 1))
    upright = Line.through(("1/2", 0), ("1/2", 1))
    level = Line.through((0, "1/2"), (1, "1/2"))
    paper.precrease(rising, flap=("3/4", "1/4"))
    paper.precrease(falling, flap=("7/8", "5/8"), mountain=True)
    paper.precrease(upright, flap=("3/4", "1/8"))
    paper.precrease(level, flap=("1/8", "3/4"))

    # Four valleys and two mountains meet at the center; the rising diagonal stays
    # flat, inside the flaps.
    paper.collapse(valleys=[upright, level], mountains=[falling], fixed=("7/8", "5/8"))
    return paper
```

## Segments

A segment names part of a line, so a line can be a valley on one side of a vertex and a mountain on the other.
The fold script below collapses a square into a quarter in one step, then unfolds the second fold:

```python title="models/quarter.py"
from origamy import Line, Paper


def fold() -> Paper:
    paper = Paper.square()
    upright = Line.through(("1/2", 0), ("1/2", 1))
    level = Line.through((0, "1/2"), (1, "1/2"))
    paper.precrease(upright, flap=("3/4", "1/4"))
    paper.precrease(level, flap=("1/4", "3/4"))
    paper.collapse(
        valleys=[upright, ((0, "1/2"), ("1/2", "1/2"))],
        mountains=[(("1/2", "1/2"), (1, "1/2"))],
        fixed=("1/4", "1/4"),
    )
    top = Line.through((0, "1/2"), ("1/2", "1/2"))
    paper.collapse(flats=paper.creases(top))
    return paper
```

Collapses name creases on the crease pattern, while the paper lies folded.
Two queries bridge the two.
The query `paper.creases(line)` gives the crease pattern segments lying along a line now.
The query `paper.origin(point)` gives where a point of the top layer lies on the unfolded paper.
The last collapse unfolds every crease along the quarter's top edge, so the paper ends folded in half.

## Vertices and stacking

A collapse checks every vertex inside the paper before it folds:

| Rule | Condition at each vertex with folds |
| --- | --- |
| `mountain-valley-count` | The numbers of mountain and valley folds differ by exactly two (Maekawa's theorem) |
| `unbalanced-angles` | The alternate angles between folds each sum to 180° (Kawasaki-Justin theorem) |

Two crossing diagonals, both valleys, put four valleys and no mountain at the center:

```text
maekawa.py: 2 steps folded, then step 3 refused
error[mountain-valley-count] step 3, collapse 2 valleys, 0 mountains and 0 flat creases: models/maekawa.py:10 in fold(): 0 mountains and 4 valleys meet at (1/2, 1/2)
  faces: 0, 1, 2, 3
1 layer at most, at (5/6, 1/2)
wrote models/maekawa.fold, models/maekawa.png, models/maekawa.steps.png, models/maekawa.html
```

The layer solver then finds how the faces stack, or the paper refuses the collapse (see [Feedback](feedback.md#the-layer-solver)).
A collapse moves the paper as little through itself as it can: each face that turns about a fold lands beside the face across it.
Faces that overlapped keep their order, reversed where they moved together and the move turned them over.

## Named moves

A named move computes its creases and collapses in one step, judged like any collapse:

| Move | Does |
| --- | --- |
| `reverse(line, flap, layers=None, outside=False)` | Turns the tip of a folded flap inside out between its two halves; `outside=True` wraps it around them |
| `petal(line, flap)` | Lifts the top layer over a line while its sides fold in along precreased creases |
| `squash(line, flap)` | Swings the top flap over a line and presses it flat, each of its layers folding in half along precreased creases |

### Reverse folds

The tip of a reverse fold is the paper at the flap point on its side of the line, taken as a fold takes it.
The tip opens into an upper and a lower half, each one piece of paper, joined only by folds.
The folds between the halves are the tip's spine.
Each half turns over the line as one, and the upper half stays above the lower.
An inside reverse fold tucks each half's tip between the halves, and an outside reverse fold wraps it around them.

The fold script below folds a square into a triangle, then reverse folds its right tip inside, between the two layers:

```python title="models/reverse.py"
from origamy import Line, Paper


def fold() -> Paper:
    paper = Paper.square()
    paper.valley(Line.through((1, 0), (0, 1)), flap=("3/4", "3/4"))
    paper.reverse(Line.through(("1/2", 0), ("1/2", 1)), flap=("3/4", "1/8"))
    return paper
```

The spine, the triangle's long edge, turns inside out: its stretch beyond x = 1/2 becomes a mountain on the crease pattern.

![The triangle with its tip reverse folded inside: the crease pattern, the folded paper and an x-ray](../images/reverse.png)

A tip can split in several ways, and the reverse fold takes the split that divides the layers at the flap point most evenly.
The paper refuses a tip that opens in no way (`no-spine`), or in several equally even ways (`several-spines`).
A reverse fold of flat paper finds no spine:

```text
flat_reverse.py: 0 steps folded, then step 1 refused
error[no-spine] step 1, inside reverse fold along x = 1/2, the paper at (3/4, 1/8): models/flat_reverse.py:6 in fold(): the paper to reverse at (3/4, 1/8) opens into two halves in no way
  faces: 0
1 layer at most, at (1/2, 1/2)
wrote models/flat_reverse.fold, models/flat_reverse.png, models/flat_reverse.steps.png, models/flat_reverse.html
```

The [crane](https://github.com/maxencefaldor/origamy/blob/main/examples/crane.py) reverse folds its neck, tail and head, each taking every layer at its flap point:

```python
paper.reverse(line, flap=flap, layers=paper.layers(flap))
```

### Petal folds

A petal fold lifts the top layer at the flap point over the line, with every face it continues into across flat creases.
The layer's creases along the line fold toward the viewer.
The creases that run from where the line meets the layer's folded sides into the lifted region fold the same way.
Their partners, in the layer joined across each side, fold too, and the sides between them open flat.
The script must precrease these creases first.

The paper refuses a petal fold with no flat crease of the layer along its line (`no-crease`), or with no folded side to open (`no-side`).
The [bird base](https://github.com/maxencefaldor/origamy/blob/main/examples/bird_base.py) precreases the kite creases and the top on each side of a preliminary base, then petal folds:

```python
paper.precrease(kite_left, flap=inside_left, layers=1)
paper.precrease(kite_right, flap=inside_right, layers=1)
# Fold the top down over the kites and back, then lift the open corner up
# along that crease, the sides folding in.
top = Line.through(top_left, top_right)
middle = meet(top, Line.through(center, corner))
near_center = midpoint(midpoint(center, top_left), midpoint(center, middle))
paper.precrease(top, flap=near_center)
paper.petal(top, flap=midpoint(corner, midpoint(top_left, middle)))
```

![Every step of the bird base, each next fold dashed](../images/bird-base-steps.png)

### Squash folds

A squash fold swings the top flap at the flap point over the line and presses it flat.
The flap's top layer is the top face at the flap point, with every face it continues into across flat creases on its side of the line.
Its creases along the line are its hinge, and its folded side, off the line, joins it to the layer beneath.
The top layer swings over the hinge, and the side opens flat, so the layer beneath spreads out on the near side.
The creases that run from where the side meets the line into both layers fold each layer in half.
The script must precrease these creases first.

The paper refuses a squash fold with no crease of the top layer along its line, or with no precreased crease in a layer (`no-crease`).
It refuses a top layer with no folded side off the line (`no-side`).
A frog base folds each flap's side to the middle line and back, then squashes the flap there:

```python
center, tip, side = (paper.where(p) for p in (("1/2", "1/2"), (0, 0), ("1/2", 0)))
flap = midpoint(side, midpoint(center, tip))
paper.precrease(bisect(side, center, tip), flap=flap, layers=2)
paper.squash(Line.through(center, tip), flap=flap)
```

## Crease patterns

The action `crease(creases)` creases the open paper along segments of the crease pattern and folds none of them.
Each crease is a segment, given by its two ends.
A collapse then folds every crease at once, as a folder collapses a precreased pattern.

The paper refuses creasing while it lies folded (`paper-folded`), or a crease with an end off the paper (`crease-off-paper`).
Every crease must end on the paper's edge or on another crease, and every face the creases leave must be convex.
The paper refuses creases that break either condition (`loose-crease`).

The [frog base](https://github.com/maxencefaldor/origamy/blob/main/examples/frog_base.py) creases its pattern on the open square, then collapses it:

```python
paper = Paper.square()
paper.crease([*valleys, *mountains, *flats])
paper.collapse(valleys=valleys, mountains=mountains)
```

![The frog base collapsed from its crease pattern: the crease pattern, the folded paper and an x-ray](../images/frog-base.png)

The [Miura-ori](https://github.com/maxencefaldor/origamy/blob/main/examples/miura_ori.py) creases and collapses 112 segments the same way, then opens its columns in a [flex](shaping.md).

### Files

`CreasePattern.read(path)` reads a crease pattern from a FOLD file or an ORIPA `.cp` file:

```python
from origamy import CreasePattern, Paper


def fold() -> Paper:
    pattern = CreasePattern.read("models/frog_base.cp")
    paper = Paper(pattern.width, pattern.height)
    paper.crease(pattern.creases)
    paper.collapse(valleys=pattern.valleys, mountains=pattern.mountains)
    return paper
```

The paper's lower left corner goes to (0, 0), and its longer side to length 1.
A file gives its coordinates as floats, and reading finds the exact number each one stands for.

| Candidate | Size |
| --- | --- |
| A fraction p / d | Its denominator d, up to a million |
| (p + q√r) / d, for r one of 2, 3, 5 and 6 | Its largest integer s, up to 10,000 |

Lattice reduction finds the integers of each candidate.
A candidate's fit is its error times d², or times s³.
A random float's best candidate has a fit near 1, so a far smaller fit shows the number the float stands for.
The candidate with the smallest fit wins, within `tolerance` of the float.

Reading takes the coordinates with a fit under `origamy.pattern.SURE`, 10⁻⁸, first.
A corner without both then lies where two lines of creases cross, each through corners already exact.
A line runs straight along creases through two exact corners, or leaves an exact corner at a slope that snaps.
Coordinates with a fit under `origamy.pattern.FIT`, 10⁻⁴, come next, then crossings again, until every corner is exact.
Patterns on grids, on 22.5° and 30° angles, with golden ratios, and on crossings of their creases all read exactly.
Reading refuses a corner left over, with a `ValueError`.

A FOLD file's unassigned creases read as flat creases, and an assignment origamy cannot fold is refused.
An ORIPA file's aid lines are ignored.
