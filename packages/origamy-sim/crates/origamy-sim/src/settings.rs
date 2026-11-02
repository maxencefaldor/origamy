//! The settings of a simulation: the world it runs in, how contact pushes and how the solver
//! works. Every setting is an input with a documented default.

use crate::parameter::{ParameterError, finite, fraction, positive, require};

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
}

/// How the solver minimizes each time step: Newton's method with a backtracking line search.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Solver {
    /// The largest vertex move of a Newton update at which a time step has converged, in meters.
    pub tolerance: f64,
    /// The most Newton iterations a time step may take.
    pub iteration_limit: usize,
    /// The longest move of any vertex a time step's first Newton update may take, in meters.
    ///
    /// A longer update shrinks to this trust radius. An update shrunk and then taken whole
    /// doubles the radius for the next update of the time step.
    pub trust_radius: f64,
    /// The fraction of the predicted decrease of energy a line search requires, between 0 and 1.
    pub sufficient_decrease: f64,
    /// The factor by which a line search shortens an update it refuses, between 0 and 1.
    pub backtracking: f64,
    /// The shortest fraction of a Newton update a line search tries, between 0 and 1.
    pub shortest_update: f64,
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
}

/// A table: the horizontal plane paper cannot pass below.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Table {
    /// The height of the table's top, in meters.
    pub height: f64,
}

impl Default for Settings {
    /// Earth's gravity along −z, time steps of a sixtieth of a second, and no table.
    fn default() -> Self {
        Self {
            gravity: [0.0, 0.0, -9.81],
            time_step: 1.0 / 60.0,
            solver: Solver::default(),
            contact: Contact::default(),
            table: None,
        }
    }
}

impl Default for Solver {
    /// A tolerance of 10 nm within 200 iterations, and a line search that halves updates. The
    /// trust radius is a quarter meter, about the size of a sheet of origami paper.
    fn default() -> Self {
        Self {
            tolerance: 1e-8,
            iteration_limit: 200,
            trust_radius: 0.25,
            sufficient_decrease: 1e-4,
            backtracking: 0.5,
            shortest_update: 1e-12,
        }
    }
}

impl Default for Contact {
    /// A barrier that starts 2 µm beyond the thickness, with an energy scale of 10 nJ.
    fn default() -> Self {
        Self {
            gap: 2e-6,
            stiffness: 1e-8,
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
        self.table.as_ref().map_or(Ok(()), Table::check)
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
        positive("trust radius", self.trust_radius)?;
        fraction("sufficient decrease", self.sufficient_decrease)?;
        fraction("backtracking", self.backtracking)?;
        fraction("shortest update", self.shortest_update)
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
        positive("contact stiffness", self.stiffness)
    }
}

impl Table {
    /// Checks that every setting takes a value it may.
    ///
    /// # Errors
    ///
    /// Returns the first setting outside its values.
    pub fn check(&self) -> Result<(), ParameterError> {
        finite("table height", self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_valid() {
        Settings::default().check().unwrap();
        let with_table = Settings {
            table: Some(Table { height: -0.1 }),
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
