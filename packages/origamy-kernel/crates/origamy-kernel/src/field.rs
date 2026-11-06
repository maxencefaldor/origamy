//! Exact real numbers built from the rationals by square roots, as `origamy.field`
//! defines them.
//!
//! The numbers live in a tower of fields. Level 0 holds the rationals, and level k's
//! root is `√d` for a radicand `d` with integer coefficients below level k. A number
//! at level k has an integer coefficient for each product of the roots up to level k,
//! over one positive denominator. Each number has one form, so two numbers are equal
//! exactly when their forms are. A float estimate with an error bound decides most
//! signs, and exact arithmetic decides the rest.

#![allow(
    clippy::many_single_char_names,
    reason = "the products name halves a to e, as the formulas do"
)]

use std::cmp::Ordering;

use smallvec::{SmallVec, smallvec};

use crate::int::Int;

/// A float's relative rounding error, with room to spare.
pub const ROUNDING: f64 = 1.0 / 281_474_976_710_656.0;

/// Coefficients of the products of a tower's roots: 1, √d1, √d2, √d1√d2, ...
pub type Coefficients = SmallVec<[Int; 4]>;

/// A float near a number, and a bound on how far from it the float lies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Estimate {
    /// The float.
    pub value: f64,
    /// The bound on its distance from the number.
    pub error: f64,
}

impl Estimate {
    /// An exact float.
    #[must_use]
    pub const fn exact(value: f64) -> Estimate {
        Estimate { value, error: 0.0 }
    }

    /// The sum, with its rounding bounded.
    #[must_use]
    pub fn plus(self, other: Estimate) -> Estimate {
        let value = self.value + other.value;
        Estimate {
            value,
            error: self.error + other.error + value.abs() * ROUNDING,
        }
    }

    /// The difference, with its rounding bounded.
    #[must_use]
    pub fn minus(self, other: Estimate) -> Estimate {
        let value = self.value - other.value;
        Estimate {
            value,
            error: self.error + other.error + value.abs() * ROUNDING,
        }
    }

    /// The product, with its rounding bounded.
    #[must_use]
    pub fn times(self, other: Estimate) -> Estimate {
        let value = self.value * other.value;
        let error = self.value.abs() * other.error
            + other.value.abs() * self.error
            + self.error * other.error
            + value.abs() * ROUNDING;
        Estimate { value, error }
    }

    /// The sign the estimate shows, or `None` when its error leaves it open.
    #[must_use]
    pub fn sign(self) -> Option<i32> {
        if self.value.abs() > self.error {
            Some(if self.value > 0.0 { 1 } else { -1 })
        } else {
            None
        }
    }
}

/// A real number of a tower: a rational, or `a + b√d` at some level.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Number {
    numerators: Coefficients,
    denominator: Int,
}

impl Number {
    /// Zero.
    #[must_use]
    pub fn zero() -> Number {
        Number {
            numerators: smallvec![Int::ZERO],
            denominator: Int::ONE,
        }
    }

    /// One.
    #[must_use]
    pub fn one() -> Number {
        Number::integer(1)
    }

    /// An integer.
    #[must_use]
    pub fn integer(value: i128) -> Number {
        Number {
            numerators: smallvec![Int::from(value)],
            denominator: Int::ONE,
        }
    }

    /// The number `numerators / denominator`, brought to its one form.
    ///
    /// # Panics
    ///
    /// When the denominator is zero, or the coefficients number no power of two.
    #[must_use]
    pub fn new(numerators: Coefficients, denominator: Int) -> Number {
        assert!(
            !denominator.is_zero(),
            "a number's denominator is never zero"
        );
        assert!(
            numerators.len().is_power_of_two(),
            "a number has a power of two of coefficients"
        );
        let mut numerators = numerators;
        while numerators.len() > 1 && numerators[numerators.len() / 2..].iter().all(Int::is_zero) {
            numerators.truncate(numerators.len() / 2);
        }
        if numerators.iter().all(Int::is_zero) {
            return Number::zero();
        }
        if denominator == Int::ONE {
            return Number {
                numerators,
                denominator,
            };
        }
        let mut common = denominator.abs();
        for n in &numerators {
            if common == Int::ONE {
                break;
            }
            if !n.is_zero() {
                common = common.gcd(n);
            }
        }
        if denominator.signum() < 0 {
            common = -common;
        }
        let (numerators, denominator) = if common == Int::ONE {
            (numerators, denominator)
        } else {
            (
                numerators.iter().map(|n| n.div_exact(&common)).collect(),
                denominator.div_exact(&common),
            )
        };
        Number {
            numerators,
            denominator,
        }
    }

