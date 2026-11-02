//! A simulation: sheets in space, stepped through time by minimizing the incremental potential.
//!
//! Each time step finds the positions x that minimize
//!
//! ½ (x − x̂)ᵀ M (x − x̂) / Δt² + E(x),  with x̂ = x₀ + Δt v₀,
//!
//! which is one step of implicit Euler: inertia, elasticity and gravity balance at the minimum.
//! An infinite time step drops inertia, so the step finds the rest the forces lead to.

use nalgebra::{SMatrix, Vector3};
use rayon::prelude::*;

use crate::energy::bending::{self, Angle, BendingEdge};
use crate::energy::membrane::{self, Stretching};
use crate::energy::{Barrier, Point, project};
use crate::material::{ParameterError, finite, positive};
use crate::sheet::Sheet;
use crate::system::{Assembly, Pattern, SingularError};

/// The settings of a simulation: the world it runs in and how its solver works.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// The acceleration of gravity, in meters per second squared.
    pub gravity: [f64; 3],
    /// The length of a time step, in seconds; infinite for quasi-static steps.
    pub time_step: f64,
    /// The largest vertex move of a Newton update at which a time step has converged, in meters.
    pub tolerance: f64,
    /// The most Newton iterations a time step may take.
    pub iteration_limit: usize,
    /// The fraction of the predicted decrease of energy a line search requires, between 0 and 1.
    pub sufficient_decrease: f64,
    /// The shortest fraction of a Newton update a line search tries, between 0 and 1.
    pub shortest_update: f64,
}

impl Default for Settings {
    /// Earth's gravity along −z, time steps of a sixtieth of a second, and a tolerance of 10 nm.
    fn default() -> Self {
        Self {
            gravity: [0.0, 0.0, -9.81],
            time_step: 1.0 / 60.0,
            tolerance: 1e-8,
            iteration_limit: 200,
            sufficient_decrease: 1e-4,
            shortest_update: 1e-12,
        }
    }
}

impl Settings {
    /// Checks that every setting takes a value it may.
    ///
    /// # Errors
    ///
    /// Returns the first setting outside its values.
    pub fn check(&self) -> Result<(), ParameterError> {
        for g in self.gravity {
            finite("gravity", g)?;
        }
        if self.time_step.is_nan() || self.time_step <= 0.0 {
            return Err(ParameterError {
                name: "time step",
                value: self.time_step,
                requirement: "positive, or infinite for quasi-static steps",
            });
        }
        positive("tolerance", self.tolerance)?;
        #[expect(
            clippy::cast_precision_loss,
            reason = "the count only appears in a message"
        )]
        if self.iteration_limit == 0 {
            return Err(ParameterError {
                name: "iteration limit",
                value: self.iteration_limit as f64,
                requirement: "at least 1",
            });
        }
        for (name, value) in [
            ("sufficient decrease", self.sufficient_decrease),
            ("shortest update", self.shortest_update),
        ] {
            if !(value > 0.0 && value < 1.0) {
                return Err(ParameterError {
                    name,
                    value,
                    requirement: "between 0 and 1",
                });
            }
        }
        Ok(())
    }
}

/// The energy of a simulation, term by term, in joules.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Energy {
    /// The elastic energy of stretching.
    pub stretching: f64,
    /// The energy of the barrier that caps strain.
    pub strain_barrier: f64,
    /// The elastic energy of bending, away from sharp creases.
    pub bending: f64,
    /// The elastic energy of sharp creases.
    pub creases: f64,
    /// The potential energy of gravity, zero at the origin.
    pub gravity: f64,
    /// The kinetic energy.
    pub kinetic: f64,
}

/// What a time step did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Report {
    /// Whether the step reached its tolerance.
    pub converged: bool,
    /// The Newton iterations the step took.
    pub iterations: usize,
    /// The largest vertex move of the last Newton update, in meters: the residual.
    pub update: f64,
    /// The energy after the step.
    pub energy: Energy,
    /// The greatest norm of any triangle's Green strain after the step.
    pub greatest_strain: f64,
    /// The simulated time after the step, in seconds.
    pub time: f64,
}

