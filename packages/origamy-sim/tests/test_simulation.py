# Copyright 2026 Maxence Faldor
import math
import re

import numpy as np
import pytest

from origamy_sim import (
    Crease,
    Material,
    Mesh,
    Settings,
    Sheet,
    Simulation,
    SingularError,
    Stiffness,
)

SIDE = 0.1
SQUARE = Mesh([[0, 0], [SIDE, 0], [SIDE, SIDE], [0, SIDE]], [[0, 1, 2], [0, 2, 3]])
STILL = Settings(gravity=(0, 0, 0), time_step=math.inf)


def strip(columns: int, rows: int, length: float, width: float) -> Mesh:
    """A rectangle of cells, each split along its rising diagonal."""
    x, y = np.meshgrid(
        np.linspace(0, length, columns + 1), np.linspace(0, width, rows + 1)
    )
    corner = np.arange(x.size).reshape(x.shape)
    low_left, low_right = corner[:-1, :-1].ravel(), corner[:-1, 1:].ravel()
    up_left, up_right = corner[1:, :-1].ravel(), corner[1:, 1:].ravel()
    triangles = np.concatenate(
        [
            np.column_stack([low_left, low_right, up_right]),
            np.column_stack([low_left, up_right, up_left]),
        ]
    )
    return Mesh(np.column_stack([x.ravel(), y.ravel()]), triangles)


def test_a_crease_folds_a_square_to_its_rest_angle() -> None:
    sheet = Sheet(SQUARE)
    sheet.set_crease((2, 0), Crease(stiffness=0.01, rest_angle=1.0))
    simulation = Simulation([sheet], STILL)
    for vertex in (0, 1, 2):
        simulation.hold(0, vertex)
    report = simulation.step()
    assert report.converged
    # Updates are refitted to their rotations, so a large turn takes few iterations.
    assert report.iterations <= 10
    assert report.greatest_strain < 1e-9
    assert simulation.fold_angles(0) == pytest.approx([1.0], abs=1e-6)
    # The free corner turned up about the diagonal, toward the sheet's front.
    assert simulation.positions(0)[3, 2] > 0
    assert simulation.held(0).tolist() == [True, True, True, False]


def test_a_held_strip_sags_under_gravity() -> None:
    mesh = strip(16, 2, 0.06, 0.01)
    simulation = Simulation([Sheet(mesh)], Settings(time_step=math.inf))
    for vertex in np.flatnonzero(mesh.rest[:, 0] < 0.005):
        simulation.hold(0, int(vertex))
    report = simulation.step()
    assert report.converged
    assert report.energy.gravity < 0 < report.energy.bending
    tip = simulation.positions(0)[mesh.rest[:, 0] > 0.0599]
    assert (tip[:, 2] < -0.001).all()


def test_a_free_sheet_falls_as_implicit_euler_says() -> None:
    dt, steps = 0.01, 5
    simulation = Simulation([Sheet(SQUARE)], Settings(time_step=dt))
    for _ in range(steps):
        report = simulation.step()
        assert report.converged
    # Implicit Euler gains g dt of speed each step, then moves at the new speed.
    assert simulation.velocities(0)[:, 2] == pytest.approx(-9.81 * dt * steps)
    drop = 9.81 * dt * dt * steps * (steps + 1) / 2
    assert simulation.positions(0)[:, 2] == pytest.approx(-drop)
    assert report.time == pytest.approx(dt * steps)
    assert report.energy.kinetic == pytest.approx(
        0.5 * 0.06 * SIDE * SIDE * (9.81 * dt * steps) ** 2
    )


def test_the_same_inputs_give_the_same_result_bit_for_bit() -> None:
    def run() -> np.ndarray:
        simulation = Simulation(
            [Sheet(strip(8, 2, 0.04, 0.01))], Settings(time_step=0.01)
        )
        simulation.hold(0, 0)
        simulation.hold(0, 9)
        for _ in range(3):
            simulation.step()
        return simulation.positions(0)

    assert np.array_equal(run(), run())


def test_a_sheet_free_of_every_force_and_hold_has_no_unique_rest() -> None:
    simulation = Simulation([Sheet(SQUARE)], STILL)
    with pytest.raises(SingularError, match="singular"):
        simulation.step()


def test_positions_must_fit_the_sheet() -> None:
    simulation = Simulation([Sheet(SQUARE)])
    with pytest.raises(
        ValueError, match=re.escape("the sheet has 4 vertices, but 3 positions")
    ):
        simulation.set_positions(0, np.zeros((3, 3)))
    stretched = simulation.positions(0) * 1.01
    with pytest.raises(ValueError, match="past the strain cap"):
        simulation.set_positions(0, stretched)
    with pytest.raises(IndexError, match="no sheet 1"):
        simulation.positions(1)
    with pytest.raises(IndexError, match="no vertex 4"):
        simulation.hold(0, 4)


def test_every_parameter_is_an_input_and_checked() -> None:
    kami = Material.kami()
    assert Material() == kami
    thick = kami.replace(thickness=1e-4)
    assert thick.thickness == 1e-4
    assert thick.stiffness == kami.stiffness
    with pytest.raises(ValueError, match="thickness is -1, but it must be positive"):
        kami.replace(thickness=-1)
    with pytest.raises(ValueError, match="Poisson's ratio"):
        Stiffness(along=4e9, across=1e9, poisson=2.5, shear=1e9)
    with pytest.raises(ValueError, match="rest angle"):
        Crease(stiffness=0.01, rest_angle=4.0)
    assert Settings().replace(time_step=0.1).time_step == 0.1
    with pytest.raises(ValueError, match="time step"):
        Settings(time_step=0)


def test_creases_lie_along_interior_edges() -> None:
    sheet = Sheet(SQUARE)
    with pytest.raises(ValueError, match="no interior edge from vertex 0 to vertex 1"):
        sheet.set_crease((0, 1), Crease(stiffness=0.01, rest_angle=0.5))
    simulation = Simulation([sheet])
    simulation.set_crease(0, (0, 2), Crease(stiffness=0.01, rest_angle=0.5))
    assert simulation.sheet(0).crease((0, 2)) == Crease(stiffness=0.01, rest_angle=0.5)
    assert sheet.crease((0, 2)) is None
