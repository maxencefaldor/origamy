# Copyright 2026 Maxence Faldor
"""The parameters of a simulation: materials, creases and settings, validated.

Each model is frozen and refuses fields it does not know. Field constraints check each
value, and model validators check the conditions that tie values together. The core
checks every value again, so a model built without validation never reaches a
simulation. Every quantity is in SI units, with a default where paper has a typical
value.
"""

import math
from typing import Annotated, Self, override

from pydantic import BaseModel, ConfigDict, Field, model_validator
from pydantic_settings import (
    BaseSettings,
    PydanticBaseSettingsSource,
    SettingsConfigDict,
    YamlConfigSettingsSource,
)

type Positive = Annotated[float, Field(gt=0, allow_inf_nan=False)]
"""A positive, finite number."""

type Finite = Annotated[float, Field(allow_inf_nan=False)]
"""A finite number."""

type Fraction = Annotated[float, Field(gt=0, lt=1)]
"""A number strictly between 0 and 1."""


class _Parameters(BaseModel):
    model_config = ConfigDict(
        frozen=True, extra="forbid", use_attribute_docstrings=True
    )

    @override
    def __replace__(self, /, **changes: object) -> Self:
        """A copy with some fields changed, validated as a new model is.

        `copy.replace` calls this method.
        """
        return self.model_validate({**self.model_dump(), **changes})


class Stiffness(_Parameters):
    """The stiffness of a sheet in its plane, orthotropic about its grain.

    The stiffness holds under plane stress. The same stiffness resists stretching,
    through the thickness, and bending, through the thickness cubed over twelve. The
    defaults are kami's: isotropic, with a Young's modulus of 4 GPa and a Poisson's
    ratio of 0.3.
    """

    along: Positive = 4e9
    """Young's modulus along the grain, in pascals."""

    across: Positive = 4e9
    """Young's modulus across the grain, in pascals."""

    poisson: Finite = 0.3
    """Poisson's ratio: how much a stretch along the grain narrows the sheet across."""

    shear: Positive = 4e9 / (2 * (1 + 0.3))
    """The shear modulus in the plane, in pascals."""

    grain: Finite = 0.0
    """The grain's direction in the flat sheet, in radians from the x axis toward y."""

    @classmethod
    def isotropic(cls, young: float, poisson: float) -> Self:
        """The stiffness of a material alike in every direction.

        Args:
            young: Young's modulus, in pascals.
            poisson: Poisson's ratio.

        Returns:
            The stiffness, with the shear modulus that isotropy gives.
        """
        return cls(
            along=young,
            across=young,
            poisson=poisson,
            shear=young / (2 * (1 + poisson)),
        )

    @model_validator(mode="after")
    def _check_positive_definite(self) -> Self:
        limit = math.sqrt(self.along / self.across)
        if not abs(self.poisson) < limit:
            message = (
                f"Poisson's ratio is {self.poisson}, but it must be smaller in size"
                f" than {limit}, the square root of the ratio of Young's moduli"
            )
            raise ValueError(message)
        return self


class StrainLimit(_Parameters):
    """The barrier that caps a sheet's strain.

    Paper barely stretches, so the barrier keeps every triangle's Green strain below a
    cap. The barrier pushes back harder the closer the strain comes to the cap.
    """

    cap: Positive = 0.005
    """The greatest norm of any triangle's Green strain: 0.005 is half a percent."""

    onset: Fraction = 0.8
    """The fraction of the cap at which the barrier starts to push back."""

    stiffness: Positive = 1e5
    """The barrier's energy per volume, in pascals: the larger, the harder it pushes."""


class Material(_Parameters):
    """What a sheet is made of: its thickness, weight and stiffness.

    The defaults are kami's, the thin paper most origami uses.
    """

    thickness: Positive = 70e-6
    """The sheet's thickness, in meters."""

    mass_per_area: Positive = 0.060
    """The sheet's mass per area, in kilograms per square meter."""

    stiffness: Stiffness = Stiffness()
    """The stiffness of the sheet in its plane."""

    strain_limit: StrainLimit = StrainLimit()
    """The barrier that caps the sheet's strain."""

    @classmethod
    def kami(cls) -> Self:
        """Kami, the thin paper most origami uses.

        Returns:
            Kami: 70 µm thick and 60 g/m², with a Young's modulus of 4 GPa, a Poisson's
            ratio of 0.3, and strain capped at half a percent.
        """
        return cls()


class Crease(_Parameters):
    """A sharp crease: a dihedral where the paper has yielded.

    The paper bends there alone, about the dihedral's rest angle, instead of with the
    plate around it.
    """

    stiffness: Positive
    """The moment per length that turns the crease a radian from rest, in N·m/m/rad."""