/// Why positions cannot be set.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum StateError {
    /// The positions are not one per vertex of the sheet.
    #[error("the sheet has {expected} vertices, but {given} positions were given")]
    Count {
        /// How many vertices the sheet has.
        expected: usize,
        /// How many positions were given.
        given: usize,
    },
    /// A coordinate is infinite or not a number.
    #[error("vertex {vertex} would lie at a coordinate that is not finite")]
    NotFinite {
        /// The vertex.
        vertex: usize,
    },
    /// The positions stretch a triangle past the strain cap.
    #[error("the positions stretch triangle {triangle} past the strain cap")]
    Overstretched {
        /// The triangle.
        triangle: usize,
    },
}

/// Sheets in space, stepped through time.
#[derive(Debug)]
pub struct Simulation {
    sheets: Vec<Sheet>,
    offsets: Vec<usize>,
    settings: Settings,
    positions: Vec<[f64; 3]>,
    velocities: Vec<[f64; 3]>,
    held: Vec<bool>,
    time: f64,
    pattern: Pattern,
}

/// The constants of one sheet's energies, from its material.
struct Constants {
    stretching: Stretching,
    bending: nalgebra::Matrix3<f64>,
    mass_per_area: f64,
}

impl Constants {
    fn of(sheet: &Sheet) -> Self {
        let material = sheet.material();
        let stiffness = material.stiffness.matrix();
        let young = material.stiffness.along.max(material.stiffness.across);
        Self {
            stretching: Stretching {
                stiffness: stiffness * material.thickness,
                cap: material.strain_cap,
                barrier: Barrier {
                    onset: 1.0 - material.strain_onset * material.strain_onset,
                },
                barrier_stiffness: material.thickness
                    * young
                    * material.strain_cap
                    * material.strain_cap,
            },
            bending: material.bending(),
            mass_per_area: material.mass_per_area,
        }
    }
}

impl Simulation {
    /// A simulation of these sheets, each lying flat at rest on the plane z = 0, still.
    ///
    /// # Errors
    ///
    /// Returns the first setting outside its values.
    pub fn new(sheets: Vec<Sheet>, settings: Settings) -> Result<Self, ParameterError> {
        settings.check()?;
        let mut offsets = Vec::with_capacity(sheets.len());
        let mut positions = Vec::new();
        for sheet in &sheets {
            offsets.push(positions.len());
            positions.extend(sheet.mesh().rest().iter().map(|&[x, y]| [x, y, 0.0]));
        }
        let pattern = Pattern::new(positions.len(), pairs(&sheets, &offsets));
        Ok(Self {
            velocities: vec![[0.0; 3]; positions.len()],
            held: vec![false; positions.len()],
            sheets,
            offsets,
            settings,
            positions,
            time: 0.0,
            pattern,
        })
    }

    /// The simulation's settings.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Changes the simulation's settings.
    ///
    /// # Errors
    ///
    /// Returns the first setting outside its values.
    pub fn set_settings(&mut self, settings: Settings) -> Result<(), ParameterError> {
        settings.check()?;
        self.settings = settings;
        Ok(())
    }

    /// The simulation's sheets.
    #[must_use]
    pub fn sheets(&self) -> &[Sheet] {
        &self.sheets
    }

    /// A sheet, to change its material or its creases.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such sheet.
    pub fn sheet_mut(&mut self, sheet: usize) -> &mut Sheet {
        &mut self.sheets[sheet]
    }

    /// The simulated time, in seconds.
    #[must_use]
    pub fn time(&self) -> f64 {
        self.time
    }

    /// Where each vertex of a sheet lies in space, in meters.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such sheet.
    #[must_use]
    pub fn positions(&self, sheet: usize) -> &[[f64; 3]] {
        &self.positions[self.range(sheet)]
    }

    /// Each vertex's velocity, in meters per second.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such sheet.
    #[must_use]
    pub fn velocities(&self, sheet: usize) -> &[[f64; 3]] {
        &self.velocities[self.range(sheet)]
    }

    /// Whether each vertex of a sheet is held where it lies.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such sheet.
    #[must_use]
    pub fn held(&self, sheet: usize) -> &[bool] {
        &self.held[self.range(sheet)]
    }

