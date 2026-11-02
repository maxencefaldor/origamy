//! Physics benchmarks: the simulator against known solutions, each with its acceptance threshold.

use origamy_sim::{Crease, Material, Mesh, Settings, Sheet, Simulation, Solver, Stiffness};

/// A rectangle of `columns` by `rows` cells of `width` by `height` meters, each split along its
/// rising diagonal.
fn grid(columns: u32, rows: u32, width: f64, height: f64) -> Mesh {
    let at = |i: u32, j: u32| j * (columns + 1) + i;
    let rest = (0..=rows)
        .flat_map(|j| (0..=columns).map(move |i| [f64::from(i) * width, f64::from(j) * height]))
        .collect();
    let triangles = (0..rows)
        .flat_map(|j| {
            (0..columns).flat_map(move |i| {
                [
                    [at(i, j), at(i + 1, j), at(i + 1, j + 1)],
                    [at(i, j), at(i + 1, j + 1), at(i, j + 1)],
                ]
            })
        })
        .collect();
    Mesh::new(rest, triangles).unwrap()
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
    for &edge in &creases {
        sheet
            .set_crease(edge, Some(Crease { stiffness: 0.1 }))
            .unwrap();
    }
    let settings = Settings {
        gravity: [0.0; 3],
        time_step: f64::INFINITY,
        solver: Solver {
            iteration_limit: 1000,
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
