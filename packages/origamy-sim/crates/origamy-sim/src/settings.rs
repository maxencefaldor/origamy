//! The settings of a simulation: the world it runs in, how contact pushes and how the solver
//! works. Every setting is an input with a documented default.

use crate::parameter::{ParameterError, finite, fraction, non_negative, positive, require};

/// The settings of a simulation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// The acceleration of gravity, in meters per second squared.
    pub gravity: [f64; 3],
    /// The length of a time step, in seconds; infinite for quasi-static steps.
    pub time_step: f64,
    /// How the solver minimizes each time step.
    pub solver: Solver,
    /// How contact pushes paper apart.
    pub contact: Contact,
    /// The table paper rests on, if any.
    pub table: Option<Table>,
    /// The air paper moves through, if any.
    pub air: Option<Air>,
}

/// How the solver minimizes each time step: Newton's method with a backtracking line search.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Solver {
    /// The largest vertex move of a Newton update at which a time step has converged, in meters.
    pub tolerance: f64,
    /// The most Newton iterations a time step may take, in each lag of friction.
    pub iteration_limit: usize,
    /// The most times a time step lags friction: from its start, then from each solution it
    /// reaches, until the solution stays where it is.
    pub lags: usize,
    /// The shortest time step a time step that fails may halve to, in seconds.
    ///
    /// A time step whose solve fails retries as two time steps of half its length.
    pub shortest_step: f64,
    /// The stiffness of the spring that pulls a moving held vertex to its target.
    ///
    /// The stiffness is a multiple of the stiffest sheet's stretching stiffness, Young's modulus
    /// times thickness.
    pub hold_stiffness: f64,
    /// The most rounds of the augmented Lagrangian that carries moving held vertices.
    ///
    /// Each round minimizes the step, then raises the force on every hold short of its target.
    pub hold_rounds: usize,
    /// The longest move of any vertex a time step's first Newton update may take, in meters.
    ///
    /// A longer update shrinks to this trust radius. A step whose decrease of energy falls
    /// well short of its update's quadratic model shrinks the radius to a quarter of the step.
    /// An update shrunk and then taken whole, its decrease close to the model's, doubles it.
    pub trust_radius: f64,
    /// The fraction of the predicted decrease of energy a line search requires, between 0 and 1.
    pub sufficient_decrease: f64,
    /// The factor by which a line search shortens an update it refuses, between 0 and 1.
    pub backtracking: f64,
    /// The shortest fraction of a Newton update a line search tries, between 0 and 1.
    pub shortest_update: f64,
    /// The predicted decrease of energy, as a share of the energy's size, under which an update
    /// that stops shrinking ends a time step, between 0 and 1.
    ///
    /// Soft motion, as wrinkles allow, can keep Newton updates long once the energy has settled,
    /// while a converging update shrinks every iteration.
    pub stall: f64,
}

/// How contact pushes paper apart.
///
/// A barrier on each gap starts a little beyond the paper's thickness and grows without bound
/// as the gap closes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    /// The gap beyond a paper thickness at which the barrier starts, in meters.
    pub gap: f64,
    /// The barrier's energy scale, in joules: the larger, the less contact yields.
    pub stiffness: f64,
    /// The squared sine of the angle under which two edges' barrier fades as they turn
    /// parallel, at their rest lengths; between 0 and 1.
    pub parallel: f64,
    /// Coulomb's coefficient of friction between two pieces of paper.
    pub friction: f64,
    /// The slip within one time step under which friction holds a contact, in meters.
    ///
    /// Below it, the friction force grows smoothly with the slip, so a held contact creeps by
    /// less than this distance a step.
    pub stick: f64,
    /// The share of each sheet's thickness that contact keeps between its layers; above 0, and
    /// at most 1.
    ///
    /// Studies of folding as paper thins toward a mathematical sheet set it below 1.
    pub thickness_share: f64,
    /// Whether layers of paper push each other apart.
    ///
    /// Without it, paper passes through paper, as a sheet without thickness does, and contact
    /// still keeps paper above the table and off bodies.
    pub layers: bool,
    /// The stiffness of the penalty that keeps a triangle of paper in order, per area of the
    /// triangle, in pascals per meter.
    ///
    /// The penalty sums over three points of the triangle, each standing for a third of its
    /// rest area. Each point takes half the stiffness, times that area, times the square of how
    /// far it lies on the wrong side.
    pub order_stiffness: f64,
}

