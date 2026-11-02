//! Physics benchmarks: the simulator against known solutions, each with its acceptance threshold.

use std::num::NonZeroU32;

use origamy_sim::{
    Air, Contact, Crease, Material, Mesh, Settings, Sheet, Simulation, Solver, Stiffness, Table,
};

/// A rectangle of `columns` by `rows` cells of `width` by `height` meters.
fn grid(columns: u32, rows: u32, width: f64, height: f64) -> Mesh {
    Mesh::grid(
        columns,
        rows,
        width * f64::from(columns),
        height * f64::from(rows),
    )
    .unwrap()
}

/// The tip deflection of a cantilever bent by its own weight, from the elastica.
///
/// The cantilever has length `length`, weight `weight` per length and bending stiffness
/// `stiffness`. The elastica EI θ'' = w (L − s) cos θ, with θ(0) = 0 and θ'(L) = 0, is solved
/// by shooting.
fn elastica(length: f64, weight: f64, stiffness: f64) -> f64 {
    let steps = 20_000;
    let h = length / f64::from(steps);
    // Integrates from the clamp with initial curvature `start`; returns θ'(L) and the deflection.
    let shoot = |start: f64| {
        let field = |s: f64, [theta, slope, _drop]: [f64; 3]| {
            [
                slope,
                weight * (length - s) * theta.cos() / stiffness,
                -theta.sin(),
            ]
        };
        let mut y = [0.0, start, 0.0];
        for k in 0..steps {
            let s = f64::from(k) * h;
            let add = |y: [f64; 3], d: [f64; 3], f: f64| {
                [y[0] + f * d[0], y[1] + f * d[1], y[2] + f * d[2]]
            };
            let k1 = field(s, y);
            let k2 = field(s + h / 2.0, add(y, k1, h / 2.0));
            let k3 = field(s + h / 2.0, add(y, k2, h / 2.0));
            let k4 = field(s + h, add(y, k3, h));
            y = [0, 1, 2].map(|i| y[i] + h / 6.0 * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i]));
        }
        (y[1], y[2])
    };
    // The free end has no moment; bisect the clamp's curvature until θ'(L) vanishes.
    let (mut low, mut high) = (-weight * length * length / stiffness, 0.0);
    for _ in 0..200 {
        let middle = 0.5 * (low + high);
        if shoot(middle).0 < 0.0 {
            low = middle;
        } else {
            high = middle;
        }
    }
    shoot(0.5 * (low + high)).1
}

#[test]
fn a_cantilever_sags_under_its_own_weight_as_the_elastica_says() {
    let (columns, rows, length, width) = (64, 4, 0.08, 0.01);
    let cell = length / f64::from(columns);
    let material = Material {
        stiffness: Stiffness::isotropic(4e9, 0.0),
        ..Material::kami()
    };
    let sheet = Sheet::new(grid(columns, rows, cell, width / f64::from(rows)), material).unwrap();
    let settings = Settings {
        time_step: f64::INFINITY,
        solver: Solver {
            iteration_limit: 1000,
            ..Solver::default()
        },
        ..Settings::default()
    };
    let mut simulation = Simulation::new(vec![sheet], settings).unwrap();
    // Holding the first two columns clamps the strip. The curvature along the second column's
    // edges spreads over the triangles on both its sides, so the clamp acts half a cell in.
    for j in 0..=rows {
        for i in 0..2 {
            simulation.hold(0, (j * (columns + 1) + i) as usize, true);
        }
    }
    let report = simulation.step().unwrap();
    assert!(report.converged);
    assert!(report.greatest_strain < 1e-6);
    let tips =
        (0..=rows).map(|j| -simulation.positions(0)[(j * (columns + 1) + columns) as usize][2]);
    let tip = tips.sum::<f64>() / f64::from(rows + 1);
    let stiffness = material.thickness.powi(3) / 12.0 * material.stiffness.along * width;
    let expected = elastica(
        length - cell / 2.0,
        material.mass_per_area * 9.81 * width,
        stiffness,
    );
    // The tip drops by about 30% of the strip's length, far past the linear theory.
    assert!(expected > 0.29 * length);
    assert!(
        (tip / expected - 1.0).abs() < 1e-3,
        "the tip drops {tip} m, the elastica {expected} m"
    );
}

