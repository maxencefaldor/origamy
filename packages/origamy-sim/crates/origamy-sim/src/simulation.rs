//! A simulation: sheets in space, stepped through time by minimizing the incremental potential.
//!
//! Each time step finds the positions x that minimize the incremental potential
//!
//! ½ (x − x̂)ᵀ M (x − x̂) / Δt² + E(x),  with x̂ = x₀ + Δt v₀.
//!
//! The minimization is one step of implicit Euler: inertia, elasticity and gravity balance at
//! the minimum. An infinite time step drops inertia, so the step finds the rest the forces
//! lead to.

use std::cell::OnceCell;

use nalgebra::{SMatrix, Vector3};
use rayon::prelude::*;

use crate::contact::{self, Kind, Pair, Primitives, Trees, ccd, distance};
use crate::energy::bending::{self, Angle, BendingEdge};
use crate::energy::membrane::{self, Stretching};
use crate::energy::{Point, Project};
use crate::parameter::ParameterError;
use crate::settings::Settings;
use crate::sheet::Sheet;
use crate::system::{Assembly, Factor, Pattern, SingularError};

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

impl Energy {
    /// The sum of every term, in joules.
    #[must_use]
    pub fn total(&self) -> f64 {
        self.terms().into_iter().sum()
    }

    /// Every term, in a fixed order.
    fn terms(&self) -> [f64; 7] {
        // The destructuring names every field, so a new term cannot be left out of the total.
        let Self {
            stretching,
            strain_barrier,
            bending,
            creases,
            contact,
            gravity,
            kinetic,
        } = *self;
        [
            stretching,
            strain_barrier,
            bending,
            creases,
            contact,
            gravity,
            kinetic,
        ]
    }
}

/// The incremental potential of a time step at some positions, term by term, in joules.
#[derive(Clone, Copy, Debug, Default)]
struct Potential {
    /// The terms of the potential energy; the kinetic term stays zero.
    energy: Energy,
    /// The inertia term ½ (x − x̂)ᵀ M (x − x̂) / Δt².
    inertia: f64,
}

impl Potential {
    /// The incremental potential.
    fn total(&self) -> f64 {
        self.energy.total() + self.inertia
    }

    /// The size of the terms, for telling a change from rounding.
    fn size(&self) -> f64 {
        self.energy.terms().iter().map(|t| t.abs()).sum::<f64>() + self.inertia.abs()
    }
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
    /// The least gap beyond the thickness within the reach of contact, in meters.
    ///
    /// The gap lies between two pieces of paper, or between paper and the table. The gap is
    /// infinite where nothing is that close.
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
    /// Hierarchies over the primitives, built at the start of each time step.
    trees: Trees,
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
        Self {
            stretching: Stretching::of(material),
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
        let rest: Vec<Point> = positions.iter().map(|&p| Point::from(p)).collect();
        let trees = Trees::new(&primitives, &rest);
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
            trees,
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
    /// Returns a [`StateError`] when the positions do not match the sheet's vertices. The error
    /// also names a coordinate that is not finite, a triangle stretched past the strain cap,
    /// and paper brought closer than its thickness.
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
        let contact = self.settings.contact;
        let overlapping = pairs.iter().any(|pair| {
            let squared = distance::squared(pair.kind, &pair.at(&x));
            contact.pair(squared, self.pair_thickness(pair)).is_none()
        }) || range
            .clone()
            .any(|v| self.clearance(v, &x[v]).is_some_and(|c| c <= 0.0));
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

    /// The dihedral angle along each interior edge of a sheet, in radians.
    ///
    /// The angles follow the order of the mesh's dihedrals. An angle is zero where the sheet
    /// lies flat, and positive where it folds toward its front.
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
        let mut energy = self.potential(&x, &x, 0.0, &self.candidates(&x, &x)).energy;
        energy.kinetic = self.kinetic();
        energy
    }

