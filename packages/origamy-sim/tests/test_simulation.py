# Copyright 2026 Maxence Faldor
import math
import re

import numpy as np
import pytest

from origamy_sim import (
    Body,
    Contact,
    Crease,
    Material,
    Mesh,
    Settings,
    Sheet,
    Simulation,
    SingularError,
    Solver,
    Table,
)

SIDE = 0.1
SQUARE = Mesh([[0, 0], [SIDE, 0], [SIDE, SIDE], [0, SIDE]], [[0, 1, 2], [0, 2, 3]])
STILL = Settings(gravity=(0, 0, 0), time_step=math.inf)


def test_a_crease_folds_a_square_to_its_rest_angle() -> None:
    sheet = Sheet(SQUARE)
    sheet.set_crease((2, 0), Crease(stiffness=0.01))
    sheet.set_rest_angle((2, 0), 1.0)
    simulation = Simulation([sheet], STILL)
    simulation.hold(0, [0, 1, 2])
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
    mesh = Mesh.grid(16, 2, 0.06, 0.01)
    simulation = Simulation([Sheet(mesh)], Settings(time_step=math.inf))
    simulation.hold(0, np.flatnonzero(mesh.rest[:, 0] < 0.005))
    report = simulation.step()
    assert report.converged
    assert report.energy.gravity < 0 < report.energy.bending
    tip = simulation.positions(0)[mesh.rest[:, 0] > 0.0599]
    assert (tip[:, 2] < -0.001).all()


def test_time_steps_converge_as_newton_does() -> None:
    mesh = Mesh.grid(24, 4, 0.06, 0.01)
    simulation = Simulation([Sheet(mesh)], Settings(time_step=1 / 120))
    simulation.hold(0, np.flatnonzero(mesh.rest[:, 0] < 0.003))
    reports = [simulation.step() for _ in range(30)]
    assert all(report.converged for report in reports)
    assert max(report.iterations for report in reports) <= 10


def test_a_free_sheet_falls_in_a_vacuum_as_implicit_euler_says() -> None:
    dt, steps = 0.01, 5
    simulation = Simulation([Sheet(SQUARE)], Settings(time_step=dt, air=None))
    for _ in range(steps):
        report = simulation.step()
        assert report.converged
    # Implicit Euler gains g dt of speed each step, then moves at the new speed.
    assert simulation.velocities(0)[:, 2] == pytest.approx(-9.81 * dt * steps)
    drop = 9.81 * dt * dt * steps * (steps + 1) / 2
    assert simulation.positions(0)[:, 2] == pytest.approx(-drop)
    assert report.time == pytest.approx(dt * steps)
    assert report.substeps == 1
    assert report.energy.kinetic == pytest.approx(
        0.5 * 0.06 * SIDE * SIDE * (9.81 * dt * steps) ** 2
    )
    energy = report.energy
    assert energy.total == pytest.approx(energy.gravity + energy.kinetic)


def test_the_same_inputs_give_the_same_result_bit_for_bit() -> None:
    def run() -> np.ndarray:
        simulation = Simulation(
            [Sheet(Mesh.grid(8, 2, 0.04, 0.01))], Settings(time_step=0.01)
        )
        simulation.hold(0, [0, 9])
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


def test_holds_take_one_vertex_or_an_array_of_them() -> None:
    simulation = Simulation([Sheet(SQUARE)])
    simulation.hold(0, 3)
    simulation.hold(0, np.array([0, 1], dtype=np.uint8))
    assert simulation.held(0).tolist() == [True, True, False, True]
    simulation.hold(0, [1, 3], held=False)
    assert simulation.held(0).tolist() == [True, False, False, False]
    with pytest.raises(
        TypeError, match="vertices must hold integers, but it holds bool"
    ):
        simulation.hold(0, [True, False])
    with pytest.raises(TypeError, match="vertices must hold integers"):
        simulation.hold(0, [0.0])
    with pytest.raises(IndexError, match="no vertex -1"):
        simulation.hold(0, [0, -1])
    with pytest.raises(ValueError, match=re.escape("shape (1, 2)")):
        simulation.hold(0, [[0, 1]])
    # A refused hold changes nothing.
    assert simulation.held(0).tolist() == [True, False, False, False]