    /// Moves every vertex of a sheet.
    ///
    /// # Errors
    ///
    /// Returns a [`StateError`] when the positions do not match the sheet's vertices, a
    /// coordinate is not finite, or a triangle would stretch past the strain cap.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such sheet.
    pub fn set_positions(
        &mut self,
        sheet: usize,
        positions: &[[f64; 3]],
    ) -> Result<(), StateError> {
        let range = self.range(sheet);
        check_count(range.len(), positions.len())?;
        if let Some(vertex) = positions
            .iter()
            .position(|p| !p.iter().all(|c| c.is_finite()))
        {
            return Err(StateError::NotFinite { vertex });
        }
        let s = &self.sheets[sheet];
        let x: Vec<Point> = positions.iter().map(|&p| Point::from(p)).collect();
        let stretching = Constants::of(s).stretching;
        for (t, (corners, rest)) in s
            .mesh()
            .triangles()
            .iter()
            .zip(s.rest_triangles())
            .enumerate()
        {
            let points = corners.map(|v| x[v as usize]);
            if membrane::energy(rest, &points, &stretching).is_none() {
                return Err(StateError::Overstretched { triangle: t });
            }
        }
        self.positions[range].copy_from_slice(positions);
        Ok(())
    }

    /// Sets every vertex's velocity.
    ///
    /// # Errors
    ///
    /// Returns a [`StateError`] when the velocities do not match the sheet's vertices or a
    /// coordinate is not finite.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such sheet.
    pub fn set_velocities(
        &mut self,
        sheet: usize,
        velocities: &[[f64; 3]],
    ) -> Result<(), StateError> {
        let range = self.range(sheet);
        check_count(range.len(), velocities.len())?;
        if let Some(vertex) = velocities
            .iter()
            .position(|p| !p.iter().all(|c| c.is_finite()))
        {
            return Err(StateError::NotFinite { vertex });
        }
        self.velocities[range].copy_from_slice(velocities);
        Ok(())
    }

    /// The dihedral angle along each interior edge of a sheet, in the order of its mesh's
    /// dihedrals, in radians: zero where the sheet lies flat, and positive where it folds toward
    /// its front.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such sheet.
    #[must_use]
    pub fn fold_angles(&self, sheet: usize) -> Vec<f64> {
        let x = self.positions(sheet);
        self.sheets[sheet]
            .mesh()
            .dihedrals()
            .iter()
            .map(|d| bending::angle_value(&stencil(d).map(|v| Point::from(x[v as usize]))))
            .collect()
    }

    /// Holds a vertex where it lies, or lets it go.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such sheet or the sheet no such vertex.
    pub fn hold(&mut self, sheet: usize, vertex: usize, held: bool) {
        let range = self.range(sheet);
        assert!(vertex < range.len(), "the sheet has no vertex {vertex}");
        self.held[range.start + vertex] = held;
        if held {
            self.velocities[range.start + vertex] = [0.0; 3];
        }
    }

    /// The simulation's energy now.
    #[must_use]
    pub fn energy(&self) -> Energy {
        let x: Vec<Point> = self.positions.iter().map(|&p| Point::from(p)).collect();
        let mut energy = self.energy_at(&x, &x, 0.0);
        energy.kinetic = self.kinetic();
        energy
    }

