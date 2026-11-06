//! Integer relations by lattice reduction, as `origamy.pattern` snaps coordinates.
//!
//! The integer combinations of a float, 1 and a square root span a lattice. Its
//! LLL-reduced basis holds short vectors: small integers whose combination nearly
//! vanishes. Cohen's integral LLL keeps the Gram-Schmidt data in integers, so the
//! reduction is exact.

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};

/// Small integers `(d, p, q)`, with `d` positive and `q` nonzero, that make
/// `d·value ≈ p + q√root`.
///
/// `scale` weighs a relation's error against its integers, and `size` bounds the
/// integers kept. The basis is the integer vectors of `value`, 1 and `√root`, each
/// with its float times `scale`, rounded half to even, as a last coordinate.
#[must_use]
pub fn relations(value: f64, root: u32, scale: i64, size: i64) -> Vec<(i64, i64, i64)> {
    let surd = f64::from(root).sqrt();
    #[allow(clippy::cast_precision_loss, reason = "the scale is a float's weight")]
    let scaled = |x: f64| BigInt::from((scale as f64 * x).round_ties_even().to_i128().unwrap_or(0));
    let basis = vec![
        vec![BigInt::one(), BigInt::zero(), BigInt::zero(), scaled(value)],
        vec![
            BigInt::zero(),
            BigInt::one(),
            BigInt::zero(),
            BigInt::from(scale),
        ],
        vec![BigInt::zero(), BigInt::zero(), BigInt::one(), scaled(surd)],
    ];
    let mut found = vec![];
    for vector in reduced(basis) {
        let sign = if vector[0].is_positive() { 1 } else { -1 };
        let (Some(times), Some(minus_p), Some(minus_q)) =
            (vector[0].to_i64(), vector[1].to_i64(), vector[2].to_i64())
        else {
            continue;
        };
        let (d, p, q) = (sign * times, -sign * minus_p, -sign * minus_q);
        if d != 0 && q != 0 && d.max(p.abs()).max(q.abs()) <= size {
            found.push((d, p, q));
        }
    }
    found
}

/// An LLL-reduced basis of the lattice the integer vectors span, with Lovász's factor
/// of 3/4.
fn reduced(basis: Vec<Vec<BigInt>>) -> Vec<Vec<BigInt>> {
    let mut lattice = Lattice::new(basis);
    let mut k = 2;
    while k <= lattice.count {
        lattice.orthogonalize(k);
        lattice.reduce(k, k - 1);
        while !lattice.lovasz(k) {
            lattice.swap(k);
            k = (k - 1).max(2);
            lattice.reduce(k, k - 1);
        }
        for j in (1..k - 1).rev() {
            lattice.reduce(k, j);
        }
        k += 1;
    }
    lattice.vectors.split_off(1)
}

/// A lattice basis under Cohen's integral LLL, its Gram-Schmidt data in integers.
///
/// `d[i]` is the Gram determinant of the first i vectors, and `mu[k][j]` is `d[j]`
/// times vector k's Gram-Schmidt coefficient on vector j. Indices count from 1.
struct Lattice {
    count: usize,
    vectors: Vec<Vec<BigInt>>,
    d: Vec<BigInt>,
    mu: Vec<Vec<BigInt>>,
    known: usize,
}

impl Lattice {
    fn new(basis: Vec<Vec<BigInt>>) -> Lattice {
        let count = basis.len();
        let mut d = vec![BigInt::one(), dot(&basis[0], &basis[0])];
        d.resize(count + 1, BigInt::zero());
        let mut vectors = vec![vec![]];
        vectors.extend(basis);
        Lattice {
            count,
            vectors,
            d,
            mu: vec![vec![BigInt::zero(); count + 1]; count + 1],
            known: 1,
        }
    }

    /// Find vector k's Gram-Schmidt coefficients, the first time it is reached.
    fn orthogonalize(&mut self, k: usize) {
        if k <= self.known {
            return;
        }
        self.known = k;
        for j in 1..=k {
            let mut u = dot(&self.vectors[k], &self.vectors[j]);
            for i in 1..j {
                u = (&self.d[i] * &u - &self.mu[k][i] * &self.mu[j][i]).div_floor(&self.d[i - 1]);
            }
            if j < k {
                self.mu[k][j] = u;
            } else {
                self.d[k] = u;
            }
        }
    }

    /// Whether vectors k - 1 and k meet Lovász's condition.
    fn lovasz(&self, k: usize) -> bool {
        let d = &self.d;
        let m = &self.mu[k][k - 1];
        BigInt::from(4) * &d[k] * &d[k - 2]
            >= BigInt::from(3) * &d[k - 1] * &d[k - 1] - BigInt::from(4) * m * m
    }

    /// Take the nearest multiple of vector j from vector k.
    fn reduce(&mut self, k: usize, j: usize) {
        let two = BigInt::from(2);
        if &two * self.mu[k][j].abs() <= self.d[j] {
            return;
        }
        let shift = (&two * &self.mu[k][j] + &self.d[j]).div_floor(&(&two * &self.d[j]));
        let step: Vec<BigInt> = self.vectors[j].iter().map(|b| &shift * b).collect();
        for (a, b) in self.vectors[k].iter_mut().zip(step) {
            *a -= b;
        }
        let dj = &shift * &self.d[j];
        self.mu[k][j] -= dj;
        for i in 1..j {
            let t = &shift * &self.mu[j][i];
            self.mu[k][i] -= t;
        }
    }

    /// Exchange vectors k - 1 and k.
    fn swap(&mut self, k: usize) {
        self.vectors.swap(k, k - 1);
        for j in 1..k - 1 {
            let (low, high) = self.mu.split_at_mut(k);
            std::mem::swap(&mut high[0][j], &mut low[k - 1][j]);
        }
        let m = self.mu[k][k - 1].clone();
        let before = (&self.d[k - 2] * &self.d[k] + &m * &m).div_floor(&self.d[k - 1]);
        for i in k + 1..=self.known {
            let t = self.mu[i][k].clone();
            let above = (&self.d[k] * &self.mu[i][k - 1] - &m * &t).div_floor(&self.d[k - 1]);
            self.mu[i][k - 1] = (&before * &t + &m * &above).div_floor(&self.d[k]);
            self.mu[i][k] = above;
        }
        self.d[k - 1] = before;
    }
}

fn dot(first: &[BigInt], second: &[BigInt]) -> BigInt {
    first.iter().zip(second).map(|(a, b)| a * b).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_half_root_two_relates_to_root_two() {
        let found = relations(
            std::f64::consts::FRAC_1_SQRT_2,
            2,
            1_000_000_000_000,
            10_000,
        );
        assert!(found.contains(&(2, 0, 1)), "{found:?}");
    }

    #[test]
    fn the_golden_ratio_relates_to_root_five() {
        let golden = f64::midpoint(1.0, 5f64.sqrt());
        let found = relations(golden, 5, 1_000_000_000_000, 10_000);
        assert!(found.contains(&(2, 1, 1)), "{found:?}");
    }
}
