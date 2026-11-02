//! The energies of a sheet, each with its exact gradient and a Hessian projected to be positive
//! semi-definite, element by element.

pub(crate) mod bending;
pub(crate) mod membrane;

use nalgebra::{SMatrix, Vector3};

/// A position in space, in meters.
pub(crate) type Point = Vector3<f64>;

/// The derivatives of one element's energy over its vertices, with `D` coordinates: the gradient
/// in newtons and the Hessian in newtons per meter.
#[derive(Clone, Debug)]
pub(crate) struct Local<const D: usize> {
    pub gradient: SMatrix<f64, D, 1>,
    pub hessian: SMatrix<f64, D, D>,
}

/// A symmetric matrix made positive semi-definite by taking the absolute value of each
/// eigenvalue.
///
/// Flipping negative curvature, rather than clearing it, keeps its size in the Newton step,
/// which then converges faster. The filter follows Chen et al., Stabler Neo-Hookean
/// Simulation: Absolute Eigenvalue Filtering for Projected Newton, 2024.
pub(crate) trait Project {
    /// The projected matrix.
    fn project(&self) -> Self;
}

/// Implements [`Project`] for the sizes of the elements' Hessians, where nalgebra's eigen
/// solver works on the stack.
macro_rules! project {
    ($($size:literal),*) => {$(
        impl Project for SMatrix<f64, $size, $size> {
            fn project(&self) -> Self {
                // A matrix with a Cholesky factor is already positive definite. A matrix that
                // gains one from a shift at the scale of rounding is semi-definite already.
                if self.cholesky().is_some() || self.amax() == 0.0 {
                    return *self;
                }
                let shifted = self + Self::identity() * (1e-12 * self.diagonal().amax());
                if shifted.cholesky().is_some() {
                    return *self;
                }
                let eigen = self.symmetric_eigen();
                let flipped = eigen.eigenvectors
                    * SMatrix::from_diagonal(&eigen.eigenvalues.abs())
                    * eigen.eigenvectors.transpose();
                (flipped + flipped.transpose()) * 0.5
            }
        }
    )*};
}

project!(2, 9, 12, 18);

/// The barrier that keeps a quantity `d` above zero: zero from `onset` up, smooth, and growing
/// without bound as `d` falls to zero.
///
/// The barrier is −(d − onset)² ln(d / onset), after incremental potential contact.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Barrier {
    pub onset: f64,
}

impl Barrier {
    /// The barrier's value and its first and second derivatives at `d`, or `None` where `d` is
    /// not positive.
    pub fn at(self, d: f64) -> Option<[f64; 3]> {
        if d <= 0.0 {
            return None;
        }
        if d >= self.onset {
            return Some([0.0; 3]);
        }
        let gap = d - self.onset;
        let log = (d / self.onset).ln();
        Some([
            -gap * gap * log,
            -2.0 * gap * log - gap * gap / d,
            -2.0 * log - 4.0 * gap / d + gap * gap / (d * d),
        ])
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! Finite-difference checks of derivatives.

    use nalgebra::SMatrix;

    /// Checks a gradient and a Hessian against central differences of the energy and gradient,
    /// to a relative tolerance.
    pub fn check<const D: usize>(
        x: SMatrix<f64, D, 1>,
        energy: impl Fn(&SMatrix<f64, D, 1>) -> f64,
        gradient: impl Fn(&SMatrix<f64, D, 1>) -> SMatrix<f64, D, 1>,
        hessian: &SMatrix<f64, D, D>,
        step: f64,
        tolerance: f64,
    ) {
        let g = gradient(&x);
        let mut numeric_gradient = SMatrix::<f64, D, 1>::zeros();
        let mut numeric_hessian = SMatrix::<f64, D, D>::zeros();
        for i in 0..D {
            let mut ahead = x;
            let mut behind = x;
            ahead[i] += step;
            behind[i] -= step;
            numeric_gradient[i] = (energy(&ahead) - energy(&behind)) / (2.0 * step);
            numeric_hessian.set_column(i, &((gradient(&ahead) - gradient(&behind)) / (2.0 * step)));
        }
        let scale = g.abs().max().max(f64::MIN_POSITIVE);
        assert!(
            (g - numeric_gradient).abs().max() <= tolerance * scale,
            "gradient {g} differs from its finite differences {numeric_gradient}"
        );
        let scale = hessian.abs().max().max(f64::MIN_POSITIVE);
        assert!(
            (hessian - numeric_hessian).abs().max() <= tolerance * scale,
            "Hessian {hessian} differs from its finite differences {numeric_hessian}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_barrier_is_smooth_where_it_starts() {
        let barrier = Barrier { onset: 0.3 };
        assert_eq!(barrier.at(0.3), Some([0.0; 3]));
        let [value, slope, curvature] = barrier.at(0.3 - 1e-9).unwrap();
        assert!(value.abs() < 1e-15 && slope.abs() < 1e-12 && curvature.abs() < 1e-6);
        assert!(barrier.at(1e-12).unwrap()[0] > barrier.at(1e-6).unwrap()[0]);
        assert_eq!(barrier.at(0.0), None);
    }

    #[test]
    fn the_barrier_derivatives_match_finite_differences() {
        let barrier = Barrier { onset: 0.3 };
        for d in [0.01, 0.1, 0.2, 0.29] {
            let h = 1e-6;
            let [_, slope, curvature] = barrier.at(d).unwrap();
            let value = |d: f64| barrier.at(d).unwrap()[0];
            let derivative = |d: f64| barrier.at(d).unwrap()[1];
            assert!(
                (slope - (value(d + h) - value(d - h)) / (2.0 * h)).abs()
                    < 1e-6 * slope.abs().max(1.0)
            );
            let numeric = (derivative(d + h) - derivative(d - h)) / (2.0 * h);
            assert!((curvature - numeric).abs() < 1e-6 * curvature.abs().max(1.0));
        }
    }

    proptest::proptest! {
        #[test]
        fn projection_takes_each_eigenvalue_in_size(
            entries in proptest::collection::vec(-1e3..1e3f64, 144),
        ) {
            let raw = SMatrix::<f64, 12, 12>::from_column_slice(&entries);
            let matrix = raw + raw.transpose();
            let size = matrix.norm();
            let mut expected: Vec<f64> =
                matrix.symmetric_eigenvalues().iter().map(|v| v.abs()).collect();
            let mut found: Vec<f64> = matrix.project().symmetric_eigenvalues().iter().copied().collect();
            expected.sort_by(f64::total_cmp);
            found.sort_by(f64::total_cmp);
            for (e, f) in expected.iter().zip(&found) {
                proptest::prop_assert!((e - f).abs() <= 1e-12 * size);
            }
        }
    }

    #[test]
    fn projection_flips_negative_eigenvalues() {
        // The eigenvalues are 3 and −1, along (1, 1) and (1, −1).
        let matrix = SMatrix::<f64, 2, 2>::new(1.0, 2.0, 2.0, 1.0);
        let projected = matrix.project();
        let expected = SMatrix::<f64, 2, 2>::new(2.0, 1.0, 1.0, 2.0);
        assert!((projected - expected).abs().max() < 1e-12);
        let definite = SMatrix::<f64, 2, 2>::new(2.0, 1.0, 1.0, 2.0);
        assert_eq!(definite.project(), definite);
    }
}
