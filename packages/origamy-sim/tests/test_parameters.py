# Copyright 2026 Maxence Faldor
import math
import re
from collections.abc import Mapping
from pathlib import Path

import pytest
from hypothesis import given
from hypothesis import strategies as st
from pydantic import BaseModel, ValidationError
from pydantic_settings import CliApp, SettingsConfigDict

from origamy_sim import (
    Config,
    Contact,
    Crease,
    Material,
    Mesh,
    Settings,
    Sheet,
    Simulation,
    Solver,
    Stiffness,
    StrainLimit,
    Table,
)

SQUARE = Mesh([[0, 0], [0.1, 0], [0.1, 0.1], [0, 0.1]], [[0, 1, 2], [0, 2, 3]])

# Values at and around every boundary a parameter has, and any float at all.
NUMBERS = st.sampled_from(
    [0.0, -0.0, 1e-300, 1e-12, 0.3, 0.5, 1.0, 1.5, 4e9, -2.0, math.inf, -math.inf]
) | st.floats(allow_nan=True, allow_infinity=True)


def validates(model: type[BaseModel], data: Mapping[str, object]) -> bool:
    """Whether pydantic accepts the data as the model."""
    try:
        model.model_validate(data)
    except ValidationError:
        return False
    return True


def stiffness_data() -> st.SearchStrategy[dict[str, object]]:
    return st.fixed_dictionaries(
        dict.fromkeys(("along", "across", "poisson", "shear", "grain"), NUMBERS)
    )


def limit_data() -> st.SearchStrategy[dict[str, object]]:
    return st.fixed_dictionaries(dict.fromkeys(("cap", "onset", "stiffness"), NUMBERS))


@given(stiffness_data(), limit_data(), NUMBERS, NUMBERS)
def test_pydantic_accepts_a_material_exactly_when_the_core_does(
    stiffness: dict[str, float],
    limit: dict[str, float],
    thickness: float,
    mass_per_area: float,
) -> None:
    data = {
        "thickness": thickness,
        "mass_per_area": mass_per_area,
        "stiffness": stiffness,
        "strain_limit": limit,
    }
    unchecked = Material.model_construct(
        thickness=thickness,
        mass_per_area=mass_per_area,
        stiffness=Stiffness.model_construct(None, **stiffness),
        strain_limit=StrainLimit.model_construct(None, **limit),
    )
    try:
        Sheet(SQUARE, unchecked)
    except ValueError:
        core = False
    else:
        core = True
    assert validates(Material, data) == core


@given(NUMBERS)
def test_pydantic_accepts_a_crease_exactly_when_the_core_does(stiffness: float) -> None:
    sheet = Sheet(SQUARE)
    try:
        sheet.set_crease((0, 2), Crease.model_construct(stiffness=stiffness))
    except ValueError:
        core = False
    else:
        core = True
    assert validates(Crease, {"stiffness": stiffness}) == core


@given(
    st.tuples(NUMBERS, NUMBERS, NUMBERS),
    NUMBERS,
    st.fixed_dictionaries(
        {
            "tolerance": NUMBERS,
            "iteration_limit": st.integers(-2, 2),
            "sufficient_decrease": NUMBERS,
            "backtracking": NUMBERS,
            "shortest_update": NUMBERS,
        }
    ),
    st.fixed_dictionaries({"gap": NUMBERS, "stiffness": NUMBERS}),
    st.none() | st.fixed_dictionaries({"height": NUMBERS}),
)
def test_pydantic_accepts_settings_exactly_when_the_core_does(
    gravity: tuple[float, float, float],
    time_step: float,
    solver: dict[str, float],
    contact: dict[str, float],
    table: dict[str, float] | None,
) -> None:
    data = {
        "gravity": gravity,
        "time_step": time_step,
        "solver": solver,
        "contact": contact,
        "table": table,
    }
    unchecked = Settings.model_construct(
        gravity=gravity,
        time_step=time_step,
        solver=Solver.model_construct(None, **solver),
        contact=Contact.model_construct(None, **contact),
        table=None if table is None else Table.model_construct(None, **table),
    )
    try:
        Simulation([], unchecked)
    except ValueError, OverflowError:
        core = False
    else:
        core = True
    assert validates(Settings, data) == core


