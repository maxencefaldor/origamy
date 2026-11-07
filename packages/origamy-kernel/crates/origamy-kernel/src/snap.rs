//! The exact numbers that explain floats, as `origamy.pattern` snaps coordinates.
//!
//! A candidate is a fraction, from the float's continued fraction, or a number of a
//! field of square roots with small integers, from lattice reduction. Its fit weighs
//! its error by its size, and the best candidate has the smallest fit, then the
//! smallest size.

use num_bigint::{BigInt, BigUint, Sign};
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::lattice::relations;

/// A field a candidate may lie in beyond the fractions: a tower of square roots.
///
/// Each level's radicand is integers over the products of the roots below it, in the
/// order a [`crate::field::Number`] keeps its coefficients: `[2]` is √2, and
/// `[2], [2, 1]` adds √(2 + √2) above it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field(pub Vec<Vec<i64>>);

impl Field {
    /// Each product of the field's roots as a float, 1 first, in the order of a
    /// number's coefficients.
    #[must_use]
    pub fn basis(&self) -> Vec<f64> {
        let mut products = vec![1.0];
        for radicand in &self.0 {
            #[allow(clippy::cast_precision_loss, reason = "the integers are small")]
            let value: f64 = radicand
                .iter()
                .zip(&products)
                .map(|(&c, &product)| c as f64 * product)
                .sum();
            let root = value.sqrt();
            let above: Vec<f64> = products.iter().map(|&product| product * root).collect();
            products.extend(above);
        }
        products
    }

    /// Whether every level's radicand is positive and its integers fit its level.
    #[must_use]
    pub fn valid(&self) -> bool {
        self.0
            .iter()
            .enumerate()
            .all(|(level, radicand)| radicand.len() <= 1 << level && !radicand.is_empty())
            && self.basis().iter().all(|x| x.is_finite() && *x > 0.0)
    }
}

/// The bounds on the candidates that may explain a float.
#[derive(Clone, Debug)]
pub struct Bounds {
    /// How far a candidate may lie from the float.
    pub tolerance: f64,
    /// How finely the float measures its value: a candidate's error counts as no
    /// less. Zero for a float a file gives.
    pub spread: f64,
    /// The fit under which a fraction explains the float surely, so that no field is
    /// tried.
    pub sure: f64,
    /// The largest denominator of a fraction.
    pub denominator: u64,
    /// The fields a candidate may lie in beyond the fractions.
    pub fields: Vec<Field>,
    /// The weight of a relation's error against its integers in lattice reduction.
    pub scale: i64,
    /// The largest integer of a candidate in a field.
    pub size: i64,
}

/// The best candidate for a float: integers over a field's basis, over a denominator,
/// with its fit.
#[derive(Clone, Debug, PartialEq)]
pub struct Explanation {
    /// The error times a power of the candidate's size: its square for a fraction, and
    /// one less than the count of its integers for a number of a field. The error is
    /// no less than the float's spread, and for a number of a field it holds the
    /// rounding of its floats.
    pub fit: f64,
    /// The integer of each product of the field's roots, as [`Field::basis`] orders
    /// them; one integer for a fraction. They and the denominator share no factor.
    pub numerators: Vec<BigInt>,
    /// The positive denominator.
    pub denominator: BigInt,
    /// The field among the bounds' the number lies in, or `None` for a fraction.
    pub field: Option<usize>,
}

