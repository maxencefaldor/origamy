//! Rigid bodies on paths: spheres and capsules that touch paper, as fingertips and tools do.
//!
//! A body is a sphere round one point, or a capsule round the segment between two points. A
//! hand moves its points along their paths, one time step at a time. Paper keeps its half
//! thickness plus the body's radius away from them.

use crate::parameter::{ParameterError, non_negative, positive};

/// A rigid body that touches paper: a sphere or a capsule of a radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    /// The radius of the sphere, or of the capsule round its axis, in meters.
    pub radius: f64,
    /// Coulomb's coefficient of friction between the body and paper.
    pub friction: f64,
}

impl Body {
    /// Checks that every parameter takes a value it may.
    ///
    /// # Errors
    ///
    /// Returns the first parameter outside its values.
    pub fn check(&self) -> Result<(), ParameterError> {
        positive("body radius", self.radius)?;
        non_negative("body friction", self.friction)
    }
}

/// Why a body cannot join a simulation, or move.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum BodyError {
    /// A parameter lies outside its values.
    #[error(transparent)]
    Parameter(#[from] ParameterError),
    /// The points are neither one, for a sphere, nor two, for a capsule.
    #[error("a body has 1 point, for a sphere, or 2, for a capsule, but {given} were given")]
    Points {
        /// How many points were given.
        given: usize,
    },
    /// The points do not match the body's.
    #[error("the body has {}, so {} cannot move it", points(*expected), points(*given))]
    Count {
        /// How many points the body has.
        expected: usize,
        /// How many points were given.
        given: usize,
    },
    /// A coordinate is infinite or not a number.
    #[error("point {point} would lie at a coordinate that is not finite")]
    NotFinite {
        /// The point.
        point: usize,
    },
    /// A capsule's two ends coincide.
    #[error("the capsule's two ends coincide")]
    Degenerate,
    /// The body would lie closer to paper than the paper's half thickness plus its radius.
    #[error("the body lies closer to paper than its radius and the paper's half thickness")]
    Overlapping,
}

/// A count of points in words, such as "1 point" or "2 points".
fn points(count: usize) -> String {
    if count == 1 {
        "1 point".to_owned()
    } else {
        format!("{count} points")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_takes_a_positive_radius_and_a_friction() {
        let finger = Body {
            radius: 0.008,
            friction: 0.5,
        };
        assert!(finger.check().is_ok());
        let flat = Body {
            radius: 0.0,
            ..finger
        };
        assert_eq!(
            flat.check().unwrap_err().to_string(),
            "body radius is 0, but it must be positive and finite"
        );
        let slick = Body {
            friction: -0.1,
            ..finger
        };
        assert!(slick.check().is_err());
    }
}
