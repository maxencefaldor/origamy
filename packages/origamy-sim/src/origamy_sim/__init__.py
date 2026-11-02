# Copyright 2026 Maxence Faldor
"""origamy-sim: a physical simulator of paper sheets.

The simulator models how paper bends, creases, stacks, slides and holds, in SI units.
A `Mesh` gives a sheet its flat shape, and a `Sheet` adds its `Material` and creases.
A `Simulation` steps sheets through time under gravity, holds and actuated creases.
"""

from origamy_sim._core import (
    Crease,
    Energy,
    Material,
    Mesh,
    Report,
    Settings,
    Sheet,
    Simulation,
    SingularError,
    Stiffness,
)

__all__ = [
    "Crease",
    "Energy",
    "Material",
    "Mesh",
    "Report",
    "Settings",
    "Sheet",
    "Simulation",
    "SingularError",
    "Stiffness",
]
