//! The exact numbers that explain floats, as `origamy.pattern` snaps coordinates.
//!
//! A candidate is a fraction, from the float's continued fraction, or `(p + q√r) / d`
//! for small integers, from lattice reduction. Its fit weighs its error by its size,
//! and the best candidate has the smallest fit, then the smallest size.

use num_bigint::{BigInt, BigUint, Sign};
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::lattice::relations;

/// The bounds on the candidates that may explain a float.
#[derive(Clone, Debug)]
pub struct Bounds {
    /// How far a candidate may lie from the float.
    pub tolerance: f64,
    /// The fit under which a fraction explains the float surely, so that no root is
    /// tried.
    pub sure: f64,
    /// The largest denominator of a fraction.
    pub denominator: u64,
    /// The integers whose square roots a candidate may hold.
    pub roots: Vec<u32>,
    /// The weight of a relation's error against its integers in lattice reduction.
    pub scale: i64,
    /// The largest integer of a candidate with a root.
    pub size: i64,
}

/// The best candidate for a float: `rational + surd·√root`, with its fit.
#[derive(Clone, Debug, PartialEq)]
pub struct Explanation {
    /// The error times the candidate's size squared.
    pub fit: f64,
    /// The rational part, as a numerator and a positive denominator in lowest terms.
    pub rational: (BigInt, BigInt),
    /// The coefficient of the root, likewise, zero for a fraction.
    pub surd: (BigInt, BigInt),
    /// The integer under the root, 1 for a fraction.
    pub root: u32,
}

/// The candidate that explains a finite float best within the bounds, if any.
#[must_use]
pub fn explain(value: f64, bounds: &Bounds) -> Option<Explanation> {
    // Each candidate with its fit and size, in the order found: the first of equals wins.
    let mut found: Vec<(f64, u64, Explanation)> = vec![];
    for (p, d) in convergents(value, bounds.denominator) {
        let error = (value - ratio(&p, &d)).abs();
        if error <= bounds.tolerance {
            // The denominator stays under the bound, a u64.
            let size = d.to_u64().unwrap_or(u64::MAX);
            let fit = error * ratio(&(&d * &d), &BigInt::one());
            let rational = (p, d);
            let surd = (BigInt::zero(), BigInt::one());
            found.push((
                fit,
                size,
                Explanation {
                    fit,
                    rational,
                    surd,
                    root: 1,
                },
            ));
        }
    }
    if !found.iter().any(|(fit, _, _)| *fit <= bounds.sure) {
        for &root in &bounds.roots {
            let surd = f64::from(root).sqrt();
            for (d, p, q) in relations(value, root, bounds.scale, bounds.size) {
                #[allow(clippy::cast_precision_loss, reason = "the integers are small")]
                let (df, pf, qf) = (d as f64, p as f64, q as f64);
                if (value - (pf + qf * surd) / df).abs() <= bounds.tolerance {
                    let size = d.max(p.abs()).max(q.abs());
                    #[allow(clippy::cast_precision_loss, reason = "the integers are small")]
                    let fit = (df * value - pf - qf * surd).abs() * (size * size) as f64;
                    let explanation = Explanation {
                        fit,
                        rational: lowest(BigInt::from(p), BigInt::from(d)),
                        surd: lowest(BigInt::from(q), BigInt::from(d)),
                        root,
                    };
                    found.push((fit, size.unsigned_abs(), explanation));
                }
            }
        }
    }
    let best = found
        .iter()
        .enumerate()
        .min_by(|(i, a), (j, b)| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(i.cmp(j)))?
        .0;
    Some(found.swap_remove(best).2)
}

/// A fraction in lowest terms, its denominator positive.
fn lowest(numerator: BigInt, denominator: BigInt) -> (BigInt, BigInt) {
    let g = numerator.gcd(&denominator);
    let (mut p, mut d) = (numerator / &g, denominator / &g);
    if d.is_negative() {
        (p, d) = (-p, -d);
    }
    (p, d)
}

/// The convergents of a finite float's continued fraction, exactly, while their
/// denominators stay at or under a bound.
fn convergents(value: f64, bound: u64) -> Vec<(BigInt, BigInt)> {
    let bound = BigInt::from(bound);
    let (mut n, mut d) = exact(value);
    let (mut previous, mut current) = (
        (BigInt::zero(), BigInt::one()),
        (BigInt::one(), BigInt::zero()),
    );
    let mut found = vec![];
    loop {
        let whole = n.div_floor(&d);
        let next = (
            &whole * &current.0 + &previous.0,
            &whole * &current.1 + &previous.1,
        );
        previous = std::mem::replace(&mut current, next);
        if current.1 > bound {
            return found;
        }
        found.push(current.clone());
        let rest = &n - &whole * &d;
        if rest.is_zero() {
            return found;
        }
        (n, d) = (d, rest);
    }
}

