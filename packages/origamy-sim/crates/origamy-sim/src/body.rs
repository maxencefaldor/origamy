//! Rigid bodies on paths: spheres and capsules that touch paper, as fingertips and tools do.
//!
//! A body is a sphere round one point, or a capsule round the segment between two points. A
//! hand moves its points along their paths, one time step at a time. Paper keeps its half
//! thickness plus the body's radius away from them, and the paper a body grips moves with it.

use nalgebra::{Rotation3, Unit};

use crate::energy::Point;
use crate::parameter::{ParameterError, non_negative, positive, require};

/// A rigid body that touches paper: a sphere or a capsule of a radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    /// The radius of the sphere, or of the capsule round its axis, in meters.
    pub radius: f64,
    /// Coulomb's coefficient of friction between the body and paper.
    pub friction: f64,
    /// The stiffness of the hand that moves the body, in newtons per meter; infinite for a
    /// hand that carries the body exactly along its path.
    ///
    /// A finite stiffness is a spring between the body and its path, shared among its points,
    /// as a finger pad yields when it presses. The body then presses with the stiffness times
    /// how far its path runs past where the paper stops it.
    pub stiffness: f64,
}

impl Body {
    /// Checks that every parameter takes a value it may.
    ///
    /// # Errors
    ///
    /// Returns the first parameter outside its values.
    pub fn check(&self) -> Result<(), ParameterError> {
        positive("body radius", self.radius)?;
        non_negative("body friction", self.friction)?;
        require(
            "body stiffness",
            self.stiffness,
            self.stiffness > 0.0,
            "positive, or infinite for a hand that carries the body exactly",
        )
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

/// A body's motion over a time step, which carries the paper it grips.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Motion {
    /// A sphere's motion: the translation of its center.
    Translation(Point),
    /// A capsule's motion, from its ends `from` to its ends `to`.
    ///
    /// A point keeps its share of the way along the axis. Its offset from the axis turns by
    /// `turn`, the least rotation that takes the old axis to the new.
    Axis {
        /// The ends before the motion.
        from: [Point; 2],
        /// The ends after the motion.
        to: [Point; 2],
        /// The least rotation from the old axis's direction to the new one's.
        turn: Rotation3<f64>,
    },
}

impl Motion {
    /// The motion of a body from its points `from` to its points `to`.
    ///
    /// A sphere has one point, and a capsule two that differ.
    pub(crate) fn of(from: &[Point], to: &[Point]) -> Self {
        match (from, to) {
            ([a], [b]) => Self::Translation(b - a),
            (&[a, b], &[c, d]) => Self::Axis {
                from: [a, b],
                to: [c, d],
                turn: least_rotation(&(b - a), &(d - c)),
            },
            _ => unreachable!("a body has 1 point, or 2"),
        }
    }

    /// Where the motion carries `point`.
    pub(crate) fn carry(&self, point: &Point) -> Point {
        match self {
            Self::Translation(shift) => point + shift,
            Self::Axis {
                from: [start, end],
                to: [new_start, new_end],
                turn,
            } => {
                let axis = end - start;
                let share = (point - start).dot(&axis) / axis.norm_squared();
                let offset = point - start - share * axis;
                new_start + share * (new_end - new_start) + turn * offset
            }
        }
    }
}

/// The least rotation that turns direction `u` to direction `v`, neither of them zero.
///
/// Opposite directions turn by a half turn about an axis across them.
fn least_rotation(u: &Point, v: &Point) -> Rotation3<f64> {
    let (u, v) = (u.normalize(), v.normalize());
    let across = u.cross(&v);
    let (sine, cosine) = (across.norm(), u.dot(&v));
    if sine > 0.0 {
        Rotation3::from_axis_angle(&Unit::new_unchecked(across / sine), sine.atan2(cosine))
    } else if cosine > 0.0 {
        Rotation3::identity()
    } else {
        // The axis lies across u, away from the coordinate axis u lies nearest.
        let nearest = u.iamax();
        let mut other = Point::zeros();
        other[(nearest + 1) % 3] = 1.0;
        let axis = Unit::new_normalize(u.cross(&other));
        Rotation3::from_axis_angle(&axis, std::f64::consts::PI)
    }
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
            stiffness: f64::INFINITY,
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
        let pad = Body {
            stiffness: 700.0,
            ..finger
        };
        assert!(pad.check().is_ok());
        for stiffness in [0.0, f64::NAN] {
            assert!(
                Body {
                    stiffness,
                    ..finger
                }
                .check()
                .is_err()
            );
        }
    }