#[test]
fn a_square_hung_on_a_hinge_swings_down_to_hang_straight() {
    // Held along one line of vertices, the flat square has a motion that costs no energy, so
    // the first Newton update runs out of bounds. The trust radius keeps the step in reach.
    let (cells, side) = (8, 0.2);
    let sheet = Sheet::new(
        Mesh::grid(cells, cells, side, side).unwrap(),
        Material::kami(),
    )
    .unwrap();
    let settings = Settings {
        time_step: f64::INFINITY,
        solver: Solver {
            iteration_limit: 1000,
            ..Solver::default()
        },
        ..Settings::default()
    };
    let mut simulation = Simulation::new(vec![sheet], settings).unwrap();
    for j in 0..=cells {
        simulation.hold(0, (j * (cells + 1)) as usize, true);
    }
    let report = simulation.step().unwrap();
    assert!(report.converged, "{report:?}");
    for (v, [x, _, z]) in simulation.positions(0).iter().enumerate() {
        let column = f64::from(u32::try_from(v).unwrap() % (cells + 1));
        let depth = side * column / f64::from(cells);
        assert!(
            x.abs() < 1e-6 && (z + depth).abs() < 1e-6,
            "vertex {v} hangs at {x}, {z}"
        );
    }
}

#[test]
fn a_strip_curved_at_rest_rolls_into_its_rest_shape() {
    // Every edge across the strip rests at the same angle. The strip rests as a polygon, each
    // cell turned by that angle from the one before, a quarter turn in all.
    let (columns, rows, cell) = (32, 2, 0.002);
    let turn = std::f64::consts::FRAC_PI_2 / f64::from(columns - 1);
    let mut sheet = Sheet::new(grid(columns, rows, cell, cell), Material::kami()).unwrap();
    let at = |i: u32, j: u32| j * (columns + 1) + i;
    for j in 0..rows {
        for i in 1..columns {
            sheet
                .set_rest_angle([at(i, j), at(i, j + 1)], turn)
                .unwrap();
        }
    }
    let settings = Settings {
        gravity: [0.0; 3],
        time_step: f64::INFINITY,
        ..Settings::default()
    };
    let mut simulation = Simulation::new(vec![sheet], settings).unwrap();
    for j in 0..=rows {
        for i in 0..2 {
            simulation.hold(0, at(i, j) as usize, true);
        }
    }
    let report = simulation.step().unwrap();
    assert!(report.converged, "{report:?}");
    let energy = report.energy;
    assert!(
        energy.bending + energy.stretching < 1e-15,
        "the rest shape keeps {energy:?}"
    );
    let mut expected = [0.0, 0.0];
    for i in 0..=columns {
        for j in 0..=rows {
            let [x, _, z] = simulation.positions(0)[at(i, j) as usize];
            let miss = (x - expected[0]).hypot(z - expected[1]);
            assert!(miss < 1e-9, "column {i} misses its rest place by {miss} m");
        }
        let heading = turn * f64::from(i);
        expected[0] += cell * heading.cos();
        expected[1] += cell * heading.sin();
    }
}

/// A Miura-ori of `columns` by `rows` parallelograms, folded rigidly by `fold`.
///
/// The parallelograms have sides `side` and `slant` and acute angle `angle`. The fold runs
/// from 0, flat, up to `angle`, folded flat. The folded sheet is the translational surface
/// P(i, j) = A(i) + B(j) of two zigzags. Zigzag A lies in the xz plane, and B in the xy plane.
/// Every face is a parallelogram whose angles stay fixed as the zigzags fold, so the faces
/// turn rigidly.
fn miura(columns: u32, rows: u32, [side, slant, angle]: [f64; 3], fold: f64) -> Vec<[f64; 3]> {
    let across = slant * angle.cos() / fold.cos();
    let along = (slant * slant - across * across).sqrt();
    let mut points = Vec::new();
    for j in 0..=rows {
        for i in 0..=columns {
            let zigzag = |k: u32| if k % 2 == 0 { 0.0 } else { 1.0 };
            points.push([
                f64::from(i) * side * fold.cos() + zigzag(j) * across,
                f64::from(j) * along,
                zigzag(i) * side * fold.sin(),
            ]);
        }
    }
    points
}

