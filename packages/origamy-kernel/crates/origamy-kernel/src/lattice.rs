//! Integer relations by lattice reduction, as `origamy.pattern` snaps coordinates.
//!
//! The integer combinations of a float and the floats of a field's basis span a
//! lattice. Its LLL-reduced basis holds short vectors: small integers whose
//! combination nearly vanishes. Cohen's integral LLL keeps the Gram-Schmidt data in
//! integers, so the reduction is exact.
//!
//! The reduction runs in 128-bit integers with 256-bit products, which hold every
//! value of the usual bounds without a heap allocation. A value that outgrows them
//! runs the reduction again in big integers, to the same basis.

#![allow(
    clippy::many_single_char_names,
    reason = "the reduction names its integers as Cohen's algorithm does"
)]

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};

/// Small integers `d` and `c`, with `d` positive, that make
/// `d·value ≈ c[0]·basis[0] + c[1]·basis[1] + …`.
///
/// `scale` weighs a relation's error against its integers, and `size` bounds the
/// integers kept. The lattice's basis is the integer vectors of `value` and of each
/// float of `basis`, each with its float times `scale`, rounded half to even, as a
/// last coordinate.
#[must_use]
pub fn relations(value: f64, basis: &[f64], scale: i64, size: i64) -> Vec<(i64, Vec<i64>)> {
    #[allow(clippy::cast_precision_loss, reason = "the scale is a float's weight")]
    let scaled = |x: f64| (scale as f64 * x).round_ties_even().to_i128().unwrap_or(0);
    let width = basis.len() + 2;
    let vectors: Vec<Vec<i128>> = std::iter::once(value)
        .chain(basis.iter().copied())
        .enumerate()
        .map(|(i, x)| {
            let mut vector = vec![0; width];
            vector[i] = 1;
            vector[width - 1] = scaled(x);
            vector
        })
        .collect();
    let reduced: Vec<Vec<Option<i64>>> = reduced(&vectors).map_or_else(
        || {
            let big: Vec<Vec<BigInt>> = vectors
                .iter()
                .map(|vector| vector.iter().map(|&x| BigInt::from(x)).collect())
                .collect();
            // Big integers hold every value, so the reduction completes.
            reduced(&big)
                .unwrap_or_default()
                .iter()
                .map(|vector| vector.iter().map(ToPrimitive::to_i64).collect())
                .collect()
        },
        |reduced: Vec<Vec<i128>>| {
            reduced
                .iter()
                .map(|vector| vector.iter().map(|&x| i64::try_from(x).ok()).collect())
                .collect()
        },
    );
    let mut found = vec![];
    for vector in reduced {
        let Some(integers) = vector[..width - 1]
            .iter()
            .copied()
            .collect::<Option<Vec<i64>>>()
        else {
            continue;
        };
        let times = integers[0];
        let sign = if times > 0 { 1 } else { -1 };
        let d = sign * times;
        let c: Vec<i64> = integers[1..].iter().map(|&x| -sign * x).collect();
        if d != 0 && c.iter().fold(d, |m, x| m.max(x.abs())) <= size {
            found.push((d, c));
        }
    }
    found
}

/// The integers a reduction runs in. Each operation gives `None` when its result
/// outgrows them.
trait Entry: Clone + Sized {
    fn zero() -> Self;
    fn one() -> Self;
    fn dot(first: &[Self], second: &[Self]) -> Option<Self>;
    /// `(a·b - c·e) / f` for a positive `f`, rounded down.
    fn ratio(a: &Self, b: &Self, c: &Self, e: &Self, f: &Self) -> Option<Self>;
    fn negated(&self) -> Option<Self>;
    /// Lovász's condition with a factor of 3/4: `4·dk·dk2 ≥ 3·dk1² - 4·m²`.
    fn lovasz(dk: &Self, dk2: &Self, dk1: &Self, m: &Self) -> Option<bool>;
    /// Whether `2|m| > d`, so that a reduction step is due.
    fn beyond_half(m: &Self, d: &Self) -> Option<bool>;
    /// The nearest integer to `m / d` for a positive `d`, halves rounded up.
    fn nearest(m: &Self, d: &Self) -> Option<Self>;
    /// `a - s·b`.
    fn less_times(a: &Self, s: &Self, b: &Self) -> Option<Self>;
}

impl Entry for BigInt {
    fn zero() -> Self {
        Zero::zero()
    }

    fn one() -> Self {
        One::one()
    }

    fn dot(first: &[Self], second: &[Self]) -> Option<Self> {
        Some(first.iter().zip(second).map(|(a, b)| a * b).sum())
    }