    /// Advances the simulation by one time step.
    ///
    /// # Errors
    ///
    /// Returns a [`StepError`] when some motion costs no energy, so the step has no unique
    /// result. The step also refuses paper that starts closer than its thickness to itself or
    /// to the table. A refused step leaves the state as it was.
    pub fn step(&mut self) -> Result<Report, StepError> {
        let dt = self.settings.time_step;
        let start: Vec<Point> = self.positions.iter().map(|&p| Point::from(p)).collect();
        self.trees = Trees::new(&self.primitives, &start);
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
        let mut potential = self.potential(&x, &predicted, inertia, &self.candidates(&x, &x));
        if !potential.total().is_finite() {
            return Err(StepError::Overlapping);
        }
        // The retraction's system is factorized once a time step needs it.
        let rigidity = OnceCell::new();
        // Contact joins vertices no element joins, so the system's pattern grows to hold them,
        // with the vertex pairs it grew by.
        let mut grown: Option<(Pattern, Vec<(u32, u32)>)> = None;
        let (mut converged, mut iterations, mut update) = (false, 0, f64::INFINITY);
        let mut radius = self.settings.solver.trust_radius;
        while iterations < self.settings.solver.iteration_limit {
            iterations += 1;
            let near = self.candidates(&x, &x);
            let derivatives = self.derivatives(&x, &predicted, inertia, &near);
            self.grow(&mut grown, &derivatives.pairs);
            let pattern = grown.as_ref().map_or(&self.pattern, |(p, _)| p);
            // The exact Hessian gives Newton's quadratic convergence wherever it is positive
            // definite; elsewhere each element's Hessian is projected to be.
            let mut assembly = Assembly::new(pattern, &self.held);
            derivatives.scatter(&mut assembly, false);
            let factor = if let Ok(factor) = assembly.factor() {
                factor
            } else {
                assembly = Assembly::new(pattern, &self.held);
                derivatives.scatter(&mut assembly, true);
                assembly.factor().map_err(StepError::Singular)?
            };
            let negative: Vec<f64> = assembly.gradient.iter().map(|g| -g).collect();
            let mut direction = factor.solve(&negative);
            update = direction
                .chunks_exact(3)
                .map(|d| d[0].hypot(d[1]).hypot(d[2]))
                .fold(0.0, f64::max);
            if update <= self.settings.solver.tolerance {
                converged = true;
                break;
            }
            // Where some motion costs nearly no energy, the update runs far past where its
            // quadratic model holds, so it shrinks to the trust radius.
            let clamped = update > radius;
            if clamped {
                let scale = radius / update;
                for d in &mut direction {
                    *d *= scale;
                }
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
                potential,
                slope,
                safe,
                pairs: &swept,
            };
            match self.line_search(&x, &direction, &search) {
                Some((next, next_potential, fraction)) => {
                    x = next;
                    potential = next_potential;
                    // An update the radius shrank, taken whole, says the model holds farther.
                    if clamped && fraction >= 1.0 {
                        radius *= 2.0;
                    }
                }
                None => break,
            }
        }
        Ok(self.finish(&start, &x, potential, converged, iterations, update))
    }

    /// Grows the system's pattern where active contacts join vertices it does not hold.
    ///
    /// The pattern only grows within a time step, so contacts that come and go do not rebuild
    /// it again and again.
    fn grow(&self, grown: &mut Option<(Pattern, Vec<(u32, u32)>)>, contacts: &[Element<4, 12>]) {
        let pattern = grown.as_ref().map_or(&self.pattern, |(p, _)| p);
        let missing: Vec<(u32, u32)> = contacts
            .iter()
            .flat_map(|(stencil, _, _)| {
                vertex_pairs(stencil.map(|v| v.expect("a pair names four vertices")))
            })
            .filter(|&(a, b)| !pattern.contains(a, b))
            .collect();
        if !missing.is_empty() {
            let mut joined = grown.take().map_or_else(Vec::new, |(_, joined)| joined);
            joined.extend(missing);
            joined.sort_unstable();
            joined.dedup();
            let pattern = Pattern::new(
                self.positions.len(),
                self.pairs.iter().chain(&joined).copied(),
            );
            *grown = Some((pattern, joined));
        }
    }