#[test]
fn a_miura_ori_folds_rigidly_when_its_creases_are_actuated() {
    let (columns, rows, shape) = (6, 6, [0.01, 0.01, 1.2]);
    let flat = miura(columns, rows, shape, 0.0);
    let at = |i: u32, j: u32| j * (columns + 1) + i;
    let mut triangles = Vec::new();
    for j in 0..rows {
        for i in 0..columns {
            triangles.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
            triangles.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
        }
    }
    let mesh = Mesh::new(flat.iter().map(|p| [p[0], p[1]]).collect(), triangles).unwrap();
    // The creases are the parallelograms' sides; the diagonals that split them stay paper.
    let creases: Vec<[u32; 2]> = mesh
        .dihedrals()
        .iter()
        .map(|d| d.edge)
        .filter(|&[u, v]| {
            let cell = |w: u32| (w % (columns + 1), w / (columns + 1));
            let ((column_u, row_u), (column_v, row_v)) = (cell(u), cell(v));
            column_u == column_v || row_u == row_v
        })
        .collect();
    let mut sheet = Sheet::new(mesh, Material::kami()).unwrap();
    // The creases never yield or age, so the actuation alone sets their rest angles.
    for &edge in &creases {
        sheet
            .set_crease(
                edge,
                Some(Crease {
                    stiffness: 0.1,
                    yield_moment: f64::INFINITY,
                    aging_rate: 0.0,
                    ..Crease::kami()
                }),
            )
            .unwrap();
    }
    let settings = Settings {
        gravity: [0.0; 3],
        time_step: f64::INFINITY,
        solver: Solver {
            iteration_limit: 1000,
            // The rigid folding has no strain, so the solver runs well past the strain bound.
            tolerance: 1e-11,
            ..Solver::default()
        },
        ..Settings::default()
    };
    // A second simulation measures the exact folds of the rigidly folded Miura-ori.
    let mut exact = Simulation::new(vec![sheet.clone()], settings).unwrap();
    let mut simulation = Simulation::new(vec![sheet], settings).unwrap();
    for v in [at(0, 0), at(1, 0), at(1, 1)] {
        simulation.hold(0, v as usize, true);
    }
    let steps = 8;
    for k in 1..=steps {
        let fold = 0.8 * shape[2] * f64::from(k) / f64::from(steps);
        exact
            .set_positions(0, &miura(columns, rows, shape, fold))
            .unwrap();
        let targets = exact.fold_angles(0);
        let dihedrals: Vec<[u32; 2]> = exact.sheets()[0]
            .mesh()
            .dihedrals()
            .iter()
            .map(|d| d.edge)
            .collect();
        for (edge, target) in dihedrals.iter().zip(&targets) {
            if creases.contains(edge) {
                simulation
                    .sheet_mut(0)
                    .set_rest_angle(*edge, *target)
                    .unwrap();
            }
        }
        let report = simulation.step().unwrap();
        assert!(report.converged, "{report:?}");
        assert!(report.greatest_strain < 1e-8, "{report:?}");
        let angles = simulation.fold_angles(0);
        let error = angles
            .iter()
            .zip(&targets)
            .map(|(a, t)| (a - t).abs())
            .fold(0.0, f64::max);
        assert!(
            error < 1e-6,
            "the folds miss the rigid folding by {error} rad"
        );
    }
}

#[test]
fn sheets_dropped_on_the_table_stack_a_thickness_apart() {
    // Four squares of kami fall onto the table, a millimeter apart, and settle into a stack.
    // Every layer rests a thickness above the one below, within the barrier's gap.
    let (layers, cells, side) = (4, 6, 0.03);
    let kami = Material::kami();
    let (h, gap) = (kami.thickness, Contact::default().gap);
    let sheets = (0..layers)
        .map(|_| Sheet::new(Mesh::grid(cells, cells, side, side).unwrap(), kami).unwrap())
        .collect();
    let settings = Settings {
        time_step: 1.0 / 120.0,
        table: Some(Table::default()),
        ..Settings::default()
    };
    let mut simulation = Simulation::new(sheets, settings).unwrap();
    for layer in 0..layers {
        let height = 1e-3 * f64::from(u32::try_from(layer + 1).unwrap());
        let placed: Vec<[f64; 3]> = simulation
            .positions(layer)
            .iter()
            .map(|&[x, y, _]| [x, y, height])
            .collect();
        simulation.set_positions(layer, &placed).unwrap();
    }
    for step in 0..60 {
        let report = simulation.step().unwrap();
        assert!(report.converged, "step {step}: {report:?}");
        assert!(report.least_gap > 0.0, "step {step}: {report:?}");
    }
    for layer in 0..layers {
        let k = f64::from(u32::try_from(layer).unwrap());
        let (low, high) = (0.5 * h + k * h, 0.5 * h + k * h + (k + 1.0) * gap);
        for [_, _, z] in simulation.positions(layer) {
            assert!(
                (low..=high).contains(z),
                "layer {layer} rests at {z}, outside [{low}, {high}]"
            );
        }
    }
}