/// The candidate that explains a finite float best within the bounds, if any.
///
/// A number of a field holds its top level's root, so that a smaller field's number
/// comes from that field alone.
#[must_use]
pub fn explain(value: f64, bounds: &Bounds) -> Option<Explanation> {
    // Each candidate with its fit and size, in the order found: the first of equals wins.
    let mut found: Vec<(f64, u64, Explanation)> = vec![];
    for (p, d) in convergents(value, bounds.denominator) {
        let error = (value - ratio(&p, &d)).abs();
        if error <= bounds.tolerance {
            // The denominator stays under the bound, a u64.
            let size = d.to_u64().unwrap_or(u64::MAX);
            let fit = error.max(bounds.spread) * ratio(&(&d * &d), &BigInt::one());
            let explanation = Explanation {
                fit,
                numerators: vec![p],
                denominator: d,
                field: None,
            };
            found.push((fit, size, explanation));
        }
    }
    if !found.iter().any(|(fit, _, _)| *fit <= bounds.sure) {
        for (index, field) in bounds.fields.iter().enumerate() {
            let basis = field.basis();
            let top = basis.len() / 2;
            for (d, c) in relations(value, &basis, bounds.scale, bounds.size) {
                if c[top..].iter().all(|&x| x == 0) {
                    continue;
                }
                #[allow(clippy::cast_precision_loss, reason = "the integers are small")]
                let sum: f64 = c.iter().zip(&basis).map(|(&x, &b)| x as f64 * b).sum();
                #[allow(clippy::cast_precision_loss, reason = "the integers are small")]
                let df = d as f64;
                if (value - sum / df).abs() > bounds.tolerance {
                    continue;
                }
                let size = c.iter().fold(d, |m, x| m.max(x.abs()));
                // Floats know the error only up to the rounding of its terms, a few units
                // in the last place of each.
                #[allow(clippy::cast_precision_loss, reason = "the integers are small")]
                let terms: f64 = (df * value).abs()
                    + c.iter()
                        .zip(&basis)
                        .map(|(&x, &b)| (x as f64 * b).abs())
                        .sum::<f64>();
                #[allow(
                    clippy::cast_precision_loss,
                    clippy::cast_possible_truncation,
                    clippy::cast_possible_wrap,
                    reason = "the integers are small, and so is their count"
                )]
                let (rounding, power) = (f64::EPSILON * c.len() as f64, c.len() as i32);
                #[allow(clippy::cast_precision_loss, reason = "the integers are small")]
                let error = (df * value - sum).abs().max(df * bounds.spread) + terms * rounding;
                #[allow(clippy::cast_precision_loss, reason = "the integers are small")]
                let fit = error * (size as f64).powi(power);
                let explanation = Explanation {
                    fit,
                    numerators: c.iter().map(|&x| BigInt::from(x)).collect(),
                    denominator: BigInt::from(d),
                    field: Some(index),
                };
                found.push((fit, size.unsigned_abs(), lowest(explanation)));
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

/// An explanation with its integers and denominator in lowest terms.
fn lowest(mut explanation: Explanation) -> Explanation {
    let common = explanation
        .numerators
        .iter()
        .fold(explanation.denominator.clone(), |g, n| g.gcd(n));
    if !common.is_one() {
        for n in &mut explanation.numerators {
            *n /= &common;
        }
        explanation.denominator /= &common;
    }
    explanation
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
            spread: 0.0,
            sure: 1e-8,
            denominator: 1_000_000,
            fields: vec![
                Field(vec![vec![2]]),
                Field(vec![vec![3]]),
                Field(vec![vec![5]]),
                Field(vec![vec![6]]),
                Field(vec![vec![2], vec![2, 1]]),
            ],
            scale: 1_000_000_000_000,
            size: 10_000,
        }
    }

    fn fraction(p: i64, d: i64) -> (BigInt, BigInt) {
        (BigInt::from(p), BigInt::from(d))
    }

    fn integers(found: &Explanation) -> (Vec<i64>, i64, Option<usize>) {
        let numerators = found
            .numerators
            .iter()
            .map(|n| n.to_i64().unwrap())
            .collect();
        (numerators, found.denominator.to_i64().unwrap(), found.field)
    }

    #[test]
    fn fractions_and_roots_explain_floats() {
        let half = explain(0.5, &bounds()).unwrap();
        assert_eq!(integers(&half), (vec![1], 2, None));
        let third = explain(1.0 / 3.0, &bounds()).unwrap();
        assert_eq!(integers(&third), (vec![1], 3, None));
        let golden = explain((1.0 + 5f64.sqrt()) / 4.0, &bounds()).unwrap();
        assert_eq!(integers(&golden), (vec![1, 1], 4, Some(2)));
        assert!(explain(0.123_456_789, &bounds()).unwrap().fit > 1e-4);
    }

    #[test]
    fn nested_roots_explain_the_floats_of_eleven_and_a_quarter_degrees() {
        // The traditional crane drawn on a square turned by 22.5° has a corner at
        // (2 - √2 - 3·√(2 + √2) + 2·√2·√(2 + √2)) / 2.
        let (two, nested) = (
            std::f64::consts::SQRT_2,
            (2.0 + std::f64::consts::SQRT_2).sqrt(),
        );
        let corner = 1.0 - 0.5 * two - 1.5 * nested + two * nested;
        let found = explain(corner, &bounds()).unwrap();
        assert_eq!(integers(&found), (vec![2, -1, -3, 2], 2, Some(4)));
        assert!(found.fit < 1e-8, "{found:?}");
        // A number of √2 alone comes from √2's field.
        let found = explain(0.5 + two / 4.0, &bounds()).unwrap();
        assert_eq!(integers(&found), (vec![2, 1], 4, Some(0)));
    }

    #[test]
    fn a_float_measured_coarsely_gives_no_candidate_a_finer_error() {
        let coarse = Bounds {
            spread: 1e-7,
            ..bounds()
        };
        let third = explain(1.0 / 3.0, &coarse).unwrap();
        assert_eq!(integers(&third), (vec![1], 3, None));
        assert!((third.fit - 9e-7).abs() < 1e-12, "{third:?}");
    }

    #[test]
    fn a_field_lists_its_roots_products() {
        let field = Field(vec![vec![2], vec![2, 1]]);
        let nested = (2.0 + std::f64::consts::SQRT_2).sqrt();
        let basis = field.basis();
        assert_eq!(basis.len(), 4);
        assert!((basis[3] - std::f64::consts::SQRT_2 * nested).abs() < 1e-15);
        assert!(field.valid());
        assert!(!Field(vec![vec![2], vec![1, 2, 3]]).valid());
        assert!(!Field(vec![vec![-2]]).valid());
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
