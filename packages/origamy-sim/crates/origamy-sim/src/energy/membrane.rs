//! Stretching: the membrane energy of each triangle, and the barrier that caps its strain.
//!
//! The membrane is Saint Venant-Kirchhoff: the energy is quadratic in the Green strain, through
//! the sheet's stiffness in its plane. The barrier acts on the norm of the Green strain, which
//! bounds every principal strain and stays smooth where principal stretches coincide.

use nalgebra::{Matrix3, SMatrix, Vector2, Vector3};

use super::{Barrier, Local, Point};

/// What a triangle keeps of its rest shape.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RestTriangle {
    /// The triangle's area in the flat sheet, in square meters.
    pub area: f64,
    /// The gradient, in the flat sheet, of each corner's linear shape function, in inverse
    /// meters.
    pub shape: [Vector2<f64>; 3],
}

impl RestTriangle {
    /// The rest shape of a counterclockwise triangle with these corners in the flat sheet.
    pub fn new([a, b, c]: [[f64; 2]; 3]) -> Self {
        let sides = nalgebra::Matrix2::new(b[0] - a[0], c[0] - a[0], b[1] - a[1], c[1] - a[1]);
        let area = 0.5 * sides.determinant();
        let inverse = sides
            .try_inverse()
            .expect("a checked mesh has triangles of positive area");
        let first = inverse.row(0).transpose();
        let second = inverse.row(1).transpose();
        Self {
            area,
            shape: [-first - second, first, second],
        }
    }
}

/// The parameters of stretching, alike for every triangle of a sheet.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Stretching {
    /// The stiffness in the plane times the thickness, in newtons per meter.
    pub stiffness: Matrix3<f64>,
    /// The greatest norm of the Green strain.
    pub cap: f64,
    /// The barrier, in the fraction of the squared cap left before the cap.
    pub barrier: Barrier,
    /// The barrier's energy per area, in joules per square meter.
    pub barrier_stiffness: f64,
}

/// The Green strain of a triangle in the voigt form (εxx, εyy, γxy), and its edges' images.
fn strain(rest: &RestTriangle, x: &[Point; 3]) -> (Vector3<f64>, [Point; 2]) {
    let column =
        |k: usize| x[0] * rest.shape[0][k] + x[1] * rest.shape[1][k] + x[2] * rest.shape[2][k];
    let f = [column(0), column(1)];
    let strain = Vector3::new(
        0.5 * (f[0].dot(&f[0]) - 1.0),
        0.5 * (f[1].dot(&f[1]) - 1.0),
        f[0].dot(&f[1]),
    );
    (strain, f)
}

/// The squared norm of a Green strain in voigt form.
fn squared_norm(strain: &Vector3<f64>) -> f64 {
    strain[0] * strain[0] + strain[1] * strain[1] + 0.5 * strain[2] * strain[2]
}

/// The norm of a triangle's Green strain.
pub(crate) fn strain_norm(rest: &RestTriangle, x: &[Point; 3]) -> f64 {
    squared_norm(&strain(rest, x).0).sqrt()
}

/// The stretching energy of a triangle, with its barrier, or `None` past the strain cap.
pub(crate) fn energy(rest: &RestTriangle, x: &[Point; 3], stretching: &Stretching) -> Option<f64> {
    let (strain, _) = strain(rest, x);
    let d = 1.0 - squared_norm(&strain) / (stretching.cap * stretching.cap);
    let [barrier, _, _] = stretching.barrier.at(d)?;
    let elastic = 0.5 * strain.dot(&(stretching.stiffness * strain));
    Some(rest.area * (elastic + stretching.barrier_stiffness * barrier))
}

/// The elastic stretching energy of a triangle, without its barrier.
pub(crate) fn elastic_energy(rest: &RestTriangle, x: &[Point; 3], stretching: &Stretching) -> f64 {
    let (strain, _) = strain(rest, x);
    0.5 * rest.area * strain.dot(&(stretching.stiffness * strain))
}