/// The air paper moves through, which resists paper moving across its surface.
///
/// Drag pushes each patch of paper against its velocity along the patch's normal. The force is
/// half the air's density, times the drag coefficient, the patch's area and that velocity
/// squared.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Air {
    /// The air's density, in kilograms per cubic meter.
    pub density: f64,
    /// The drag coefficient of paper moving across its surface.
    pub drag: f64,
}

impl Default for Air {
    /// Dry air at 20 °C at sea level, of density 1.204 kg/m³. The drag coefficient, 1.17, is a
    /// square plate's across the flow, from Hoerner's Fluid-Dynamic Drag.
    fn default() -> Self {
        Self {
            density: 1.204,
            drag: 1.17,
        }
    }
}

impl Air {
    /// Checks that every setting takes a value it may.
    ///
    /// # Errors
    ///
    /// Returns the first setting outside its values.
    pub fn check(&self) -> Result<(), ParameterError> {
        positive("air density", self.density)?;
        positive("drag coefficient", self.drag)
    }
}

/// A table: the horizontal plane paper cannot pass below.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Table {
    /// The height of the table's top, in meters.
    pub height: f64,
    /// Coulomb's coefficient of friction between paper and the table.
    pub friction: f64,
}

impl Default for Table {
    /// A table at height zero, as rough as paper.
    fn default() -> Self {
        Self {
            height: 0.0,
            friction: Contact::default().friction,
        }
    }
}

impl Default for Settings {
    /// Earth's gravity along −z, time steps of a sixtieth of a second, air, and no table.
    fn default() -> Self {
        Self {
            gravity: [0.0, 0.0, -9.81],
            time_step: 1.0 / 60.0,
            solver: Solver::default(),
            contact: Contact::default(),
            table: None,
            air: Some(Air::default()),
        }
    }
}

impl Default for Solver {
    /// A tolerance of 10 nm within 200 iterations, or an update that stops shrinking while it
    /// would lower the energy by under a billionth, and a line search that halves updates. The
    /// trust radius is a quarter meter, about the size of a sheet of origami paper. Friction
    /// lags up to three times a step, and a step halves down to 10 µs. A moving hold pulls a
    /// hundred times harder than the paper stretches, for up to ten rounds.
    fn default() -> Self {
        Self {
            tolerance: 1e-8,
            iteration_limit: 200,
            lags: 3,
            shortest_step: 1e-5,
            hold_stiffness: 100.0,
            hold_rounds: 10,
            trust_radius: 0.25,
            sufficient_decrease: 1e-4,
            backtracking: 0.5,
            shortest_update: 1e-12,
            stall: 1e-9,
        }
    }
}

