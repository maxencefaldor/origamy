//! Friction: Coulomb's law between pieces of paper in contact, and between paper and the table.
//!
//! Friction is lagged, as in incremental potential contact. At the start of a time step, each
//! contact fixes its normal force, its tangent plane and its closest points. Over the step,
//! friction resists the slip of those points in that plane with the potential
//!
//! D = μ λ f₀(|u|).
//!
//! Here μ is the coefficient of friction, λ the normal force and u the slip. The function f₀ is
//! the slip itself past a small stick distance ε, and smooths the turn below it. Friction thus
//! holds a contact that slips less than ε a step, and resists sliding with the force μ λ.

use nalgebra::{Matrix2, SMatrix, Vector2, Vector3};

use crate::energy::Point;

/// One contact's friction, fixed at the start of a time step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Slip {
    /// The contact's vertices; a vertex on the table has one.
    pub stencil: [Option<u32>; 4],
    /// Each vertex's weight in the slip of the contact's closest points.
    pub weights: [f64; 4],
    /// Two orthonormal directions of the contact's tangent plane, as columns.
    pub tangent: SMatrix<f64, 3, 2>,
    /// The friction force when sliding: the coefficient of friction times the normal force,
    /// in newtons.
    pub force: f64,
}

impl Slip {
    /// The slip u of the contact's closest points in its tangent plane, from `start` to `x`.
    fn slip(&self, x: &[Point], start: &[Point]) -> Vector2<f64> {
        let mut moved = Vector3::zeros();
        for (vertex, weight) in self.stencil.iter().zip(self.weights) {
            if let Some(v) = vertex {
                let v = *v as usize;
                moved += (x[v] - start[v]) * weight;
            }
        }
        self.tangent.transpose() * moved
    }

    /// The friction potential at positions `x`, in joules, with stick distance `stick`.
    pub fn energy(&self, x: &[Point], start: &[Point], stick: f64) -> f64 {
        self.force * smooth(self.slip(x, start).norm(), stick)[0]
    }

    /// The gradient and Hessian of the friction potential over the stencil, with stick distance
    /// `stick`. The Hessian is exact, and positive semi-definite.
    pub fn derivatives(
        &self,
        x: &[Point],
        start: &[Point],
        stick: f64,
    ) -> (SMatrix<f64, 12, 1>, SMatrix<f64, 12, 12>) {
        let slip = self.slip(x, start);
        let size = slip.norm();
        let [_, slope, curvature] = smooth(size, stick);
        // The derivatives of f₀(|u|) in u: f₁(s) û, and the Hessian below.
        let (gradient, hessian) = if size > 0.0 {
            let direction = slip / size;
            let along = direction * direction.transpose();
            (
                direction * slope,
                along * curvature + (Matrix2::identity() - along) * (slope / size),
            )
        } else {
            // At no slip, f₁(s) / s tends to the curvature 2 / ε, alike in every direction.
            (Vector2::zeros(), Matrix2::identity() * curvature)
        };
        // The slip is linear in the positions: u = J x, with J the weights times the tangent.
        let mut jacobian = SMatrix::<f64, 2, 12>::zeros();
        for (k, weight) in self.weights.iter().enumerate() {
            if self.stencil[k].is_some() {
                jacobian
                    .fixed_view_mut::<2, 3>(0, 3 * k)
                    .copy_from(&(self.tangent.transpose() * *weight));
            }
        }
        (
            jacobian.transpose() * gradient * self.force,
            jacobian.transpose() * hessian * jacobian * self.force,
        )
    }
}

/// The smoothed slip f₀(s), with its first and second derivatives, for a stick distance `stick`.
///
/// Past the stick distance, f₀(s) = s, so friction is the sliding force. Below it,
/// f₀(s) = −s³ / 3ε² + s² / ε + ε / 3. The slope f₁(s) = −s² / ε² + 2s / ε rises from 0 to 1,
/// so the force grows with the slip. The function is continuously differentiable throughout.
pub(crate) fn smooth(size: f64, stick: f64) -> [f64; 3] {
    if size >= stick {
        [size, 1.0, 0.0]
    } else {
        let s = size / stick;
        [
            stick * (-s * s * s / 3.0 + s * s + 1.0 / 3.0),
            -s * s + 2.0 * s,
            (2.0 - 2.0 * s) / stick,
        ]
    }
}