#[test]
fn a_strip_folded_over_itself_never_crosses_and_rests_above_itself() {
    // The right half of a strip on the table folds over its crease onto the left half, which a
    // hand holds. No step brings paper within its thickness of paper. The folded half springs
    // into a low arch, as kami does. The arch rises half a millimeter, and its end comes down
    // to within a tenth of a millimeter of the near end.
    let (columns, rows, length, width) = (40, 4, 0.1, 0.01);
    let kami = Material::kami();
    let (h, gap) = (kami.thickness, Contact::default().gap);
    let mut sheet = Sheet::new(Mesh::grid(columns, rows, length, width).unwrap(), kami).unwrap();
    let at = |i: u32, j: u32| j * (columns + 1) + i;
    let middle = columns / 2;
    let crease: Vec<[u32; 2]> = (0..rows)
        .map(|j| [at(middle, j), at(middle, j + 1)])
        .collect();
    for &edge in &crease {
        sheet
            .set_crease(
                edge,
                Some(Crease {
                    stiffness: 1e-3,
                    ..Crease::kami()
                }),
            )
            .unwrap();
    }
    let settings = Settings {
        time_step: 1.0 / 120.0,
        table: Some(Table::default()),
        ..Settings::default()
    };
    let mut simulation = Simulation::new(vec![sheet], settings).unwrap();
    let rest = 0.5 * h + 0.5 * gap;
    let lying: Vec<[f64; 3]> = simulation
        .positions(0)
        .iter()
        .map(|&[x, y, _]| [x, y, rest])
        .collect();
    simulation.set_positions(0, &lying).unwrap();
    for j in 0..=rows {
        for i in 0..=middle {
            simulation.hold(0, at(i, j) as usize, true);
        }
    }
    let target = std::f64::consts::PI - 0.01;
    for step in 0..180 {
        let ramp = (f64::from(step + 1) / 60.0).min(1.0);
        for &edge in &crease {
            simulation
                .sheet_mut(0)
                .set_rest_angle(edge, ramp * target)
                .unwrap();
        }
        let report = simulation.step().unwrap();
        assert!(report.converged, "step {step}: {report:?}");
        assert!(report.least_gap > 0.0, "step {step}: {report:?}");
    }
    for j in 0..=rows {
        for i in middle + 1..=columns {
            let above = simulation.positions(0)[at(i, j) as usize][2] - rest;
            assert!(above >= h, "column {i} lies {above} m above the near half");
        }
        let [x, _, z] = simulation.positions(0)[at(columns, j) as usize];
        assert!(x.abs() < 2e-3, "the far end lies at x = {x}");
        assert!(
            z - rest < 1e-4,
            "the far end lies {} m above the near end",
            z - rest
        );
    }
}

