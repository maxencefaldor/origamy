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

type NonNegative = Annotated[float, Field(ge=0, allow_inf_nan=False)]
"""A finite number, zero or positive."""

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
    plate around it. Past its yield moment, the crease yields further, and its rest
    angle follows the fold. Released, the crease springs back by its moment over its
    stiffness. Below its yield moment, the crease ages: its rest angle creeps toward the
    fold, faster the larger its moment. The defaults are kami's.
    """

    stiffness: Positive = 9.0e-3
    """The moment per length that turns the crease a radian from rest, in N·m/m/rad.

    The default is the bending stiffness of kami over 200 thicknesses, the length that
    creased sheets measure.
    """

    yield_moment: Annotated[float, Field(gt=0)] = 5.65e-3
    """The moment per length past which the crease yields, in N·m/m.

    The yield moment is infinite for a crease that never yields. Paper pressed flat
    springs back to 36°, so the default is the stiffness times that angle.
    """

    hardening: NonNegative = 0.0
    """The rise of the yield moment per radian the crease flows through, in N·m/m/rad.

    Pressing harder closes a fold of paper no further, so the default is zero.
    """

    aging_moment: Positive = 3.89e-4
    """The moment per length that multiplies the rate of aging by e, in N·m/m.

    A crease held at an angle loses this moment for each factor e its hold time grows
    by. Paper held folded sets 5.7° deeper for each tenfold of the hold, which gives the
    default.
    """

    aging_rate: NonNegative = 4.25e-8
    """The rate of aging, in radians per second; zero for a crease that never ages.

    The rest angle creeps by this rate times the hyperbolic sine of the moment over the
    aging moment. Held at its yield moment, a crease of the defaults begins to relax
    after about a second.
    """

    @classmethod
    def kami(cls) -> Self:
        """A crease in kami.

        Returns:
            The crease of the stiffness, yield, hardening and aging measured for paper.
        """
        return cls()

    def band(self, lines: int) -> Self:
        """The crease of each of `lines` parallel lines spreading it into a crease band.

        The lines turn in series, each by a `lines`-th of the band's turn, under one
        moment. Each line takes `lines` times the stiffness and hardening, and a
        `lines`-th of the aging rate, at the same yield and aging moments. Each line
        rests at a `lines`-th of the rest angle. The band then turns, yields and ages
        as this crease does.

        Args:
            lines: How many lines of edges the band spreads over, at least 1.

        Returns:
            The crease of each line.

        Raises:
            ValueError: When `lines` is less than 1.
        """
        if lines < 1:
            message = f"lines is {lines}, but it must be at least 1"
            raise ValueError(message)
        return self.model_validate(
            {
                **self.model_dump(),
                "stiffness": lines * self.stiffness,
                "hardening": lines * self.hardening,
                "aging_rate": self.aging_rate / lines,
            }
        )


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

    hold_stiffness: Positive = 100.0
    """The stiffness of the spring that pulls a moving held vertex to its target.

    The stiffness is a multiple of the stiffest sheet's stretching stiffness, Young's
    modulus times thickness. The default pulls a hundred times harder than paper
    stretches.
    """

    hold_rounds: Annotated[int, Field(gt=0)] = 10
    """The most rounds of the augmented Lagrangian that carries moving held vertices.

    Each round minimizes the step, then raises the force on every hold short of its
    target.
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

    friction: NonNegative = 0.4
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

    friction: NonNegative = 0.4
    """Coulomb's coefficient of friction between paper and the table.

    The default takes a table as rough as paper.
    """


class Body(_Parameters):
    """A rigid body that touches paper: a sphere or a capsule of a radius.

    The body is a sphere round one point, or a capsule round the segment between two.
    Paper keeps its half thickness plus the body's radius away from the body.
    """

    radius: Positive
    """The radius of the sphere, or of the capsule round its axis, in meters."""

    friction: NonNegative = 0.4
    """Coulomb's coefficient of friction between the body and paper.

    The default takes the body as rough as paper. Skin on paper measures between 0.26
    and 0.68, from Vilhena, Afonso and Ramalho, 2023, so the default suits a fingertip
    too.
    """

    stiffness: Annotated[float, Field(gt=0)] = math.inf
    """The stiffness of the hand that moves the body, in newtons per meter.

    The default, infinite, carries the body exactly along its path. A finite stiffness
    is a spring between the body and its path, shared among its points, as a finger pad
    yields when it presses. The body then presses with the stiffness times how far its
    path runs past where the paper stops it. The body presses on while the hand stays.
    A finger pad yields at about 700 N/m under a few newtons, from Dzidek et al., 2017.
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