/// Two orthonormal directions perpendicular to a unit normal, as columns.
pub(crate) fn tangent_plane(normal: &Vector3<f64>) -> SMatrix<f64, 3, 2> {
    // The axis least aligned with the normal leaves the most of itself in the plane.
    let axis = if normal[0].abs() < 0.9 {
        Vector3::x()
    } else {
        Vector3::y()
    };
    let first = (axis - normal * axis.dot(normal)).normalize();
    let second = normal.cross(&first);
    SMatrix::from_columns(&[first, second])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::energy::testing::check;

    fn slip_between_edges() -> (Slip, Vec<Point>) {
        let normal = Vector3::new(0.1, -0.2, 1.0).normalize();
        let slip = Slip {
            stencil: [Some(0), Some(1), Some(2), Some(3)],
            weights: [0.7, 0.3, -0.4, -0.6],
            tangent: tangent_plane(&normal),
            force: 0.02,
        };
        let start = vec![
            Point::new(0.0, 0.0, 0.0),
            Point::new(0.01, 0.0, 0.0),
            Point::new(0.004, -0.003, 7e-5),
            Point::new(0.005, 0.004, 7e-5),
        ];
        (slip, start)
    }

    #[test]
    fn the_smoothing_is_continuously_differentiable() {
        let stick = 1e-6;
        let below = smooth(stick * (1.0 - 1e-12), stick);
        let above = smooth(stick, stick);
        assert!((below[0] - above[0]).abs() < 1e-17 && (below[1] - above[1]).abs() < 1e-11);
        assert!(smooth(0.0, stick)[1].abs() < f64::MIN_POSITIVE);
        for k in 1..100 {
            let s = stick * f64::from(k) / 100.0;
            let [value, slope, _] = smooth(s, stick);
            assert!(value >= s && (0.0..=1.0).contains(&slope));
        }
    }

    #[test]
    fn the_tangent_plane_is_orthonormal_and_perpendicular() {
        for normal in [
            Vector3::z(),
            Vector3::x(),
            Vector3::new(1.0, 2.0, -3.0).normalize(),
        ] {
            let plane = tangent_plane(&normal);
            assert!(
                (plane.transpose() * plane - Matrix2::identity())
                    .abs()
                    .max()
                    < 1e-15
            );
            assert!((plane.transpose() * normal).abs().max() < 1e-15);
        }
    }

    #[test]
    fn friction_derivatives_match_finite_differences() {
        let (slip, start) = slip_between_edges();
        let stick = 1e-6;
        // Slips past, within and at the edge of the stick distance.
        for size in [3e-6, 4e-7, 1e-6, 0.0] {
            let mut moved = start.clone();
            moved[0] += slip.tangent.column(0) * (size / 0.7) + Vector3::new(0.0, 0.0, 1e-7);
            moved[3] -= slip.tangent.column(1) * (0.3 * size / 0.6);
            let x =
                SMatrix::<f64, 12, 1>::from_iterator(moved.iter().flat_map(|p| p.iter().copied()));
            let points = |x: &SMatrix<f64, 12, 1>| -> Vec<Point> {
                (0..4)
                    .map(|k| x.fixed_rows::<3>(3 * k).into_owned())
                    .collect()
            };
            let (_, hessian) = slip.derivatives(&points(&x), &start, stick);
            check(
                x,
                |x| slip.energy(&points(x), &start, stick),
                |x| slip.derivatives(&points(x), &start, stick).0,
                &hessian,
                1e-11,
                1e-4,
            );
        }
    }

    #[test]
    fn sliding_friction_resists_with_its_full_force_against_the_slip() {
        let (slip, start) = slip_between_edges();
        let mut moved = start.clone();
        moved[0] += slip.tangent.column(0) * 1e-3;
        let (gradient, hessian) = slip.derivatives(&moved, &start, 1e-6);
        // The force on the point is μλ times its weight, along the slip, in the plane.
        let force = gradient.fixed_rows::<3>(0).into_owned();
        assert!((force - slip.tangent.column(0) * (0.02 * 0.7)).norm() < 1e-15);
        let eigen = hessian.symmetric_eigenvalues();
        assert!(eigen.min() > -1e-12 * eigen.max());
    }
}