    /// Ends a time step at positions `x`, from `start`, and reports it.
    fn finish(
        &mut self,
        start: &[Point],
        x: &[Point],
        potential: Potential,
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
        let energy = Energy {
            kinetic: self.kinetic(),
            ..potential.energy
        };
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

    /// The first point along a Newton update that the line search accepts, with its potential
    /// and the fraction of the update it took.
    ///
    /// An accepted point keeps every pair its thickness apart, stays below the strain cap and
    /// lowers the energy enough. The search gives `None` when no fraction of the update does.
    /// The search starts from the longest fraction that collision detection allows. A Newton
    /// update moves vertices along straight lines, so turning paper stretches it. Where a
    /// fraction fails, its retraction refits it to the update's rotations without stretch. The
    /// search tries the retraction before a shorter fraction, if no pair meets on the way.
    fn line_search<'a>(
        &'a self,
        x: &[Point],
        direction: &[f64],
        search: &Search<'a, '_>,
    ) -> Option<(Vec<Point>, Potential, f64)> {
        let scale = 1e-12 * search.potential.size();
        let accepts = |potential: &Potential, fraction: f64| {
            let change = potential.total() - search.potential.total();
            change <= self.settings.solver.sufficient_decrease * fraction * search.slope
                || change.abs() <= scale
        };
        let mut fraction = search.safe;
        while fraction >= self.settings.solver.shortest_update {
            let trial = moved(x, direction, fraction);
            let trial_potential =
                self.potential(&trial, search.predicted, search.inertia, search.pairs);
            if accepts(&trial_potential, fraction) {
                return Some((trial, trial_potential, fraction));
            }
            if let Ok(rigidity) = search.rigidity.get_or_init(|| self.rigidity()) {
                let retracted = self.retract(&trial, rigidity);
                let way: Vec<Point> = retracted.iter().zip(x).map(|(r, p)| r - p).collect();
                let (safe, pairs) = self.safe_fraction(x, &way);
                if safe >= 1.0 {
                    let retracted_potential =
                        self.potential(&retracted, search.predicted, search.inertia, &pairs);
                    if accepts(&retracted_potential, fraction) {
                        return Some((retracted, retracted_potential, fraction));
                    }
                }
            }
            fraction *= self.settings.solver.backtracking;
        }
        None
    }

    /// The factorized system of the as-rigid-as-possible fit, which minimizes Σ Ā |F − R|².
    ///
    /// The sum runs over triangles, with F a triangle's deformation gradient and R its nearest
    /// rotation. Held vertices stay where they are.
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

    /// Positions refitted, as rigid as possible, to the rotations of their triangles.
    ///
    /// The refitted positions turn each triangle as it turns in `x`, without stretch.
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