    /// Advances the simulation by one time step.
    ///
    /// # Errors
    ///
    /// Returns [`SingularError`] when some motion costs no energy, so the step has no unique
    /// result; the state stays as it was.
    pub fn step(&mut self) -> Result<Report, SingularError> {
        let dt = self.settings.time_step;
        let start: Vec<Point> = self.positions.iter().map(|&p| Point::from(p)).collect();
        let predicted: Vec<Point> = if dt.is_finite() {
            start
                .iter()
                .zip(&self.velocities)
                .zip(&self.held)
                .map(|((x, &v), &held)| if held { *x } else { x + Point::from(v) * dt })
                .collect()
        } else {
            start.clone()
        };
        let inertia = if dt.is_finite() { 1.0 / (dt * dt) } else { 0.0 };
        let mut x = start.clone();
        let mut energy = self.energy_at(&x, &predicted, inertia);
        let (mut converged, mut iterations, mut update) = (false, 0, f64::INFINITY);
        while iterations < self.settings.iteration_limit {
            iterations += 1;
            let mut assembly = Assembly::new(&self.pattern, &self.held);
            self.assemble(&x, &predicted, inertia, &mut assembly);
            let negative: Vec<f64> = assembly.gradient.iter().map(|g| -g).collect();
            let direction = assembly.factor()?.solve(&negative);
            update = direction
                .chunks_exact(3)
                .map(|d| d[0].hypot(d[1]).hypot(d[2]))
                .fold(0.0, f64::max);
            if update <= self.settings.tolerance {
                converged = true;
                break;
            }
            let slope: f64 = assembly
                .gradient
                .iter()
                .zip(&direction)
                .map(|(g, d)| g * d)
                .sum();
            let search = Search {
                predicted: &predicted,
                inertia,
                energy: total(&energy),
                slope,
            };
            match self.line_search(&x, &direction, &search) {
                Some((next, next_energy)) => {
                    x = next;
                    energy = next_energy;
                }
                None => break,
            }
        }
        for (i, point) in x.iter().enumerate() {
            let velocity = if dt.is_finite() {
                (point - start[i]) / dt
            } else {
                Vector3::zeros()
            };
            self.velocities[i] = velocity.into();
            self.positions[i] = (*point).into();
        }
        if dt.is_finite() {
            self.time += dt;
        }
        energy.kinetic = self.kinetic();
        Ok(Report {
            converged,
            iterations,
            update,
            energy,
            greatest_strain: self.greatest_strain(&x),
            time: self.time,
        })
    }

    /// The first point along a Newton update that stays below the strain cap and lowers the
    /// energy enough, with its energy, or `None` when no fraction of the update does.
    fn line_search(
        &self,
        x: &[Point],
        direction: &[f64],
        search: &Search,
    ) -> Option<(Vec<Point>, Energy)> {
        let scale = 1e-12 * size(&self.energy_at(x, search.predicted, search.inertia));
        let accepts = |energy: &Energy, fraction: f64| {
            let change = total(energy) - search.energy;
            change <= self.settings.sufficient_decrease * fraction * search.slope
                || change.abs() <= scale
        };
        let mut fraction = 1.0;
        while fraction >= self.settings.shortest_update {
            let trial = moved(x, direction, fraction);
            let trial_energy = self.energy_at(&trial, search.predicted, search.inertia);
            if accepts(&trial_energy, fraction) {
                return Some((trial, trial_energy));
            }
            fraction *= 0.5;
        }
        None
    }

    /// The energy's terms at positions `x`, with the inertia term in `kinetic`; the strain
    /// barrier is infinite past the strain cap.
    fn energy_at(&self, x: &[Point], predicted: &[Point], inertia: f64) -> Energy {
        let gravity = Point::from(self.settings.gravity);
        let mut energy = Energy::default();
        for (s, sheet) in self.sheets.iter().enumerate() {
            let offset = self.offsets[s];
            let constants = Constants::of(sheet);
            let x = &x[offset..offset + sheet.mesh().vertex_count()];
            let predicted = &predicted[offset..offset + sheet.mesh().vertex_count()];
            for (v, area) in sheet.areas().iter().enumerate() {
                let mass = constants.mass_per_area * area;
                energy.gravity -= mass * gravity.dot(&x[v]);
                if !self.held[offset + v] {
                    energy.kinetic += 0.5 * mass * inertia * (x[v] - predicted[v]).norm_squared();
                }
            }
            let mesh = sheet.mesh();
            let stretching: Vec<(f64, f64)> = mesh
                .triangles()
                .par_iter()
                .zip(sheet.rest_triangles())
                .map(|(t, rest)| {
                    let points = t.map(|v| x[v as usize]);
                    let elastic = membrane::elastic_energy(rest, &points, &constants.stretching);
                    let total = membrane::energy(rest, &points, &constants.stretching);
                    (
                        elastic,
                        total.map_or(f64::INFINITY, |total| total - elastic),
                    )
                })
                .collect();
            for (elastic, barrier) in stretching {
                energy.stretching += elastic;
                energy.strain_barrier += barrier;
            }
            let angles: Vec<f64> = mesh
                .dihedrals()
                .par_iter()
                .map(|d| bending::angle_value(&stencil(d).map(|v| x[v as usize])))
                .collect();
            for (t, plate) in sheet.plates().iter().enumerate() {
                let sines = sheet.stencils()[t].map(|edge| {
                    let (d, _) = edge?;
                    let d = d as usize;
                    sheet.creases()[d]
                        .is_none()
                        .then(|| (0.5 * angles[d]).sin())
                });
                let matrix = plate.energy_matrix(&constants.bending);
                energy.bending += bending::plate(&matrix, sines, [0.0; 3]).0;
            }
            for (d, crease) in sheet.creases().iter().enumerate() {
                if let Some(crease) = crease {
                    let [value, _, _] = bending::crease(
                        angles[d],
                        sheet.lengths()[d],
                        crease.stiffness,
                        crease.rest_angle,
                    );
                    energy.creases += value;
                }
            }
        }
        energy
    }