/// A 4 cm square of kami on an incline, on the table or on a 10 cm square held on the table.
///
/// Gravity tilts by `tilt` from the table's normal, which makes the table an incline. The
/// function returns the square's mean velocity along the incline after `steps` time steps of
/// 1/240 s, and its mean slip.
///
/// The square first settles for a quarter second under upright gravity. Friction thus lags
/// the square's weight as its normal force from the first tilted step.
fn incline(tilt: f64, on_paper: bool, steps: u32) -> (f64, f64) {
    let kami = Material::kami();
    let contact = Contact::default();
    let (h, gap) = (kami.thickness, contact.gap);
    let square = |cells: u32, side: f64| {
        Sheet::new(Mesh::grid(cells, cells, side, side).unwrap(), kami).unwrap()
    };
    let mut sheets = vec![square(8, 0.04)];
    if on_paper {
        sheets.push(square(10, 0.1));
    }
    let g = 9.81;
    let settings = Settings {
        time_step: 1.0 / 240.0,
        table: Some(Table::default()),
        ..Settings::default()
    };
    let mut simulation = Simulation::new(sheets, settings).unwrap();
    let rest = 0.5 * h + 0.5 * gap;
    let height = if on_paper { rest + h + 0.5 * gap } else { rest };
    let placed: Vec<[f64; 3]> = simulation
        .positions(0)
        .iter()
        .map(|&[x, y, _]| [x + 0.03, y + 0.03, height])
        .collect();
    simulation.set_positions(0, &placed).unwrap();
    if on_paper {
        let lying: Vec<[f64; 3]> = simulation
            .positions(1)
            .iter()
            .map(|&[x, y, _]| [x, y, rest])
            .collect();
        simulation.set_positions(1, &lying).unwrap();
        let count = simulation.positions(1).len();
        for v in 0..count {
            simulation.hold(1, v, true);
        }
    }
    let run = |simulation: &mut Simulation, steps: u32| {
        for step in 0..steps {
            let report = simulation.step().unwrap();
            assert!(report.converged, "step {step}: {report:?}");
            assert!(report.least_gap > 0.0, "step {step}: {report:?}");
        }
    };
    run(&mut simulation, 60);
    let mean = |values: &[[f64; 3]]| {
        values.iter().map(|p| p[0]).sum::<f64>() / f64::from(u32::try_from(values.len()).unwrap())
    };
    let settled = mean(simulation.positions(0));
    simulation
        .set_settings(Settings {
            gravity: [g * tilt.sin(), 0.0, -g * tilt.cos()],
            ..settings
        })
        .unwrap();
    run(&mut simulation, steps);
    let slip = mean(simulation.positions(0)) - settled;
    (mean(simulation.velocities(0)), slip)
}

#[test]
fn kami_slides_down_an_incline_steeper_than_its_friction_angle() {
    // Past the friction angle, the square slides with acceleration g (sin θ − μ cos θ).
    let mu = Contact::default().friction;
    for (tilt, on_paper) in [(0.5, false), (0.45, true)] {
        let steps = 48;
        let (velocity, _) = incline(tilt, on_paper, steps);
        let expected = 9.81 * (tilt.sin() - mu * tilt.cos()) * f64::from(steps) / 240.0;
        assert!(
            (velocity / expected - 1.0).abs() < 0.01,
            "the square slides at {velocity} m/s, Coulomb's law says {expected}"
        );
    }
}

#[test]
fn kami_sticks_on_an_incline_gentler_than_its_friction_angle() {
    // Below the friction angle, friction holds the square: it creeps by less than the stick
    // distance a step.
    for on_paper in [false, true] {
        let steps = 120;
        let (_, slip) = incline(0.25, on_paper, steps);
        let bound = Contact::default().stick * f64::from(steps);
        assert!(
            slip.abs() < bound,
            "the square slips {slip} m, past {bound} m"
        );
    }
}

#[test]
fn kami_falls_flat_at_the_terminal_velocity_that_drag_sets() {
    // A square of kami falls flat through still air. Per area, drag ½ ρ C v² balances the
    // weight m g at the terminal velocity v = √(2 m g / ρ C). Each time step takes implicit
    // Euler of v' = −g + k v², with k = ρ C / 2m. The square follows it to that velocity.
    // A tight tolerance lets the velocities match implicit Euler's to 10 nm/s.
    let (kami, air, g) = (Material::kami(), Air::default(), 9.81);
    let dt = 1.0 / 120.0;
    let settings = Settings {
        time_step: dt,
        solver: Solver {
            tolerance: 1e-12,
            ..Solver::default()
        },
        ..Settings::default()
    };
    let sheet = Sheet::new(Mesh::grid(4, 4, 0.1, 0.1).unwrap(), kami).unwrap();
    let mut simulation = Simulation::new(vec![sheet], settings).unwrap();
    let k = air.density * air.drag / (2.0 * kami.mass_per_area);
    let mut expected: f64 = 0.0;
    for step in 0..120 {
        let report = simulation.step().unwrap();
        assert!(report.converged, "step {step}: {report:?}");
        // The root of dt k v² − v + (v₀ − g dt) = 0 nearest v₀ − g dt, in a stable form.
        let c = expected - g * dt;
        expected = 2.0 * c / (1.0 + (1.0 - 4.0 * dt * k * c).sqrt());
        for [x, y, z] in simulation.velocities(0) {
            assert!(
                x.abs() < 1e-12 && y.abs() < 1e-12 && (z - expected).abs() < 1e-8,
                "step {step}: a vertex falls at {z} m/s, implicit Euler says {expected}"
            );
        }
    }
    let terminal = (2.0 * kami.mass_per_area * g / (air.density * air.drag)).sqrt();
    let [_, _, z] = simulation.velocities(0)[0];
    assert!(
        (-z / terminal - 1.0).abs() < 1e-3,
        "the square falls at {z} m/s, its terminal velocity is {terminal}"
    );
}

