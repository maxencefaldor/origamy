# Copyright 2026 Maxence Faldor
"""origamy-kernel: the exact kernel of origamy, in Rust.

The kernel decides with exact numbers in towers of square roots, as `origamy.field`
defines them. `crease` divides open paper along a crease pattern's segments. `along`
finds the creases along lines and segments, and `largest` the face of largest area.
`collapse` folds every crease at once. `stack` finds which face lies above which
wherever folded faces overlap, and `violations` names the constraints a stacking
breaks.
"""

from origamy_kernel._core import along, collapse, crease, largest, stack, violations

__all__ = ["along", "collapse", "crease", "largest", "stack", "violations"]