    /// Fills the gradient and projected Hessian of the incremental potential at positions `x`.
    fn assemble(&self, x: &[Point], predicted: &[Point], inertia: f64, assembly: &mut Assembly) {
        let gravity = Point::from(self.settings.gravity);
        for (s, sheet) in self.sheets.iter().enumerate() {
            let offset = self.offsets[s];
            let global =
                |v: u32| u32::try_from(offset).expect("a scene counts its vertices in 32 bits") + v;
            let constants = Constants::of(sheet);
            let mesh = sheet.mesh();
            let x = &x[offset..offset + mesh.vertex_count()];
            let predicted = &predicted[offset..offset + mesh.vertex_count()];
            for (v, area) in sheet.areas().iter().enumerate() {
                let mass = constants.mass_per_area * area;
                let force = (x[v] - predicted[v]) * (mass * inertia) - gravity * mass;
                let vertex = u32::try_from(v).expect("a mesh counts its vertices in 32 bits");
                assembly.add_vertex(global(vertex), force.into(), mass * inertia);
            }
            let stretching: Vec<_> = mesh
                .triangles()
                .par_iter()
                .zip(sheet.rest_triangles())
                .map(|(t, rest)| {
                    let points = t.map(|v| x[v as usize]);
                    let local = membrane::local(rest, &points, &constants.stretching)
                        .expect("the simulation keeps every state below the strain cap");
                    (local.gradient, project(&local.hessian))
                })
                .collect();
            for (t, (gradient, hessian)) in mesh.triangles().iter().zip(&stretching) {
                assembly.add(&t.map(|v| Some(global(v))), gradient, hessian);
            }
            let angles: Vec<(Angle, Angle)> = mesh
                .dihedrals()
                .par_iter()
                .map(|d| {
                    let angle = bending::angle(&stencil(d).map(|v| x[v as usize]));
                    let sine = bending::half_sine(&angle);
                    (angle, sine)
                })
                .collect();
            let plates: Vec<_> = (0..mesh.triangles().len())
                .into_par_iter()
                .map(|t| {
                    let edges = sheet.stencils()[t].map(|edge| {
                        let (d, slots) = edge?;
                        let d = d as usize;
                        let (_, sine) = &angles[d];
                        sheet.creases()[d].is_none().then_some(BendingEdge {
                            sine: sine.value,
                            gradient: &sine.gradient,
                            slots,
                        })
                    });
                    let matrix = sheet.plates()[t].energy_matrix(&constants.bending);
                    bending::triangle(&matrix, edges, [0.0; 3])
                })
                .collect();
            let mut moments = vec![0.0; mesh.dihedrals().len()];
            for (t, (local, moment)) in plates.iter().enumerate() {
                let stencil = sheet.bending_stencil(t).map(|v| v.map(&global));
                assembly.add(&stencil, &local.gradient, &local.hessian);
                for (i, edge) in sheet.stencils()[t].iter().enumerate() {
                    if let Some((d, _)) = edge {
                        moments[*d as usize] += moment[i];
                    }
                }
            }
            let dihedrals: Vec<_> = mesh
                .dihedrals()
                .par_iter()
                .enumerate()
                .map(|(d, _)| {
                    let (angle, sine) = &angles[d];
                    let mut matrix = sine.hessian * moments[d];
                    let mut gradient = SMatrix::<f64, 12, 1>::zeros();
                    if let Some(crease) = &sheet.creases()[d] {
                        let [_, slope, curvature] = bending::crease(
                            angle.value,
                            sheet.lengths()[d],
                            crease.stiffness,
                            crease.rest_angle,
                        );
                        gradient += angle.gradient * slope;
                        matrix += angle.gradient * angle.gradient.transpose() * curvature
                            + angle.hessian * slope;
                    }
                    (gradient, project(&matrix))
                })
                .collect();
            for (d, (gradient, hessian)) in mesh.dihedrals().iter().zip(&dihedrals) {
                assembly.add(&stencil(d).map(|v| Some(global(v))), gradient, hessian);
            }
        }
    }