    /// The coefficients, one for each product of the tower's roots up to the level.
    #[must_use]
    pub fn numerators(&self) -> &[Int] {
        &self.numerators
    }

    /// The positive denominator.
    #[must_use]
    pub fn denominator(&self) -> &Int {
        &self.denominator
    }

    /// The lowest level of the tower that holds the number.
    #[must_use]
    pub fn level(&self) -> usize {
        self.numerators.len().trailing_zeros() as usize
    }

    /// Whether the number is rational.
    #[must_use]
    pub fn is_rational(&self) -> bool {
        self.numerators.len() == 1
    }

    /// Whether the number is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.numerators.len() == 1 && self.numerators[0].is_zero()
    }

    /// The sum.
    #[must_use]
    pub fn plus(&self, other: &Number) -> Number {
        if let Some(sum) = self.small_pair(other).and_then(|((a, b), (c, d))| {
            rational(
                i128::from(a) * i128::from(d) + i128::from(c) * i128::from(b),
                i128::from(b) * i128::from(d),
            )
        }) {
            return sum;
        }
        if let Some(sum) = self
            .small()
            .zip(other.small())
            .and_then(|(x, y)| level_one_sum(x, y, 1))
        {
            return sum;
        }
        if self.denominator == other.denominator {
            return Number::new(
                plus(&self.numerators, &other.numerators),
                self.denominator.clone(),
            );
        }
        let common = &self.denominator * &other.denominator;
        let x = scaled(&self.numerators, &other.denominator);
        let y = scaled(&other.numerators, &self.denominator);
        Number::new(plus(&x, &y), common)
    }

    /// The difference.
    #[must_use]
    pub fn minus(&self, other: &Number) -> Number {
        if let Some(difference) = self.small_pair(other).and_then(|((a, b), (c, d))| {
            rational(
                i128::from(a) * i128::from(d) - i128::from(c) * i128::from(b),
                i128::from(b) * i128::from(d),
            )
        }) {
            return difference;
        }
        if let Some(difference) = self
            .small()
            .zip(other.small())
            .and_then(|(x, y)| level_one_sum(x, y, -1))
        {
            return difference;
        }
        if self.denominator == other.denominator {
            return Number::new(
                difference(&self.numerators, &other.numerators),
                self.denominator.clone(),
            );
        }
        let common = &self.denominator * &other.denominator;
        let x = scaled(&self.numerators, &other.denominator);
        let y = scaled(&other.numerators, &self.denominator);
        Number::new(difference(&x, &y), common)
    }

    /// The opposite number.
    #[must_use]
    pub fn negated(&self) -> Number {
        Number {
            numerators: self.numerators.iter().map(|n| -n).collect(),
            denominator: self.denominator.clone(),
        }
    }

    /// The product.
    #[must_use]
    pub fn times(&self, other: &Number, tower: &Tower) -> Number {
        if let Some(product) = self.small_pair(other).and_then(|((a, b), (c, d))| {
            rational(i128::from(a) * i128::from(c), i128::from(b) * i128::from(d))
        }) {
            return product;
        }
        if let Some(product) = tower.first.and_then(|d| {
            let (x, y) = self.small().zip(other.small())?;
            level_one_product(x, y, d)
        }) {
            return product;
        }
        let denominator = &self.denominator * &other.denominator;
        if other.is_rational() {
            return Number::new(scaled(&self.numerators, &other.numerators[0]), denominator);
        }
        if self.is_rational() {
            return Number::new(scaled(&other.numerators, &self.numerators[0]), denominator);
        }
        if let Some(factors) = &tower.factors {
            return Number::new(
                sparse_product(&self.numerators, &other.numerators, factors),
                denominator,
            );
        }
        let size = self.numerators.len().max(other.numerators.len());
        let x = padded(&self.numerators, size);
        let y = padded(&other.numerators, size);
        Number::new(product(&x, &y, &tower.vectors), denominator)
    }

    /// One over the number: its conjugate over its norm, one level down.
    ///
    /// # Panics
    ///
    /// When the number is zero.
    #[must_use]
    pub fn inverse(&self, tower: &Tower) -> Number {
        assert!(!self.is_zero(), "division by zero");
        if self.is_rational() {
            return Number::new(
                smallvec![self.denominator.clone()],
                self.numerators[0].clone(),
            );
        }
        if let Some(inverse) = tower.first.and_then(|d| {
            let (a, b, p) = self.small()?;
            let (a, b, p) = (i128::from(a), i128::from(b), i128::from(p));
            let norm = a
                .checked_mul(a)?
                .checked_sub(b.checked_mul(b)?.checked_mul(i128::from(d))?)?;
            level_one(p.checked_mul(a)?, p.checked_mul(-b)?, norm)
        }) {
            return inverse;
        }
        let (a, b) = self.halves();
        let d = &tower.radicands[self.level() - 1];
        let norm = a
            .times(&a, tower)
            .minus(&b.times(&b, tower).times(d, tower));
        joined(&a, &b.negated(), self.level()).times(&norm.inverse(tower), tower)
    }

    /// The quotient.
    #[must_use]
    pub fn over(&self, other: &Number, tower: &Tower) -> Number {
        self.times(&other.inverse(tower), tower)
    }

    /// The sign, -1, 0 or 1, decided exactly.
    #[must_use]
    pub fn sign(&self, tower: &Tower) -> i32 {
        if let Some(sign) = tower.estimate(self).sign() {
            return sign;
        }
        self.exact_sign(tower)
    }

    /// The sign by exact arithmetic alone, for when an estimate leaves it open.
    #[must_use]
    pub fn exact_sign(&self, tower: &Tower) -> i32 {
        if self.is_rational() {
            return self.numerators[0].signum();
        }
        let (a, b) = self.halves();
        let (sa, sb) = (a.sign(tower), b.sign(tower));
        if sa == 0 || sa == sb {
            return sb;
        }
        // The signs differ: the larger part wins, compared through squares.
        let d = &tower.radicands[self.level() - 1];
        let difference = a
            .times(&a, tower)
            .minus(&b.times(&b, tower).times(d, tower));
        if difference.sign(tower) > 0 { sa } else { sb }
    }

    /// How the number compares with another, decided exactly.
    #[must_use]
    pub fn compare(&self, other: &Number, tower: &Tower) -> Ordering {
        if let Some(sign) = tower.estimate(self).minus(tower.estimate(other)).sign() {
            return sign.cmp(&0);
        }
        self.minus(other).sign(tower).cmp(&0)
    }

    /// Both numbers as fractions of small integers, when both are rational and small.
    /// A rational or a number of level 1 whose integers fit in 64 bits: `(a, b, p)` for
    /// `(a + b√d) / p`.
    fn small(&self) -> Option<(i64, i64, i64)> {
        let Int::Small(p) = self.denominator else {
            return None;
        };
        match self.numerators[..] {
            [Int::Small(a)] => Some((a, 0, p)),
            [Int::Small(a), Int::Small(b)] => Some((a, b, p)),
            _ => None,
        }
    }

    fn small_pair(&self, other: &Number) -> Option<((i64, i64), (i64, i64))> {
        match (
            &self.numerators[..],
            &self.denominator,
            &other.numerators[..],
            &other.denominator,
        ) {
            ([Int::Small(a)], Int::Small(b), [Int::Small(c)], Int::Small(d)) => {
                Some(((*a, *b), (*c, *d)))
            }
            _ => None,
        }
    }

    /// The number as `a + b√d` at its level, `a` and `b` one level down.
    fn halves(&self) -> (Number, Number) {
        let half = self.numerators.len() / 2;
        (
            Number::new(
                self.numerators[..half].iter().cloned().collect(),
                self.denominator.clone(),
            ),
            Number::new(
                self.numerators[half..].iter().cloned().collect(),
                self.denominator.clone(),
            ),
        )
    }
}

