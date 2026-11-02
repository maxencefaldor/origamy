//! Sharp creases: elastic about a rest angle, plastic past a yield moment, and aging in time.
//!
//! A crease turned from its rest angle θ̄ to θ carries the moment m = k (θ − θ̄) per length,
//! with k its stiffness. Past the yield moment, the crease yields: its rest angle follows the
//! turn, and the moment stays at the yield moment. Hardening raises the yield moment with the
//! rotation the crease has flowed through. Released, the crease springs back by its moment over
//! its stiffness.
//!
//! Below the yield moment, the crease ages. Its rest angle creeps toward the turn at the rate
//! ω sinh(m / m*), as thermally activated flow does. Here ω is the aging rate, and m* the aging
//! moment. A crease held at an angle therefore loses moment logarithmically in time, and sets
//! deeper the longer it is held.
//!
//! A time step yields and ages each crease by the variational update of viscoplasticity. Within
//! the step, the crease's energy is the least, over how far it flows, of its elastic energy and
//! the work that flowing takes. That energy is convex and continuously differentiable in the
//! angle, and its slope is the moment.

use crate::sheet::Crease;

/// Where a crease rests, and how far it has flowed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Plastic {
    /// The rest angle, in radians.
    pub rest: f64,
    /// The rotation the crease has yielded and crept through, in radians, which hardening reads.
    pub rotation: f64,
}

/// A crease turned to an angle within a time step, flowed as far as it must.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Turn {
    /// The elastic energy about the rest angle the crease flows to, in joules.
    pub elastic: f64,
    /// The work flowing takes, dissipated or stored by hardening, in joules.
    pub yielding: f64,
    /// The moment, the slope of the energy in the angle, in newton meters.
    pub moment: f64,
    /// The slope of the moment in the angle, in newton meters per radian.
    pub stiffness: f64,
    /// Where the crease rests, and how far it has flowed, at the end of the step.
    pub state: Plastic,
}

/// A crease of rest length `length`, in state `start` at the start of a time step that spans
/// `elapsed` seconds, turned to `angle`.
///
/// Quasi-static steps span no time, so creases yield in them but do not age.
pub(crate) fn turn(crease: &Crease, length: f64, start: Plastic, angle: f64, elapsed: f64) -> Turn {
    let trial = angle - start.rest;
    let flow = Flow::of(crease, start.rotation, trial.abs(), elapsed);
    let flowed = flow.creep + flow.yielded;
    // The elastic part of the turn is what flowing leaves. The energy is then the step's
    // potential at the flow found, with the moment as its slope.
    let elastic = trial.abs() - flowed;
    Turn {
        elastic: 0.5 * length * crease.stiffness * elastic * elastic,
        yielding: length * flow.work,
        moment: (length * crease.stiffness * elastic).copysign(trial),
        stiffness: length * flow.stiffness,
        state: Plastic {
            rest: start.rest + flowed.copysign(trial),
            rotation: start.rotation + flowed,
        },
    }
}

/// How a crease turned by `turn` from rest flows within a time step, per length.
struct Flow {
    /// The rotation the crease creeps through, in radians.
    creep: f64,
    /// The rotation the crease yields through, in radians.
    yielded: f64,
    /// The work flowing takes, in joules per meter.
    work: f64,
    /// The slope of the moment's size in the turn, in newton meters per meter per radian.
    stiffness: f64,
}