/// A finite float as an exact fraction, its denominator a positive power of two.
pub(crate) fn exact(value: f64) -> (BigInt, BigInt) {
    let bits = value.to_bits();
    let sign = if bits >> 63 == 1 {
        Sign::Minus
    } else {
        Sign::Plus
    };
    let exponent = i64::try_from((bits >> 52) & 0x7ff).expect("eleven bits");
    let fraction = bits & ((1 << 52) - 1);
    let (mut mantissa, mut power) = if exponent == 0 {
        (fraction, -1074)
    } else {
        (fraction | 1 << 52, exponent - 1075)
    };
    if mantissa != 0 {
        let zeros = mantissa.trailing_zeros();
        mantissa >>= zeros;
        power += i64::from(zeros);
    }
    let magnitude = BigInt::from_biguint(sign, BigUint::from(mantissa));
    if power >= 0 {
        (magnitude << power, BigInt::one())
    } else {
        (magnitude, BigInt::one() << -power)
    }
}

/// A fraction with a positive denominator as the nearest float, ties to even.
pub(crate) fn ratio(n: &BigInt, d: &BigInt) -> f64 {
    let limit = BigInt::one() << 53;
    if n.abs() <= limit && *d <= limit {
        return n.to_f64().expect("exact") / d.to_f64().expect("exact");
    }
    let (magnitude, denominator) = (n.magnitude(), d.magnitude());
    let bits = |x: &BigUint| i64::try_from(x.bits()).expect("bits fit");
    // A quotient of 55 or 56 bits, with a sticky bit for any remainder, rounds once.
    let shift = 55 - (bits(magnitude) - bits(denominator));
    let (top, bottom) = if shift >= 0 {
        (magnitude << shift, denominator.clone())
    } else {
        (magnitude.clone(), denominator << -shift)
    };
    let (quotient, remainder) = top.div_rem(&bottom);
    // The value's exponent, and the bits a float of it keeps: fewer once subnormal.
    let exponent = bits(&quotient) - 1 - shift;
    let precision = 53 - (-1022 - exponent).max(0);
    let extra = u64::try_from(bits(&quotient) - precision.max(0)).expect("a positive count");
    let mut mantissa = &quotient >> extra;
    let half = quotient.bit(extra - 1);
    let below = &quotient & ((BigUint::one() << (extra - 1)) - 1u32);
    if half && (!below.is_zero() || !remainder.is_zero() || mantissa.bit(0)) {
        mantissa += 1u32;
    }
    let power = i64::try_from(extra).expect("bits fit") - shift;
    let float = mantissa.to_f64().expect("54 bits at most") * power_of_two(power);
    if n.is_negative() { -float } else { float }
}

/// Two to a power, exactly, or infinity past the largest float.
fn power_of_two(power: i64) -> f64 {
    match power {
        1024.. => f64::INFINITY,
        -1022..=1023 => {
            f64::from_bits(u64::try_from(power + 1023).expect("a biased exponent") << 52)
        }
        -1074..=-1023 => f64::from_bits(1 << (power + 1074)),
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> Bounds {
        Bounds {
            tolerance: 1e-9,
            sure: 1e-8,
            denominator: 1_000_000,
            roots: vec![2, 3, 5, 6],
            scale: 1_000_000_000_000,
            size: 10_000,
        }
    }

    fn fraction(p: i64, d: i64) -> (BigInt, BigInt) {
        (BigInt::from(p), BigInt::from(d))
    }

    #[test]
    fn fractions_and_roots_explain_floats() {
        let half = explain(0.5, &bounds()).unwrap();
        assert_eq!((half.rational, half.root), (fraction(1, 2), 1));
        let third = explain(1.0 / 3.0, &bounds()).unwrap();
        assert_eq!(third.rational, fraction(1, 3));
        let golden = explain((1.0 + 5f64.sqrt()) / 4.0, &bounds()).unwrap();
        assert_eq!(
            (golden.rational, golden.surd, golden.root),
            (fraction(1, 4), fraction(1, 4), 5)
        );
        assert!(explain(0.123_456_789, &bounds()).unwrap().fit > 1e-4);
    }

    #[test]
    fn floats_convert_exactly() {
        assert_eq!(exact(0.75), fraction(3, 4));
        assert_eq!(exact(-6.0), fraction(-6, 1));
        assert_eq!(exact(f64::MIN_POSITIVE / 4.0).1, BigInt::one() << 1024);
    }

    #[test]
    fn ties_round_to_even() {
        let two = BigInt::from(2);
        let top = BigInt::one() << 54u32;
        // 2⁵³ + 1/2 and 2⁵³ + 3/2 lie halfway between floats two apart.
        assert_eq!(
            ratio(&(&top + 1u32), &two).to_bits(),
            (2f64.powi(53)).to_bits()
        );
        assert_eq!(
            ratio(&(&top + 3u32), &two).to_bits(),
            (2f64.powi(53) + 2.0).to_bits()
        );
        assert_eq!(
            ratio(&(&top + 5u32), &two).to_bits(),
            (2f64.powi(53) + 2.0).to_bits()
        );
        assert_eq!(
            ratio(&(-(&top + 7u32)), &two).to_bits(),
            (-(2f64.powi(53) + 4.0)).to_bits()
        );
    }

    proptest::proptest! {
        #[test]
        fn floats_come_back_from_their_fractions(
            value in proptest::num::f64::NORMAL,
            scale in 1u32..1000,
        ) {
            let (n, d) = exact(value);
            proptest::prop_assert_eq!(ratio(&n, &d).to_bits(), value.to_bits());
            let k = BigInt::from(scale);
            proptest::prop_assert_eq!(ratio(&(&n * &k), &(&d * &k)).to_bits(), value.to_bits());
        }
    }
}
