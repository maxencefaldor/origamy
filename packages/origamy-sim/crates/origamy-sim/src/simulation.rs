//! A simulation: sheets in space, stepped through time by minimizing the incremental potential.
//!
//! Each time step finds the positions x that minimize
//!
//! ½ (x − x̂)ᵀ M (x − x̂) / Δt² + E(x),  with x̂ = x₀ + Δt v₀,
//!
//! which is one step of implicit Euler: inertia, elasticity and gravity balance at the minimum.
//! An infinite time step drops inertia, so the step finds the rest the forces lead to.

use std::cell::OnceCell;

use nalgebra::{SMatrix, Vector3};
use rayon::prelude::*;

use crate::contact::{self, Contact, Kind, Pair, Primitives, ccd, distance};
use crate::energy::bending::{self, Angle, BendingEdge};
use crate::energy::membrane::{self, Stretching};
use crate::energy::{Barrier, Point, project};
use crate::material::{ParameterError, finite, positive};
use crate::sheet::Sheet;
use crate::system::{Assembly, Factor, Pattern, SingularError};

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
    /// The gap beyond a paper thickness at which contact starts to push back, in meters.
    pub contact_gap: f64,
    /// The energy scale of contact's barrier, in joules: the larger, the less contact yields.
    pub contact_stiffness: f64,
    /// The height of the table's top, in meters; minus infinity for no table.
    pub table: f64,
}