/// A strip of kami 6 cm long with a crease across its middle, in quasi-static steps without
/// gravity. A hand holds the strip and turns its far half about the crease to each of `folds`
/// in turn. The hand holds the last fold for `hold` seconds, then lets the far half go.
///
/// Returns the crease's rest angle, its fold angle once the strip rests again, and the rotation
/// it has flowed through.
fn turn_and_release(folds: &[f64], hold: f64) -> (f64, f64, f64) {
    let (columns, rows, length, width) = (12, 4, 0.06, 0.015);
    let mesh = Mesh::grid(columns, rows, length, width).unwrap();
    let mut sheet = Sheet::new(mesh, Material::kami()).unwrap();
    let at = |i: u32, j: u32| j * (columns + 1) + i;
    let crease: Vec<[u32; 2]> = (0..rows)
        .map(|j| [at(columns / 2, j), at(columns / 2, j + 1)])
        .collect();
    for &edge in &crease {
        sheet.set_crease(edge, Some(Crease::kami())).unwrap();
    }
    let settings = Settings {
        gravity: [0.0; 3],
        time_step: f64::INFINITY,
        ..Settings::default()
    };
    let mut simulation = Simulation::new(vec![sheet], settings).unwrap();
    let flat = simulation.positions(0).to_vec();
    let hinge = 0.5 * length;
    let far: Vec<usize> = (0..flat.len())
        .filter(|&v| flat[v][0] > hinge + 1e-9)
        .collect();
    for v in 0..flat.len() {
        simulation.hold(0, v, true);
    }
    for &fold in folds {
        let mut turned = flat.clone();
        for &v in &far {
            let r = flat[v][0] - hinge;
            turned[v] = [hinge + r * fold.cos(), flat[v][1], r * fold.sin()];
        }
        simulation.set_positions(0, &turned).unwrap();
        assert!(simulation.step().unwrap().converged);
    }
    // Time passes in time steps that grow by a tenth each, since the crease ages in log time.
    let (mut elapsed, mut dt) = (0.0, 1e-3);
    while elapsed < hold {
        let step = f64::min(dt, hold - elapsed);
        simulation
            .set_settings(Settings {
                time_step: step,
                ..settings
            })
            .unwrap();
        assert!(simulation.step().unwrap().converged);
        (elapsed, dt) = (elapsed + step, 1.1 * dt);
    }
    simulation.set_settings(settings).unwrap();
    for &v in &far {
        simulation.hold(0, v, false);
    }
    let report = simulation.step().unwrap();
    assert!(report.converged, "{report:?}");
    let dihedrals = simulation.sheets()[0].mesh().dihedrals();
    let angles = simulation.fold_angles(0);
    let creased: Vec<f64> = dihedrals
        .iter()
        .zip(&angles)
        .filter(|(d, _)| crease.contains(&d.edge))
        .map(|(_, &angle)| angle)
        .collect();
    let fold = creased.iter().sum::<f64>() / f64::from(rows);
    let sheet = &simulation.sheets()[0];
    let rest = sheet.rest_angle(crease[0]).unwrap();
    for &edge in &crease {
        assert!((sheet.rest_angle(edge).unwrap() - rest).abs() < 1e-12);
    }
    (rest, fold, sheet.plastic_rotation(crease[0]).unwrap())
}

/// The angles a hand turns the strip's far half through, from `from` to `to` in `count` equal
/// turns.
fn ramp(from: f64, to: f64, count: u32) -> Vec<f64> {
    (1..=count)
        .map(|k| from + (to - from) * f64::from(k) / f64::from(count))
        .collect()
}

