# Coordinates

Fold scripts locate points and fold lines on the paper as it lies now, in exact numbers.

## The paper's frame

The paper starts unfolded and front up, from (0, 0) to (width, height).
The x axis points right and the y axis points up, as seen from above.
The call `Paper.square()` makes a square from (0, 0) to (1, 1), and `Paper(2, 1)` makes a rectangle twice as wide as it is tall.
The call `Paper.polygon(corners)` makes paper of any convex shape, its corners counterclockwise, touching both axes.
A hexagon is `Paper.polygon([(1, 0), (3, 0), (4, "√3"), (3, "2√3"), (1, "2√3"), (0, "√3")])`.
The paper's front is vermilion and its back is cream, and the arguments `front` and `back` take other colors as hex codes.

Every coordinate names a place where the paper lies now, after every step so far.
A fold moves paper, so a point of the unfolded square can lie elsewhere once folded.
Two queries convert between the crease pattern and the folded paper:

| Query | Returns |
| --- | --- |
| `paper.where(point)` | Where a point of the unfolded paper lies now, such as a corner |
| `paper.origin(point)` | Where a point of the top layer lies on the unfolded paper; `layer=2` takes the layer below |

The [cup example](https://github.com/maxencefaldor/origamy/blob/main/examples/cup.py) finds its corners with `where` after each fold:

```python
corner, left, right = (paper.where(p) for p in (a, b, c))
rim = Line.through(left, right)
```

## Points and lines

A point is a `Point` or a pair of exact numbers, such as `("1/2", 0)`.
The call `Line.through(p, q)` makes the line through two distinct points.
The call `Line.at(point, direction)` makes the line through a point along a direction.
Reports print a line as its equation, scaled so that its first nonzero coefficient is 1:

```pycon
>>> from origamy import Line
>>> print(Line.through((0, 0), (1, 1)))
x - y = 0
>>> print(Line.through((1, 0), (0, 1)))
x + y = 1
>>> print(Line.at((0, 0), (1, 2)))
x - 1/2y = 0
```

Two descriptions of one line compare equal, since they scale to the same equation.

## Exact numbers

Every coordinate is a `Number`, an exact real number built from the rationals by square roots.
A script writes a number as an `int`, a `Fraction`, or text:

| Written | Number |
| --- | --- |
| `1`, `Fraction(1, 3)` | 1, 1/3 |
| `"1/2"` | 1/2 |
| `"√2 - 1"` | -1 + √2 |
| `"3√2/4"` | 3√2/4 |
| `"1 - √2/2"` | 1 - √2/2 |

Text sums terms such as `1`, `-3/4` and `3√2/4`, each with an integer under its root.
A float stops the script with a `TypeError`, since its value is rarely the number written.
A `bool` stops it the same way.

The numbers live in a tower of fields.
Level 0 holds the rationals, and each square root that no level holds adds a level above the others.
Each number has one form, so two numbers are equal exactly when their forms are.
Float estimates with error bounds decide most signs fast, and exact arithmetic decides the rest.

```pycon
>>> from origamy import Number
>>> half = Number.of("1/2")
>>> root = Number.of("√2")
>>> print(half * root)
√2/2
>>> (half * root) ** 2 == half
True
>>> print((2 + root).sqrt())
√(2 + √2)
>>> from origamy.field import radicands
>>> print(*radicands(), sep=", ")
2, 2 + √2
```

The square root of 2 + √2 lies in no level, so it adds a second level above √2.
Each build grows its own tower from the rationals, and the build's [FOLD file](exports.md) records it.

## Constructions

A person finds a fold line on real paper by lining up points and lines already there.
The constructions build the same lines exactly, after the Huzita-Justin axioms, so a script says how it finds each fold:

| Construction | Returns | Axiom |
| --- | --- | --- |
| `Line.through(p, q)` | The line through two points | 1 |
| `fold_onto(p, q)` | The fold line that puts point `p` onto point `q` | 2 |
| `bisect(a, vertex, b)` | The fold line that halves the angle from `a` through `vertex` to `b` | 3 |
| `perpendicular(line, through)` | The fold line through a point that folds a line onto itself | 4 |
| `meet(first, second)` | The point where two lines cross | |
| `midpoint(p, q)` | The point halfway between two points | |
| `Point.polar(radius, degrees)` | The point at a radius from the origin, turned by a multiple of 3° | |

```pycon
>>> from origamy import Line, bisect, fold_onto, meet, midpoint, perpendicular
>>> print(fold_onto((0, 0), (1, 0)))
x = 1/2
>>> print(bisect((1, 0), (0, 0), (0, 1)))
x - y = 0
>>> print(bisect((1, 0), (0, 0), (1, 1)))
x - (1 + √2)y = 0
>>> print(perpendicular(Line.through((0, 0), (1, 0)), (1, 1)))
x = 1
>>> print(meet(Line.through((0, 0), (1, 1)), Line.through((0, 1), (1, 0))))
(1/2, 1/2)
>>> print(midpoint((0, 0), (1, "1/2")))
(1/2, 1/4)
```

The bisector of a 45° angle makes a 22.5° line, and its equation takes √2.
Regular polygons need points at fixed angles, and `Point.polar` builds them exactly.
Every multiple of 3° is 18° less 15°, so its cosine and sine take square roots alone.
`Point.turned` turns a vector by such an angle.

```pycon
>>> from origamy import Point
>>> Point.polar(1, 60) == Point.of(("1/2", "√3/2"))
True
>>> Point.of((1, 0)).turned(36) == Point.polar(1, 36)
True
```

The first five Huzita-Justin axioms build lines with arithmetic and square roots alone, so every line they build stays exact.
The cup example brings a corner to the opposite side along the bisector of its angle:

```python
paper.precrease(bisect(a, b, c), flap=("1/2", "1/32"))
to_side = meet(Line.through(a, c), bisect(a, b, c))
paper.valley(fold_onto(b, to_side), flap=("15/16", "1/32"))
```

## Lines the paper locates

A fold line that no point or line of the paper locates draws the warning `unreferenced-fold`.
The check asks whether the first four axioms build the line from the folded paper's corners, creases and edges.
A fold along x = 2/5 on a fresh square draws the warning, since nothing on the square marks 2/5:

```text
warned.py: 1 step folded
warn[unreferenced-fold] step 1, valley fold along x = 2/5, the paper at (3/4, 1/2): warned.py:6 in fold(): no point or line on the paper locates the fold line x = 2/5
wrote warned.fold, warned.png, warned.steps.png, warned.html
```

The warning states a fact and lets the fold happen.
The [Feedback](feedback.md) page covers warnings and refusals.