@given(stiffness_data(), limit_data(), NUMBERS)
def test_parameters_cross_into_the_core_and_back_unchanged(
    stiffness: dict[str, float], limit: dict[str, float], thickness: float
) -> None:
    data = {"thickness": thickness, "stiffness": stiffness, "strain_limit": limit}
    if validates(Material, data):
        material = Material.model_validate(data)
        assert Sheet(SQUARE, material).material == material
    settings = Settings(
        gravity=(0, 0, -1.62),
        time_step=math.inf,
        solver=Solver(backtracking=0.25),
        table=Table(height=-0.01),
    )
    assert Simulation([], settings).settings == settings


def test_the_defaults_are_the_cores() -> None:
    assert Sheet(SQUARE).material == Material() == Material.kami()
    assert Simulation([]).settings == Settings()
    assert Stiffness.isotropic(4e9, 0.3) == Stiffness()


def test_a_refused_parameter_is_named() -> None:
    with pytest.raises(
        ValidationError, match=r"thickness\n  Input should be greater than 0"
    ):
        Material(thickness=-1)
    with pytest.raises(
        ValidationError, match=re.escape("Poisson's ratio is 2.5, but it must be")
    ):
        Stiffness(along=4e9, across=1e9, poisson=2.5, shear=1e9)
    with pytest.raises(ValidationError, match="Input should be a finite number"):
        Table(height=-math.inf)
    with pytest.raises(ValidationError, match="Extra inputs are not permitted"):
        Crease.model_validate({"stiffness": 0.01, "rest_angle": 1.0})
    with pytest.raises(ValidationError, match="Instance is frozen"):
        Settings().time_step = 1  # ty: ignore[invalid-assignment]


def test_the_core_takes_parameters_as_models_alone() -> None:
    with pytest.raises(TypeError, match="'dict' object is not a Material"):
        Sheet(SQUARE, {"thickness": 1e-4})  # ty: ignore[invalid-argument-type]
    unchecked = Material.model_construct(
        thickness=-1.0,
        mass_per_area=0.06,
        stiffness=Stiffness(),
        strain_limit=StrainLimit(),
    )
    with pytest.raises(ValueError, match="thickness is -1, but it must be positive"):
        Sheet(SQUARE, unchecked)


def test_a_config_reads_a_yaml_file_under_the_command_line(tmp_path: Path) -> None:
    path = tmp_path / "simulation.yaml"
    path.write_text(
        """\
settings:
  time_step: .inf
  table:
    height: -0.01
material:
  thickness: 1.0e-4
  strain_limit:
    cap: 0.01
"""
    )

    class FromFile(Config):
        model_config = SettingsConfigDict(yaml_file=path)

    config = CliApp.run(
        FromFile,
        cli_args=[
            "--material.thickness",
            "2e-4",
            "--settings.solver.iteration-limit",
            "50",
        ],
    )
    assert config.settings.time_step == math.inf
    assert config.settings.table == Table(height=-0.01)
    assert config.settings.solver == Solver(iteration_limit=50)
    # The command line ranks above the file, and the defaults fill what neither gives.
    assert config.material == Material(
        thickness=2e-4, strain_limit=StrainLimit(cap=0.01)
    )
    assert FromFile(material=Material.kami()).material == Material.kami()
    assert Config() == Config(settings=Settings(), material=Material())


def test_a_config_refuses_a_file_value_outside_its_values(tmp_path: Path) -> None:
    path = tmp_path / "simulation.yaml"
    path.write_text("settings:\n  contact:\n    gap: -1.0e-6\n")

    class FromFile(Config):
        model_config = SettingsConfigDict(yaml_file=path)

    with pytest.raises(
        ValidationError,
        match=re.escape("settings.contact.gap\n  Input should be greater than 0"),
    ):
        FromFile()