/// The radicands of a tower, with what arithmetic and estimates need of them.
#[derive(Clone, Debug, Default)]
pub struct Tower {
    radicands: Vec<Number>,
    /// The first level's radicand, when it is an integer that fits in 64 bits.
    first: Option<i64>,
    /// Each level's radicand as integer coefficients, as long as half that level.
    vectors: Vec<Coefficients>,
    roots: Vec<(f64, f64)>,
    /// The products of the roots, each a float and its error bound.
    basis: Vec<(f64, f64)>,
    /// When every radicand is an integer: for each set of levels, as bits, the product
    /// of their radicands. The square of a product of roots is that product.
    factors: Option<Vec<Int>>,
}

impl Tower {
    /// A tower of these radicands, level 1 first, each with integer coefficients.
    ///
    /// # Panics
    ///
    /// When a radicand has a denominator other than 1, or lies above its own level.
    #[must_use]
    pub fn new(radicands: Vec<Number>) -> Tower {
        let mut tower = Tower {
            radicands: vec![],
            first: None,
            vectors: vec![smallvec![]],
            roots: vec![],
            basis: vec![(1.0, 0.0)],
            factors: Some(vec![Int::ONE]),
        };
        for radicand in radicands {
            tower.add(radicand);
        }
        tower
    }