/// The gradient and exact Hessian of a triangle's stretching energy, with its barrier, or `None`
/// past the strain cap.
pub(crate) fn local(
    rest: &RestTriangle,
    x: &[Point; 3],
    stretching: &Stretching,
) -> Option<Local<9>> {
    let (strain, f) = strain(rest, x);
    let cap2 = stretching.cap * stretching.cap;
    let s = squared_norm(&strain);
    let [_, slope, curvature] = stretching.barrier.at(1.0 - s / cap2)?;
    let g = &rest.shape;
    // The rows of `jacobian` are the gradients of the three strain components.
    let mut jacobian = SMatrix::<f64, 3, 9>::zeros();
    for (a, ga) in g.iter().enumerate() {
        let rows = [f[0] * ga[0], f[1] * ga[1], f[1] * ga[0] + f[0] * ga[1]];
        for (k, row) in rows.iter().enumerate() {
            jacobian
                .fixed_view_mut::<1, 3>(k, 3 * a)
                .copy_from(&row.transpose());
        }
    }
    let stress = stretching.stiffness * strain;
    let weights = Vector3::new(2.0 * strain[0], 2.0 * strain[1], strain[2]);
    let s_gradient = jacobian.transpose() * weights;
    let kappa = stretching.barrier_stiffness;
    // The barrier's pull on the squared strain norm, positive when the barrier is active.
    let pull = -kappa * slope / cap2;
    let gradient = (jacobian.transpose() * stress + s_gradient * pull) * rest.area;
    // Each strain component is quadratic in the positions, with a Hessian of 3 by 3 identity
    // blocks; `second` weighs them by the stress and the barrier's pull.
    let second = stress + weights * pull;
    let mut hessian = jacobian.transpose() * stretching.stiffness * jacobian
        + jacobian.transpose()
            * Matrix3::from_diagonal(&Vector3::new(2.0, 2.0, 1.0))
            * jacobian
            * pull
        + s_gradient * s_gradient.transpose() * (kappa * curvature / (cap2 * cap2));
    for (a, ga) in g.iter().enumerate() {
        for (b, gb) in g.iter().enumerate() {
            let weight = second[0] * ga[0] * gb[0]
                + second[1] * ga[1] * gb[1]
                + second[2] * (ga[0] * gb[1] + ga[1] * gb[0]);
            for i in 0..3 {
                hessian[(3 * a + i, 3 * b + i)] += weight;
            }
        }
    }
    Some(Local {
        gradient,
        hessian: hessian * rest.area,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::energy::testing::check;
    use crate::material::Material;

    fn stretching() -> Stretching {
        let kami = Material::kami();
        Stretching {
            stiffness: kami.stiffness.matrix() * kami.thickness,
            cap: kami.strain_cap,
            barrier: Barrier {
                onset: 1.0 - kami.strain_onset * kami.strain_onset,
            },
            barrier_stiffness: kami.thickness * 4e9 * kami.strain_cap * kami.strain_cap,
        }
    }

    fn rest() -> RestTriangle {
        RestTriangle::new([[0.0, 0.0], [0.01, 0.0], [0.003, 0.008]])
    }

    /// The rest triangle, turned in space and stretched by `stretch` along x.
    fn placed(stretch: f64) -> SMatrix<f64, 9, 1> {
        let turn = nalgebra::Rotation3::from_euler_angles(0.3, -0.5, 1.1);
        let corners = [[0.0, 0.0], [0.01, 0.0], [0.003, 0.008]];
        let mut x = SMatrix::<f64, 9, 1>::zeros();
        for (a, c) in corners.iter().enumerate() {
            let p = turn * Vector3::new(c[0] * (1.0 + stretch), c[1] - 0.2 * stretch * c[0], 0.001);
            x.fixed_rows_mut::<3>(3 * a).copy_from(&p);
        }
        x
    }

    fn corners(x: &SMatrix<f64, 9, 1>) -> [Point; 3] {
        [0, 1, 2].map(|a| x.fixed_rows::<3>(3 * a).into_owned())
    }

    #[test]
    fn a_turned_triangle_at_rest_has_no_energy() {
        let local = local(&rest(), &corners(&placed(0.0)), &stretching()).unwrap();
        assert!(
            energy(&rest(), &corners(&placed(0.0)), &stretching())
                .unwrap()
                .abs()
                < 1e-20
        );
        assert!(local.gradient.abs().max() < 1e-12);
    }

    #[test]
    fn derivatives_match_finite_differences() {
        let rest = rest();
        let stretching = stretching();
        // Below the barrier's onset, and inside it.
        for stretch in [0.002, 0.0044] {
            let x = placed(stretch);
            let local = local(&rest, &corners(&x), &stretching).unwrap();
            check(
                x,
                |x| energy(&rest, &corners(x), &stretching).unwrap(),
                |x| {
                    self::local(&rest, &corners(x), &stretching)
                        .unwrap()
                        .gradient
                },
                &local.hessian,
                1e-9,
                1e-5,
            );
        }
    }

    #[test]
    fn strain_past_the_cap_has_no_energy() {
        let x = corners(&placed(0.006));
        assert!(strain_norm(&rest(), &x) > 0.005);
        assert!(energy(&rest(), &x, &stretching()).is_none());
    }
}