#[test]
fn kami_pressed_flat_springs_back_to_the_measured_angle() {
    // The hand presses the fold closed until the 5 mm cells beside the crease lie just over a
    // thickness apart where their diagonals cross. Released, the fold opens to between 30° and
    // 40°, as measured creases rest, and the crease rests where the fold does.
    let closed = std::f64::consts::PI - 0.04;
    let (rest, fold, _) = turn_and_release(&ramp(0.0, closed, 32), 0.0);
    let opening = (std::f64::consts::PI - fold).to_degrees();
    assert!(
        (30.0..=40.0).contains(&opening),
        "the fold opens to {opening}°"
    );
    assert!(
        (fold - rest).abs() < 1e-6,
        "the fold rests at {fold}, its crease at {rest}"
    );
}

#[test]
fn a_crease_springs_back_by_its_yield_angle_or_whole_below_it() {
    // Folded past the angle its yield moment allows, a crease springs back by that angle.
    // Folded short of it, the crease springs back flat.
    let kami = Crease::kami();
    let springback = kami.yield_moment / kami.stiffness;
    for (folded, expected) in [
        (
            std::f64::consts::FRAC_PI_2,
            std::f64::consts::FRAC_PI_2 - springback,
        ),
        (0.5, 0.0),
    ] {
        let (rest, fold, rotation) = turn_and_release(&ramp(0.0, folded, 16), 0.0);
        assert!(
            (rest - expected).abs() < 1e-12,
            "the crease rests at {rest}"
        );
        assert!((fold - expected).abs() < 1e-6, "the fold rests at {fold}");
        assert!((rotation - expected).abs() < 1e-12);
    }
}

#[test]
fn a_precrease_pressed_flat_again_springs_back_by_its_yield_angle() {
    // Folded flat and unfolded flat again, the crease yields both ways. Released, it rises by
    // its yield angle on the side it was folded to, and has yielded through both turns.
    let kami = Crease::kami();
    let springback = kami.yield_moment / kami.stiffness;
    let closed = std::f64::consts::PI - 0.04;
    let mut folds = ramp(0.0, closed, 32);
    folds.extend(ramp(closed, 0.0, 32));
    let (rest, fold, rotation) = turn_and_release(&folds, 0.0);
    assert!(
        (rest - springback).abs() < 1e-12,
        "the crease rests at {rest}"
    );
    assert!((fold - springback).abs() < 1e-6, "the fold rests at {fold}");
    let yielded = 2.0 * (closed - springback) - springback;
    assert!(
        (rotation - yielded).abs() < 1e-12,
        "the crease yielded {rotation} rad"
    );
}

#[test]
fn a_fold_of_kami_held_longer_springs_back_less() {
    // Held past its yield, the crease ages: its moment falls by m* ln(1 + t / τ), with τ about
    // a second. The fold therefore springs back less. Between holds of 10 and 100 s, the
    // springback falls by about the 5.7° a decade that paper measures.
    let kami = Crease::kami();
    let tau = 2.0 * kami.aging_moment
        / (kami.stiffness * kami.aging_rate * (kami.yield_moment / kami.aging_moment).exp());
    let folded = std::f64::consts::FRAC_PI_2;
    let mut springbacks = Vec::new();
    for hold in [1.0, 10.0, 100.0] {
        let (rest, fold, _) = turn_and_release(&ramp(0.0, folded, 16), hold);
        let springback = folded - fold;
        let law =
            (kami.yield_moment - kami.aging_moment * (1.0 + hold / tau).ln()) / kami.stiffness;
        assert!(
            (springback - law).abs() < 2e-3,
            "held {hold} s, the fold springs back {springback} rad; the law says {law}"
        );
        assert!((fold - rest).abs() < 1e-6);
        springbacks.push(springback);
    }
    let decade = (springbacks[1] - springbacks[2]).to_degrees();
    assert!(
        (decade / 5.7 - 1.0).abs() < 0.05,
        "a decade of holding sets {decade}° deeper"
    );
}