impl Default for Settings {
    /// Earth's gravity along −z, time steps of a sixtieth of a second, a tolerance of 10 nm,
    /// contact that starts 2 µm beyond the thickness, and no table.
    fn default() -> Self {
        Self {
            gravity: [0.0, 0.0, -9.81],
            time_step: 1.0 / 60.0,
            tolerance: 1e-8,
            iteration_limit: 200,
            sufficient_decrease: 1e-4,
            shortest_update: 1e-12,
            contact_gap: 2e-6,
            contact_stiffness: 1e-8,
            table: f64::NEG_INFINITY,
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
        positive("contact gap", self.contact_gap)?;
        positive("contact stiffness", self.contact_stiffness)?;
        if self.table.is_nan() || self.table == f64::INFINITY {
            return Err(ParameterError {
                name: "table",
                value: self.table,
                requirement: "finite, or minus infinity for no table",
            });
        }
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
    /// The energy of contact's barrier, between sheets and with the table.
    pub contact: f64,
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
    /// The pairs of primitives, and the vertices on the table, that contact pushes apart.
    pub contacts: usize,
    /// The least gap beyond the thickness between two pieces of paper, or between paper and the
    /// table, within the reach of contact, in meters; infinite where nothing is that close.
    pub least_gap: f64,
    /// The simulated time after the step, in seconds.
    pub time: f64,
}

/// Why a time step cannot be taken.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum StepError {
    /// Some motion costs no energy, so the step has no unique result.
    #[error(transparent)]
    Singular(#[from] SingularError),
    /// Paper starts closer than its thickness to itself or to the table.
    #[error("the paper lies closer than its thickness to itself or to the table")]
    Overlapping,
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
    /// The positions bring paper closer than its thickness to itself or to the table.
    #[error("the positions bring paper closer than its thickness to itself or to the table")]
    Overlapping,
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
    pairs: Vec<(u32, u32)>,
    primitives: Primitives,
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
        let pairs = pairs(&sheets, &offsets);
        let pattern = Pattern::new(positions.len(), pairs.iter().copied());
        let primitives = primitives(&sheets, &offsets);
        Ok(Self {
            velocities: vec![[0.0; 3]; positions.len()],
            held: vec![false; positions.len()],
            sheets,
            offsets,
            settings,
            positions,
            time: 0.0,
            pattern,
            pairs,
            primitives,
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
        let mut x: Vec<Point> = self.positions.iter().map(|&p| Point::from(p)).collect();
        for (i, p) in positions.iter().enumerate() {
            x[range.start + i] = Point::from(*p);
        }
        let moved = |v: u32| range.contains(&(v as usize));
        let pairs: Vec<Pair> = self
            .candidates(&x, &x)
            .into_iter()
            .filter(|pair| pair.vertices.iter().any(|&v| moved(v)))
            .collect();
        let contact = self.contact();
        let overlapping = pairs.iter().any(|pair| {
            let squared = distance::squared(pair.kind, &pair.at(&x));
            contact.pair(squared, self.pair_thickness(pair)).is_none()
        }) || range.clone().any(|v| {
            let vertex = u32::try_from(v).expect("a scene counts its vertices in 32 bits");
            contact.table(x[v][2], self.thickness(vertex)).is_none()
        });
        if overlapping {
            return Err(StateError::Overlapping);
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
        let mut energy = self.energy_at(&x, &x, 0.0, &self.candidates(&x, &x));
        energy.kinetic = self.kinetic();
        energy
    }

    /// Advances the simulation by one time step.
    ///
    /// # Errors
    ///
    /// Returns a [`StepError`] when some motion costs no energy, so the step has no unique
    /// result, or when paper starts closer than its thickness to itself or to the table; the
    /// state stays as it was.
    pub fn step(&mut self) -> Result<Report, StepError> {
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
        let mut energy = self.energy_at(&x, &predicted, inertia, &self.candidates(&x, &x));
        if !total(&energy).is_finite() {
            return Err(StepError::Overlapping);
        }
        // The retraction's system is factorized once a time step needs it.
        let rigidity = OnceCell::new();
        // Contact joins vertices no element joins, so the system's pattern grows to hold them.
        let mut grown: Option<Pattern> = None;
        let (mut converged, mut iterations, mut update) = (false, 0, f64::INFINITY);
        while iterations < self.settings.iteration_limit {
            iterations += 1;
            let near = self.candidates(&x, &x);
            self.grow(&mut grown, &near);
            let pattern = grown.as_ref().unwrap_or(&self.pattern);
            // The exact Hessian gives Newton's quadratic convergence wherever it is positive
            // definite; elsewhere each element's Hessian is projected to be.
            let mut assembly = Assembly::new(pattern, &self.held);
            self.assemble(&x, &predicted, inertia, &mut assembly, false, &near);
            let factor = if let Ok(factor) = assembly.factor() {
                factor
            } else {
                assembly = Assembly::new(pattern, &self.held);
                self.assemble(&x, &predicted, inertia, &mut assembly, true, &near);
                assembly.factor().map_err(StepError::Singular)?
            };
            let negative: Vec<f64> = assembly.gradient.iter().map(|g| -g).collect();
            let direction = factor.solve(&negative);
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
            let displacement = moved(&vec![Point::zeros(); x.len()], &direction, 1.0);
            let (safe, swept) = self.safe_fraction(&x, &displacement);
            let search = Search {
                predicted: &predicted,
                inertia,
                rigidity: &rigidity,
                energy: total(&energy),
                slope,
                safe,
                pairs: &swept,
            };
            match self.line_search(&x, &direction, &search) {
                Some((next, next_energy)) => {
                    x = next;
                    energy = next_energy;
                }
                None => break,
            }
        }
        Ok(self.finish(&start, &x, energy, converged, iterations, update))
    }

    /// Grows the system's pattern, where contacts `near` join vertices it does not hold.
    fn grow(&self, grown: &mut Option<Pattern>, near: &[Pair]) {
        let pattern = grown.as_ref().unwrap_or(&self.pattern);
        if !near.iter().all(|pair| joined(pattern, pair)) {
            let contacts = near.iter().flat_map(vertex_pairs);
            *grown = Some(Pattern::new(
                self.positions.len(),
                self.pairs.iter().copied().chain(contacts),
            ));
        }
    }

    /// Ends a time step at positions `x`, from `start`, and reports it.
    fn finish(
        &mut self,
        start: &[Point],
        x: &[Point],
        mut energy: Energy,
        converged: bool,
        iterations: usize,
        update: f64,
    ) -> Report {
        let dt = self.settings.time_step;
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
        let (contacts, least_gap) = self.contacts(x);
        Report {
            converged,
            iterations,
            update,
            energy,
            greatest_strain: self.greatest_strain(x),
            contacts,
            least_gap,
            time: self.time,
        }
    }

    /// The first point along a Newton update that keeps every pair its thickness apart, stays
    /// below the strain cap and lowers the energy enough, with its energy, or `None` when no
    /// fraction of the update does.
    ///
    /// The search starts from the longest fraction that collision detection allows. A Newton
    /// update moves vertices along straight lines, so turning paper stretches it. Where a
    /// fraction of the update fails, its retraction, refitted to the update's rotations without
    /// stretch, is tried before a shorter fraction, if no pair meets on the way there.
    fn line_search<'a>(
        &'a self,
        x: &[Point],
        direction: &[f64],
        search: &Search<'a, '_>,
    ) -> Option<(Vec<Point>, Energy)> {
        let scale =
            1e-12 * size(&self.energy_at(x, search.predicted, search.inertia, search.pairs));
        let accepts = |energy: &Energy, fraction: f64| {
            let change = total(energy) - search.energy;
            change <= self.settings.sufficient_decrease * fraction * search.slope
                || change.abs() <= scale
        };
        let mut fraction = search.safe;
        while fraction >= self.settings.shortest_update {
            let trial = moved(x, direction, fraction);
            let trial_energy =
                self.energy_at(&trial, search.predicted, search.inertia, search.pairs);
            if accepts(&trial_energy, fraction) {
                return Some((trial, trial_energy));
            }
            if let Ok(rigidity) = search.rigidity.get_or_init(|| self.rigidity()) {
                let retracted = self.retract(&trial, rigidity);
                let way: Vec<Point> = retracted.iter().zip(x).map(|(r, p)| r - p).collect();
                let (safe, pairs) = self.safe_fraction(x, &way);
                if safe >= 1.0 {
                    let retracted_energy =
                        self.energy_at(&retracted, search.predicted, search.inertia, &pairs);
                    if accepts(&retracted_energy, fraction) {
                        return Some((retracted, retracted_energy));
                    }
                }
            }
            fraction *= 0.5;
        }
        None
    }

    /// The factorized system of the as-rigid-as-possible fit: Σ Ā |F − R|² over triangles,
    /// with F a triangle's deformation gradient and R its nearest rotation; held vertices stay.
    fn rigidity(&self) -> Result<Factor<'_>, SingularError> {
        let mut assembly = Assembly::new(&self.pattern, &self.held);
        for (s, sheet) in self.sheets.iter().enumerate() {
            let offset =
                u32::try_from(self.offsets[s]).expect("a scene counts its vertices in 32 bits");
            for (t, rest) in sheet.mesh().triangles().iter().zip(sheet.rest_triangles()) {
                let mut hessian = SMatrix::<f64, 9, 9>::zeros();
                for (a, ga) in rest.shape.iter().enumerate() {
                    for (b, gb) in rest.shape.iter().enumerate() {
                        let weight = rest.area * ga.dot(gb);
                        for c in 0..3 {
                            hessian[(3 * a + c, 3 * b + c)] = weight;
                        }
                    }
                }
                assembly.add(&t.map(|v| Some(v + offset)), &SMatrix::zeros(), &hessian);
            }
        }
        // A faint pull toward the positions being fitted fixes the fit's free translation.
        let anchor = 1e-9;
        for vertex in 0..self.positions.len() {
            let vertex = u32::try_from(vertex).expect("a scene counts its vertices in 32 bits");
            assembly.add_vertex(vertex, [0.0; 3], anchor);
        }
        assembly.factor()
    }

    /// Positions refitted, as rigid as possible, to the rotations of their triangles: the
    /// positions nearest to turning each triangle as it turns in `x`, without stretch.
    fn retract(&self, x: &[Point], rigidity: &Factor) -> Vec<Point> {
        let mut pull = vec![0.0; 3 * x.len()];
        for (s, sheet) in self.sheets.iter().enumerate() {
            let offset = self.offsets[s];
            for (t, rest) in sheet.mesh().triangles().iter().zip(sheet.rest_triangles()) {
                let corners = t.map(|v| x[offset + v as usize]);
                let Some(gap) = membrane::rotation_gap(rest, &corners) else {
                    continue;
                };
                for (a, &v) in t.iter().enumerate() {
                    let force = gap * rest.shape[a] * rest.area;
                    let index = 3 * (offset + v as usize);
                    for c in 0..3 {
                        pull[index + c] += force[c];
                    }
                }
            }
        }
        for (v, &held) in self.held.iter().enumerate() {
            if held {
                pull[3 * v..3 * v + 3].fill(0.0);
            }
        }
        moved(x, &rigidity.solve(&pull), 1.0)
    }

    /// The energy's terms at positions `x`, with the inertia term in `kinetic`; the strain
    /// barrier is infinite past the strain cap.
    fn energy_at(&self, x: &[Point], predicted: &[Point], inertia: f64, pairs: &[Pair]) -> Energy {
        let gravity = Point::from(self.settings.gravity);
        let mut energy = Energy::default();
        let contact = self.contact();
        let barriers: Vec<f64> = pairs
            .par_iter()
            .map(|pair| {
                let squared = distance::squared(pair.kind, &pair.at(x));
                contact
                    .pair(squared, self.pair_thickness(pair))
                    .map_or(f64::INFINITY, |[value, _, _]| value)
            })
            .collect();
        energy.contact = barriers.iter().sum();
        for (v, p) in x.iter().enumerate() {
            let vertex = u32::try_from(v).expect("a scene counts its vertices in 32 bits");
            energy.contact += contact
                .table(p[2], self.thickness(vertex))
                .map_or(f64::INFINITY, |[value, _, _]| value);
        }
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

    /// Fills the gradient and Hessian of contact's barriers at positions `x`.
    fn assemble_contact(
        &self,
        x: &[Point],
        assembly: &mut Assembly,
        projected: bool,
        pairs: &[Pair],
    ) {
        let contact = self.contact();
        let barriers: Vec<_> = pairs
            .par_iter()
            .filter_map(|pair| {
                let (gradient, hessian) =
                    contact.pair_derivatives(pair, x, self.pair_thickness(pair))?;
                Some((pair.stencil(), gradient, projected_if(&hessian, projected)))
            })
            .collect();
        for (stencil, gradient, hessian) in &barriers {
            assembly.add(stencil, gradient, hessian);
        }
        for (v, p) in x.iter().enumerate() {
            let vertex = u32::try_from(v).expect("a scene counts its vertices in 32 bits");
            if let Some([_, slope, curvature]) = contact.table(p[2], self.thickness(vertex)) {
                if slope != 0.0 || curvature != 0.0 {
                    assembly.add_vertex_z(vertex, slope, curvature);
                }
            }
        }
    }

    /// Fills the gradient and Hessian of the incremental potential at positions `x`.
    /// With `projected`, each element's Hessian is projected to be positive semi-definite.
    fn assemble(
        &self,
        x: &[Point],
        predicted: &[Point],
        inertia: f64,
        assembly: &mut Assembly,
        projected: bool,
        pairs: &[Pair],
    ) {
        self.assemble_contact(x, assembly, projected, pairs);
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
                    (local.gradient, projected_if(&local.hessian, projected))
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
                    (gradient, projected_if(&matrix, projected))
                })
                .collect();
            for (d, (gradient, hessian)) in mesh.dihedrals().iter().zip(&dihedrals) {
                assembly.add(&stencil(d).map(|v| Some(global(v))), gradient, hessian);
            }
        }
    }