    /// The greatest norm of any triangle's Green strain.
    fn greatest_strain(&self, x: &[Point]) -> f64 {
        let mut greatest: f64 = 0.0;
        for (s, sheet) in self.sheets.iter().enumerate() {
            let x = &x[self.offsets[s]..];
            for (t, rest) in sheet.mesh().triangles().iter().zip(sheet.rest_triangles()) {
                greatest = greatest.max(membrane::strain_norm(rest, &t.map(|v| x[v as usize])));
            }
        }
        greatest
    }

    /// The kinetic energy of the free vertices.
    fn kinetic(&self) -> f64 {
        let mut kinetic = 0.0;
        for (s, sheet) in self.sheets.iter().enumerate() {
            let offset = self.offsets[s];
            for (v, area) in sheet.areas().iter().enumerate() {
                let velocity = Point::from(self.velocities[offset + v]);
                kinetic += 0.5 * sheet.material().mass_per_area * area * velocity.norm_squared();
            }
        }
        kinetic
    }

    /// The global indices of a sheet's vertices.
    fn range(&self, sheet: usize) -> std::ops::Range<usize> {
        let start = self.offsets[sheet];
        start..start + self.sheets[sheet].mesh().vertex_count()
    }
}

/// What a line search needs from its Newton iteration.
struct Search<'a> {
    predicted: &'a [Point],
    inertia: f64,
    energy: f64,
    slope: f64,
}

/// Points moved by a fraction of a displacement given in flat coordinates.
fn moved(x: &[Point], displacement: &[f64], fraction: f64) -> Vec<Point> {
    x.iter()
        .enumerate()
        .map(|(i, p)| p + Vector3::from_column_slice(&displacement[3 * i..3 * i + 3]) * fraction)
        .collect()
}

/// The size of the energy's terms, for telling a change from rounding.
fn size(energy: &Energy) -> f64 {
    energy.stretching.abs()
        + energy.strain_barrier.abs()
        + energy.bending.abs()
        + energy.creases.abs()
        + energy.gravity.abs()
        + energy.kinetic.abs()
}

/// The sum of the energy's terms.
fn total(energy: &Energy) -> f64 {
    energy.stretching
        + energy.strain_barrier
        + energy.bending
        + energy.creases
        + energy.gravity
        + energy.kinetic
}

/// The stencil `[u, v, a, b]` of a dihedral.
fn stencil(dihedral: &crate::mesh::Dihedral) -> [u32; 4] {
    let ([u, v], [a, b]) = (dihedral.edge, dihedral.opposite);
    [u, v, a, b]
}

/// Checks that a sheet's vertices and the values given for them are as many.
fn check_count(expected: usize, given: usize) -> Result<(), StateError> {
    if expected == given {
        Ok(())
    } else {
        Err(StateError::Count { expected, given })
    }
}

/// Every pair of vertices some element of the sheets joins.
fn pairs(sheets: &[Sheet], offsets: &[usize]) -> Vec<(u32, u32)> {
    let mut pairs = Vec::new();
    for (sheet, &offset) in sheets.iter().zip(offsets) {
        let offset = u32::try_from(offset).expect("a scene counts its vertices in 32 bits");
        for t in 0..sheet.mesh().triangles().len() {
            let stencil: Vec<u32> = sheet
                .bending_stencil(t)
                .iter()
                .flatten()
                .map(|v| v + offset)
                .collect();
            for (i, &a) in stencil.iter().enumerate() {
                for &b in &stencil[..i] {
                    pairs.push((a, b));
                }
            }
        }
    }
    pairs
}
