# Exports

Every build writes a FOLD file, the format other origami tools read.

## FOLD files

A build of `models/name.py` writes `models/name.fold`, a [FOLD 1.2](https://edemaine.github.io/fold/doc/spec.html) file in JSON.
The file holds the last crease pattern at the top level, then the folded paper after every step in `file_frames`.
A refused build writes a frame for each step the paper took.
The fold script below precreases a diagonal, then folds the bottom edge onto it along a 22.5° line:

```python title="models/kite.py"
from origamy import Line, Paper, bisect


def fold() -> Paper:
    paper = Paper.square()
    paper.precrease(Line.through((0, 0), (1, 1)), flap=("3/4", "1/4"))
    paper.valley(bisect((1, 0), (0, 0), (1, 1)), flap=("7/8", "1/16"))
    return paper
```

The file `models/kite.fold` holds these keys:

| Key | Holds |
| --- | --- |
| `file_spec`, `file_creator`, `file_classes` | `1.2`, `"origamy"` and `["singleModel"]` |
| `frame_title` | The script's name, such as `"kite"`, and in each frame the step's description |
| `frame_classes`, `frame_attributes` | `["creasePattern"]` at the top level and `["foldedForm"]` in each frame, `["2D"]` or `["3D"]` once shaped |
| `vertices_coords` | Each vertex as floats: on the crease pattern at the top level, and where it lies in each frame |
| `edges_vertices`, `edges_assignment` | Each edge's two vertices, and `M`, `V` or `F` for a crease, or `B` for the paper's edge |
| `edges_foldAngle` | Each edge's fold angle in degrees, valleys positive: 180 for a flat fold, less once shaped |
| `faces_vertices` | Each face's vertices, counterclockwise on the crease pattern |
| `faceOrders` | In each frame, which face lies above wherever two faces overlap |
| `origamy:vertices_exact` | Each vertex of the crease pattern as exact data |
| `origamy:tower` | The radicands of the build's tower of square roots, level 1 first |

Assignments read from the paper's front, as on the crease pattern.
A frame after a shape step or a tilt gives each vertex three coordinates, where the paper lies in space (see [Shaping](shaping.md)).
The top level gives the fold angles of the last frame.
Each entry of `faceOrders` follows the FOLD convention.
The entry `[f, g, 1]` puts face `f` on the side that the normal of face `g` points to, and `[f, g, -1]` on the other side.

## Exact coordinates

FOLD stores coordinates as floats, so the file carries each exact coordinate beside its float, as data.
A rational number is its text, such as `"1/2"`.
A number a + b√d at level k of the tower is `[k, a, b]`, with `a` and `b` one level down.
The radicand d of each level sits in `origamy:tower`.
The kite's vertex 4, where the fold line meets the right edge, is (1, √2 - 1):

| Key | Vertex 4 |
| --- | --- |
| `vertices_coords` | `[1.0, 0.41421356237309515]` |
| `origamy:vertices_exact` | `["1", [1, "-1", "1"]]` |
| `origamy:tower` | `["2"]` |

The function `Number.from_data` reads the data back, inside the tower the file records:

```pycon
>>> import json
>>> from origamy import Number
>>> from origamy.field import tower
>>> with open("models/kite.fold", encoding="utf-8") as file:
...     fold = json.load(file)
>>> with tower(Number.from_data(radicand) for radicand in fold["origamy:tower"]):
...     x, y = (Number.from_data(c) for c in fold["origamy:vertices_exact"][4])
...     print(x, y, y * y + 2 * y == 1)
1 -1 + √2 True
```

The floats serve tools that read FOLD, and the exact data keeps every vertex exactly as the build left it.