def test_a_crease_folded_past_its_yield_moment_springs_back_short_of_the_fold() -> None:
    sheet = Sheet(SQUARE)
    sheet.set_crease((0, 2), Crease.kami())
    simulation = Simulation([sheet], STILL)
    simulation.hold(0, [0, 1, 2, 3])
    # A hand turns the free corner about the diagonal, a radian and a half toward the
    # front, then lets it go.
    fold = 1.5
    diagonal = np.array([1.0, 1.0, 0.0]) / math.sqrt(2)
    across = np.array([-SIDE / 2, SIDE / 2, 0.0])
    positions = simulation.positions(0)
    positions[3] = (
        diagonal * SIDE / math.sqrt(2)
        + across * math.cos(fold)
        + np.array([0.0, 0.0, SIDE / math.sqrt(2)]) * math.sin(fold)
    )
    simulation.set_positions(0, positions)
    folding = simulation.step()
    assert folding.energy.yielding > 0
    simulation.hold(0, 3, held=False)
    assert simulation.step().converged
    springback = Crease.kami().yield_moment / Crease.kami().stiffness
    assert simulation.fold_angles(0) == pytest.approx([fold - springback], abs=1e-6)
    crease = simulation.sheet(0)
    assert crease.rest_angle((0, 2)) == pytest.approx(fold - springback)
    assert crease.plastic_rotation((0, 2)) == pytest.approx(fold - springback)


def test_a_hand_carries_held_vertices_and_feels_the_weight() -> None:
    mesh = Mesh.grid(4, 4, SIDE, SIDE)
    # The hand feels the forces the solve leaves, so a tight tolerance balances them.
    # Hanging from flat takes many iterations.
    solver = Solver(tolerance=1e-12, iteration_limit=1000)
    settings = Settings(time_step=math.inf, solver=solver)
    simulation = Simulation([Sheet(mesh)], settings)
    edge = np.flatnonzero(mesh.rest[:, 0] > SIDE - 1e-9)
    simulation.hold(0, edge)
    lifted = simulation.positions(0)[edge] + [0.0, 0.0, 0.01]
    simulation.move_held(0, edge, lifted)
    assert simulation.step().converged
    assert simulation.positions(0)[edge] == pytest.approx(lifted, abs=1e-12)
    weight = 0.06 * SIDE * SIDE * 9.81
    forces = simulation.hold_forces(0)
    assert forces[:, 2].sum() == pytest.approx(weight, rel=1e-6)
    assert not forces[simulation.held(0) == 0].any()
    with pytest.raises(
        ValueError, match="vertex 0 is not held, so no hold can move it"
    ):
        simulation.move_held(0, [edge[0], 0], np.zeros((2, 3)))
    with pytest.raises(ValueError, match="2 vertices were given, but 1 positions"):
        simulation.move_held(0, edge[:2], np.zeros((1, 3)))


def test_forces_on_every_vertex_equal_to_its_weight_hold_a_sheet_up() -> None:
    simulation = Simulation([Sheet(SQUARE)], Settings(time_step=0.01, air=None))
    # Each vertex carries a third of the area of each triangle round it.
    areas = np.array([2, 1, 2, 1]) * SIDE * SIDE / 6
    up = np.outer(areas * 0.06 * 9.81, [0.0, 0.0, 1.0])
    simulation.set_forces(0, [0, 1, 2, 3], up)
    assert simulation.forces(0) == pytest.approx(up)
    simulation.set_pressure(0, 0.0)
    for _ in range(5):
        assert simulation.step().converged
    assert simulation.velocities(0) == pytest.approx(np.zeros((4, 3)), abs=1e-9)
    assert simulation.pressure(0) == 0.0
    with pytest.raises(ValueError, match="pressure is inf, but it must be finite"):
        simulation.set_pressure(0, math.inf)