    /// A unit direction from spherical angles.
    fn direction(polar: f64, azimuth: f64) -> Point {
        Point::new(
            polar.sin() * azimuth.cos(),
            polar.sin() * azimuth.sin(),
            polar.cos(),
        )
    }

    proptest::proptest! {
        #[test]
        fn the_least_rotation_turns_one_direction_to_the_other(
            polar in 0.0..std::f64::consts::PI,
            azimuth in 0.0..std::f64::consts::TAU,
            turn in 0.0..std::f64::consts::PI,
            across in 0.0..std::f64::consts::TAU,
            length in 1e-3..1e3,
        ) {
            let u = direction(polar, azimuth);
            // The second direction lies `turn` from the first, toward a direction across it.
            let side = u.cross(&direction(polar + 1.0, azimuth + 2.0)).normalize();
            let side = nalgebra::Rotation3::from_axis_angle(&Unit::new_normalize(u), across) * side;
            let v = u * turn.cos() + side * turn.sin();
            let rotation = least_rotation(&(u * length), &(v / length));
            proptest::prop_assert!((rotation * u - v).norm() < 1e-12);
            // The least rotation turns about the axis across both, by the angle between them.
            proptest::prop_assert!((rotation.angle() - turn).abs() < 1e-9);
        }

        #[test]
        fn a_capsule_carries_paper_rigidly_when_it_keeps_its_length(
            polar in 0.0..std::f64::consts::PI,
            azimuth in 0.0..std::f64::consts::TAU,
            spin in -3.0..3.0_f64,
            shift in proptest::array::uniform3(-0.1..0.1_f64),
            point in proptest::array::uniform3(-0.05..0.05_f64),
        ) {
            let [a, b] = [Point::new(0.01, -0.02, 0.03), Point::new(0.04, 0.0, 0.01)];
            let rotation = nalgebra::Rotation3::from_axis_angle(
                &Unit::new_normalize(direction(polar, azimuth)),
                spin,
            );
            let shift = Point::from(shift);
            let motion = Motion::of(&[a, b], &[rotation * a + shift, rotation * b + shift]);
            let p = Point::from(point);
            let q = Point::new(-0.02, 0.05, 0.0);
            let (carried_p, carried_q) = (motion.carry(&p), motion.carry(&q));
            // The motion keeps distances, and carries the ends to their places.
            proptest::prop_assert!(((carried_p - carried_q).norm() - (p - q).norm()).abs() < 1e-12);
            proptest::prop_assert!(((carried_p - motion.carry(&a)).norm() - (p - a).norm()).abs() < 1e-12);
            proptest::prop_assert!((motion.carry(&b) - (rotation * b + shift)).norm() < 1e-12);
        }
    }

    #[test]
    fn a_body_carries_paper_by_its_motion() {
        let p = Point::new(0.3, -0.2, 0.5);
        let sphere = Motion::of(&[Point::zeros()], &[Point::new(1.0, 2.0, 3.0)]);
        assert_eq!(sphere.carry(&p), Point::new(1.3, 1.8, 3.5));
        // A capsule along x turns a half turn onto itself. The point keeps its share of the
        // axis, and its offset turns about an axis across x.
        let [a, b] = [Point::zeros(), Point::x()];
        let flipped = Motion::of(&[a, b], &[b, a]);
        let carried = flipped.carry(&p);
        assert!((carried.x - 0.7).abs() < 1e-15);
        assert!((Point::new(0.0, carried.y, carried.z).norm() - p.yz().norm()).abs() < 1e-15);
        assert!(
            ((carried - Point::new(0.7, 0.2, 0.5)).norm()
                * (carried - Point::new(0.7, -0.2, -0.5)).norm()
                * (carried - Point::new(0.7, 0.2, -0.5)).norm())
                < 1e-15
        );
        // A capsule that stretches carries a point along its axis in proportion.
        let stretched = Motion::of(&[a, b], &[a, 2.0 * b]);
        assert!((stretched.carry(&p) - Point::new(0.6, -0.2, 0.5)).norm() < 1e-15);
    }
}