    /// The incremental potential at positions `x`, term by term, with contact between `pairs`.
    ///
    /// The strain barrier is infinite past the strain cap, and contact is infinite where paper
    /// reaches its thickness.
    fn potential(
        &self,
        x: &[Point],
        predicted: &[Point],
        inertia: f64,
        pairs: &[Pair],
    ) -> Potential {
        let gravity = Point::from(self.settings.gravity);
        let mut energy = Energy::default();
        let mut inertia_term = 0.0;
        let contact = self.settings.contact;
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
            if let Some(clearance) = self.clearance(v, p) {
                energy.contact += contact
                    .table(clearance)
                    .map_or(f64::INFINITY, |[value, _, _]| value);
            }
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
                    inertia_term += 0.5 * mass * inertia * (x[v] - predicted[v]).norm_squared();
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
                energy.bending += bending::plate(&matrix, sines, sheet.rest_sines(t)).0;
            }
            for (d, crease) in sheet.creases().iter().enumerate() {
                if let Some(crease) = crease {
                    let [value, _, _] = bending::crease(
                        angles[d],
                        sheet.lengths()[d],
                        crease.stiffness,
                        sheet.rest_angles()[d],
                    );
                    energy.creases += value;
                }
            }
        }
        Potential {
            energy,
            inertia: inertia_term,
        }
    }

    /// Every element's gradient and Hessian at positions `x`, with contact between `pairs`.
    fn derivatives(
        &self,
        x: &[Point],
        predicted: &[Point],
        inertia: f64,
        pairs: &[Pair],
    ) -> Derivatives {
        let contact = self.settings.contact;
        let mut derivatives = Derivatives {
            pairs: pairs
                .par_iter()
                .filter_map(|pair| {
                    let (gradient, hessian) =
                        contact.pair_derivatives(pair, x, self.pair_thickness(pair))?;
                    Some((pair.stencil(), gradient, hessian))
                })
                .collect(),
            ..Derivatives::default()
        };
        for (v, p) in x.iter().enumerate() {
            let vertex = u32::try_from(v).expect("a scene counts its vertices in 32 bits");
            let barrier = self.clearance(v, p).and_then(|c| contact.table(c));
            if let Some([_, slope, curvature]) = barrier {
                if slope != 0.0 || curvature != 0.0 {
                    derivatives.table.push((vertex, slope, curvature));
                }
            }
        }
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
                derivatives
                    .vertices
                    .push((global(vertex), force.into(), mass * inertia));
            }
            derivatives.triangles.par_extend(
                mesh.triangles()
                    .par_iter()
                    .zip(sheet.rest_triangles())
                    .map(|(t, rest)| {
                        let points = t.map(|v| x[v as usize]);
                        let local = membrane::local(rest, &points, &constants.stretching)
                            .expect("the simulation keeps every state below the strain cap");
                        (t.map(|v| Some(global(v))), local.gradient, local.hessian)
                    }),
            );
            bend(sheet, x, &constants.bending, &global, &mut derivatives);
        }
        derivatives
    }
}

/// Adds a sheet's bending derivatives at its positions `x`, its vertices numbered by `global`.
fn bend(
    sheet: &Sheet,
    x: &[Point],
    stiffness: &nalgebra::Matrix3<f64>,
    global: &(impl Fn(u32) -> u32 + Sync),
    derivatives: &mut Derivatives,
) {
    let mesh = sheet.mesh();
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
            let matrix = sheet.plates()[t].energy_matrix(stiffness);
            bending::triangle(&matrix, edges, sheet.rest_sines(t))
        })
        .collect();
    // Each plate's moments on its edges' sines act through the curvature of those sines.
    let mut moments = vec![0.0; mesh.dihedrals().len()];
    for (t, (local, moment)) in plates.into_iter().enumerate() {
        for (i, edge) in sheet.stencils()[t].iter().enumerate() {
            if let Some((d, _)) = edge {
                moments[*d as usize] += moment[i];
            }
        }
        let stencil = sheet.bending_stencil(t).map(|v| v.map(global));
        derivatives
            .plates
            .push((stencil, local.gradient, local.hessian));
    }
    derivatives
        .dihedrals
        .par_extend(
            mesh.dihedrals()
                .par_iter()
                .enumerate()
                .map(|(d, dihedral)| {
                    let (angle, sine) = &angles[d];
                    let mut hessian = sine.hessian * moments[d];
                    let mut gradient = SMatrix::<f64, 12, 1>::zeros();
                    if let Some(crease) = &sheet.creases()[d] {
                        let [_, slope, curvature] = bending::crease(
                            angle.value,
                            sheet.lengths()[d],
                            crease.stiffness,
                            sheet.rest_angles()[d],
                        );
                        gradient += angle.gradient * slope;
                        hessian += angle.gradient * angle.gradient.transpose() * curvature
                            + angle.hessian * slope;
                    }
                    let stencil = stencil(dihedral).map(|v| Some(global(v)));
                    (stencil, gradient, hessian)
                }),
        );
}