def test_a_body_moves_along_its_path_and_feels_the_paper() -> None:
    simulation = Simulation([Sheet(SQUARE)], Settings(time_step=0.01))
    finger = simulation.add_body(Body(radius=0.01, friction=0.6), [[0.05, 0.02, 0.05]])
    assert finger == 0
    assert simulation.bodies == [Body(radius=0.01, friction=0.6)]
    simulation.move_body(finger, [[0.05, 0.02, 0.04]])
    assert simulation.step().converged
    assert simulation.body_points(finger) == pytest.approx(
        np.array([[0.05, 0.02, 0.04]])
    )
    assert simulation.body_force(finger) == pytest.approx([0.0, 0.0, 0.0])
    with pytest.raises(
        ValueError, match="the body lies closer to paper than its radius"
    ):
        simulation.add_body(Body(radius=0.01), [[0.05, 0.05, 0.0]])
    with pytest.raises(ValueError, match="a body has 1 point, for a sphere, or 2"):
        simulation.add_body(Body(radius=0.01), np.zeros((3, 3)))
    with pytest.raises(ValueError, match="the body has 1 point, so 2 points cannot move it"):
        simulation.move_body(finger, np.zeros((2, 3)))
    with pytest.raises(IndexError, match="the simulation has 1 bodies, so no body 1"):
        simulation.body_points(1)


def test_creases_and_rest_angles_lie_along_interior_edges() -> None:
    sheet = Sheet(SQUARE)
    with pytest.raises(ValueError, match="no interior edge from vertex 0 to vertex 1"):
        sheet.set_crease((0, 1), Crease(stiffness=0.01))
    with pytest.raises(ValueError, match="no interior edge from vertex 0 to vertex 1"):
        sheet.set_rest_angle((0, 1), 0.5)
    with pytest.raises(ValueError, match="rest angle is 4, but it must be smaller"):
        sheet.set_rest_angle((0, 2), 4.0)
    simulation = Simulation([sheet])
    simulation.set_crease(0, (0, 2), Crease(stiffness=0.01))
    simulation.set_rest_angle(0, (2, 0), 0.5)
    assert simulation.sheet(0).crease((0, 2)) == Crease(stiffness=0.01)
    assert simulation.sheet(0).rest_angle((0, 2)) == 0.5
    assert sheet.crease((0, 2)) is None
    assert sheet.rest_angle((0, 2)) == 0.0


def test_a_rest_angle_curves_plain_paper() -> None:
    # Without a crease, the plate rests curved round the diagonal: the corner lifts.
    sheet = Sheet(SQUARE)
    sheet.set_rest_angle((0, 2), 0.3)
    simulation = Simulation([sheet], STILL)
    simulation.hold(0, [0, 1, 2])
    report = simulation.step()
    assert report.converged
    assert report.energy.bending == pytest.approx(0, abs=1e-15)
    assert simulation.fold_angles(0) == pytest.approx([0.3], abs=1e-6)


def test_stacked_sheets_land_on_the_table_a_thickness_apart() -> None:
    mesh = Mesh.grid(4, 4, 0.02, 0.02)
    table = Table(height=0.0)
    simulation = Simulation([Sheet(mesh), Sheet(mesh)], Settings(table=table))
    for sheet, height in enumerate((1e-3, 2e-3)):
        lifted = simulation.positions(sheet)
        lifted[:, 2] = height
        simulation.set_positions(sheet, lifted)
    reports = [simulation.step() for _ in range(30)]
    assert all(r.converged and r.least_gap > 0 for r in reports)
    h, gap = Material().thickness, Contact().gap
    lower, upper = simulation.positions(0)[:, 2], simulation.positions(1)[:, 2]
    assert ((lower > h / 2) & (lower < h / 2 + gap)).all()
    assert ((upper - lower > h) & (upper - lower < h + 2 * gap)).all()
    assert reports[-1].contacts > 0
    assert reports[-1].energy.contact > 0
