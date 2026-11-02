//! The energies of a sheet, each with its exact gradient and a Hessian projected to be positive
//! semi-definite, element by element.

pub(crate) mod bending;
pub(crate) mod membrane;

use nalgebra::{DMatrix, SMatrix, SymmetricEigen, Vector3};

/// A position in space, in meters.
pub(crate) type Point = Vector3<f64>;

/// The derivatives of one element's energy over its vertices, with `D` coordinates: the gradient
/// in newtons and the Hessian in newtons per meter.
#[derive(Clone, Debug)]
pub(crate) struct Local<const D: usize> {
    pub gradient: SMatrix<f64, D, 1>,
    pub hessian: SMatrix<f64, D, D>,
}

/// The nearest positive semi-definite matrix to a symmetric one: its negative eigenvalues set to
/// zero.
pub(crate) fn project<const D: usize>(matrix: &SMatrix<f64, D, D>) -> SMatrix<f64, D, D> {
    // A matrix with a Cholesky factor is already positive definite.
    if matrix.cholesky().is_some() {
        return *matrix;
    }
    let mut eigen = SymmetricEigen::new(DMatrix::from_column_slice(D, D, matrix.as_slice()));
    for value in eigen.eigenvalues.iter_mut() {
        *value = value.max(0.0);
    }
    SMatrix::from_column_slice(eigen.recompose().as_slice())
}

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

    #[test]
    fn projection_clears_negative_eigenvalues() {
        let matrix = SMatrix::<f64, 2, 2>::new(1.0, 2.0, 2.0, 1.0);
        let projected = project(&matrix);
        let eigen = SymmetricEigen::new(projected);
        assert!(eigen.eigenvalues.min() > -1e-12);
        assert!(
            (projected - SMatrix::<f64, 2, 2>::new(1.5, 1.5, 1.5, 1.5))
                .abs()
                .max()
                < 1e-12
        );
    }
}