    /// The radicands, level 1 first.
    #[must_use]
    pub fn radicands(&self) -> &[Number] {
        &self.radicands
    }

    fn add(&mut self, radicand: Number) {
        let level = self.radicands.len();
        assert!(
            radicand.denominator == Int::ONE,
            "a radicand has integer coefficients"
        );
        assert!(
            radicand.level() <= level,
            "a radicand lies below its own level"
        );
        self.vectors.push(padded(&radicand.numerators, 1 << level));
        if let ([Int::Small(d)], 0) = (&radicand.numerators[..], level) {
            self.first = Some(*d);
        }
        let integer = radicand
            .is_rational()
            .then(|| radicand.numerators[0].clone());
        let estimate = self.estimate(&radicand);
        let low = estimate.value - estimate.error;
        let root = estimate.value.max(0.0).sqrt();
        let slack = if low > 0.0 {
            estimate.error / (root + low.sqrt())
        } else {
            f64::INFINITY
        };
        let root = (root, slack + root * ROUNDING);
        self.roots.push(root);
        self.radicands.push(radicand);
        let (value, error) = root;
        let products: Vec<(f64, f64)> = self
            .basis
            .iter()
            .map(|&(v, e)| (v * value, v * error + e * (value + error)))
            .collect();
        self.basis.extend(products);
        self.factors = match (self.factors.take(), integer) {
            (Some(factors), Some(d)) => {
                let more: Vec<Int> = factors.iter().map(|f| f * &d).collect();
                Some(factors.into_iter().chain(more).collect())
            }
            _ => None,
        };
    }