    fn ratio(a: &Self, b: &Self, c: &Self, e: &Self, f: &Self) -> Option<Self> {
        Some((a * b - c * e).div_floor(f))
    }

    fn negated(&self) -> Option<Self> {
        Some(-self)
    }

    fn lovasz(dk: &Self, dk2: &Self, dk1: &Self, m: &Self) -> Option<bool> {
        Some(BigInt::from(4) * dk * dk2 >= BigInt::from(3) * dk1 * dk1 - BigInt::from(4) * m * m)
    }

    fn beyond_half(m: &Self, d: &Self) -> Option<bool> {
        Some(BigInt::from(2) * m.abs() > *d)
    }

    fn nearest(m: &Self, d: &Self) -> Option<Self> {
        let two = BigInt::from(2);
        Some((&two * m + d).div_floor(&(&two * d)))
    }

    fn less_times(a: &Self, s: &Self, b: &Self) -> Option<Self> {
        Some(a - s * b)
    }
}

/// The bound under which a 128-bit entry stays, so that sums of a few 256-bit
/// products of entries never overflow.
const LIMIT: i128 = 1 << 125;

fn bounded(value: i128) -> Option<i128> {
    (-LIMIT < value && value < LIMIT).then_some(value)
}

impl Entry for i128 {
    fn zero() -> Self {
        0
    }

    fn one() -> Self {
        1
    }

    fn dot(first: &[Self], second: &[Self]) -> Option<Self> {
        first.iter().zip(second).try_fold(0i128, |sum, (a, b)| {
            bounded(sum.checked_add(a.checked_mul(*b)?)?)
        })
    }

    fn ratio(a: &Self, b: &Self, c: &Self, e: &Self, f: &Self) -> Option<Self> {
        let numerator = Wide::product(*a, *b).minus(Wide::product(*c, *e));
        bounded(numerator.exact_quotient(*f)?)
    }

    fn negated(&self) -> Option<Self> {
        Some(-self)
    }

    fn lovasz(dk: &Self, dk2: &Self, dk1: &Self, m: &Self) -> Option<bool> {
        let four = |x: Wide| x.plus(x).plus(x).plus(x);
        let three = |x: Wide| x.plus(x).plus(x);
        let left = four(Wide::product(*dk, *dk2));
        let right = three(Wide::product(*dk1, *dk1)).minus(four(Wide::product(*m, *m)));
        Some(left.minus(right).signum() >= 0)
    }

    fn beyond_half(m: &Self, d: &Self) -> Option<bool> {
        Some(2 * m.abs() > *d)
    }

    fn nearest(m: &Self, d: &Self) -> Option<Self> {
        bounded((2 * m + d).div_euclid(2 * d))
    }

    fn less_times(a: &Self, s: &Self, b: &Self) -> Option<Self> {
        bounded(a.checked_sub(s.checked_mul(*b)?)?)
    }
}

/// A 256-bit integer in two's complement, for products of two bounded entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Wide {
    high: i128,
    low: u128,
}

impl Wide {
    /// The exact product of two 128-bit integers.
    fn product(a: i128, b: i128) -> Wide {
        let (x, y) = (a.unsigned_abs(), b.unsigned_abs());
        let (x1, x0, y1, y0) = (
            x >> 64,
            x & u128::from(u64::MAX),
            y >> 64,
            y & u128::from(u64::MAX),
        );
        let (low, middle_one, middle_two, high) = (x0 * y0, x0 * y1, x1 * y0, x1 * y1);
        let (low, carry_one) = low.overflowing_add(middle_one << 64);
        let (low, carry_two) = low.overflowing_add(middle_two << 64);
        let high = high
            + (middle_one >> 64)
            + (middle_two >> 64)
            + u128::from(carry_one)
            + u128::from(carry_two);
        #[allow(
            clippy::cast_possible_wrap,
            reason = "a product of entries under 2^127 fits"
        )]
        let magnitude = Wide {
            high: high as i128,
            low,
        };
        if (a < 0) == (b < 0) {
            magnitude
        } else {
            magnitude.negated()
        }
    }

    fn negated(self) -> Wide {
        let (low, borrow) = 0u128.overflowing_sub(self.low);
        Wide {
            high: self.high.wrapping_neg().wrapping_sub(i128::from(borrow)),
            low,
        }
    }

    fn plus(self, other: Wide) -> Wide {
        let (low, carry) = self.low.overflowing_add(other.low);
        Wide {
            high: self
                .high
                .wrapping_add(other.high)
                .wrapping_add(i128::from(carry)),
            low,
        }
    }

    fn minus(self, other: Wide) -> Wide {
        self.plus(other.negated())
    }

    fn signum(self) -> i32 {
        match self.high.cmp(&0) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Greater => 1,
            std::cmp::Ordering::Equal => i32::from(self.low != 0),
        }
    }

    /// The quotient by a positive divisor that divides the integer exactly, when it
    /// fits in 128 bits: the low bits times the divisor's inverse modulo 2^128, checked
    /// by multiplying back.
    fn exact_quotient(self, divisor: i128) -> Option<i128> {
        if divisor <= 0 {
            return None;
        }
        let shift = divisor.trailing_zeros();
        let odd = divisor.unsigned_abs() >> shift;
        // Newton's iteration doubles the correct low bits of the inverse: 3, 6, ... 192.
        let mut inverse = odd;
        for _ in 0..6 {
            inverse = inverse.wrapping_mul(2u128.wrapping_sub(odd.wrapping_mul(inverse)));
        }
        let shifted = if shift == 0 {
            self.low
        } else {
            #[allow(clippy::cast_sign_loss, reason = "the high bits move down as bits")]
            let high = self.high as u128;
            (self.low >> shift) | (high << (128 - shift))
        };
        #[allow(
            clippy::cast_possible_wrap,
            reason = "the quotient's bits, as two's complement"
        )]
        let quotient = shifted.wrapping_mul(inverse) as i128;
        (Wide::product(quotient, divisor) == self).then_some(quotient)
    }
}

