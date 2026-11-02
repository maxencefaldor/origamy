//! Bending: the plate energy of each triangle from a discrete curvature, and sharp creases.
//!
//! A triangle's curvature comes from the normals at the midpoints of its edges. Each is the
//! mean of the normals of the two triangles along that edge. Each tilts from the triangle's
//! normal by half the dihedral angle there. Summed over the triangle's edges, the curvature is
//!
//! K = Σ (|ē| / Ā) sin(θ / 2) t̄ t̄ᵀ.
//!
//! Here ē is an edge at rest, t̄ its outward normal in the flat sheet, Ā the triangle's rest
//! area and θ the dihedral angle. The curvature is exact for paper rolled round a cylinder
//! through its vertices. The energy is the plate energy ½ Ā (K − K̄)ᵀ D (K − K̄), with D the
//! bending stiffness. The rest curvature K̄ comes from each dihedral's rest angle alike. A
//! sharp crease takes no part in its triangles' curvature. The paper bends there alone, with
//! an energy of its own about the crease's rest angle.

use nalgebra::{SMatrix, Vector3};

use super::{Local, Point};
use crate::generated::dihedral_angle;

/// The dihedral angle of a stencil, with its gradient and Hessian.
#[derive(Clone, Debug)]
pub(crate) struct Angle {
    pub value: f64,
    pub gradient: SMatrix<f64, 12, 1>,
    pub hessian: SMatrix<f64, 12, 12>,
}

/// The dihedral angle of the stencil `[start, end, left, right]`: the edge from `start` to
/// `end`, with `left` opposite it on its left and `right` on its right. The angle is positive
/// when the sheet folds toward its front.
pub(crate) fn angle_value([start, end, left, right]: &[Point; 4]) -> f64 {
    let edge = end - start;
    let left_normal = edge.cross(&(left - start));
    let right_normal = (right - start).cross(&edge);
    edge.dot(&right_normal.cross(&left_normal))
        .atan2(edge.norm() * left_normal.dot(&right_normal))
}

/// The dihedral angle of a stencil with its exact gradient and Hessian.
pub(crate) fn angle([start, end, left, right]: &[Point; 4]) -> Angle {
    let array = |p: Point| [p[0], p[1], p[2]];
    let (value, relative_gradient, relative_hessian) = dihedral_angle(
        array(end - start),
        array(left - start),
        array(right - start),
    );
    // The angle depends on the stencil relative to its start; `relative` maps it to the stencil.
    let mut relative = SMatrix::<f64, 9, 12>::zeros();
    for k in 0..3 {
        for i in 0..3 {
            relative[(3 * k + i, i)] = -1.0;
            relative[(3 * k + i, 3 * (k + 1) + i)] = 1.0;
        }
    }
    let gradient = relative.transpose() * SMatrix::<f64, 9, 1>::from_row_slice(&relative_gradient);
    let hessian9 = SMatrix::<f64, 9, 9>::from_fn(|i, j| relative_hessian[i][j]);
    Angle {
        value,
        gradient,
        hessian: relative.transpose() * hessian9 * relative,
    }
}

/// The half-angle sine σ = sin(θ / 2) of a dihedral angle, with its gradient and Hessian.
pub(crate) fn half_sine(angle: &Angle) -> Angle {
    let (sin, cos) = (0.5 * angle.value).sin_cos();
    Angle {
        value: sin,
        gradient: angle.gradient * (0.5 * cos),
        hessian: angle.hessian * (0.5 * cos)
            - angle.gradient * angle.gradient.transpose() * (0.25 * sin),
    }
}

/// An edge of a triangle that bends: the half-angle sine there, its gradient over the
/// dihedral's stencil, and that stencil's slots in the triangle's stencil.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BendingEdge<'a> {
    pub sine: f64,
    pub gradient: &'a SMatrix<f64, 12, 1>,
    pub slots: [usize; 4],
}