    /// A float near a number of this tower, and a bound on its error.
    #[must_use]
    pub fn estimate(&self, number: &Number) -> Estimate {
        let denominator = number.denominator.to_f64();
        let (mut value, mut size, mut error) = (0.0, 0.0, 0.0);
        for (n, &(root, slack)) in number.numerators.iter().zip(&self.basis) {
            if !n.is_zero() {
                let term = n.to_f64() / denominator;
                value += term * root;
                size += (term * root).abs();
                error += term.abs() * slack;
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let rounding = 4.0 * number.numerators.len() as f64 * size * ROUNDING;
        if !(value.is_finite() && error.is_finite() && rounding.is_finite()) {
            return Estimate {
                value: 0.0,
                error: f64::INFINITY,
            };
        }
        Estimate {
            value,
            error: error + rounding,
        }
    }
}

fn padded(numerators: &[Int], size: usize) -> Coefficients {
    let mut out: Coefficients = numerators.iter().cloned().collect();
    out.resize(size, Int::ZERO);
    out
}

fn scaled(numerators: &[Int], factor: &Int) -> Coefficients {
    numerators.iter().map(|n| n * factor).collect()
}

fn plus(x: &[Int], y: &[Int]) -> Coefficients {
    let (long, short) = if x.len() >= y.len() { (x, y) } else { (y, x) };
    long.iter()
        .enumerate()
        .map(|(i, a)| short.get(i).map_or_else(|| a.clone(), |b| a + b))
        .collect()
}

/// The coefficients of a difference, as long as the longer of the two.
fn difference(x: &[Int], y: &[Int]) -> Coefficients {
    (0..x.len().max(y.len()))
        .map(|i| match (x.get(i), y.get(i)) {
            (Some(a), Some(b)) => a - b,
            (Some(a), None) => a.clone(),
            (None, Some(b)) => -b,
            (None, None) => Int::ZERO,
        })
        .collect()
}

fn minus(x: &[Int], y: &[Int]) -> Coefficients {
    x.iter().zip(y).map(|(a, b)| a - b).collect()
}

/// The product of two equally long coefficient vectors, in integers.
///
/// With `x = a + b√d` and `y = c + e√d`, the product is `ac + be·d` plus
/// `(a + b)(c + e) - ac - be` times `√d`: three products one level down.
fn product(x: &[Int], y: &[Int], vectors: &[Coefficients]) -> Coefficients {
    if x.len() == 1 {
        return smallvec![&x[0] * &y[0]];
    }
    let half = x.len() / 2;
    let (a, b, c, e) = (&x[..half], &x[half..], &y[..half], &y[half..]);
    let ac = product(a, c, vectors);
    let be = product(b, e, vectors);
    let mixed = product(&plus(a, b), &plus(c, e), vectors);
    let radicand = &vectors[half.trailing_zeros() as usize + 1];
    let mut out = plus(&ac, &product(&be, radicand, vectors));
    out.extend(minus(&minus(&mixed, &ac), &be));
    out
}

/// The sum of `(a + b√d) / p` and `sign` times `(c + e√d) / q`, when its integers fit.
fn level_one_sum(
    (a, b, p): (i64, i64, i64),
    (c, e, q): (i64, i64, i64),
    sign: i128,
) -> Option<Number> {
    let (a, b, p, c, e, q) = (
        i128::from(a),
        i128::from(b),
        i128::from(p),
        sign * i128::from(c),
        sign * i128::from(e),
        i128::from(q),
    );
    if p == q {
        return level_one(a + c, b + e, p);
    }
    let first = a.checked_mul(q)?.checked_add(c.checked_mul(p)?)?;
    let second = b.checked_mul(q)?.checked_add(e.checked_mul(p)?)?;
    level_one(first, second, p.checked_mul(q)?)
}

/// The product of `(a + b√d) / p` and `(c + e√d) / q`, when its integers fit.
fn level_one_product(
    (a, b, p): (i64, i64, i64),
    (c, e, q): (i64, i64, i64),
    d: i64,
) -> Option<Number> {
    let (a, b, p, c, e, q) = (
        i128::from(a),
        i128::from(b),
        i128::from(p),
        i128::from(c),
        i128::from(e),
        i128::from(q),
    );
    let first = a
        .checked_mul(c)?
        .checked_add(b.checked_mul(e)?.checked_mul(i128::from(d))?)?;
    let second = a.checked_mul(e)?.checked_add(b.checked_mul(c)?)?;
    level_one(first, second, p.checked_mul(q)?)
}

/// The number `(a + b√d) / p` of level 1, or a rational, in its one form, when its
/// integers fit in 64 bits once reduced. The denominator is never zero.
fn level_one(a: i128, b: i128, p: i128) -> Option<Number> {
    if b == 0 {
        return rational(a, p.checked_abs()?).map(|n| if p < 0 { n.negated() } else { n });
    }
    let mut common = gcd_i128(b, p);
    if a != 0 && common != 1 {
        common = gcd_i128(common, a);
    }
    if p < 0 {
        common = -common;
    }
    let (a, b, p) = if common == 1 {
        (a, b, p)
    } else {
        (a / common, b / common, p / common)
    };
    Some(Number {
        numerators: smallvec![
            Int::Small(i64::try_from(a).ok()?),
            Int::Small(i64::try_from(b).ok()?),
        ],
        denominator: Int::Small(i64::try_from(p).ok()?),
    })
}

/// The fraction `numerator / denominator` in lowest terms, its denominator positive, when
/// both fit in 64 bits once reduced. The denominator is positive and never zero.
fn rational(numerator: i128, denominator: i128) -> Option<Number> {
    if numerator == 0 {
        return Some(Number::zero());
    }
    let common = gcd_i128(numerator, denominator);
    let (n, d) = (
        i64::try_from(numerator / common).ok()?,
        i64::try_from(denominator / common).ok()?,
    );
    Some(Number {
        numerators: smallvec![Int::Small(n)],
        denominator: Int::Small(d),
    })
}

/// The greatest common divisor of two integers, the second positive.
fn gcd_i128(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.unsigned_abs(), b.unsigned_abs());
    let shift = (a | b).trailing_zeros();
    a >>= a.trailing_zeros();
    loop {
        b >>= b.trailing_zeros();
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        b -= a;
        if b == 0 {
            return i128::try_from(a << shift).expect("a divisor of an i128 fits in one");
        }
    }
}

/// The product of two coefficient vectors over a tower of integer radicands: each pair
/// of nonzero coefficients lands on the levels in one and not both, times the radicands
/// of the levels in both.
fn sparse_product(x: &[Int], y: &[Int], factors: &[Int]) -> Coefficients {
    let mut out: Coefficients = smallvec![Int::ZERO; x.len().max(y.len())];
    for (i, a) in x.iter().enumerate().filter(|(_, a)| !a.is_zero()) {
        for (j, b) in y.iter().enumerate().filter(|(_, b)| !b.is_zero()) {
            let term = a * b;
            let both = i & j;
            let term = if both == 0 {
                term
            } else {
                &term * &factors[both]
            };
            out[i ^ j] = &out[i ^ j] + &term;
        }
    }
    out
}

/// The number `a + b√d` at a level, from `a` and `b` below it.
fn joined(a: &Number, b: &Number, level: usize) -> Number {
    let half = 1 << (level - 1);
    let common = &a.denominator * &b.denominator;
    let mut first = padded(&scaled(&a.numerators, &b.denominator), half);
    let second = padded(&scaled(&b.numerators, &a.denominator), half);
    first.extend(second);
    Number::new(first, common)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(numerators: &[i128], denominator: i128) -> Number {
        Number::new(
            numerators.iter().map(|&v| Int::from(v)).collect(),
            Int::from(denominator),
        )
    }

    #[test]
    fn half_root_two_squared_is_a_half() {
        let tower = Tower::new(vec![Number::integer(2)]);
        let x = n(&[0, 1], 2);
        assert_eq!(x.times(&x, &tower), n(&[1], 2));
    }

    #[test]
    fn inverse_of_one_plus_root_two() {
        let tower = Tower::new(vec![Number::integer(2)]);
        let x = n(&[1, 1], 1);
        assert_eq!(x.times(&x.inverse(&tower), &tower), Number::one());
        assert_eq!(x.inverse(&tower), n(&[-1, 1], 1));
    }

    #[test]
    fn signs_too_close_for_floats() {
        let tower = Tower::new(vec![Number::integer(2)]);
        // 665857/470832 - √2 is about 1.6e-12, positive.
        let x = n(&[665_857, -470_832], 470_832);
        assert_eq!(x.sign(&tower), 1);
        assert_eq!(x.negated().sign(&tower), -1);
    }

    fn numbers() -> impl proptest::strategy::Strategy<Value = Number> {
        use proptest::prelude::*;
        (proptest::collection::vec(-50i128..50, 4), 1i128..30).prop_map(
            |(coefficients, denominator)| {
                Number::new(
                    coefficients.into_iter().map(Int::from).collect(),
                    Int::from(denominator),
                )
            },
        )
    }

    proptest::proptest! {
        #[test]
        fn products_over_integer_radicands_match_the_general_product(x in numbers(), y in numbers()) {
            let tower = Tower::new(vec![Number::integer(2), Number::integer(3)]);
            let size = x.numerators.len().max(y.numerators.len());
            let general = Number::new(
                product(&padded(&x.numerators, size), &padded(&y.numerators, size), &tower.vectors),
                &x.denominator * &y.denominator,
            );
            proptest::prop_assert_eq!(x.times(&y, &tower), general);
        }

        #[test]
        fn arithmetic_undoes_itself(x in numbers(), y in numbers()) {
            let tower = Tower::new(vec![Number::integer(2), Number::integer(3)]);
            proptest::prop_assert_eq!(x.plus(&y).minus(&y), x.clone());
            if !y.is_zero() {
                proptest::prop_assert_eq!(x.times(&y, &tower).over(&y, &tower), x.clone());
            }
        }

        #[test]
        fn exact_signs_agree_with_clear_floats(x in numbers()) {
            let tower = Tower::new(vec![Number::integer(2), Number::integer(3)]);
            let estimate = tower.estimate(&x);
            if estimate.value.abs() > 1e-6 {
                proptest::prop_assert_eq!(x.exact_sign(&tower), if estimate.value > 0.0 { 1 } else { -1 });
            }
            proptest::prop_assert_eq!(x.sign(&tower), x.exact_sign(&tower));
        }
    }

    /// Numbers of level 1, with integers small or near the limits of 64 bits.
    fn level_ones() -> impl proptest::strategy::Strategy<Value = Number> {
        use proptest::prelude::*;
        let integer = prop_oneof![-50i128..50, -(1i128 << 62)..(1i128 << 62)];
        (
            proptest::collection::vec(integer.clone(), 2),
            integer.prop_filter("nonzero", |&d| d != 0),
        )
            .prop_map(|(coefficients, denominator)| {
                Number::new(
                    coefficients.into_iter().map(Int::from).collect(),
                    Int::from(denominator),
                )
            })
    }

    proptest::proptest! {
        #[test]
        fn level_one_arithmetic_matches_the_general_arithmetic(x in level_ones(), y in level_ones()) {
            let tower = Tower::new(vec![Number::integer(2)]);
            let (dx, dy) = (&x.denominator, &y.denominator);
            let (sx, sy) = (scaled(&x.numerators, dy), scaled(&y.numerators, dx));
            proptest::prop_assert_eq!(x.plus(&y), Number::new(plus(&sx, &sy), dx * dy));
            proptest::prop_assert_eq!(x.minus(&y), Number::new(difference(&sx, &sy), dx * dy));
            let general = product(&padded(&x.numerators, 2), &padded(&y.numerators, 2), &tower.vectors);
            proptest::prop_assert_eq!(x.times(&y, &tower), Number::new(general, dx * dy));
            if !x.is_zero() {
                proptest::prop_assert_eq!(x.times(&x.inverse(&tower), &tower), Number::one());
            }
        }
    }

    #[test]
    fn two_levels_multiply() {
        let tower = Tower::new(vec![Number::integer(2), Number::integer(3)]);
        let root_two = n(&[0, 1], 1);
        let root_three = n(&[0, 0, 1, 0], 1);
        let root_six = root_two.times(&root_three, &tower);
        assert_eq!(root_six, n(&[0, 0, 0, 1], 1));
        assert_eq!(root_six.times(&root_six, &tower), Number::integer(6));
    }
}