impl Default for Contact {
    /// A barrier that starts 2 µm beyond the thickness, with an energy scale of 10 nJ. The
    /// barrier between two edges fades within about 1.8° of parallel. Friction between paper
    /// has a coefficient of 0.4, inside the measured ranges: 0.17 to 0.44 sliding, and 0.33 to
    /// 0.61 at rest. Friction holds contacts that slip less than 0.1 µm a step. Contact keeps
    /// each sheet's whole thickness. Paper under ten layers of kami sinks 60 nm into the layer
    /// below it, as orders keep it.
    fn default() -> Self {
        Self {
            gap: 2e-6,
            stiffness: 1e-8,
            parallel: 1e-3,
            friction: 0.4,
            stick: 1e-7,
            thickness_share: 1.0,
            layers: true,
            order_stiffness: 1e8,
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
        require(
            "time step",
            self.time_step,
            self.time_step > 0.0,
            "positive, or infinite for quasi-static steps",
        )?;
        self.solver.check()?;
        self.contact.check()?;
        self.table.as_ref().map_or(Ok(()), Table::check)?;
        self.air.as_ref().map_or(Ok(()), Air::check)
    }
}

impl Solver {
    /// Checks that every setting takes a value it may.
    ///
    /// # Errors
    ///
    /// Returns the first setting outside its values.
    pub fn check(&self) -> Result<(), ParameterError> {
        positive("tolerance", self.tolerance)?;
        #[expect(
            clippy::cast_precision_loss,
            reason = "the count only appears in a message"
        )]
        require(
            "iteration limit",
            self.iteration_limit as f64,
            self.iteration_limit > 0,
            "at least 1",
        )?;
        #[expect(
            clippy::cast_precision_loss,
            reason = "the count only appears in a message"
        )]
        require("lags", self.lags as f64, self.lags > 0, "at least 1")?;
        positive("shortest step", self.shortest_step)?;
        positive("hold stiffness", self.hold_stiffness)?;
        #[expect(
            clippy::cast_precision_loss,
            reason = "the count only appears in a message"
        )]
        require(
            "hold rounds",
            self.hold_rounds as f64,
            self.hold_rounds > 0,
            "at least 1",
        )?;
        positive("trust radius", self.trust_radius)?;
        fraction("sufficient decrease", self.sufficient_decrease)?;
        fraction("backtracking", self.backtracking)?;
        fraction("shortest update", self.shortest_update)?;
        fraction("stall", self.stall)
    }
}

impl Contact {
    /// Checks that every setting takes a value it may.
    ///
    /// # Errors
    ///
    /// Returns the first setting outside its values.
    pub fn check(&self) -> Result<(), ParameterError> {
        positive("contact gap", self.gap)?;
        positive("contact stiffness", self.stiffness)?;
        fraction("parallel", self.parallel)?;
        non_negative("friction", self.friction)?;
        positive("stick", self.stick)?;
        require(
            "thickness share",
            self.thickness_share,
            self.thickness_share > 0.0 && self.thickness_share <= 1.0,
            "above 0, and at most 1",
        )?;
        positive("order stiffness", self.order_stiffness)
    }
}

impl Table {
    /// Checks that every setting takes a value it may.
    ///
    /// # Errors
    ///
    /// Returns the first setting outside its values.
    pub fn check(&self) -> Result<(), ParameterError> {
        finite("table height", self.height)?;
        non_negative("table friction", self.friction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_valid() {
        Settings::default().check().unwrap();
        let with_table = Settings {
            table: Some(Table {
                height: -0.1,
                ..Table::default()
            }),
            time_step: f64::INFINITY,
            ..Settings::default()
        };
        with_table.check().unwrap();
    }

    #[test]
    fn each_setting_outside_its_values_is_named() {
        let base = Settings::default();
        let refused = [
            (
                Settings {
                    time_step: f64::NAN,
                    ..base
                },
                "time step",
            ),
            (
                Settings {
                    gravity: [0.0, f64::INFINITY, 0.0],
                    ..base
                },
                "gravity",
            ),
            (
                Settings {
                    solver: Solver {
                        iteration_limit: 0,
                        ..base.solver
                    },
                    ..base
                },
                "iteration limit",
            ),
            (
                Settings {
                    solver: Solver {
                        backtracking: 1.0,
                        ..base.solver
                    },
                    ..base
                },
                "backtracking",
            ),
            (
                Settings {
                    contact: Contact {
                        gap: 0.0,
                        ..base.contact
                    },
                    ..base
                },
                "contact gap",
            ),
            (
                Settings {
                    table: Some(Table {
                        height: f64::NEG_INFINITY,
                        ..Table::default()
                    }),
                    ..base
                },
                "table height",
            ),
        ];
        for (settings, name) in refused {
            assert_eq!(settings.check().unwrap_err().name, name);
        }
    }
}
