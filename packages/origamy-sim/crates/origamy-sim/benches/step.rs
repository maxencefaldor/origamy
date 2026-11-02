//! Benchmarks of one time step in scenes that stress each part of the solver.
//!
//! Each scene is built afresh before every measured step, so the step alone is timed. Every
//! scene is a 20 cm square of kami.

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use origamy_sim::{Crease, Material, Mesh, Settings, Sheet, Simulation, Solver, Table};

/// A square of kami 20 cm wide, of `cells` by `cells` cells.
fn square(cells: u32) -> Sheet {
    Sheet::new(
        Mesh::grid(cells, cells, 0.2, 0.2).unwrap(),
        Material::kami(),
    )
    .unwrap()
}

/// Settings for quasi-static steps, with room for the many iterations far from rest.
fn quasi_static() -> Settings {
    Settings {
        time_step: f64::INFINITY,
        solver: Solver {
            iteration_limit: 1000,
            ..Solver::default()
        },
        ..Settings::default()
    }
}

/// Holds the first two columns of a square of `cells` cells: a clamp along its side.
fn clamp(simulation: &mut Simulation, cells: u32) {
    for j in 0..=cells {
        for i in 0..2 {
            simulation.hold(0, (j * (cells + 1) + i) as usize, true);
        }
    }
}

/// A square of 64 by 64 cells landing on the table: contact at every vertex.
fn drop_on_table() -> Simulation {
    let settings = Settings {
        table: Some(Table {
            height: -1e-3,
            ..Table::default()
        }),
        ..Settings::default()
    };
    let mut simulation = Simulation::new(vec![square(64)], settings).unwrap();
    let falling = vec![[0.0, 0.0, -0.3]; simulation.positions(0).len()];
    simulation.set_velocities(0, &falling).unwrap();
    simulation
}

/// A square of 32 by 32 cells clamped along one side, starting to fall under its weight.
fn swing() -> Simulation {
    let mut simulation = Simulation::new(vec![square(32)], Settings::default()).unwrap();
    clamp(&mut simulation, 32);
    simulation
}

/// A square of 24 by 24 cells clamped along one side, sagging to rest under its weight.
fn sag() -> Simulation {
    let mut simulation = Simulation::new(vec![square(24)], quasi_static()).unwrap();
    clamp(&mut simulation, 24);
    simulation
}

/// A square of 24 by 24 cells folded 1.5 radians along its diagonal crease, to rest.
///
/// The crease is actuated, as in a self-folding sheet, so it never yields.
fn fold() -> Simulation {
    let cells = 24;
    let mut sheet = square(cells);
    let actuated = Crease {
        yield_moment: f64::INFINITY,
        ..Crease::kami()
    };
    for k in 0..cells {
        let edge = [k * (cells + 1) + k, (k + 1) * (cells + 1) + k + 1];
        sheet.set_crease(edge, Some(actuated)).unwrap();
        sheet.set_rest_angle(edge, 1.5).unwrap();
    }
    let settings = Settings {
        gravity: [0.0; 3],
        ..quasi_static()
    };
    let mut simulation = Simulation::new(vec![sheet], settings).unwrap();
    // The lower right half stays; the rest folds over the diagonal.
    for j in 0..=cells {
        for i in j..=cells {
            simulation.hold(0, (j * (cells + 1) + i) as usize, true);
        }
    }
    simulation
}

/// A scene: a simulation built afresh, ready to step.
type Scene = fn() -> Simulation;

/// Times one time step of each scene.
fn step(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("step");
    group.sample_size(10);
    let scenes: [(&str, Scene); 4] = [
        ("drop on table", drop_on_table),
        ("swing", swing),
        ("sag", sag),
        ("fold", fold),
    ];
    for (name, scene) in scenes {
        group.bench_function(name, |bench| {
            bench.iter_batched(
                scene,
                |mut simulation| {
                    let report = simulation.step().unwrap();
                    assert!(report.converged, "{name}: {report:?}");
                    report
                },
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, step);
criterion_main!(benches);
