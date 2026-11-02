# Copyright 2026 Maxence Faldor
"""origamy-sim: a physical simulator of paper sheets.

The simulator models how paper bends, creases, stacks, slides and holds, in SI units.
A `Mesh` gives a sheet its flat shape, and a `Sheet` adds its `Material`, its creases
and its rest angles. A `Simulation` steps sheets through time under gravity, holds and
actuated creases, with its `Settings`. Every parameter is a validated pydantic model,
and a `Config` reads them from YAML and the command line.
"""

from origamy_sim._core import (
    Energy,
    Mesh,
    Report,
    Sheet,
    Simulation,
    SingularError,
)
from origamy_sim.parameters import (
    Config,
    Contact,
    Crease,
    Material,
    Settings,
    Solver,
    Stiffness,
    StrainLimit,
    Table,
)

__all__ = [
    "Config",
    "Contact",
    "Crease",
    "Energy",
    "Material",
    "Mesh",
    "Report",
    "Settings",
    "Sheet",
    "Simulation",
    "SingularError",
    "Solver",
    "Stiffness",
    "StrainLimit",
    "Table",
]
