# Copyright 2026 Maxence Faldor
"""origamy-kernel: the exact kernel of origamy, in Rust.

The kernel decides with exact numbers in towers of square roots, as `origamy.field`
defines them. A `State` holds a crease pattern, each face's place, and the stacking it
found. It creases open paper, finds the creases along lines and the face of largest
area, folds the faces, overlays their footprints, collapses creases, solves the
stacking and names the constraints a stacking breaks. `explain` finds the exact numbers
that explain floats best, and `read_corners` the exact corners of a crease pattern.
"""

from origamy_kernel._core import State, explain, read_corners

__all__ = ["State", "explain", "read_corners"]