    /// The contact barrier's parameters.
    fn contact(&self) -> Contact {
        Contact {
            gap: self.settings.contact_gap,
            stiffness: self.settings.contact_stiffness,
            table: self.settings.table,
        }
    }

    /// The thickness of a vertex's sheet, in meters.
    fn thickness(&self, vertex: u32) -> f64 {
        self.sheets[self.primitives.sheet[vertex as usize] as usize]
            .material()
            .thickness
    }

    /// The thickness a pair keeps between mid-surfaces: the mean of its two sheets'.
    fn pair_thickness(&self, pair: &Pair) -> f64 {
        let [a, b, c, _] = pair.vertices;
        match pair.kind {
            Kind::PointTriangle => 0.5 * (self.thickness(a) + self.thickness(b)),
            Kind::EdgeEdge => 0.5 * (self.thickness(a) + self.thickness(c)),
        }
    }

    /// How far contact reaches: the greatest thickness and the gap beyond it.
    fn reach(&self) -> f64 {
        let thickest = self
            .sheets
            .iter()
            .map(|s| s.material().thickness)
            .fold(0.0, f64::max);
        thickest + self.settings.contact_gap
    }

    /// The pairs that may touch while the vertices move from `start` to `end`.
    fn candidates(&self, start: &[Point], end: &[Point]) -> Vec<Pair> {
        contact::candidates(&self.primitives, start, end, self.reach())
    }

