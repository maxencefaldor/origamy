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

use crate::body::{Body, BodyError, Motion};
use crate::contact::friction::Slip;
use crate::contact::{self, Pair, Primitives, Trees, ccd, distance};
use crate::energy::bending::{self, Angle, BendingEdge};
use crate::energy::crease;
use crate::energy::membrane::{self, Stretching};
use crate::energy::{Point, Project};
use crate::parameter::{ParameterError, finite};
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
    /// The work creases take as they yield over the time step, dissipated or stored by
    /// hardening.
    pub yielding: f64,
    /// The energy of contact's barrier, between sheets and with the table.
    pub contact: f64,
    /// The work friction does against the slip of contacts over the time step, up to the
    /// smoothing below the stick distance.
    pub friction: f64,
    /// The work air drag does against the paper over the time step.
    pub drag: f64,
    /// The potential energy of the point forces, zero at the origin, and the work pressure
    /// does over the time step.
    pub loads: f64,
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
    fn terms(&self) -> [f64; 11] {
        // The destructuring names every field, so a new term cannot be left out of the total.
        let Self {
            stretching,
            strain_barrier,
            bending,
            creases,
            yielding,
            contact,
            friction,
            drag,
            loads,
            gravity,
            kinetic,
        } = *self;
        [
            stretching,
            strain_barrier,
            bending,
            creases,
            yielding,
            contact,
            friction,
            drag,
            loads,
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
    /// The augmented Lagrangian that pulls each moving held vertex to its target.
    holding: f64,
}

impl Potential {
    /// The incremental potential.
    fn total(&self) -> f64 {
        self.energy.total() + self.inertia + self.holding
    }

    /// The size of the terms, for telling a change from rounding.
    fn size(&self) -> f64 {
        self.energy.terms().iter().map(|t| t.abs()).sum::<f64>()
            + self.inertia.abs()
            + self.holding.abs()
    }
}

/// What a time step did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Report {
    /// Whether the step reached its tolerance, in every time step it took.
    pub converged: bool,
    /// The Newton iterations the step took, over every time step and every lag of friction.
    pub iterations: usize,
    /// The time steps the step took: 1, or more where a time step that did not converge was
    /// halved.
    pub substeps: usize,
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
    /// A vertex to move is not held.
    #[error("vertex {vertex} is not held, so no hold can move it")]
    NotHeld {
        /// The vertex.
        vertex: usize,
    },
    /// A vertex to move is gripped, so its body moves it.
    #[error("vertex {vertex} is gripped by body {body}, so only the body moves it")]
    Gripped {
        /// The vertex.
        vertex: usize,
        /// The body that grips it.
        body: usize,
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
    /// The body that grips each vertex, if any; a gripped vertex is held.
    grips: Vec<Option<usize>>,
    /// Where each moving held vertex goes over the next time step.
    targets: Vec<Option<Point>>,
    /// The held vertices that stay where they are over the time step being solved.
    fixed: Vec<bool>,
    /// The point force on each vertex, in newtons.
    forces: Vec<[f64; 3]>,
    /// The force each hold exerted on its vertex at the end of the last time step, in newtons.
    hold_forces: Vec<[f64; 3]>,
    /// The pressure on each sheet's front, in pascals.
    pressures: Vec<f64>,
    bodies: Vec<Body>,
    /// The first point of each body, after the paper's vertices.
    body_starts: Vec<usize>,
    /// Where each body's hand holds its points, in order after the paper's vertices: where its
    /// path has reached.
    hands: Vec<Point>,
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
            grips: vec![None; positions.len()],
            targets: vec![None; positions.len()],
            fixed: vec![false; positions.len()],
            forces: vec![[0.0; 3]; positions.len()],
            hold_forces: vec![[0.0; 3]; positions.len()],
            pressures: vec![0.0; sheets.len()],
            bodies: Vec::new(),
            body_starts: Vec::new(),
            hands: Vec::new(),
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
            contact
                .pair(squared, self.primitives.thickness(pair))
                .is_none()
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
    /// A hold takes the vertex from any body that grips it.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such sheet or the sheet no such vertex.
    pub fn hold(&mut self, sheet: usize, vertex: usize, held: bool) {
        let range = self.range(sheet);
        assert!(vertex < range.len(), "the sheet has no vertex {vertex}");
        let vertex = range.start + vertex;
        self.held[vertex] = held;
        self.grips[vertex] = None;
        self.targets[vertex] = None;
        if held {
            self.velocities[vertex] = [0.0; 3];
        }
    }

    /// Ties vertices of a sheet to a body, which then carries them as it moves, as a pinch does.
    ///
    /// The vertices are held, and each time step carries them by the motion of the body's
    /// hand. A sphere carries them as its center moves. A capsule keeps each vertex's share of
    /// the way along its axis. The vertex's offset from the axis turns as the axis turns, by
    /// the least rotation. A vertex another body grips moves to this one.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such body or sheet, or the sheet no such vertex.
    pub fn grip(&mut self, body: usize, sheet: usize, vertices: &[usize]) {
        assert!(
            body < self.bodies.len(),
            "the simulation has no body {body}"
        );
        let range = self.range(sheet);
        for &vertex in vertices {
            assert!(vertex < range.len(), "the sheet has no vertex {vertex}");
        }
        for &vertex in vertices {
            let vertex = range.start + vertex;
            self.held[vertex] = true;
            self.grips[vertex] = Some(body);
            self.targets[vertex] = None;
            self.velocities[vertex] = [0.0; 3];
        }
    }

    /// Lets go of every vertex a body grips.
    ///
    /// Each vertex falls free with the velocity the body last gave it.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such body.
    pub fn release(&mut self, body: usize) {
        assert!(
            body < self.bodies.len(),
            "the simulation has no body {body}"
        );
        for (grip, held) in self.grips.iter_mut().zip(&mut self.held) {
            if *grip == Some(body) {
                (*grip, *held) = (None, false);
            }
        }
    }

    /// Which body grips each vertex of a sheet, if any.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such sheet.
    #[must_use]
    pub fn grips(&self, sheet: usize) -> &[Option<usize>] {
        &self.grips[self.range(sheet)]
    }

    /// Moves held vertices of a sheet, each to its position, over the next time step, as a hand
    /// carries them.
    ///
    /// The vertices move within the step, so collision detection covers their paths, and the
    /// paper around them follows. The step converges only once every vertex reaches its position.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::NotHeld`] when a vertex is not held, [`StateError::Gripped`] when
    /// a body grips it, and [`StateError::NotFinite`] when a coordinate is not finite. A refused
    /// move changes nothing.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such sheet or the sheet no such vertex.
    pub fn move_held(
        &mut self,
        sheet: usize,
        moves: &[(usize, [f64; 3])],
    ) -> Result<(), StateError> {
        let range = self.range(sheet);
        for &(vertex, position) in moves {
            assert!(vertex < range.len(), "the sheet has no vertex {vertex}");
            if !self.held[range.start + vertex] {
                return Err(StateError::NotHeld { vertex });
            }
            if let Some(body) = self.grips[range.start + vertex] {
                return Err(StateError::Gripped { vertex, body });
            }
            if !position.iter().all(|c| c.is_finite()) {
                return Err(StateError::NotFinite { vertex });
            }
        }
        for &(vertex, position) in moves {
            self.targets[range.start + vertex] = Some(Point::from(position));
        }
        Ok(())
    }

    /// The force each hold exerted on each vertex of a sheet at the end of the last time step.
    ///
    /// The forces are in newtons, and zero where a vertex is free.
    /// The force balances every other force on the vertex, to within the forces the solver's
    /// tolerance leaves.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such sheet.
    #[must_use]
    pub fn hold_forces(&self, sheet: usize) -> &[[f64; 3]] {
        &self.hold_forces[self.range(sheet)]
    }

    /// The point force on each vertex of a sheet, in newtons.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such sheet.
    #[must_use]
    pub fn forces(&self, sheet: usize) -> &[[f64; 3]] {
        &self.forces[self.range(sheet)]
    }

    /// Sets the point forces on vertices of a sheet, in newtons, each until set again.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::NotFinite`] when a component is not finite. A refused force
    /// changes nothing.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such sheet or the sheet no such vertex.
    pub fn set_forces(
        &mut self,
        sheet: usize,
        forces: &[(usize, [f64; 3])],
    ) -> Result<(), StateError> {
        let range = self.range(sheet);
        for &(vertex, force) in forces {
            assert!(vertex < range.len(), "the sheet has no vertex {vertex}");
            if !force.iter().all(|c| c.is_finite()) {
                return Err(StateError::NotFinite { vertex });
            }
        }
        for &(vertex, force) in forces {
            self.forces[range.start + vertex] = force;
        }
        Ok(())
    }

    /// The pressure on a sheet's front, in pascals.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such sheet.
    #[must_use]
    pub fn pressure(&self, sheet: usize) -> f64 {
        self.pressures[sheet]
    }

    /// Sets the pressure on a sheet's front, in pascals, until set again.
    ///
    /// The pressure pushes each patch of the sheet against its normal at the start of each time
    /// step; a negative pressure pulls.
    ///
    /// # Errors
    ///
    /// Returns a [`ParameterError`] when the pressure is not finite.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such sheet.
    pub fn set_pressure(&mut self, sheet: usize, pressure: f64) -> Result<(), ParameterError> {
        assert!(
            sheet < self.sheets.len(),
            "the simulation has no sheet {sheet}"
        );
        finite("pressure", pressure)?;
        self.pressures[sheet] = pressure;
        Ok(())
    }

    /// Adds a rigid body, and gives its index.
    ///
    /// The body is a sphere round one point, or a capsule round the segment between two. Its
    /// points stay where they are until `move_body` carries them.
    ///
    /// # Errors
    ///
    /// Returns a [`BodyError`] when a parameter lies outside its values, or the points are
    /// neither one nor two. The error also names a coordinate that is not finite, or a capsule
    /// whose ends coincide. A body may not lie closer to paper than its radius and half the
    /// paper's thickness.
    ///
    /// # Panics
    ///
    /// Panics when the scene would count more vertices than 32 bits can name.
    pub fn add_body(&mut self, body: Body, points: &[[f64; 3]]) -> Result<usize, BodyError> {
        body.check()?;
        if !matches!(points.len(), 1 | 2) {
            return Err(BodyError::Points {
                given: points.len(),
            });
        }
        check_points(points)?;
        let start = self.positions.len();
        let first = u32::try_from(start).expect("a scene counts its vertices in 32 bits");
        let mut primitives = self.primitives.clone();
        primitives
            .rest
            .extend(points.iter().map(|&p| Point::from(p)));
        primitives
            .half
            .extend(std::iter::repeat_n(body.radius, points.len()));
        if points.len() == 2 {
            primitives.axes.push([first, first + 1]);
        }
        let x: Vec<Point> = self
            .positions
            .iter()
            .chain(points)
            .map(|&p| Point::from(p))
            .collect();
        let trees = Trees::new(&primitives, &x);
        let gap = self.settings.contact.gap;
        let overlapping = contact::candidates(&primitives, &trees, &x, &x, gap)
            .iter()
            .filter(|pair| pair.vertices.iter().any(|&v| v >= first))
            .any(|pair| {
                let squared = distance::squared(pair.kind, &pair.at(&x));
                squared <= primitives.thickness(pair).powi(2)
            });
        if overlapping {
            return Err(BodyError::Overlapping);
        }
        let count = points.len();
        self.positions.extend_from_slice(points);
        self.velocities.extend(std::iter::repeat_n([0.0; 3], count));
        self.held.extend(std::iter::repeat_n(true, count));
        self.grips.extend(std::iter::repeat_n(None, count));
        self.targets.extend(std::iter::repeat_n(None, count));
        self.fixed.extend(std::iter::repeat_n(true, count));
        self.forces.extend(std::iter::repeat_n([0.0; 3], count));
        self.hold_forces
            .extend(std::iter::repeat_n([0.0; 3], count));
        self.pattern = Pattern::new(self.positions.len(), self.pairs.iter().copied());
        self.primitives = primitives;
        self.trees = trees;
        self.bodies.push(body);
        self.body_starts.push(start);
        self.hands.extend(points.iter().map(|&p| Point::from(p)));
        Ok(self.bodies.len() - 1)
    }

    /// The simulation's bodies, in the order they joined it.
    #[must_use]
    pub fn bodies(&self) -> &[Body] {
        &self.bodies
    }

    /// Where a body's points lie: a sphere's center, or a capsule's two ends.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such body.
    #[must_use]
    pub fn body_points(&self, body: usize) -> &[[f64; 3]] {
        &self.positions[self.body_range(body)]
    }

    /// Moves a body's points to `points` over the next time step, as a hand carries it.
    ///
    /// The body moves within the step, so collision detection covers its path, and paper in
    /// its way moves aside. The step converges only once the body reaches its points. A body
    /// its hand holds by a spring stops instead where the spring balances the paper's push, and
    /// presses on until the hand moves again.
    ///
    /// # Errors
    ///
    /// Returns a [`BodyError`] when the points are not as many as the body's, a coordinate is
    /// not finite, or a capsule's ends coincide.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such body.
    pub fn move_body(&mut self, body: usize, points: &[[f64; 3]]) -> Result<(), BodyError> {
        let range = self.body_range(body);
        if points.len() != range.len() {
            return Err(BodyError::Count {
                expected: range.len(),
                given: points.len(),
            });
        }
        check_points(points)?;
        for (target, &point) in self.targets[range].iter_mut().zip(points) {
            *target = Some(Point::from(point));
        }
        Ok(())
    }

    /// The force a body exerted on the paper at the end of the last time step, in newtons.
    ///
    /// The force sums the body's contact with the paper and its grip on it.
    ///
    /// # Panics
    ///
    /// Panics when the simulation has no such body.
    #[must_use]
    pub fn body_force(&self, body: usize) -> [f64; 3] {
        let gripped = (0..self.primitives.sheet.len()).filter(|&v| self.grips[v] == Some(body));
        self.body_range(body)
            .chain(gripped)
            .map(|v| self.hold_forces[v])
            .fold([0.0; 3], |[a, b, c], [x, y, z]| [a + x, b + y, c + z])
    }

    /// The simulation's energy now.
    #[must_use]
    pub fn energy(&self) -> Energy {
        let x: Vec<Point> = self.positions.iter().map(|&p| Point::from(p)).collect();
        let mut energy = self
            .potential(&x, &Incremental::still(&x), &self.candidates(&x, &x))
            .energy;
        energy.kinetic = self.kinetic();
        energy
    }

    /// Advances the simulation by one time step.
    ///
    /// A time step whose solve does not converge retries as two of half its length, down to
    /// the solver's shortest time step.
    ///
    /// # Errors
    ///
    /// Returns a [`StepError`] when some motion costs no energy, so the step has no unique
    /// result. The step also refuses paper that starts closer than its thickness to itself or
    /// to the table. A refused step leaves the state as it was.
    pub fn step(&mut self) -> Result<Report, StepError> {
        let dt = self.settings.time_step;
        let report = self.advance(dt)?;
        self.targets.fill(None);
        Ok(report)
    }

    /// Advances the simulation by `dt`, halving the time step wherever it does not converge,
    /// down to the shortest time step. A refused step leaves the state as it was.
    fn advance(&mut self, dt: f64) -> Result<Report, StepError> {
        let attempt = self.attempt(dt)?;
        let half = 0.5 * dt;
        let floor = half < self.settings.solver.shortest_step;
        if attempt.solve.converged || !dt.is_finite() || floor {
            return Ok(self.finish(dt, &attempt));
        }
        // The halves change the state, so a refused half restores it.
        let saved = (
            self.positions.clone(),
            self.velocities.clone(),
            self.time,
            self.sheets.clone(),
            self.targets.clone(),
            self.hands.clone(),
        );
        // The first half carries each moving hold halfway, and the second the rest of the way.
        for (target, &position) in self.targets.iter_mut().zip(&self.positions) {
            if let Some(target) = target {
                *target = (*target + Point::from(position)) * 0.5;
            }
        }
        let halves = self.advance(half).and_then(|first| {
            self.targets.clone_from(&saved.4);
            Ok((first, self.advance(half)?))
        });
        match halves {
            Ok((first, second)) => Ok(Report {
                converged: first.converged && second.converged,
                iterations: attempt.iterations + first.iterations + second.iterations,
                substeps: first.substeps + second.substeps,
                ..second
            }),
            Err(error) => {
                (
                    self.positions,
                    self.velocities,
                    self.time,
                    self.sheets,
                    self.targets,
                    self.hands,
                ) = saved;
                Err(error)
            }
        }
    }

    /// One time step of length `dt`, solved whether it converges or not, and not yet taken.
    ///
    /// Friction lags from the step's start, then from each solution the step reaches, until
    /// the solution stays where it is or the lags run out.
    fn attempt(&mut self, dt: f64) -> Result<Attempt, StepError> {
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
        let mut slips = self.slips(&start);
        let normals = self.normals(&start);
        // A moving held vertex is free in the solve, pulled to its target; the others stay.
        let targets = self.destinations();
        self.fixed = self
            .held
            .iter()
            .zip(&targets)
            .map(|(&held, target)| held && target.is_none())
            .collect();
        let springs = self.springs();
        let mut holds: Vec<Option<Hold>> = targets
            .iter()
            .zip(&springs)
            .map(|(target, &spring)| {
                target.map(|target| Hold {
                    target,
                    multiplier: Point::zeros(),
                    spring,
                })
            })
            .collect();
        let mut solve = Solve {
            x: start.clone(),
            potential: Potential::default(),
            converged: false,
            iterations: 0,
            update: f64::INFINITY,
        };
        let mut iterations = 0;
        let mut holding = vec![Point::zeros(); start.len()];
        for lag in 0..self.settings.solver.lags {
            let incremental = Incremental {
                start: &start,
                predicted: &predicted,
                inertia,
                elapsed: elapsed(dt),
                slips: &slips,
                normals: &normals,
                holds: &[],
                stiffness: 0.0,
            };
            solve = self.carry(solve.x, &incremental, &mut holds)?;
            iterations += solve.iterations;
            let settled = lag > 0 && solve.iterations <= 1;
            let last = lag + 1 == self.settings.solver.lags;
            if !solve.converged || slips.is_empty() || settled || last {
                // A hold exerts the force every other term of the potential leaves on its
                // vertex, in the lag the step ends with.
                if self.held.contains(&true) {
                    let near = self.candidates(&solve.x, &solve.x);
                    let gradient = self
                        .derivatives(&solve.x, &incremental, &near)
                        .gradient(solve.x.len());
                    holding = gradient
                        .iter()
                        .zip(&self.held)
                        .map(|(g, &held)| if held { *g } else { Point::zeros() })
                        .collect();
                }
                break;
            }
            slips = self.slips(&solve.x);
        }
        Ok(Attempt {
            start,
            solve,
            iterations,
            holding,
        })
    }

    /// The minimum of a time step's incremental potential with each moving held vertex at its
    /// target, from `x`.
    ///
    /// The augmented Lagrangian of incremental potential contact carries the holds. A stiff
    /// spring pulls each hold to its target, and each round of minimization raises a force on
    /// every hold short of it. The solve converges only once every hold lies within the tolerance
    /// of its target.
    fn carry(
        &self,
        mut x: Vec<Point>,
        terms: &Incremental,
        holds: &mut [Option<Hold>],
    ) -> Result<Solve, StepError> {
        let solver = self.settings.solver;
        let stiffness = solver.hold_stiffness * self.stiffest();
        let mut iterations = 0;
        for _ in 0..solver.hold_rounds {
            let incremental = Incremental {
                holds,
                stiffness,
                ..*terms
            };
            let solve = self.minimize(x, &incremental)?;
            iterations += solve.iterations;
            // A hold by a spring stops where its forces balance, short of its target.
            let gap = holds
                .iter()
                .zip(&solve.x)
                .filter_map(|(hold, p)| {
                    hold.filter(|hold| hold.spring.is_none())
                        .map(|hold| (p - hold.target).norm())
                })
                .fold(0.0, f64::max);
            if !solve.converged || gap <= solver.tolerance {
                return Ok(Solve {
                    iterations,
                    ..solve
                });
            }
            for (hold, p) in holds.iter_mut().zip(&solve.x) {
                if let Some(hold) = hold.as_mut().filter(|hold| hold.spring.is_none()) {
                    hold.multiplier -= (p - hold.target) * stiffness;
                }
            }
            x = solve.x;
        }
        let mut solve = self.minimize(
            x,
            &Incremental {
                holds,
                stiffness,
                ..*terms
            },
        )?;
        solve.iterations += iterations;
        solve.converged = false;
        Ok(solve)
    }

    /// The force rounding leaves in the gradient at positions `x`, in newtons.
    ///
    /// A position rounds by about 10⁻¹⁶ of its size, and a stiff sheet feels that strain. The
    /// stiffest sheet's stiffness times the largest coordinate, times 10⁻¹², bounds the force.
    fn rounding(&self, x: &[Point]) -> f64 {
        let size = x.iter().map(Point::amax).fold(0.0, f64::max);
        1e-12 * self.stiffest() * size
    }

    /// The largest stretching stiffness of any sheet, Young's modulus times thickness, in newtons
    /// per meter.
    fn stiffest(&self) -> f64 {
        self.sheets
            .iter()
            .map(|sheet| {
                let material = sheet.material();
                material.stiffness.along.max(material.stiffness.across) * material.thickness
            })
            .fold(0.0, f64::max)
    }

    /// The minimum of a time step's incremental potential, by Newton's method from `x`.
    fn minimize(&self, mut x: Vec<Point>, incremental: &Incremental) -> Result<Solve, StepError> {
        let mut potential = self.potential(&x, incremental, &self.candidates(&x, &x));
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
            let derivatives = self.derivatives(&x, incremental, &near);
            self.grow(
                &mut grown,
                derivatives.pairs.iter().chain(&derivatives.friction),
            );
            let pattern = grown.as_ref().map_or(&self.pattern, |(p, _)| p);
            // The exact Hessian gives Newton's quadratic convergence wherever it is positive
            // definite; elsewhere each element's Hessian is projected to be.
            let mut assembly = Assembly::new(pattern, &self.fixed);
            derivatives.scatter(&mut assembly, false);
            let factor = if let Ok(factor) = assembly.factor() {
                factor
            } else {
                assembly = Assembly::new(pattern, &self.fixed);
                derivatives.scatter(&mut assembly, true);
                assembly.factor_within(radius, self.rounding(&x))?
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
                incremental,
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
        Ok(Solve {
            x,
            potential,
            converged,
            iterations,
            update,
        })
    }

    /// Grows the system's pattern where active contacts, or the friction lagged from earlier
    /// contacts, join vertices it does not hold.
    ///
    /// The pattern only grows within a time step, so contacts that come and go do not rebuild
    /// it again and again.
    fn grow<'e>(
        &self,
        grown: &mut Option<(Pattern, Vec<(u32, u32)>)>,
        contacts: impl Iterator<Item = &'e Element<4, 12>>,
    ) {
        let pattern = grown.as_ref().map_or(&self.pattern, |(p, _)| p);
        let missing: Vec<(u32, u32)> = contacts
            .flat_map(|(stencil, _, _)| vertex_pairs(*stencil))
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

    /// Takes an attempted time step of length `dt`, and reports it.
    ///
    /// The vertices move to the solution, and each crease yields as far as the solution turns
    /// it.
    fn finish(&mut self, dt: f64, attempt: &Attempt) -> Report {
        let Attempt {
            start,
            solve,
            iterations,
            holding,
        } = attempt;
        for (force, hold) in self.hold_forces.iter_mut().zip(holding) {
            *force = (*hold).into();
        }
        // Each hand reaches where its path goes, whether or not a spring stops its body short.
        let first = self.primitives.sheet.len();
        for (hand, target) in self.hands.iter_mut().zip(&self.targets[first..]) {
            if let Some(target) = target {
                *hand = *target;
            }
        }
        for (i, point) in solve.x.iter().enumerate() {
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
        for (sheet, &offset) in self.sheets.iter_mut().zip(&self.offsets) {
            let yielded: Vec<(usize, crease::Plastic)> = sheet
                .creases()
                .iter()
                .zip(sheet.mesh().dihedrals())
                .enumerate()
                .filter_map(|(d, (crease, dihedral))| {
                    let points = stencil(dihedral).map(|v| solve.x[offset + v as usize]);
                    let angle = bending::angle_value(&points);
                    let turned = crease::turn(
                        crease.as_ref()?,
                        sheet.lengths()[d],
                        sheet.plastic(d),
                        angle,
                        elapsed(dt),
                    );
                    Some((d, turned.state))
                })
                .collect();
            for (d, state) in yielded {
                sheet.set_plastic(d, state);
            }
        }
        let energy = Energy {
            kinetic: self.kinetic(),
            ..solve.potential.energy
        };
        let (contacts, least_gap) = self.contacts(&solve.x);
        Report {
            converged: solve.converged,
            iterations: *iterations,
            substeps: 1,
            update: solve.update,
            energy,
            greatest_strain: self.greatest_strain(&solve.x),
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
            let trial_potential = self.potential(&trial, search.incremental, search.pairs);
            if accepts(&trial_potential, fraction) {
                return Some((trial, trial_potential, fraction));
            }
            if let Ok(rigidity) = search.rigidity.get_or_init(|| self.rigidity()) {
                let retracted = self.retract(&trial, rigidity);
                let way: Vec<Point> = retracted.iter().zip(x).map(|(r, p)| r - p).collect();
                let (safe, pairs) = self.safe_fraction(x, &way);
                if safe >= 1.0 {
                    let retracted_potential =
                        self.potential(&retracted, search.incremental, &pairs);
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
    /// rotation. Held vertices stay where the update puts them, so the paper turns about them.
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
    fn potential(&self, x: &[Point], incremental: &Incremental, pairs: &[Pair]) -> Potential {
        let (mut inertia_term, mut holding) = (0.0, 0.0);
        let (friction, contact) = self.touching(x, incremental, pairs);
        let mut energy = Energy {
            contact,
            friction,
            ..Energy::default()
        };
        for (s, sheet) in self.sheets.iter().enumerate() {
            let offset = self.offsets[s];
            let constants = Constants::of(sheet);
            let x = &x[offset..offset + sheet.mesh().vertex_count()];
            for (v, area) in sheet.areas().iter().enumerate() {
                let mass = constants.mass_per_area * area;
                let terms = self.vertex(incremental, s, offset + v, &x[v], *area, mass);
                energy.gravity += terms.gravity;
                energy.loads += terms.loads;
                inertia_term += terms.inertia;
                holding += terms.holding;
                if !self.held[offset + v] {
                    if let Some(drag) = self.drag(incremental, offset + v, *area) {
                        let across = (x[v] - incremental.start[offset + v]).dot(&drag.normal);
                        energy.drag += drag.scale * across.abs().powi(3) / 3.0;
                    }
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
                    let turned = crease::turn(
                        crease,
                        sheet.lengths()[d],
                        sheet.plastic(d),
                        angles[d],
                        incremental.elapsed,
                    );
                    energy.creases += turned.elastic;
                    energy.yielding += turned.yielding;
                }
            }
        }
        // The bodies' points carry no energy of their own, but a moving body pulls them.
        for (v, p) in x.iter().enumerate().skip(self.primitives.sheet.len()) {
            if let Some(hold) = incremental.hold(v) {
                holding += hold.terms(p, incremental.stiffness).0;
            }
        }
        Potential {
            energy,
            inertia: inertia_term,
            holding,
        }
    }

    /// The friction and contact energies at positions `x`, with contact between `pairs`.
    ///
    /// Contact is infinite where paper reaches its thickness, from paper, a body or the table.
    fn touching(&self, x: &[Point], incremental: &Incremental, pairs: &[Pair]) -> (f64, f64) {
        let contact = self.settings.contact;
        let frictions: Vec<f64> = incremental
            .slips
            .par_iter()
            .map(|slip| slip.energy(x, incremental.start, contact.stick))
            .collect();
        let barriers: Vec<f64> = pairs
            .par_iter()
            .map(|pair| {
                let fade = contact.fade(&self.primitives, pair);
                contact.pair_energy(pair, x, self.primitives.thickness(pair), fade)
            })
            .collect();
        let table: f64 = x
            .iter()
            .enumerate()
            .filter_map(|(v, p)| self.clearance(v, p))
            .map(|clearance| {
                contact
                    .table(clearance)
                    .map_or(f64::INFINITY, |[value, _, _]| value)
            })
            .sum();
        (frictions.iter().sum(), barriers.iter().sum::<f64>() + table)
    }

    /// Every element's gradient and Hessian at positions `x`, with contact between `pairs`.
    fn derivatives(&self, x: &[Point], incremental: &Incremental, pairs: &[Pair]) -> Derivatives {
        let contact = self.settings.contact;
        let mut derivatives = Derivatives {
            pairs: pairs
                .par_iter()
                .filter_map(|pair| {
                    let fade = contact.fade(&self.primitives, pair);
                    let (gradient, hessian) =
                        contact.pair_derivatives(pair, x, self.primitives.thickness(pair), fade)?;
                    Some((pair.stencil(), gradient, hessian))
                })
                .collect(),
            friction: incremental
                .slips
                .par_iter()
                .map(|slip| {
                    let (gradient, hessian) = slip.derivatives(x, incremental.start, contact.stick);
                    (slip.stencil, gradient, hessian)
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
        for (s, sheet) in self.sheets.iter().enumerate() {
            let offset = self.offsets[s];
            let global =
                |v: u32| u32::try_from(offset).expect("a scene counts its vertices in 32 bits") + v;
            let constants = Constants::of(sheet);
            let mesh = sheet.mesh();
            let x = &x[offset..offset + mesh.vertex_count()];
            for (v, area) in sheet.areas().iter().enumerate() {
                let mass = constants.mass_per_area * area;
                let terms = self.vertex(incremental, s, offset + v, &x[v], *area, mass);
                let vertex = u32::try_from(v).expect("a mesh counts its vertices in 32 bits");
                derivatives
                    .vertices
                    .push((global(vertex), terms.gradient.into(), terms.diagonal));
                if self.held[offset + v] {
                    continue;
                }
                if let Some(drag) = self.drag(incremental, offset + v, *area) {
                    let across = (x[v] - incremental.start[offset + v]).dot(&drag.normal);
                    let gradient = drag.normal * (drag.scale * across.abs() * across);
                    let hessian =
                        drag.normal * drag.normal.transpose() * (2.0 * drag.scale * across.abs());
                    derivatives
                        .drag
                        .push(([Some(global(vertex))], gradient, hessian));
                }
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
            bend(
                sheet,
                x,
                &constants.bending,
                incremental.elapsed,
                &global,
                &mut derivatives,
            );
        }
        for (v, p) in x.iter().enumerate().skip(self.primitives.sheet.len()) {
            if let Some(hold) = incremental.hold(v) {
                let (_, gradient, diagonal) = hold.terms(p, incremental.stiffness);
                let vertex = u32::try_from(v).expect("a scene counts its vertices in 32 bits");
                derivatives
                    .vertices
                    .push((vertex, gradient.into(), diagonal));
            }
        }
        derivatives
    }
}

/// Checks that every coordinate of a body's points is finite, and that a capsule's ends differ.
fn check_points(points: &[[f64; 3]]) -> Result<(), BodyError> {
    if let Some(point) = points.iter().position(|p| !p.iter().all(|c| c.is_finite())) {
        return Err(BodyError::NotFinite { point });
    }
    if let [a, b] = points {
        if (Point::from(*b) - Point::from(*a)).norm_squared() == 0.0 {
            return Err(BodyError::Degenerate);
        }
    }
    Ok(())
}

/// Adds a sheet's bending derivatives at its positions `x`, its vertices numbered by `global`,
/// in a time step that spans `elapsed` seconds.
fn bend(
    sheet: &Sheet,
    x: &[Point],
    stiffness: &nalgebra::Matrix3<f64>,
    elapsed: f64,
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
                        let turned = crease::turn(
                            crease,
                            sheet.lengths()[d],
                            sheet.plastic(d),
                            angle.value,
                            elapsed,
                        );
                        gradient += angle.gradient * turned.moment;
                        hessian += angle.gradient * angle.gradient.transpose() * turned.stiffness
                            + angle.hessian * turned.moment;
                    }
                    let stencil = stencil(dihedral).map(|v| Some(global(v)));
                    (stencil, gradient, hessian)
                }),
        );
}

impl Simulation {
    /// The terms of the incremental potential at vertex `vertex` of sheet `sheet`, at position
    /// `p`, with lumped area `area` and mass `mass`.
    ///
    /// The terms are the vertex's weight and loads, and either its moving hold or, when free,
    /// its inertia. A vertex that stays held moves not at all, so its other terms are constant.
    fn vertex(
        &self,
        incremental: &Incremental,
        sheet: usize,
        vertex: usize,
        p: &Point,
        area: f64,
        mass: f64,
    ) -> VertexTerms {
        let weight = Point::from(self.settings.gravity) * mass;
        let force = Point::from(self.forces[vertex]);
        let mut terms = VertexTerms {
            gravity: -weight.dot(p),
            loads: -force.dot(p),
            gradient: -weight - force,
            ..VertexTerms::default()
        };
        // Pressure pushes the patch against its normal from the step's start; its potential is
        // the work it does over the step.
        if let Some(normal) = incremental.normals.get(vertex) {
            let push = normal * (self.pressures[sheet] * area);
            terms.loads += push.dot(&(p - incremental.start[vertex]));
            terms.gradient += push;
        }
        if let Some(hold) = incremental.hold(vertex) {
            let (value, gradient, diagonal) = hold.terms(p, incremental.stiffness);
            terms.holding = value;
            terms.gradient += gradient;
            terms.diagonal = diagonal;
        } else if !self.held[vertex] {
            let lag = p - incremental.predicted[vertex];
            terms.inertia = 0.5 * mass * incremental.inertia * lag.norm_squared();
            terms.gradient += lag * (mass * incremental.inertia);
            terms.diagonal = mass * incremental.inertia;
        }
        terms
    }

    /// The air drag on a vertex with lumped area `area`, in a time step.
    ///
    /// The drag is `None` without air, in quasi-static steps, and for a vertex without a
    /// normal. Otherwise the drag is the potential k |w·n|³ / 3, with k = ρ C A / 2Δt². Here w
    /// is the vertex's move over the step, and n its normal at the step's start. The potential's
    /// gradient is the drag force ½ ρ C A |v·n| (v·n) n, with v = w / Δt the vertex's velocity.
    fn drag(&self, incremental: &Incremental, vertex: usize, area: f64) -> Option<Drag> {
        let air = self.settings.air?;
        let normal = *incremental.normals.get(vertex)?;
        (incremental.inertia > 0.0 && normal != Point::zeros()).then_some(Drag {
            normal,
            scale: 0.5 * air.density * air.drag * area * incremental.inertia,
        })
    }

    /// Each vertex's unit normal at positions `x`: the mean of its triangles' normals, weighted
    /// by their areas.
    fn normals(&self, x: &[Point]) -> Vec<Point> {
        let mut normals = vec![Point::zeros(); x.len()];
        for (s, sheet) in self.sheets.iter().enumerate() {
            let offset = self.offsets[s];
            for triangle in sheet.mesh().triangles() {
                let [a, b, c] = triangle.map(|v| offset + v as usize);
                // The cross product's size is twice the triangle's area.
                let weighted = (x[b] - x[a]).cross(&(x[c] - x[a]));
                for v in [a, b, c] {
                    normals[v] += weighted;
                }
            }
        }
        for normal in &mut normals {
            let size = normal.norm();
            if size > 0.0 {
                *normal /= size;
            }
        }
        normals
    }

    /// Each contact's friction at positions `x`, fixed for a time step that starts there.
    fn slips(&self, x: &[Point]) -> Vec<Slip> {
        let contact = self.settings.contact;
        let pairs = self.candidates(x, x);
        let mut slips: Vec<Slip> = pairs
            .par_iter()
            .filter_map(|pair| {
                let fade = contact.fade(&self.primitives, pair);
                let thickness = self.primitives.thickness(pair);
                contact.pair_slip(pair, x, thickness, fade, self.friction(pair))
            })
            .collect();
        if let Some(table) = self.settings.table {
            for (v, p) in x.iter().enumerate() {
                let vertex = u32::try_from(v).expect("a scene counts its vertices in 32 bits");
                if let Some(clearance) = self.clearance(v, p) {
                    slips.extend(contact.table_slip(vertex, clearance, table.friction));
                }
            }
        }
        slips
    }

    /// The coefficient of friction of a pair: its body's, or paper's on paper.
    fn friction(&self, pair: &Pair) -> f64 {
        pair.vertices
            .iter()
            .find(|&&v| self.primitives.is_body(v))
            .map_or(self.settings.contact.friction, |&v| {
                self.bodies[self.body_of(v as usize)].friction
            })
    }

    /// The body a point belongs to.
    fn body_of(&self, vertex: usize) -> usize {
        self.body_starts.partition_point(|&start| start <= vertex) - 1
    }

    /// Where each moving held vertex goes over the time step: to its own target, or where its
    /// body's hand carries it.
    ///
    /// A grip follows its hand's motion. A body its hand holds by a spring keeps pulling toward
    /// the hand, though the hand stays.
    fn destinations(&self) -> Vec<Option<Point>> {
        let first = self.primitives.sheet.len();
        let motions: Vec<Option<Motion>> = (0..self.bodies.len())
            .map(|body| {
                let range = self.body_range(body);
                let to: Option<Vec<Point>> = self.targets[range.clone()].iter().copied().collect();
                let from = &self.hands[range.start - first..range.end - first];
                to.map(|to| Motion::of(from, &to))
            })
            .collect();
        let springs = self.springs();
        self.grips
            .iter()
            .zip(&self.targets)
            .zip(&self.positions)
            .enumerate()
            .map(|(v, ((grip, target), &p))| match grip {
                Some(body) => motions[*body].map(|motion| motion.carry(&p.into())),
                None if springs[v].is_some() => target.or(Some(self.hands[v - first])),
                None => *target,
            })
            .collect()
    }

    /// The stiffness of the spring that holds each vertex, where a body's hand holds its points
    /// by one: the body's stiffness shared among its points.
    fn springs(&self) -> Vec<Option<f64>> {
        let mut springs = vec![None; self.positions.len()];
        for (body, &Body { stiffness, .. }) in self.bodies.iter().enumerate() {
            if stiffness.is_finite() {
                let range = self.body_range(body);
                #[expect(clippy::cast_precision_loss, reason = "a body has 1 point or 2")]
                let share = stiffness / range.len() as f64;
                springs[range].fill(Some(share));
            }
        }
        springs
    }

    /// The global indices of a body's points.
    fn body_range(&self, body: usize) -> std::ops::Range<usize> {
        let start = self.body_starts[body];
        let end = self
            .body_starts
            .get(body + 1)
            .copied()
            .unwrap_or(self.positions.len());
        start..end
    }

    /// A vertex's clearance at position `p`: its height over the table less its half
    /// thickness, or `None` without a table.
    fn clearance(&self, vertex: usize, p: &Point) -> Option<f64> {
        let table = self.settings.table?;
        (vertex < self.primitives.sheet.len())
            .then(|| p[2] - table.height - self.primitives.half[vertex])
    }

    /// The pairs that may touch while the vertices move from `start` to `end`.
    fn candidates(&self, start: &[Point], end: &[Point]) -> Vec<Pair> {
        let gap = self.settings.contact.gap;
        contact::candidates(&self.primitives, &self.trees, start, end, gap)
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
            .map(|pair| ccd::pair(pair, x, displacement, self.primitives.thickness(pair)))
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
            let h = self.primitives.thickness(&pair);
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
    /// The friction of each contact lagged from the step's start, positive semi-definite already.
    friction: Vec<Element<4, 12>>,
    /// The air drag on each vertex, positive semi-definite already.
    drag: Vec<Element<1, 3>>,
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
        scatter_elements(&self.friction, assembly, false);
        scatter_elements(&self.drag, assembly, false);
        scatter_elements(&self.triangles, assembly, projected);
        scatter_elements(&self.plates, assembly, false);
        scatter_elements(&self.dihedrals, assembly, projected);
    }

    /// The gradient at each of `count` vertices, every vertex free.
    fn gradient(&self, count: usize) -> Vec<Point> {
        let mut gradient = vec![Point::zeros(); count];
        for &(vertex, force, _) in &self.vertices {
            gradient[vertex as usize] += Point::from(force);
        }
        for &(vertex, slope, _) in &self.table {
            gradient[vertex as usize].z += slope;
        }
        gather(&self.pairs, &mut gradient);
        gather(&self.friction, &mut gradient);
        gather(&self.drag, &mut gradient);
        gather(&self.triangles, &mut gradient);
        gather(&self.plates, &mut gradient);
        gather(&self.dihedrals, &mut gradient);
        gradient
    }
}

/// Adds elements' gradients to each vertex's.
fn gather<const N: usize, const D: usize>(elements: &[Element<N, D>], gradient: &mut [Point]) {
    for (stencil, local, _) in elements {
        for (i, vertex) in stencil.iter().enumerate() {
            if let Some(v) = vertex {
                gradient[*v as usize] += local.fixed_rows::<3>(3 * i);
            }
        }
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

/// Air drag on one vertex in a time step: the direction it acts along, and its scale.
struct Drag {
    normal: Point,
    /// The scale k = ρ C A / 2Δt², in kilograms per square meter.
    scale: f64,
}

/// A time step solved but not yet taken.
///
/// The attempt keeps where the step starts and where its last lag's solve ended. The attempt
/// also keeps the Newton iterations of all its lags, and the forces of its holds.
struct Attempt {
    start: Vec<Point>,
    solve: Solve,
    iterations: usize,
    /// The force each hold exerts on its vertex at the solution; zero where the vertex is free.
    holding: Vec<Point>,
}

/// Where Newton's method ended a time step's minimization, and how.
struct Solve {
    x: Vec<Point>,
    potential: Potential,
    converged: bool,
    iterations: usize,
    /// The largest vertex move of the last Newton update, in meters.
    update: f64,
}

/// The terms of a time step's incremental potential that a minimization holds fixed.
///
/// The terms are where the step starts, where inertia carries each vertex, each contact's
/// friction, each vertex's normal and each moving hold.
struct Incremental<'a> {
    start: &'a [Point],
    predicted: &'a [Point],
    /// The inertia term's weight, 1 / Δt², or zero for quasi-static steps.
    inertia: f64,
    /// The time the step spans, in seconds, over which creases age; zero for quasi-static steps.
    elapsed: f64,
    slips: &'a [Slip],
    /// Each vertex's unit normal at the step's start, along which air drags it and pressure
    /// pushes it.
    normals: &'a [Point],
    /// Each vertex's moving hold, if any, indexed by vertex.
    holds: &'a [Option<Hold>],
    /// The stiffness of the spring that pulls each moving hold to its target, in newtons per
    /// meter.
    stiffness: f64,
}

impl<'a> Incremental<'a> {
    /// The terms of no time step: positions `x` alone, without inertia, friction or holds.
    fn still(x: &'a [Point]) -> Self {
        Self {
            start: x,
            predicted: x,
            inertia: 0.0,
            elapsed: 0.0,
            slips: &[],
            normals: &[],
            holds: &[],
            stiffness: 0.0,
        }
    }

    /// A vertex's moving hold, if any.
    fn hold(&self, vertex: usize) -> Option<Hold> {
        self.holds.get(vertex).copied().flatten()
    }
}

/// One vertex's terms of the incremental potential, with their gradient and the diagonal of their
/// Hessian.
#[derive(Clone, Copy, Debug, Default)]
struct VertexTerms {
    gravity: f64,
    loads: f64,
    inertia: f64,
    holding: f64,
    gradient: Point,
    diagonal: f64,
}

/// A held vertex moving over a time step: where it goes, and the force the augmented Lagrangian
/// has raised to bring it there.
#[derive(Clone, Copy, Debug)]
struct Hold {
    target: Point,
    multiplier: Point,
    /// The stiffness of a hand that holds the vertex by a spring, or `None` for one that carries
    /// it exactly.
    spring: Option<f64>,
}

impl Hold {
    /// The augmented Lagrangian at position `p`, with spring stiffness `k`: its value, its
    /// gradient and the diagonal of its Hessian. A hold by a spring takes the spring's
    /// stiffness, and its multiplier stays zero.
    fn terms(&self, p: &Point, k: f64) -> (f64, Point, f64) {
        let k = self.spring.unwrap_or(k);
        let gap = p - self.target;
        (
            0.5 * k * gap.norm_squared() - self.multiplier.dot(&gap),
            gap * k - self.multiplier,
            k,
        )
    }
}

/// What a line search needs from its Newton iteration.
struct Search<'a, 'b> {
    incremental: &'b Incremental<'b>,
    rigidity: &'a OnceCell<Result<Factor<'a>, SingularError>>,
    /// The incremental potential where the update starts.
    potential: Potential,
    slope: f64,
    /// The longest fraction of the update that collision detection allows.
    safe: f64,
    /// The pairs that may touch along the update.
    pairs: &'b [Pair],
}

/// The time a time step of length `dt` spans, in seconds: none for a quasi-static step.
fn elapsed(dt: f64) -> f64 {
    if dt.is_finite() { dt } else { 0.0 }
}

/// Every pair of a contact's four vertices, the greater first.
fn vertex_pairs(stencil: [Option<u32>; 4]) -> Vec<(u32, u32)> {
    let v: Vec<u32> = stencil.into_iter().flatten().collect();
    (0..v.len())
        .flat_map(|i| (0..i).map(move |j| (i, j)))
        .map(|(i, j)| (v[i].max(v[j]), v[i].min(v[j])))
        .collect()
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
        axes: Vec::new(),
        sheet: Vec::new(),
        rest: Vec::new(),
        half: Vec::new(),
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
        primitives.half.extend(std::iter::repeat_n(
            0.5 * sheet.material().thickness,
            mesh.vertex_count(),
        ));
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
    use crate::settings::{Solver, Table};
    use crate::sheet::Crease;

    /// A small deterministic wobble, to keep every quantity away from symmetric values.
    fn wobble(v: usize, amplitude: f64) -> Point {
        #[expect(clippy::cast_precision_loss, reason = "the index only seeds a wobble")]
        let k = v as f64;
        Point::new((1.3 * k).sin(), (2.1 * k).cos(), (0.7 * k + 0.4).sin()) * amplitude
    }

    /// Checks the assembled gradient and Hessian at `x` against central differences of the
    /// incremental potential and of the gradient, to a relative tolerance.
    ///
    /// The time step starts at `start`, where friction is lagged, and inertia carries every
    /// vertex about a millimeter from there. Every fifth vertex moves on a hold toward a target
    /// a little away.
    fn check(simulation: &Simulation, x: &[Point], start: &[Point], step: f64) {
        let predicted: Vec<Point> = start
            .iter()
            .map(|p| p + Point::new(0.3, -0.2, 1.0) * 1e-3)
            .collect();
        let slips = simulation.slips(start);
        let normals = simulation.normals(start);
        let holds: Vec<Option<Hold>> = start
            .iter()
            .enumerate()
            .map(|(v, p)| {
                // Every other hold is a hand's spring.
                (v % 5 == 0).then(|| Hold {
                    target: p + wobble(v + 11, 1e-5),
                    multiplier: wobble(v + 13, 1e-3),
                    spring: (v % 10 == 5).then_some(700.0),
                })
            })
            .collect();
        let incremental = Incremental {
            start,
            predicted: &predicted,
            inertia: 3600.0,
            elapsed: 1.0 / 60.0,
            slips: &slips,
            normals: &normals,
            holds: &holds,
            stiffness: 50.0,
        };
        let pairs = simulation.candidates(x, x);
        let pattern = Pattern::new(
            x.len(),
            simulation
                .pairs
                .iter()
                .copied()
                .chain(pairs.iter().flat_map(|p| vertex_pairs(p.stencil()))),
        );
        // Every vertex is free here, the bodies' points too, so every derivative is checked.
        let free = vec![false; x.len()];
        let assemble = |y: &[Point]| {
            let mut assembly = Assembly::new(&pattern, &free);
            simulation
                .derivatives(y, &incremental, &pairs)
                .scatter(&mut assembly, false);
            assembly
        };
        let gradient = |y: &[Point]| assemble(y).gradient;
        let at_x = assemble(x);
        let hessian = at_x.dense();
        let potential = |y: &[Point]| simulation.potential(y, &incremental, &pairs).total();
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
        // stretched along x into its strain barrier. The crease yields, hardens and ages.
        let mut sheet =
            Sheet::new(Mesh::grid(3, 3, 0.01, 0.01).unwrap(), Material::kami()).unwrap();
        let at = |i: u32, j: u32| j * 4 + i;
        for j in 0..3 {
            let edge = [at(1, j), at(1, j + 1)];
            sheet
                .set_crease(
                    edge,
                    Some(Crease {
                        yield_moment: 1e-3,
                        hardening: 2e-3,
                        aging_rate: 0.1,
                        ..Crease::kami()
                    }),
                )
                .unwrap();
            sheet.set_rest_angle(edge, 0.9).unwrap();
            sheet
                .set_rest_angle([at(2, j), at(2, j + 1)], -0.2)
                .unwrap();
        }
        let mut simulation = Simulation::new(vec![sheet], Settings::default()).unwrap();
        // A hand pushes one vertex, and air presses on the sheet's front.
        simulation
            .set_forces(0, &[(5, [0.01, -0.02, 0.03])])
            .unwrap();
        simulation.set_pressure(0, 30.0).unwrap();
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
        let energy = simulation
            .potential(&x, &Incremental::still(&x), &[])
            .energy;
        assert!(energy.strain_barrier > 0.0 && energy.creases > 0.0 && energy.bending > 0.0);
        assert!(energy.yielding > 0.0);
        // The step started a tenth of a millimeter away, so inertia and air drag act.
        let start: Vec<Point> = x
            .iter()
            .enumerate()
            .map(|(v, p)| p - wobble(v + 3, 1e-4))
            .collect();
        check(&simulation, &x, &start, 1e-9);
    }

    #[test]
    fn the_assembly_differentiates_contact() {
        // A small sheet lies turned over a larger one, which lies on the table, each within
        // the reach of contact. At a turn of 0.02 rad, edges lie within the fade of parallel.
        for turn in [0.3, 0.02] {
            let (h, gap) = (70e-6, 2e-6);
            let lower =
                Sheet::new(Mesh::grid(3, 3, 0.01, 0.01).unwrap(), Material::kami()).unwrap();
            let upper =
                Sheet::new(Mesh::grid(2, 2, 0.006, 0.006).unwrap(), Material::kami()).unwrap();
            let settings = Settings {
                table: Some(Table::default()),
                ..Settings::default()
            };
            let simulation = Simulation::new(vec![lower, upper], settings).unwrap();
            let rotation = nalgebra::Rotation3::from_axis_angle(&Vector3::z_axis(), turn);
            let x: Vec<Point> = simulation
                .positions
                .iter()
                .enumerate()
                .map(|(v, p)| {
                    let flat = Point::new(p[0], p[1], 0.0);
                    let placed = if v < 16 {
                        flat + Point::new(0.0, 0.0, 0.5 * h + 0.6 * gap)
                    } else {
                        rotation * flat + Point::new(0.003, 0.001, 1.5 * h + 1.2 * gap)
                    };
                    placed + wobble(v, 0.1 * gap)
                })
                .collect();
            let pairs = simulation.candidates(&x, &x);
            let energy = simulation
                .potential(&x, &Incremental::still(&x), &pairs)
                .energy;
            assert!(
                energy.contact > 0.0 && pairs.len() > 10,
                "{} pairs",
                pairs.len()
            );
            // The step started where each vertex lay up to 0.3 µm away in the plane. Some
            // contacts thus slip past the stick distance, and some within it.
            let start: Vec<Point> = x
                .iter()
                .enumerate()
                .map(|(v, p)| {
                    let shift = wobble(v + 7, 3e-7);
                    p - Point::new(shift[0], shift[1], 0.0)
                })
                .collect();
            let slips = simulation.slips(&start);
            assert!(slips.len() > 10, "{} slips", slips.len());
            // The differences step well under the stick distance and the barrier's gap.
            check(&simulation, &x, &start, 1e-10);
        }
    }

    #[test]
    fn the_assembly_differentiates_contact_with_bodies() {
        // A capsule lies across a sheet's front and a sphere touches its back, each within the
        // reach of contact, with frictions of their own.
        let (h, gap) = (70e-6, 2e-6);
        let sheet = Sheet::new(Mesh::grid(3, 3, 0.01, 0.01).unwrap(), Material::kami()).unwrap();
        let mut simulation = Simulation::new(vec![sheet], Settings::default()).unwrap();
        let (finger, ball) = (0.002, 0.001);
        let above = 0.5 * h + finger + 0.6 * gap;
        simulation
            .add_body(
                Body {
                    radius: finger,
                    friction: 0.7,
                    stiffness: f64::INFINITY,
                },
                &[[0.002, 0.004, above], [0.008, 0.006, above]],
            )
            .unwrap();
        let below = -(0.5 * h + ball + 0.5 * gap);
        simulation
            .add_body(
                Body {
                    radius: ball,
                    friction: 0.2,
                    stiffness: f64::INFINITY,
                },
                &[[0.0045, 0.0052, below]],
            )
            .unwrap();
        // The whole scene turns, so no coordinate lies across the sheet alone, where only its
        // soft bending would scale the differences.
        let turn = nalgebra::Rotation3::from_scaled_axis(Vector3::new(0.4, -0.3, 0.2));
        let x: Vec<Point> = simulation
            .positions
            .iter()
            .enumerate()
            .map(|(v, &p)| turn * (Point::from(p) + wobble(v, 0.05 * gap)))
            .collect();
        let pairs = simulation.candidates(&x, &x);
        let energy = simulation
            .potential(&x, &Incremental::still(&x), &pairs)
            .energy;
        assert!(energy.contact > 0.0, "{} pairs", pairs.len());
        assert!(pairs.iter().any(|p| p.kind == contact::Kind::PointEdge));
        let start: Vec<Point> = x
            .iter()
            .enumerate()
            .map(|(v, p)| {
                let shift = wobble(v + 7, 3e-7);
                p - turn * Point::new(shift[0], shift[1], 0.0)
            })
            .collect();
        assert!(!simulation.slips(&start).is_empty());
        check(&simulation, &x, &start, 1e-10);
    }

    #[test]
    fn a_body_joins_and_moves_only_with_sound_points() {
        let sheet = Sheet::new(Mesh::grid(3, 3, 0.01, 0.01).unwrap(), Material::kami()).unwrap();
        let mut simulation = Simulation::new(vec![sheet], Settings::default()).unwrap();
        let finger = Body {
            radius: 0.002,
            friction: 0.5,
            stiffness: f64::INFINITY,
        };
        let refusals = [
            (finger, vec![[0.0; 3]; 3], BodyError::Points { given: 3 }),
            (
                finger,
                vec![[0.0, 0.0, 0.01], [0.0, f64::NAN, 0.01]],
                BodyError::NotFinite { point: 1 },
            ),
            (finger, vec![[0.0, 0.0, 0.01]; 2], BodyError::Degenerate),
            // The sphere's surface lies 1 µm from the paper's surface.
            (
                finger,
                vec![[0.005, 0.005, 0.002 + 35e-6 - 1e-6]],
                BodyError::Overlapping,
            ),
            (
                Body {
                    radius: -1.0,
                    ..finger
                },
                vec![[0.0, 0.0, 0.01]],
                BodyError::Parameter(ParameterError {
                    name: "body radius",
                    value: -1.0,
                    requirement: "positive and finite",
                }),
            ),
        ];
        for (body, points, error) in refusals {
            assert_eq!(simulation.add_body(body, &points), Err(error));
        }
        // A refused body leaves the scene as it was.
        assert_eq!(simulation.positions.len(), 16);
        assert!(simulation.bodies().is_empty());
        let ends = [[0.0, 0.0, 0.01], [0.01, 0.0, 0.01]];
        let tip = simulation.add_body(finger, &ends).unwrap();
        assert_eq!(simulation.body_points(tip), ends);
        assert_eq!(
            simulation
                .move_body(tip, &ends[..1])
                .unwrap_err()
                .to_string(),
            "the body has 2 points, so 1 point cannot move it"
        );
        assert_eq!(
            simulation.move_body(tip, &[ends[0]; 2]),
            Err(BodyError::Degenerate)
        );
        assert!(simulation.targets.iter().all(Option::is_none));
    }

    #[test]
    fn friction_lagged_from_a_touch_outlasts_the_touch() {
        // A sphere touching paper on the table lifts off it, while another pushes paper into the
        // table. The push takes the augmented Lagrangian round after round. Each round keeps
        // the friction lagged from the lifted sphere's touch, though its contact has gone.
        let kami = Material::kami();
        let gap = Settings::default().contact.gap;
        let sheet = Sheet::new(Mesh::grid(4, 4, 0.02, 0.02).unwrap(), kami).unwrap();
        let dt = 1.0 / 240.0;
        let settings = Settings {
            time_step: dt,
            table: Some(Table::default()),
            solver: Solver {
                shortest_step: 0.6 * dt,
                ..Solver::default()
            },
            ..Settings::default()
        };
        let mut simulation = Simulation::new(vec![sheet], settings).unwrap();
        let lying = 0.5 * kami.thickness + 0.5 * gap;
        let flat: Vec<[f64; 3]> = simulation
            .positions(0)
            .iter()
            .map(|&[x, y, _]| [x, y, lying])
            .collect();
        simulation.set_positions(0, &flat).unwrap();
        let radius = 0.003;
        let touching = lying + 0.5 * kami.thickness + radius + 0.5 * gap;
        let body = Body {
            radius,
            friction: 0.5,
            stiffness: f64::INFINITY,
        };
        let lifted = simulation
            .add_body(body, &[[0.006, 0.01, touching]])
            .unwrap();
        let pushing = simulation
            .add_body(body, &[[0.014, 0.01, touching]])
            .unwrap();
        simulation
            .move_body(lifted, &[[0.006, 0.01, touching + 1e-3]])
            .unwrap();
        simulation
            .move_body(pushing, &[[0.014, 0.01, touching - 1e-4]])
            .unwrap();
        let report = simulation.step().unwrap();
        assert!(!report.converged && report.least_gap > 0.0, "{report:?}");
    }

    #[test]
    fn a_grip_holds_paper_until_its_body_lets_go() {
        let sheet = Sheet::new(Mesh::grid(2, 2, 0.01, 0.01).unwrap(), Material::kami()).unwrap();
        let mut simulation = Simulation::new(vec![sheet], Settings::default()).unwrap();
        let finger = Body {
            radius: 0.002,
            friction: 0.5,
            stiffness: f64::INFINITY,
        };
        let tip = simulation
            .add_body(finger, &[[0.0, 0.0, 0.01], [0.0, 0.01, 0.01]])
            .unwrap();
        let other = simulation.add_body(finger, &[[0.01, 0.0, 0.01]]).unwrap();
        simulation.grip(tip, 0, &[0, 3, 6]);
        simulation.grip(other, 0, &[6]);
        let gripped: Vec<Option<usize>> = (0..9)
            .map(|v| match v {
                0 | 3 => Some(tip),
                6 => Some(other),
                _ => None,
            })
            .collect();
        assert_eq!(simulation.grips(0), gripped);
        assert!(simulation.held(0)[6]);
        assert_eq!(
            simulation.move_held(0, &[(3, [0.0; 3])]),
            Err(StateError::Gripped {
                vertex: 3,
                body: tip
            })
        );
        // A hand's hold takes a vertex from its grip, and a release frees the rest.
        simulation.hold(0, 3, true);
        assert_eq!(simulation.grips(0)[3], None);
        simulation.release(tip);
        assert_eq!(simulation.grips(0)[0], None);
        assert_eq!(simulation.held(0)[..4], [false, false, false, true]);
        assert_eq!(simulation.grips(0)[6], Some(other));
    }

    #[test]
    fn a_turning_capsule_carries_the_paper_it_grips() {
        // A capsule a centimeter over a square grips its near edge. The capsule then turns a
        // quarter turn about the vertical and rises 2 cm, in four quasi-static steps. The edge
        // turns and rises with it, and the capsule carries the square's weight.
        let side = 0.02;
        let sheet = Sheet::new(Mesh::grid(4, 4, side, side).unwrap(), Material::kami()).unwrap();
        let settings = Settings {
            time_step: f64::INFINITY,
            solver: Solver {
                tolerance: 1e-12,
                ..Solver::default()
            },
            ..Settings::default()
        };
        let mut simulation = Simulation::new(vec![sheet], settings).unwrap();
        let ends = [[0.0, 0.0, 0.01], [0.0, side, 0.01]].map(Point::from);
        let tip = simulation
            .add_body(
                Body {
                    radius: 0.002,
                    friction: 0.5,
                    stiffness: f64::INFINITY,
                },
                &ends.map(Into::into),
            )
            .unwrap();
        let edge: Vec<usize> = (0..25).filter(|v| v % 5 == 0).collect();
        simulation.grip(tip, 0, &edge);
        for step in 1..=4 {
            let turn = nalgebra::Rotation3::from_axis_angle(
                &Vector3::z_axis(),
                std::f64::consts::FRAC_PI_8 * f64::from(step),
            );
            let lift = Vector3::new(0.0, 0.0, 0.005 * f64::from(step));
            let moved = ends.map(|p| turn * p + lift);
            let from: Vec<Point> = simulation
                .body_points(tip)
                .iter()
                .map(|&p| p.into())
                .collect();
            let motion = Motion::of(&from, &moved);
            let before = simulation.positions(0).to_vec();
            simulation.move_body(tip, &moved.map(Into::into)).unwrap();
            let report = simulation.step().unwrap();
            assert!(report.converged, "{report:?}");
            for &v in &edge {
                let carried = motion.carry(&before[v].into());
                let p = Point::from(simulation.positions(0)[v]);
                assert!(
                    (p - carried).norm() < 1e-12,
                    "vertex {v} lies {p}, not {carried}"
                );
            }
        }
        let weight = Material::kami().mass_per_area * side * side * 9.81;
        let [x, y, z] = simulation.body_force(tip);
        assert!(
            (z / weight - 1.0).abs() < 1e-6 && x.abs().max(y.abs()) < 1e-6 * weight,
            "the capsule carries [{x}, {y}, {z}] N of a {weight} N weight"
        );
    }

    /// A square of kami gliding at 1 m/s in its plane through a vacuum.
    ///
    /// The trust radius is a millimeter, and the time steps last 1/60 s.
    fn glide(solver: Solver) -> Simulation {
        let settings = Settings {
            time_step: 1.0 / 60.0,
            air: None,
            solver: Solver {
                trust_radius: 1e-3,
                ..solver
            },
            ..Settings::default()
        };
        let sheet = Sheet::new(Mesh::grid(2, 2, 0.1, 0.1).unwrap(), Material::kami()).unwrap();
        let mut simulation = Simulation::new(vec![sheet], settings).unwrap();
        simulation.set_velocities(0, &[[1.0, 0.0, 0.0]; 9]).unwrap();
        simulation
    }

    #[test]
    fn a_time_step_the_solver_cannot_finish_halves_into_two_it_can() {
        // A step moves the square 17 mm. The update starts at the trust radius and doubles,
        // so Newton's method spans the step in six iterations and each half step in five.
        let mut simulation = glide(Solver {
            iteration_limit: 5,
            ..Solver::default()
        });
        let rest = simulation.positions(0).to_vec();
        let report = simulation.step().unwrap();
        assert!(report.converged && report.substeps == 2, "{report:?}");
        assert_eq!(
            report.iterations, 15,
            "the halves follow five spent iterations"
        );
        let dt = 1.0 / 60.0;
        assert!((report.time - dt).abs() < f64::EPSILON);
        // Implicit Euler over two half steps h: the speed gains g h twice, and the drop is
        // h (g h + 2 g h).
        let h = 0.5 * dt;
        let drop = 3.0 * 9.81 * h * h;
        for (p, r) in simulation.positions(0).iter().zip(&rest) {
            assert!((p[0] - r[0] - dt).abs() < 1e-12 && (p[2] + drop).abs() < 1e-12);
        }
    }

    #[test]
    fn a_time_step_that_never_converges_halves_down_to_the_shortest_time_step() {
        // One iteration never confirms a move, so every attempt fails. A fifth of the time
        // step lies between a quarter and an eighth of it, so the step ends as four quarters.
        let dt = 1.0 / 60.0;
        let mut simulation = glide(Solver {
            iteration_limit: 1,
            shortest_step: 0.2 * dt,
            ..Solver::default()
        });
        let report = simulation.step().unwrap();
        assert!(!report.converged && report.substeps == 4, "{report:?}");
        assert_eq!(report.iterations, 1 + 2 + 4);
        assert!((report.time - dt).abs() < f64::EPSILON);
    }
}
