# Copyright 2026 Maxence Faldor
"""origamy-sim: a physical simulator of paper sheets.

The simulator models how paper bends, creases, stacks, slides and holds, in SI units.
A `Mesh` gives a sheet its flat rest shape.
"""

from origamy_sim._core import Mesh

__all__ = ["Mesh"]