/// An LLL-reduced basis of the lattice the integer vectors span, with Lovász's factor
/// of 3/4, or `None` when a value outgrows the integers.
fn reduced<T: Entry, V: AsRef<[T]>>(basis: &[V]) -> Option<Vec<Vec<T>>> {
    let mut lattice = Lattice::new(basis)?;
    let mut k = 2;
    while k <= lattice.count {
        lattice.orthogonalize(k)?;
        lattice.reduce(k, k - 1)?;
        while !T::lovasz(
            &lattice.d[k],
            &lattice.d[k - 2],
            &lattice.d[k - 1],
            &lattice.mu[k][k - 1],
        )? {
            lattice.swap(k)?;
            k = (k - 1).max(2);
            lattice.reduce(k, k - 1)?;
        }
        for j in (1..k - 1).rev() {
            lattice.reduce(k, j)?;
        }
        k += 1;
    }
    Some(lattice.vectors.split_off(1))
}

/// A lattice basis under Cohen's integral LLL, its Gram-Schmidt data in integers.
///
/// `d[i]` is the Gram determinant of the first i vectors, and `mu[k][j]` is `d[j]`
/// times vector k's Gram-Schmidt coefficient on vector j. Indices count from 1.
struct Lattice<T> {
    count: usize,
    vectors: Vec<Vec<T>>,
    d: Vec<T>,
    mu: Vec<Vec<T>>,
    known: usize,
}

impl<T: Entry> Lattice<T> {
    fn new<V: AsRef<[T]>>(basis: &[V]) -> Option<Lattice<T>> {
        let count = basis.len();
        let first = basis[0].as_ref();
        let mut d = vec![T::one(), T::dot(first, first)?];
        d.resize(count + 1, T::zero());
        let mut vectors = vec![vec![]];
        vectors.extend(basis.iter().map(|vector| vector.as_ref().to_vec()));
        Some(Lattice {
            count,
            vectors,
            d,
            mu: vec![vec![T::zero(); count + 1]; count + 1],
            known: 1,
        })
    }

    /// Find vector k's Gram-Schmidt coefficients, the first time it is reached.
    fn orthogonalize(&mut self, k: usize) -> Option<()> {
        if k <= self.known {
            return Some(());
        }
        self.known = k;
        for j in 1..=k {
            let mut u = T::dot(&self.vectors[k], &self.vectors[j])?;
            for i in 1..j {
                u = T::ratio(
                    &self.d[i],
                    &u,
                    &self.mu[k][i],
                    &self.mu[j][i],
                    &self.d[i - 1],
                )?;
            }
            if j < k {
                self.mu[k][j] = u;
            } else {
                self.d[k] = u;
            }
        }
        Some(())
    }

    /// Take the nearest multiple of vector j from vector k.
    fn reduce(&mut self, k: usize, j: usize) -> Option<()> {
        if !T::beyond_half(&self.mu[k][j], &self.d[j])? {
            return Some(());
        }
        let shift = T::nearest(&self.mu[k][j], &self.d[j])?;
        let (low, high) = self.vectors.split_at_mut(k);
        for (a, b) in high[0].iter_mut().zip(&low[j]) {
            *a = T::less_times(a, &shift, b)?;
        }
        self.mu[k][j] = T::less_times(&self.mu[k][j], &shift, &self.d[j])?;
        for i in 1..j {
            self.mu[k][i] = T::less_times(&self.mu[k][i], &shift, &self.mu[j][i])?;
        }
        Some(())
    }