/// What a triangle keeps of its rest shape for bending.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RestPlate {
    /// The triangle's rest area, in square meters.
    pub area: f64,
    /// For the edge opposite each corner, (|ē| / Ā) t̄ t̄ᵀ in voigt form, in inverse meters.
    pub directions: [Vector3<f64>; 3],
}

impl RestPlate {
    /// The rest plate of a counterclockwise triangle with these corners in the flat sheet.
    pub fn new(corners: [[f64; 2]; 3]) -> Self {
        let [a, b, c] = corners;
        let area = 0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]));
        let directions = [0, 1, 2].map(|i| {
            let (from, to) = (corners[(i + 1) % 3], corners[(i + 2) % 3]);
            let edge = [to[0] - from[0], to[1] - from[1]];
            let length = edge[0].hypot(edge[1]);
            // The outward normal of a counterclockwise triangle's edge turns the edge clockwise.
            let (tx, ty) = (edge[1] / length, -edge[0] / length);
            Vector3::new(tx * tx, ty * ty, 2.0 * tx * ty) * (length / area)
        });
        Self { area, directions }
    }

    /// The matrix P of the plate energy ½ (σ − σ̄)ᵀ P (σ − σ̄), for the bending stiffness
    /// `stiffness` in voigt form. Here σ holds the half-angle sines of the triangle's edges.
    pub fn energy_matrix(&self, stiffness: &nalgebra::Matrix3<f64>) -> nalgebra::Matrix3<f64> {
        nalgebra::Matrix3::from_fn(|i, j| {
            self.area * self.directions[i].dot(&(stiffness * self.directions[j]))
        })
    }
}

/// The plate energy of a triangle from the half-angle sines of its edges and their rest values,
/// with its derivatives in the sines.
///
/// Edges that take no part, on the boundary or along a sharp crease, are `None`.
pub(crate) fn plate(
    matrix: &nalgebra::Matrix3<f64>,
    sines: [Option<f64>; 3],
    rest: [f64; 3],
) -> (f64, Vector3<f64>, nalgebra::Matrix3<f64>) {
    let mask = Vector3::from(sines.map(|s| if s.is_some() { 1.0 } else { 0.0 }));
    let deviation = Vector3::from([0, 1, 2].map(|i| sines[i].map_or(0.0, |s| s - rest[i])));
    let masked = matrix.component_mul(&(mask * mask.transpose()));
    let moment = masked * deviation;
    (0.5 * deviation.dot(&moment), moment, masked)
}

/// The energy of a sharp crease, with its derivatives in the dihedral angle.
///
/// The crease has rest length `length`, stiffness `stiffness` per length per radian, and rest
/// angle `rest`.
pub(crate) fn crease(angle: f64, length: f64, stiffness: f64, rest: f64) -> [f64; 3] {
    let k = stiffness * length;
    let turn = angle - rest;
    [0.5 * k * turn * turn, k * turn, k]
}