/// Where a point at `x` along the flat strip lies once a crease band of `lines` folds to `fold`.
///
/// The point lies along the strip, and up from it.
///
/// Each line turns by its share of the fold, so the band bends as a polygon of facets.
fn fold_band(x: f64, lines: &[f64], fold: f64) -> [f64; 2] {
    let share = fold / f64::from(u32::try_from(lines.len()).unwrap());
    let [mut along, mut up] = [x.min(lines[0]), 0.0];
    for (i, &line) in lines.iter().enumerate() {
        let next = lines.get(i + 1).copied().unwrap_or(f64::INFINITY);
        if x <= line {
            break;
        }
        let turn = share * f64::from(u32::try_from(i + 1).unwrap());
        let length = x.min(next) - line;
        along += length * turn.cos();
        up += length * turn.sin();
    }
    [along, up]
}

#[test]
fn a_crease_band_of_kami_pressed_flat_springs_back_to_the_measured_angle() {
    // A strip 6 cm by 5 mm has cells 5 mm long, and five lines of edges 0.1 mm apart across its
    // middle. The lines make a crease band 0.4 mm wide. A hand folds the band fully flat, its far
    // half 0.3 mm over its near half, as a sharp crease cannot for the thickness. Released, the
    // band springs back to the 36° that paper measures, as its sharp crease would.
    let (rows, width) = (10, 0.005);
    let lines: Vec<f64> = (0..5).map(|i| 0.03 + 1e-4 * (f64::from(i) - 2.0)).collect();
    let mut columns: Vec<f64> = (0..6).map(|i| 0.005 * f64::from(i)).collect();
    columns.extend(&lines);
    columns.extend((7..13).map(|i| 0.005 * f64::from(i)));
    let count = u32::try_from(columns.len()).unwrap();
    let at = |i: u32, j: u32| j * count + i;
    let rest: Vec<[f64; 2]> = (0..=rows)
        .flat_map(|j| {
            columns
                .iter()
                .map(move |&x| [x, width * f64::from(j) / f64::from(rows)])
        })
        .collect();
    let triangles: Vec<[u32; 3]> = (0..rows)
        .flat_map(|j| {
            (0..count - 1).flat_map(move |i| {
                [
                    [at(i, j), at(i + 1, j), at(i + 1, j + 1)],
                    [at(i, j), at(i + 1, j + 1), at(i, j + 1)],
                ]
            })
        })
        .collect();
    let mut sheet = Sheet::new(Mesh::new(rest, triangles).unwrap(), Material::kami()).unwrap();
    let band = Crease::kami().band(NonZeroU32::new(5).unwrap());
    let edges: Vec<[u32; 2]> = (6..11)
        .flat_map(|i| (0..rows).map(move |j| [at(i, j), at(i, j + 1)]))
        .collect();
    for &edge in &edges {
        sheet.set_crease(edge, Some(band)).unwrap();
    }
    let settings = Settings {
        gravity: [0.0; 3],
        time_step: f64::INFINITY,
        ..Settings::default()
    };
    let mut simulation = Simulation::new(vec![sheet], settings).unwrap();
    let flat = simulation.positions(0).to_vec();
    for v in 0..flat.len() {
        simulation.hold(0, v, true);
    }
    for fold in ramp(0.0, std::f64::consts::PI, 32) {
        let folded: Vec<[f64; 3]> = flat
            .iter()
            .map(|&[x, y, z]| {
                let [along, up] = fold_band(x, &lines, fold);
                [along, y, z + up]
            })
            .collect();
        simulation.set_positions(0, &folded).unwrap();
        assert!(simulation.step().unwrap().converged);
    }
    for (v, &[x, _, _]) in flat.iter().enumerate() {
        simulation.hold(0, v, x <= lines[0]);
    }
    let report = simulation.step().unwrap();
    assert!(report.converged, "{report:?}");
    let dihedrals = simulation.sheets()[0].mesh().dihedrals();
    let fold: f64 = dihedrals
        .iter()
        .zip(simulation.fold_angles(0))
        .filter(|(d, _)| edges.contains(&d.edge))
        .map(|(_, angle)| angle)
        .sum::<f64>()
        / f64::from(rows);
    let opening = std::f64::consts::PI - fold;
    let springback = Crease::kami().yield_moment / Crease::kami().stiffness;
    assert!(
        (opening - springback).abs() < 1e-6,
        "the band opens to {}°",
        opening.to_degrees()
    );
}