impl Flow {
    /// The flow of `crease`, having flowed through `rotation`, turned by `turn` from rest over a
    /// time step of `elapsed` seconds.
    ///
    /// The moment m splits the turn: m / k springs back, c sinh(m / m*) creeps, and the rest
    /// yields. Here c is the aging rate times the step. The crease yields only once m reaches the
    /// yield moment r, and with hardening h then yields (m − r) / h.
    fn of(crease: &Crease, rotation: f64, turn: f64, elapsed: f64) -> Self {
        let Crease {
            stiffness: k,
            yield_moment,
            hardening: h,
            aging_moment: scale,
            aging_rate,
        } = *crease;
        let reach = yield_moment + h * rotation;
        let c = aging_rate * elapsed;
        // Without aging, creep vanishes, even where the hyperbolic functions overflow.
        let creep = |m: f64| if c > 0.0 { c * (m / scale).sinh() } else { 0.0 };
        let yielded = |m: f64| if m > reach { (m - reach) / h } else { 0.0 };
        // The slope of the turn in the moment.
        let slope = |m: f64| {
            let creeping = if c > 0.0 {
                c / scale * (m / scale).cosh()
            } else {
                0.0
            };
            let past = if m > reach { 1.0 / h } else { 0.0 };
            1.0 / k + creeping + past
        };
        // Without hardening, a crease whose turn passes what springs back and creeps at the
        // yield moment yields the rest at that moment.
        let capped = h == 0.0 && reach / k + creep(reach) <= turn;
        let moment = if capped {
            reach
        } else {
            // The turn grows with the moment, so the moment is the root of one increasing
            // function, between zero and the turn's elastic moment.
            root(
                |m: f64| m / k + creep(m) + yielded(m) - turn,
                slope,
                0.0,
                k * turn,
            )
        };
        let creep = creep(moment);
        let (yielded, stiffness) = if capped {
            (turn - moment / k - creep, 0.0)
        } else {
            (yielded(moment), 1.0 / slope(moment))
        };
        // The creep's work is the dual of its rate: c m* (u sinh u − cosh u + 1), u = m / m*.
        let creeping = if c > 0.0 {
            let u = moment / scale;
            let half = (0.5 * u).sinh();
            c * scale * (u * u.sinh() - 2.0 * half * half)
        } else {
            0.0
        };
        Self {
            creep,
            yielded,
            // A crease that never yields has an infinite reach, and yields nothing.
            work: creeping
                + if yielded > 0.0 {
                    reach * yielded + 0.5 * h * yielded * yielded
                } else {
                    0.0
                },
            stiffness,
        }
    }
}