impl Simulation {
    /// A vertex's clearance at position `p`: its height over the table less its half
    /// thickness, or `None` without a table.
    fn clearance(&self, vertex: usize, p: &Point) -> Option<f64> {
        let vertex = u32::try_from(vertex).expect("a scene counts its vertices in 32 bits");
        let table = self.settings.table?;
        Some(p[2] - table.height - 0.5 * self.thickness(vertex))
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
        thickest + self.settings.contact.gap
    }

    /// The pairs that may touch while the vertices move from `start` to `end`.
    fn candidates(&self, start: &[Point], end: &[Point]) -> Vec<Pair> {
        contact::candidates(&self.primitives, &self.trees, start, end, self.reach())
    }

    /// The largest safe fraction of a motion from `x`, at most 1, and the pairs that may touch.
    ///
    /// A safe fraction keeps every pair its thickness apart, and every vertex its half
    /// thickness above the table.
    fn safe_fraction(&self, x: &[Point], displacement: &[Point]) -> (f64, Vec<Pair>) {
        let end: Vec<Point> = x.iter().zip(displacement).map(|(p, d)| p + d).collect();
        let pairs = self.candidates(x, &end);
        let mut fraction: f64 = pairs
            .par_iter()
            .map(|pair| ccd::pair(pair, x, displacement, self.pair_thickness(pair)))
            .collect::<Vec<f64>>()
            .into_iter()
            .fold(1.0, f64::min);
        for (v, (p, d)) in x.iter().zip(displacement).enumerate() {
            if let Some(clearance) = self.clearance(v, p) {
                fraction = fraction.min(ccd::table(clearance, -d[2]));
            }
        }
        (fraction, pairs)
    }