    /// The largest fraction, at most 1, of a motion from `x` that keeps every pair its
    /// thickness apart and every vertex its half thickness above the table, and the pairs that
    /// may touch along the motion.
    fn safe_fraction(&self, x: &[Point], displacement: &[Point]) -> (f64, Vec<Pair>) {
        let end: Vec<Point> = x.iter().zip(displacement).map(|(p, d)| p + d).collect();
        let pairs = self.candidates(x, &end);
        let mut fraction: f64 = pairs
            .par_iter()
            .map(|pair| ccd::pair(pair, x, displacement, self.pair_thickness(pair)))
            .collect::<Vec<f64>>()
            .into_iter()
            .fold(1.0, f64::min);
        if self.settings.table > f64::NEG_INFINITY {
            for (v, (p, d)) in x.iter().zip(displacement).enumerate() {
                let vertex = u32::try_from(v).expect("a scene counts its vertices in 32 bits");
                let half = 0.5 * self.thickness(vertex);
                fraction = fraction.min(ccd::table(p[2] - self.settings.table, -d[2], half));
            }
        }
        (fraction, pairs)
    }

    /// The pairs and table contacts that contact pushes apart at positions `x`, and the least
    /// gap within contact's reach.
    fn contacts(&self, x: &[Point]) -> (usize, f64) {
        let contact = self.contact();
        let mut count = 0;
        let mut least = f64::INFINITY;
        for pair in self.candidates(x, x) {
            let h = self.pair_thickness(&pair);
            let gap = distance::squared(pair.kind, &pair.at(x)).sqrt() - h;
            if gap < contact.gap {
                count += 1;
                least = least.min(gap);
            }
        }
        if contact.table > f64::NEG_INFINITY {
            for (v, p) in x.iter().enumerate() {
                let vertex = u32::try_from(v).expect("a scene counts its vertices in 32 bits");
                let gap = p[2] - contact.table - 0.5 * self.thickness(vertex);
                if gap < contact.gap {
                    count += 1;
                    least = least.min(gap);
                }
            }
        }
        (count, least)
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
struct Search<'a, 'b> {
    predicted: &'b [Point],
    inertia: f64,
    rigidity: &'a OnceCell<Result<Factor<'a>, SingularError>>,
    energy: f64,
    slope: f64,
    /// The longest fraction of the update that collision detection allows.
    safe: f64,
    /// The pairs that may touch along the update.
    pairs: &'b [Pair],
}

/// Whether a pattern holds every pair of a contact's vertices.
fn joined(pattern: &Pattern, pair: &Pair) -> bool {
    vertex_pairs(pair).all(|(a, b)| pattern.contains(a, b))
}

/// Every pair of a contact's vertices.
fn vertex_pairs(pair: &Pair) -> impl Iterator<Item = (u32, u32)> + '_ {
    let v = pair.vertices;
    (0..4).flat_map(move |i| (0..i).map(move |j| (v[i], v[j])))
}

/// A Hessian projected to be positive semi-definite when `projected` holds.
fn projected_if<const D: usize>(
    matrix: &SMatrix<f64, D, D>,
    projected: bool,
) -> SMatrix<f64, D, D> {
    if projected { project(matrix) } else { *matrix }
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

/// Every triangle and edge of the sheets, in the scene's vertex numbering.
fn primitives(sheets: &[Sheet], offsets: &[usize]) -> Primitives {
    let mut primitives = Primitives {
        triangles: Vec::new(),
        edges: Vec::new(),
        sheet: Vec::new(),
        rest: Vec::new(),
    };
    for (s, (sheet, &offset)) in sheets.iter().zip(offsets).enumerate() {
        let offset = u32::try_from(offset).expect("a scene counts its vertices in 32 bits");
        let index = u32::try_from(s).expect("a scene counts its sheets in 32 bits");
        let mesh = sheet.mesh();
        primitives
            .triangles
            .extend(mesh.triangles().iter().map(|t| t.map(|v| v + offset)));
        primitives
            .edges
            .extend(mesh.edges().iter().map(|e| e.map(|v| v + offset)));
        primitives
            .sheet
            .extend(std::iter::repeat_n(index, mesh.vertex_count()));
        primitives
            .rest
            .extend(mesh.rest().iter().map(|&[x, y]| Point::new(x, y, 0.0)));
    }
    primitives
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