/// The bending energy of one triangle over its stencil of up to six vertices: its corners, then
/// the vertex across the edge opposite each corner.
///
/// Edges that take no part, on the boundary or along a sharp crease, are `None`. The Hessian is
/// the Gauss-Newton part, positive semi-definite; the derivatives of the energy in each edge's
/// sine, the moments, come back for the rest.
pub(crate) fn triangle(
    matrix: &nalgebra::Matrix3<f64>,
    edges: [Option<BendingEdge>; 3],
    rest: [f64; 3],
) -> (Local<18>, Vector3<f64>) {
    let (_, moment, masked) = plate(matrix, edges.map(|e| e.map(|e| e.sine)), rest);
    let mut jacobian = SMatrix::<f64, 3, 18>::zeros();
    for (i, edge) in edges.iter().enumerate() {
        if let Some(edge) = edge {
            for (k, &slot) in edge.slots.iter().enumerate() {
                let rows = edge.gradient.fixed_rows::<3>(3 * k).transpose();
                let mut target = jacobian.fixed_view_mut::<1, 3>(i, 3 * slot);
                target += rows;
            }
        }
    }
    let local = Local {
        gradient: jacobian.transpose() * moment,
        hessian: jacobian.transpose() * masked * jacobian,
    };
    (local, moment)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::energy::testing::check;
    use crate::material::Material;

    fn stencil(x: &SMatrix<f64, 12, 1>) -> [Point; 4] {
        [0, 1, 2, 3].map(|k| x.fixed_rows::<3>(3 * k).into_owned())
    }

    /// An edge along x, with a wing on its left and a wing on its right lifted by `lift`.
    fn dihedral(lift: f64) -> SMatrix<f64, 12, 1> {
        let turn = nalgebra::Rotation3::from_euler_angles(0.4, 0.2, -0.9);
        let points = [
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(0.01, 0.001, 0.0),
            Vector3::new(0.004, 0.009, 0.0),
            Vector3::new(0.006, -0.008, lift),
        ];
        SMatrix::from_iterator(
            points
                .iter()
                .flat_map(|p| (turn * p).iter().copied().collect::<Vec<_>>()),
        )
    }

    #[test]
    fn the_angle_is_positive_toward_the_front() {
        assert!(angle_value(&stencil(&dihedral(0.0))).abs() < 1e-12);
        let up = angle_value(&stencil(&dihedral(0.004)));
        assert!(up > 0.0);
        // Folding the right wing over onto the left gives nearly a half turn.
        assert!(angle_value(&stencil(&dihedral(-0.004))) < 0.0);
        let generated = angle(&stencil(&dihedral(0.004)));
        assert!((generated.value - up).abs() < 1e-14);
    }

    #[test]
    fn the_angle_derivatives_match_finite_differences() {
        for lift in [0.0, 0.003, -0.007, 0.03] {
            let x = dihedral(lift);
            let exact = angle(&stencil(&x));
            check(
                x,
                |x| angle_value(&stencil(x)),
                |x| angle(&stencil(x)).gradient,
                &exact.hessian,
                1e-8,
                1e-5,
            );
        }
    }

    #[test]
    fn the_half_sine_derivatives_match_finite_differences() {
        let x = dihedral(0.006);
        let sine = half_sine(&angle(&stencil(&x)));
        check(
            x,
            |x| (0.5 * angle_value(&stencil(x))).sin(),
            |x| half_sine(&angle(&stencil(x))).gradient,
            &sine.hessian,
            1e-8,
            1e-5,
        );
    }

    #[test]
    fn a_cylinder_through_the_vertices_has_its_exact_curvature() {
        // A grid cell rolls round the y axis. The dihedral at its vertical edges is the angle
        // the cell's chord subtends, and the triangle's curvature is exactly the cylinder's.
        let (dx, dy, radius): (f64, f64, f64) = (0.002, 0.003, 0.05);
        let chord = 2.0 * (dx / (2.0 * radius)).asin();
        let plate = RestPlate::new([[0.0, 0.0], [dx, 0.0], [dx, dy]]);
        let stiffness = Material::kami().bending();
        let matrix = plate.energy_matrix(&stiffness);
        // In the lower-right triangle of a cell, only the vertical edge, opposite corner 0, bends.
        let (energy, _, _) = plate_energy(&matrix, [Some((0.5 * chord).sin()), None, None]);
        let curvature = Vector3::new(1.0 / radius, 0.0, 0.0);
        let exact = 0.5 * plate.area * curvature.dot(&(stiffness * curvature));
        assert!(
            (energy / exact - 1.0).abs() < 1e-12,
            "{energy} against {exact}"
        );
    }

    fn plate_energy(
        matrix: &nalgebra::Matrix3<f64>,
        sines: [Option<f64>; 3],
    ) -> (f64, Vector3<f64>, nalgebra::Matrix3<f64>) {
        plate(matrix, sines, [0.0; 3])
    }

    #[test]
    fn a_sharp_crease_rests_at_its_angle() {
        let [energy, slope, curvature] = crease(0.5, 0.01, 2.0, 0.5);
        assert!(energy.abs() < 1e-15 && slope.abs() < 1e-15);
        assert!((curvature - 0.02).abs() < 1e-15);
    }
}