    /// The pairs and table contacts that contact pushes apart at positions `x`, and the least
    /// gap within contact's reach.
    fn contacts(&self, x: &[Point]) -> (usize, f64) {
        let contact = self.settings.contact;
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
        for (v, p) in x.iter().enumerate() {
            if let Some(gap) = self.clearance(v, p) {
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

/// One element's stencil of vertices, its gradient and its Hessian.
type Element<const N: usize, const D: usize> =
    ([Option<u32>; N], SMatrix<f64, D, 1>, SMatrix<f64, D, D>);

/// Every element's gradient and Hessian at some positions, evaluated once and scattered into
/// a system as many times as it takes.
#[derive(Default)]
struct Derivatives {
    /// Each vertex's force from inertia and gravity, and the diagonal of its mass term.
    vertices: Vec<(u32, [f64; 3], f64)>,
    /// Each vertex the table pushes: its barrier's slope and curvature along z.
    table: Vec<(u32, f64, f64)>,
    /// The contact barrier of each pair within reach.
    pairs: Vec<Element<4, 12>>,
    /// Each triangle's stretching.
    triangles: Vec<Element<3, 9>>,
    /// Each triangle's plate bending in the half-angle sines, positive semi-definite already.
    plates: Vec<Element<6, 18>>,
    /// Each dihedral's bending moment on its angle's curvature, and its sharp crease.
    dihedrals: Vec<Element<4, 12>>,
}

impl Derivatives {
    /// Fills an assembly. With `projected`, each element's Hessian is projected to be positive
    /// semi-definite first.
    fn scatter(&self, assembly: &mut Assembly, projected: bool) {
        for &(vertex, force, diagonal) in &self.vertices {
            assembly.add_vertex(vertex, force, diagonal);
        }
        for &(vertex, slope, curvature) in &self.table {
            assembly.add_vertex_z(vertex, slope, curvature);
        }
        scatter_elements(&self.pairs, assembly, projected);
        scatter_elements(&self.triangles, assembly, projected);
        scatter_elements(&self.plates, assembly, false);
        scatter_elements(&self.dihedrals, assembly, projected);
    }
}

/// Fills an assembly with elements, their Hessians projected in parallel when `projected`.
fn scatter_elements<const N: usize, const D: usize>(
    elements: &[Element<N, D>],
    assembly: &mut Assembly,
    projected: bool,
) where
    SMatrix<f64, D, D>: Project,
{
    if projected {
        let hessians: Vec<SMatrix<f64, D, D>> =
            elements.par_iter().map(|(_, _, h)| h.project()).collect();
        for ((stencil, gradient, _), hessian) in elements.iter().zip(&hessians) {
            assembly.add(stencil, gradient, hessian);
        }
    } else {
        for (stencil, gradient, hessian) in elements {
            assembly.add(stencil, gradient, hessian);
        }
    }
}

/// What a line search needs from its Newton iteration.
struct Search<'a, 'b> {
    predicted: &'b [Point],
    inertia: f64,
    rigidity: &'a OnceCell<Result<Factor<'a>, SingularError>>,
    /// The incremental potential where the update starts.
    potential: Potential,
    slope: f64,
    /// The longest fraction of the update that collision detection allows.
    safe: f64,
    /// The pairs that may touch along the update.
    pairs: &'b [Pair],
}

/// Every pair of a contact's four vertices, the greater first.
fn vertex_pairs(v: [u32; 4]) -> impl Iterator<Item = (u32, u32)> {
    (0..4).flat_map(move |i| (0..i).map(move |j| (v[i].max(v[j]), v[i].min(v[j]))))
}

/// Points moved by a fraction of a displacement given in flat coordinates.
fn moved(x: &[Point], displacement: &[f64], fraction: f64) -> Vec<Point> {
    x.iter()
        .enumerate()
        .map(|(i, p)| p + Vector3::from_column_slice(&displacement[3 * i..3 * i + 3]) * fraction)
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::Material;
    use crate::mesh::Mesh;
    use crate::settings::Table;
    use crate::sheet::Crease;

    /// A small deterministic wobble, to keep every quantity away from symmetric values.
    fn wobble(v: usize, amplitude: f64) -> Point {
        #[expect(clippy::cast_precision_loss, reason = "the index only seeds a wobble")]
        let k = v as f64;
        Point::new((1.3 * k).sin(), (2.1 * k).cos(), (0.7 * k + 0.4).sin()) * amplitude
    }

    /// Checks the assembled gradient and Hessian at `x` against central differences of the
    /// incremental potential and of the gradient, to a relative tolerance.
    fn check(simulation: &Simulation, x: &[Point], predicted: &[Point], step: f64) {
        let inertia = 3600.0;
        let pairs = simulation.candidates(x, x);
        let pattern = Pattern::new(
            x.len(),
            simulation
                .pairs
                .iter()
                .copied()
                .chain(pairs.iter().flat_map(|p| vertex_pairs(p.vertices))),
        );
        let assemble = |y: &[Point]| {
            let mut assembly = Assembly::new(&pattern, &simulation.held);
            simulation
                .derivatives(y, predicted, inertia, &pairs)
                .scatter(&mut assembly, false);
            assembly
        };
        let gradient = |y: &[Point]| assemble(y).gradient;
        let at_x = assemble(x);
        let hessian = at_x.dense();
        let potential = |y: &[Point]| simulation.potential(y, predicted, inertia, &pairs).total();
        // Rounding limits the differences to about the scale of a column. Each entry is held to
        // a tolerance relative to itself and to its column.
        let gradient_scale = at_x.gradient.iter().fold(0.0, |m: f64, g| m.max(g.abs()));
        for i in 0..3 * x.len() {
            let mut ahead = x.to_vec();
            ahead[i / 3][i % 3] += step;
            let mut behind = x.to_vec();
            behind[i / 3][i % 3] -= step;
            let numeric = (potential(&ahead) - potential(&behind)) / (2.0 * step);
            let exact = at_x.gradient[i];
            assert!(
                (numeric - exact).abs() <= 1e-6 * (exact.abs() + gradient_scale),
                "coordinate {i}: the gradient is {exact}, its differences {numeric}"
            );
            let (g_ahead, g_behind) = (gradient(&ahead), gradient(&behind));
            let column = hessian.column(i).amax();
            for j in 0..3 * x.len() {
                let numeric = (g_ahead[j] - g_behind[j]) / (2.0 * step);
                let exact = hessian[(j, i)];
                assert!(
                    (numeric - exact).abs() <= 1e-5 * exact.abs() + 1e-6 * column,
                    "entry ({j}, {i}): the Hessian is {exact}, its differences {numeric}"
                );
            }
        }
    }

    #[test]
    fn the_assembly_differentiates_the_elastic_potential() {
        // A creased sheet folded off its rest angle, curved off its rest curvature, and
        // stretched along x into its strain barrier.
        let mut sheet =
            Sheet::new(Mesh::grid(3, 3, 0.01, 0.01).unwrap(), Material::kami()).unwrap();
        let at = |i: u32, j: u32| j * 4 + i;
        for j in 0..3 {
            let edge = [at(1, j), at(1, j + 1)];
            sheet
                .set_crease(edge, Some(Crease { stiffness: 0.01 }))
                .unwrap();
            sheet.set_rest_angle(edge, 0.9).unwrap();
            sheet
                .set_rest_angle([at(2, j), at(2, j + 1)], -0.2)
                .unwrap();
        }
        let simulation = Simulation::new(vec![sheet], Settings::default()).unwrap();
        let fold = nalgebra::Rotation3::from_axis_angle(&Vector3::y_axis(), -0.5);
        let x: Vec<Point> = simulation
            .positions
            .iter()
            .enumerate()
            .map(|(v, p)| {
                let flat = Point::new(p[0] * 1.0042, p[1], 0.0);
                let creased = Point::new(0.01 / 3.0 * 1.0042, 0.0, 0.0);
                let turned = if p[0] > 0.004 {
                    creased + fold * (flat - creased)
                } else {
                    flat
                };
                turned + wobble(v, 2e-7)
            })
            .collect();
        let energy = simulation.potential(&x, &x, 0.0, &[]).energy;
        assert!(energy.strain_barrier > 0.0 && energy.creases > 0.0 && energy.bending > 0.0);
        let predicted: Vec<Point> = x.iter().map(|p| p + Point::new(0.0, 0.0, 1e-3)).collect();
        check(&simulation, &x, &predicted, 1e-9);
    }

    #[test]
    fn the_assembly_differentiates_contact() {
        // A small sheet lies turned over a larger one, which lies on the table, each within
        // the reach of contact.
        let (h, gap) = (70e-6, 2e-6);
        let lower = Sheet::new(Mesh::grid(3, 3, 0.01, 0.01).unwrap(), Material::kami()).unwrap();
        let upper = Sheet::new(Mesh::grid(2, 2, 0.006, 0.006).unwrap(), Material::kami()).unwrap();
        let settings = Settings {
            table: Some(Table { height: 0.0 }),
            ..Settings::default()
        };
        let simulation = Simulation::new(vec![lower, upper], settings).unwrap();
        let turn = nalgebra::Rotation3::from_axis_angle(&Vector3::z_axis(), 0.3);
        let x: Vec<Point> = simulation
            .positions
            .iter()
            .enumerate()
            .map(|(v, p)| {
                let flat = Point::new(p[0], p[1], 0.0);
                let placed = if v < 16 {
                    flat + Point::new(0.0, 0.0, 0.5 * h + 0.6 * gap)
                } else {
                    turn * flat + Point::new(0.003, 0.001, 1.5 * h + 1.2 * gap)
                };
                placed + wobble(v, 0.1 * gap)
            })
            .collect();
        let pairs = simulation.candidates(&x, &x);
        let energy = simulation.potential(&x, &x, 0.0, &pairs).energy;
        assert!(
            energy.contact > 0.0 && pairs.len() > 10,
            "{} pairs",
            pairs.len()
        );
        let predicted: Vec<Point> = x.iter().map(|p| p - Point::new(0.0, 0.0, 1e-3)).collect();
        check(&simulation, &x, &predicted, 1e-9);
    }
}
