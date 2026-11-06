//! Integers of any size, kept in 64 bits while they fit.
//!
//! Folding multiplies small integers almost always, and a heap allocation for each
//! would cost more than the arithmetic. An [`Int`] holds an `i64`, and a [`BigInt`] once
//! a result outgrows it. Sums and products of two `i64`s fit in an `i128`, so each
//! operation on small integers is exact before it narrows.

use std::cmp::Ordering;
use std::fmt;
use std::ops::{Add, Mul, Neg, Sub};

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{Signed, ToPrimitive, Zero};

/// An integer, small or big.
#[derive(Clone, Debug)]
pub enum Int {
    /// An integer that fits in 64 bits.
    Small(i64),
    /// An integer too large for 64 bits.
    Big(Box<BigInt>),
}

impl Int {
    /// Zero.
    pub const ZERO: Int = Int::Small(0);
    /// One.
    pub const ONE: Int = Int::Small(1);

    /// An integer from a big one, small when it fits.
    #[must_use]
    pub fn from_big(value: BigInt) -> Int {
        value
            .to_i64()
            .map_or_else(|| Int::Big(Box::new(value)), Int::Small)
    }

    /// The integer as a big one.
    #[must_use]
    pub fn to_big(&self) -> BigInt {
        match self {
            Int::Small(v) => BigInt::from(*v),
            Int::Big(v) => (**v).clone(),
        }
    }

    /// Whether the integer is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        matches!(self, Int::Small(0))
    }

    /// -1, 0 or 1.
    #[must_use]
    pub fn signum(&self) -> i32 {
        match self {
            Int::Small(v) => i32::from(v.signum() as i8),
            Int::Big(v) => {
                if v.is_negative() {
                    -1
                } else {
                    i32::from(!v.is_zero())
                }
            }
        }
    }

    /// The nearest float, or infinity beyond the floats' range.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn to_f64(&self) -> f64 {
        match self {
            Int::Small(v) => *v as f64,
            Int::Big(v) => v.to_f64().unwrap_or(f64::INFINITY),
        }
    }

    /// The absolute value.
    #[must_use]
    pub fn abs(&self) -> Int {
        match self {
            Int::Small(v) => v
                .checked_abs()
                .map_or_else(|| Int::from_big(BigInt::from(*v).abs()), Int::Small),
            Int::Big(v) => Int::from_big(v.abs()),
        }
    }

    /// The greatest common divisor, never negative.
    #[must_use]
    pub fn gcd(&self, other: &Int) -> Int {
        match (self, other) {
            (Int::Small(a), Int::Small(b)) => {
                let g = gcd_u64(a.unsigned_abs(), b.unsigned_abs());
                i64::try_from(g).map_or_else(|_| Int::from_big(BigInt::from(g)), Int::Small)
            }
            _ => Int::from_big(self.to_big().gcd(&other.to_big())),
        }
    }

    /// The quotient of a division that leaves no remainder.
    #[must_use]
    pub fn div_exact(&self, divisor: &Int) -> Int {
        match (self, divisor) {
            (Int::Small(a), Int::Small(b)) => a.checked_div(*b).map_or_else(
                || Int::from_big(BigInt::from(*a) / BigInt::from(*b)),
                Int::Small,
            ),
            _ => Int::from_big(self.to_big() / divisor.to_big()),
        }
    }
}

fn gcd_u64(mut a: u64, mut b: u64) -> u64 {
    if a == 0 {
        return b;
    }
    if b == 0 {
        return a;
    }
    let shift = (a | b).trailing_zeros();
    a >>= a.trailing_zeros();
    loop {
        b >>= b.trailing_zeros();
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        b -= a;
        if b == 0 {
            return a << shift;
        }
    }
}

impl From<i128> for Int {
    fn from(value: i128) -> Int {
        i64::try_from(value).map_or_else(|_| Int::Big(Box::new(BigInt::from(value))), Int::Small)
    }
}

impl From<i64> for Int {
    fn from(value: i64) -> Int {
        Int::Small(value)
    }
}

macro_rules! arithmetic {
    ($trait:ident, $method:ident) => {
        impl $trait<&Int> for &Int {
            type Output = Int;
            fn $method(self, other: &Int) -> Int {
                if let (Int::Small(a), Int::Small(b)) = (self, other) {
                    return Int::from(i128::from(*a).$method(i128::from(*b)));
                }
                Int::from_big(self.to_big().$method(other.to_big()))
            }
        }
        impl $trait<Int> for Int {
            type Output = Int;
            fn $method(self, other: Int) -> Int {
                (&self).$method(&other)
            }
        }
    };
}

arithmetic!(Add, add);
arithmetic!(Sub, sub);
arithmetic!(Mul, mul);

impl Neg for &Int {
    type Output = Int;
    fn neg(self) -> Int {
        match self {
            Int::Small(v) => v
                .checked_neg()
                .map_or_else(|| Int::from_big(-BigInt::from(*v)), Int::Small),
            Int::Big(v) => Int::from_big(-(**v).clone()),
        }
    }
}

impl Neg for Int {
    type Output = Int;
    fn neg(self) -> Int {
        -&self
    }
}

impl PartialEq for Int {
    fn eq(&self, other: &Int) -> bool {
        match (self, other) {
            (Int::Small(a), Int::Small(b)) => a == b,
            // A big integer never fits in 64 bits, so it never equals a small one.
            (Int::Big(a), Int::Big(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Int {}

impl std::hash::Hash for Int {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Int::Small(v) => v.hash(state),
            Int::Big(v) => v.hash(state),
        }
    }
}

impl PartialOrd for Int {
    fn partial_cmp(&self, other: &Int) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Int {
    fn cmp(&self, other: &Int) -> Ordering {
        match (self, other) {
            (Int::Small(a), Int::Small(b)) => a.cmp(b),
            _ => self.to_big().cmp(&other.to_big()),
        }
    }
}

impl fmt::Display for Int {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Int::Small(v) => write!(f, "{v}"),
            Int::Big(v) => write!(f, "{v}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow_moves_to_big_and_back() {
        let big = &Int::Small(i64::MAX) + &Int::ONE;
        assert!(matches!(big, Int::Big(_)));
        assert_eq!(&big - &Int::ONE, Int::Small(i64::MAX));
        let square = &big * &big;
        assert_eq!(square.div_exact(&big), big);
    }

    #[test]
    fn gcd_of_small_integers() {
        assert_eq!(Int::Small(-12).gcd(&Int::Small(18)), Int::Small(6));
        assert_eq!(Int::Small(0).gcd(&Int::Small(-7)), Int::Small(7));
    }
}