class Solver(_Parameters):
    """How the solver minimizes each time step: Newton's method with a line search."""

    tolerance: Positive = 1e-8
    """The largest vertex move of a Newton update that ends a time step, in meters."""

    iteration_limit: Annotated[int, Field(gt=0)] = 200
    """The most Newton iterations a time step may take, in each lag of friction."""

    lags: Annotated[int, Field(gt=0)] = 3
    """The most times a time step lags friction.

    Friction lags first from the step's start, then from each solution the step reaches,
    until the solution stays where it is.
    """

    shortest_step: Positive = 1e-5
    """The shortest time step a time step that fails may halve to, in seconds.

    A time step whose solve fails retries as two time steps of half its length.
    """

    trust_radius: Positive = 0.25
    """The longest vertex move of a time step's first Newton update, in meters.

    A longer update shrinks to this trust radius. An update shrunk and then taken whole
    doubles the radius for the next update of the time step.
    """

    sufficient_decrease: Fraction = 1e-4
    """The fraction of the predicted decrease of energy a line search requires."""

    backtracking: Fraction = 0.5
    """The factor by which a line search shortens an update it refuses."""

    shortest_update: Fraction = 1e-12
    """The shortest fraction of a Newton update a line search tries."""


class Contact(_Parameters):
    """How contact pushes paper apart.

    A barrier on each gap starts a little beyond the paper's thickness and grows without
    bound as the gap closes.
    """

    gap: Positive = 2e-6
    """The gap beyond a paper thickness at which the barrier starts, in meters."""

    stiffness: Positive = 1e-8
    """The barrier's energy scale, in joules: the larger, the less contact yields."""

    parallel: Fraction = 1e-3
    """The squared sine of the angle under which two edges' barrier fades as they turn
    parallel, at their rest lengths: 0.001 is about 1.8°."""

    friction: Annotated[float, Field(ge=0, allow_inf_nan=False)] = 0.4
    """Coulomb's coefficient of friction between two pieces of paper.

    The default lies inside the measured ranges for copy paper and sticky notes: 0.17
    to 0.44 sliding, and 0.33 to 0.61 at rest.
    """

    stick: Positive = 1e-7
    """The slip within one time step under which friction holds a contact, in meters.

    Below it, the friction force grows smoothly with the slip, so a held contact creeps
    by less than this distance a step.
    """


class Table(_Parameters):
    """A table: the horizontal plane paper cannot pass below."""

    height: Finite = 0.0
    """The height of the table's top, in meters."""

    friction: Annotated[float, Field(ge=0, allow_inf_nan=False)] = 0.4
    """Coulomb's coefficient of friction between paper and the table.

    The default takes a table as rough as paper.
    """


class Air(_Parameters):
    """The air paper moves through, which resists paper moving across its surface.

    Drag pushes each patch of paper against its velocity along the patch's normal. The
    force is half the air's density, times the drag coefficient, the patch's area and
    that velocity squared.
    """

    density: Positive = 1.204
    """The air's density, in kilograms per cubic meter.

    The default is dry air's, at 20 °C at sea level.
    """

    drag: Positive = 1.17
    """The drag coefficient of paper moving across its surface.

    The default is a square plate's, from Hoerner's Fluid-Dynamic Drag.
    """


class Settings(_Parameters):
    """The settings of a simulation: its world, its contact and its solver."""

    gravity: tuple[Finite, Finite, Finite] = (0.0, 0.0, -9.81)
    """The acceleration of gravity, in meters per second squared."""

    time_step: Annotated[float, Field(gt=0)] = 1 / 60
    """The length of a time step, in seconds; infinite for quasi-static steps."""

    solver: Solver = Solver()
    """How the solver minimizes each time step."""

    contact: Contact = Contact()
    """How contact pushes paper apart."""

    table: Table | None = None
    """The table paper rests on, if any."""

    air: Air | None = Air()
    """The air paper moves through, if any."""


class Config(BaseSettings):
    """The configuration of a simulation: its settings and the material of its paper.

    Keyword arguments rank first, then the YAML file that `yaml_file` in the model
    configuration names, then the defaults. A source may give part of a model, and the
    defaults fill the rest. The command line, read through `pydantic_settings.CliApp`,
    ranks above every other source. Applications subclass the configuration to name
    their YAML file and add fields of their own.
    """

    model_config = SettingsConfigDict(
        frozen=True,
        extra="forbid",
        use_attribute_docstrings=True,
        nested_model_default_partial_update=True,
        cli_kebab_case=True,
        cli_implicit_flags=True,
        cli_avoid_json=True,
    )

    settings: Settings = Settings()
    """The settings of the simulation."""

    material: Material = Material()
    """The material of the paper."""

    @classmethod
    @override
    def settings_customise_sources(
        cls,
        settings_cls: type[BaseSettings],
        init_settings: PydanticBaseSettingsSource,
        env_settings: PydanticBaseSettingsSource,
        dotenv_settings: PydanticBaseSettingsSource,
        file_secret_settings: PydanticBaseSettingsSource,
    ) -> tuple[PydanticBaseSettingsSource, ...]:
        """Read keyword arguments, then the YAML file."""
        return (init_settings, YamlConfigSettingsSource(settings_cls))
