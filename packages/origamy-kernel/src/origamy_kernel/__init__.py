# Copyright 2026 Maxence Faldor
"""origamy-kernel: the exact kernel of origamy, in Rust.

The kernel decides with exact numbers in towers of square roots, as `origamy.field`
defines them. A `State` holds a crease pattern and each face's place: it finds the
creases along lines and the face of largest area, folds the faces, overlays their
footprints, collapses creases, solves the stacking and names the constraints a
stacking breaks. `crease` divides open paper along a crease pattern's segments, and
`relations` finds the small integer relations that explain a float.
"""

from origamy_kernel._core import State, crease, relations

__all__ = ["State", "crease", "relations"]