/// The root of `f`, increasing between `low` and `high`, where it changes sign, by Newton's
/// method safeguarded by bisection.
fn root(f: impl Fn(f64) -> f64, slope: impl Fn(f64) -> f64, low: f64, high: f64) -> f64 {
    let (mut low, mut high) = (low, high);
    let mut x = high;
    for _ in 0..200 {
        let value = f(x);
        if value == 0.0 {
            return x;
        }
        if value > 0.0 {
            high = x;
        } else {
            low = x;
        }
        let newton = x - value / slope(x);
        let next = if newton > low && newton < high && newton.is_finite() {
            newton
        } else {
            0.5 * (low + high)
        };
        if (next - x).abs() <= 4.0 * f64::EPSILON * x.abs() || high - low <= f64::MIN_POSITIVE {
            return next;
        }
        x = next;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    const KAMI: Crease = Crease::kami();

    /// A crease that yields and hardens, and ages a million times faster than kami.
    const AGING: Crease = Crease {
        hardening: 2.0e-3,
        aging_rate: 4.25e-2,
        ..Crease::kami()
    };

    #[test]
    fn a_crease_rests_at_its_angle() {
        let start = Plastic {
            rest: 0.5,
            rotation: 0.0,
        };
        let turned = turn(&KAMI, 0.01, start, 0.5, 0.0);
        assert!(turned.elastic.abs() < 1e-18 && turned.moment.abs() < 1e-18);
        assert!((turned.stiffness - 9.0e-5).abs() < 1e-18);
        assert_eq!(turned.state, start);
        // Over a time step, creep softens the crease even at rest, by its linear rate.
        let (dt, length) = (0.01, 0.01);
        let aged = turn(&KAMI, length, start, 0.5, dt);
        let creep = KAMI.aging_rate * dt / KAMI.aging_moment;
        let softened = length / (1.0 / KAMI.stiffness + creep);
        assert!((aged.stiffness / softened - 1.0).abs() < 1e-15);
    }

    #[test]
    fn a_crease_yields_at_its_yield_moment_and_springs_back_by_it() {
        // Turned a radian from flat, past the 0.63 rad its yield moment allows, the crease
        // yields. The crease then rests 0.63 rad short of the turn, and springs back by that.
        let (length, angle) = (0.01, 1.0);
        let turned = turn(&KAMI, length, Plastic::default(), angle, 0.0);
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
        let turned = turn(&hardened, 1.0, start, -1.5, 0.0);
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
        let turned = turn(&elastic, 1.0, Plastic::default(), 3.0, 0.0);
        assert_eq!(turned.state, Plastic::default());
        assert!(turned.yielding.abs() < f64::MIN_POSITIVE);
        assert!((turned.moment - 3.0 * KAMI.stiffness).abs() < 1e-15);
    }

    #[test]
    fn a_held_crease_loses_moment_logarithmically_in_time() {
        // Held a radian from rest, past its yield, the crease relaxes from its yield moment by
        // m* ln(1 + t / τ). The time τ = 2 m* / (k ω e^(r / m*)) is about a second for kami.
        let tau = 2.0 * KAMI.aging_moment
            / (KAMI.stiffness * KAMI.aging_rate * (KAMI.yield_moment / KAMI.aging_moment).exp());
        let dt = 0.01;
        let mut state = Plastic::default();
        let mut moment = 0.0;
        let mut time = 0.0;
        for step in 1..=100_000_u32 {
            let turned = turn(&KAMI, 1.0, state, 1.0, dt);
            (state, moment, time) = (turned.state, turned.moment, f64::from(step) * dt);
            if [10.0, 100.0, 1000.0]
                .iter()
                .any(|t| (time - t).abs() < 0.5 * dt)
            {
                let expected = KAMI.yield_moment - KAMI.aging_moment * (1.0 + time / tau).ln();
                assert!(
                    (moment / expected - 1.0).abs() < 2e-3,
                    "at {time} s the moment is {moment}, the law says {expected}"
                );
            }
        }
        assert!(moment < KAMI.yield_moment && time > 999.0);
    }

    #[test]
    fn the_energy_is_continuously_differentiable_with_the_moment_as_its_slope() {
        let start = Plastic {
            rest: -0.3,
            rotation: 0.1,
        };
        for (crease, elapsed) in [(AGING, 0.0), (AGING, 0.02), (KAMI, 0.02)] {
            let energy = |angle: f64| {
                let turned = turn(&crease, 0.02, start, angle, elapsed);
                turned.elastic + turned.yielding
            };
            let moment = |angle: f64| turn(&crease, 0.02, start, angle, elapsed).moment;
            let step = 1e-6;
            // Angles on both sides that yield, and between them, away from the yield's kinks.
            for angle in [-1.4, -1.1, -0.3, 0.1, 0.6, 1.2] {
                let turned = turn(&crease, 0.02, start, angle, elapsed);
                let slope = (energy(angle + step) - energy(angle - step)) / (2.0 * step);
                let curvature = (moment(angle + step) - moment(angle - step)) / (2.0 * step);
                assert!(
                    (slope - turned.moment).abs() < 1e-9 * turned.moment.abs().max(1e-6),
                    "{angle}: slope {slope}, moment {}",
                    turned.moment
                );
                assert!(
                    (curvature - turned.stiffness).abs() < 1e-5 * turned.stiffness.max(1e-9),
                    "{angle}: curvature {curvature}, stiffness {}",
                    turned.stiffness
                );
            }
        }
    }

    #[test]
    fn creep_and_yield_split_the_turn() {
        // The moment springs back, creep and yield take the rest of the turn, and the rest
        // angle moves by both.
        for angle in [0.4, 2.0] {
            let turned = turn(&AGING, 1.0, Plastic::default(), angle, 0.05);
            let springback = turned.moment / AGING.stiffness;
            assert!((turned.state.rest + springback - angle).abs() < 1e-12);
            assert!(turned.state.rotation > 0.0 && turned.yielding > 0.0);
        }
    }
}
