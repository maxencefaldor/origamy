//! Sharp creases: elastic about a rest angle, and plastic past a yield moment.
//!
//! A crease turned from its rest angle θ̄ to θ carries the moment m = k (θ − θ̄) per length,
//! with k its stiffness. Past the yield moment, the crease yields: its rest angle follows the
//! turn, and the moment stays at the yield moment. Hardening raises the yield moment with the
//! rotation the crease has yielded through. Released, the crease springs back by its moment over
//! its stiffness.
//!
//! A time step yields each crease by the variational update of plasticity. Within the step, the
//! crease's energy is the least, over how far it yields, of its elastic energy and the work that
//! yielding takes. That energy is convex and continuously differentiable in the angle, and its
//! slope is the moment.

use crate::sheet::Crease;

/// Where a crease rests, and how far it has yielded.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Plastic {
    /// The rest angle, in radians.
    pub rest: f64,
    /// The rotation the crease has yielded through, in radians, which hardening reads.
    pub rotation: f64,
}

/// A crease turned to an angle within a time step, yielded as far as it must.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Turn {
    /// The elastic energy about the rest angle the crease yields to, in joules.
    pub elastic: f64,
    /// The work yielding takes, dissipated or stored by hardening, in joules.
    pub yielding: f64,
    /// The moment, the slope of the energy in the angle, in newton meters.
    pub moment: f64,
    /// The slope of the moment in the angle, in newton meters per radian.
    pub stiffness: f64,
    /// Where the crease rests, and how far it has yielded, at the end of the step.
    pub state: Plastic,
}

/// A crease of rest length `length`, in state `start` at the start of a time step, turned to
/// `angle`.
pub(crate) fn turn(crease: &Crease, length: f64, start: Plastic, angle: f64) -> Turn {
    let k = crease.stiffness * length;
    let h = crease.hardening * length;
    let reach = (crease.yield_moment + crease.hardening * start.rotation) * length;
    let trial = angle - start.rest;
    // An infinite yield moment gives an excess of −∞, so the crease stays elastic.
    let excess = k * trial.abs() - reach;
    if excess <= 0.0 {
        return Turn {
            elastic: 0.5 * k * trial * trial,
            yielding: 0.0,
            moment: k * trial,
            stiffness: k,
            state: start,
        };
    }
    // The yield condition holds at the end of the step: k (|θ − θ̄| − Δγ) = r + h Δγ.
    let flow = excess / (k + h);
    let rest = start.rest + flow.copysign(trial);
    let elastic = angle - rest;
    Turn {
        elastic: 0.5 * k * elastic * elastic,
        yielding: reach * flow + 0.5 * h * flow * flow,
        moment: k * elastic,
        stiffness: k * h / (k + h),
        state: Plastic {
            rest,
            rotation: start.rotation + flow,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KAMI: Crease = Crease::kami();

    #[test]
    fn a_crease_rests_at_its_angle() {
        let start = Plastic {
            rest: 0.5,
            rotation: 0.0,
        };
        let turned = turn(&KAMI, 0.01, start, 0.5);
        assert!(turned.elastic.abs() < 1e-18 && turned.moment.abs() < 1e-18);
        assert!((turned.stiffness - 9.0e-5).abs() < 1e-18);
        assert_eq!(turned.state, start);
    }

    #[test]
    fn a_crease_yields_at_its_yield_moment_and_springs_back_by_it() {
        // Turned a radian from flat, past the 0.63 rad its yield moment allows, the crease
        // yields. The crease then rests 0.63 rad short of the turn, and springs back by that.
        let (length, angle) = (0.01, 1.0);
        let turned = turn(&KAMI, length, Plastic::default(), angle);
        let springback = KAMI.yield_moment / KAMI.stiffness;
        assert!((turned.moment - KAMI.yield_moment * length).abs() < 1e-15);
        assert!((angle - turned.state.rest - springback).abs() < 1e-12);
        assert!((turned.state.rotation - turned.state.rest).abs() < 1e-15);
        // Without hardening, the yielded crease turns freely: its moment no longer grows.
        assert!(turned.stiffness.abs() < f64::MIN_POSITIVE);
        assert!((turned.yielding - turned.moment * turned.state.rotation).abs() < 1e-15);
    }

    #[test]
    fn hardening_raises_the_yield_moment_with_the_rotation_yielded() {
        let hardened = Crease {
            hardening: 4.0e-3,
            ..KAMI
        };
        let start = Plastic {
            rest: 0.2,
            rotation: 0.5,
        };
        let turned = turn(&hardened, 1.0, start, -1.5);
        let flow = turned.state.rotation - start.rotation;
        // The crease yields toward a negative turn, and the yield moment has grown.
        assert!((turned.state.rest - (start.rest - flow)).abs() < 1e-15);
        let reach = hardened.yield_moment + hardened.hardening * turned.state.rotation;
        assert!((turned.moment + reach).abs() < 1e-15);
        let tangent =
            hardened.stiffness * hardened.hardening / (hardened.stiffness + hardened.hardening);
        assert!((turned.stiffness - tangent).abs() < 1e-18);
    }

    #[test]
    fn an_infinite_yield_moment_keeps_a_crease_elastic() {
        let elastic = Crease {
            yield_moment: f64::INFINITY,
            ..KAMI
        };
        let turned = turn(&elastic, 1.0, Plastic::default(), 3.0);
        assert_eq!(turned.state, Plastic::default());
        assert!((turned.moment - 3.0 * KAMI.stiffness).abs() < 1e-15);
    }

    #[test]
    fn the_energy_is_continuously_differentiable_with_the_moment_as_its_slope() {
        let hardened = Crease {
            hardening: 2.0e-3,
            ..KAMI
        };
        let start = Plastic {
            rest: -0.3,
            rotation: 0.1,
        };
        let energy = |angle: f64| {
            let turned = turn(&hardened, 0.02, start, angle);
            turned.elastic + turned.yielding
        };
        let step = 1e-6;
        // Elastic and yielding angles on both sides, away from the kinks at the yield moment.
        for angle in [-1.4, -1.1, -0.3, 0.1, 0.6, 1.2] {
            let turned = turn(&hardened, 0.02, start, angle);
            let slope = (energy(angle + step) - energy(angle - step)) / (2.0 * step);
            let moment = |a: f64| turn(&hardened, 0.02, start, a).moment;
            let curvature = (moment(angle + step) - moment(angle - step)) / (2.0 * step);
            assert!((slope - turned.moment).abs() < 1e-9 * turned.moment.abs().max(1e-6));
            assert!((curvature - turned.stiffness).abs() < 1e-6 * turned.stiffness.max(1e-9));
        }
        // At the yield moment, the energy and its slope meet from both sides.
        let edge = start.rest
            + (hardened.yield_moment + hardened.hardening * start.rotation) / hardened.stiffness;
        let (below, above) = (
            turn(&hardened, 0.02, start, edge - 1e-12),
            turn(&hardened, 0.02, start, edge + 1e-12),
        );
        assert!((below.moment - above.moment).abs() < 1e-12);
    }
}
