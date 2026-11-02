# Copyright 2026 Maxence Faldor
import copy
import math
import re
from collections.abc import Mapping
from pathlib import Path
from typing import Any

import pytest
from hypothesis import given
from hypothesis import strategies as st
from pydantic import BaseModel, ValidationError
from pydantic_settings import CliApp, SettingsConfigDict

from origamy_sim import (
    Air,
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


@given(
    st.fixed_dictionaries(
        dict.fromkeys(
            ("stiffness", "yield_moment", "hardening", "aging_moment", "aging_rate"),
            NUMBERS,
        )
    )
)
def test_pydantic_accepts_a_crease_exactly_when_the_core_does(
    crease: dict[str, float],
) -> None:
    sheet = Sheet(SQUARE)
    try:
        sheet.set_crease((0, 2), Crease.model_construct(None, **crease))
    except ValueError:
        core = False
    else:
        core = True
    assert validates(Crease, crease) == core


def settings_data() -> st.SearchStrategy[dict[str, Any]]:
    return st.fixed_dictionaries(
        {
            "gravity": st.tuples(NUMBERS, NUMBERS, NUMBERS),
            "time_step": NUMBERS,
            "solver": st.fixed_dictionaries(
                {
                    "iteration_limit": st.integers(-2, 2),
                    "lags": st.integers(-2, 2),
                    **dict.fromkeys(
                        (
                            "tolerance",
                            "shortest_step",
                            "trust_radius",
                            "sufficient_decrease",
                            "backtracking",
                            "shortest_update",
                        ),
                        NUMBERS,
                    ),
                }
            ),
            "contact": st.fixed_dictionaries(
                dict.fromkeys(
                    ("gap", "stiffness", "parallel", "friction", "stick"), NUMBERS
                )
            ),
            "table": st.none()
            | st.fixed_dictionaries(dict.fromkeys(("height", "friction"), NUMBERS)),
            "air": st.none()
            | st.fixed_dictionaries(dict.fromkeys(("density", "drag"), NUMBERS)),
        }
    )


@given(settings_data())
def test_pydantic_accepts_settings_exactly_when_the_core_does(
    data: dict[str, Any],
) -> None:
    table, air = data["table"], data["air"]
    unchecked = Settings.model_construct(
        gravity=data["gravity"],
        time_step=data["time_step"],
        solver=Solver.model_construct(None, **data["solver"]),
        contact=Contact.model_construct(None, **data["contact"]),
        table=None if table is None else Table.model_construct(None, **table),
        air=None if air is None else Air.model_construct(None, **air),
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
    assert Crease.kami() == Crease()


def test_a_band_spreads_a_crease_over_lines_in_series() -> None:
    crease = Crease(hardening=1e-3)
    assert crease.band(1) == crease
    assert crease.band(4) == Crease(
        stiffness=4 * crease.stiffness,
        hardening=4e-3,
        aging_rate=crease.aging_rate / 4,
    )
    with pytest.raises(ValueError, match="lines is 0, but it must be at least 1"):
        crease.band(0)


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


def test_a_replaced_model_is_validated() -> None:
    settings = copy.replace(Settings(), time_step=0.01, table=Table(height=-0.1))
    assert settings.time_step == 0.01
    assert settings.table == Table(height=-0.1)
    assert settings.solver == Solver()
    assert copy.replace(Material(), thickness=1e-4).stiffness == Stiffness()
    with pytest.raises(ValidationError, match="time_step"):
        copy.replace(settings, time_step=-1.0)


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