    /// Exchange vectors k - 1 and k.
    fn swap(&mut self, k: usize) -> Option<()> {
        self.vectors.swap(k, k - 1);
        for j in 1..k - 1 {
            let (low, high) = self.mu.split_at_mut(k);
            std::mem::swap(&mut high[0][j], &mut low[k - 1][j]);
        }
        let m = self.mu[k][k - 1].clone();
        let minus_m = m.negated()?;
        let before = T::ratio(&self.d[k - 2], &self.d[k], &minus_m, &m, &self.d[k - 1])?;
        for i in k + 1..=self.known {
            let t = self.mu[i][k].clone();
            let above = T::ratio(&self.d[k], &self.mu[i][k - 1], &m, &t, &self.d[k - 1])?;
            self.mu[i][k - 1] = T::ratio(&before, &t, &minus_m, &above, &self.d[k])?;
            self.mu[i][k] = above;
        }
        self.d[k - 1] = before;
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_half_root_two_relates_to_root_two() {
        let basis = [1.0, std::f64::consts::SQRT_2];
        let found = relations(
            std::f64::consts::FRAC_1_SQRT_2,
            &basis,
            1_000_000_000_000,
            10_000,
        );
        assert!(found.contains(&(2, vec![0, 1])), "{found:?}");
    }

    #[test]
    fn the_golden_ratio_relates_to_root_five() {
        let golden = f64::midpoint(1.0, 5f64.sqrt());
        let found = relations(golden, &[1.0, 5f64.sqrt()], 1_000_000_000_000, 10_000);
        assert!(found.contains(&(2, vec![1, 1])), "{found:?}");
    }

    #[test]
    fn a_cosine_of_eleven_and_a_quarter_degrees_relates_to_nested_roots() {
        // cos 22.5° is √(2 + √2) / 2, in the basis 1, √2, √(2 + √2), √2·√(2 + √2).
        let (two, nested) = (
            std::f64::consts::SQRT_2,
            (2.0 + std::f64::consts::SQRT_2).sqrt(),
        );
        let basis = [1.0, two, nested, two * nested];
        let found = relations(nested / 4.0, &basis, 1_000_000_000_000, 10_000);
        assert!(found.contains(&(4, vec![0, 0, 1, 0])), "{found:?}");
    }

    proptest::proptest! {
        #[test]
        fn wide_products_and_quotients_match_big_integers(a in -(1i128 << 124)..(1i128 << 124), b in -(1i128 << 124)..(1i128 << 124), f in 1i128..(1i128 << 100)) {
            let big = BigInt::from(a) * BigInt::from(b);
            let wide = Wide::product(a, b);
            let back = BigInt::from(wide.high) * (<BigInt as One>::one() << 128u32) + BigInt::from(wide.low);
            proptest::prop_assert_eq!(&back, &big);
            let exact = Wide::product(a % (1 << 60), f);
            proptest::prop_assert_eq!(exact.exact_quotient(f), Some(a % (1 << 60)));
            let (q, r) = big.div_rem(&BigInt::from(f));
            if !r.is_zero() {
                proptest::prop_assert_eq!(wide.exact_quotient(f), None);
            } else if let Some(q) = q.to_i128() {
                proptest::prop_assert_eq!(wide.exact_quotient(f), Some(q));
            }
        }

        #[test]
        fn reductions_in_128_bits_match_big_integers(value in 0.0f64..4.0, root in proptest::sample::select(vec![2u32, 3, 5, 6, 7]), scale in proptest::sample::select(vec![1_000_000i64, 1_000_000_000_000, 1 << 50])) {
            let surd = f64::from(root).sqrt();
            #[allow(clippy::cast_precision_loss)]
            let scaled = |x: f64| (scale as f64 * x).round_ties_even().to_i128().unwrap_or(0);
            let basis = [[1, 0, 0, scaled(value)], [0, 1, 0, i128::from(scale)], [0, 0, 1, scaled(surd)]];
            let big = basis.map(|vector| vector.map(BigInt::from).to_vec()).to_vec();
            let expected = reduced(&big).expect("big integers hold every value");
            if let Some(found) = reduced(&basis) {
                let found: Vec<Vec<BigInt>> = found.iter().map(|v| v.iter().map(|&x| BigInt::from(x)).collect()).collect();
                proptest::prop_assert_eq!(found, expected);
            }
        }
    }
}
