//! Stretching: the membrane energy of each triangle, and the barrier that caps its tension.
//!
//! The membrane is Saint Venant-Kirchhoff: the energy is quadratic in the Green strain, through
//! the sheet's stiffness in its plane. A compressed sheet buckles into wrinkles finer than the
//! mesh, so where a triangle's stress turns compressive its energy relaxes, as tension field
//! theory has it. A wrinkled triangle carries tension along its wrinkles alone, and a slack one,
//! compressed every way, carries none. The relaxed energy is the least energy over every wrinkle
//! direction and depth. A sheet alike in every direction relaxes in Pipkin's closed form, and
//! any other through a search over wrinkle directions.
//!
//! Each triangle keeps a share of the full energy under compression, so compression still
//! resists a little. The material's wrinkling strain shifts where wrinkles start, so a sheet
//! can resist small compression in full. The barrier acts on the norm of the principal strains,
//! each compressive one weighed by the triangle's share: it caps tension, and compression as far
//! as the triangle resists it.

use nalgebra::{Matrix3, SMatrix, Vector2, Vector3};

use super::{Barrier, Local, Point};
use crate::material::Material;

/// What a triangle keeps of its rest shape.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RestTriangle {
    /// The triangle's area in the flat sheet, in square meters.
    pub area: f64,
    /// The gradient, in the flat sheet, of each corner's linear shape function, in inverse
    /// meters.
    pub shape: [Vector2<f64>; 3],
    /// The share of the full stretching energy the triangle keeps where it is compressed.
    pub compression: f64,
}

impl RestTriangle {
    /// The rest shape of a counterclockwise triangle with these corners in the flat sheet,
    /// keeping `compression` of its energy where compressed.
    pub fn new([a, b, c]: [[f64; 2]; 3], compression: f64) -> Self {
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
            compression,
        }
    }
}

/// The parameters of stretching, alike for every triangle of a sheet.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Stretching {
    /// The stiffness in the plane times the thickness, in newtons per meter.
    pub stiffness: Matrix3<f64>,
    /// Young's modulus times the thickness, in newtons per meter, and Poisson's ratio, where
    /// the stiffness is alike in every direction.
    pub isotropic: Option<(f64, f64)>,
    /// The compression a sheet squeezed alike every way takes before it wrinkles.
    pub wrinkling: f64,
    /// The greatest norm of the Green strain.
    pub cap: f64,
    /// The barrier, in the fraction of the squared cap left before the cap.
    pub barrier: Barrier,
    /// The barrier's energy per area, in joules per square meter.
    pub barrier_stiffness: f64,
}

impl Stretching {
    /// The parameters of stretching a sheet of this material.
    pub fn of(material: &Material) -> Self {
        let limit = material.strain_limit;
        Self {
            stiffness: material.stiffness.matrix() * material.thickness,
            isotropic: isotropic(&material.stiffness)
                .map(|(young, poisson)| (young * material.thickness, poisson)),
            wrinkling: material.wrinkling,
            cap: limit.cap,
            barrier: Barrier {
                onset: 1.0 - limit.onset * limit.onset,
            },
            barrier_stiffness: material.thickness * limit.stiffness,
        }
    }
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

/// R − F for a triangle: its deformation gradient F's gap to the nearest rotation R.
///
/// Both matrices are 3 by 2. A triangle collapsed to a line has no nearest rotation, so it
/// gives `None`.
pub(crate) fn rotation_gap(
    rest: &RestTriangle,
    x: &[Point; 3],
) -> Option<nalgebra::Matrix3x2<f64>> {
    let column =
        |k: usize| x[0] * rest.shape[0][k] + x[1] * rest.shape[1][k] + x[2] * rest.shape[2][k];
    let f = nalgebra::Matrix3x2::from_columns(&[column(0), column(1)]);
    // R = F (FᵀF)^(-1/2), with the square root of a 2 by 2 positive definite matrix in closed
    // form: √C = (C + √det C I) / √(tr C + 2 √det C).
    let c = f.transpose() * f;
    let root_det = c.determinant().max(0.0).sqrt();
    let scale = (c.trace() + 2.0 * root_det).sqrt();
    if root_det <= f64::EPSILON * c.trace() || scale <= 0.0 {
        return None;
    }
    let root = (c + nalgebra::Matrix2::identity() * root_det) / scale;
    Some(f * root.try_inverse()? - f)
}

/// A function of the Green strain, per area, with its gradient and Hessian in the strain.
#[derive(Clone, Copy, Debug)]
struct Response {
    value: f64,
    gradient: Vector3<f64>,
    hessian: Matrix3<f64>,
}

impl Response {
    const ZERO: Self = Self {
        value: 0.0,
        gradient: Vector3::new(0.0, 0.0, 0.0),
        hessian: Matrix3::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
    };

    /// `self` times `a` plus `other` times `b`.
    fn blend(&self, a: f64, other: &Self, b: f64) -> Self {
        Self {
            value: self.value * a + other.value * b,
            gradient: self.gradient * a + other.gradient * b,
            hessian: self.hessian * a + other.hessian * b,
        }
    }
}

/// The gradient of a quantity in the strain, and its Hessian.
type Derivatives = (Vector3<f64>, Matrix3<f64>);

/// The principal strains of a strain in voigt form, greater first, and the derivatives of the
/// greater, which need the two apart.
fn principal(strain: &Vector3<f64>) -> ([f64; 2], Option<Derivatives>) {
    let mean = 0.5 * (strain[0] + strain[1]);
    let (half, shear) = (0.5 * (strain[0] - strain[1]), 0.5 * strain[2]);
    let radius = half.hypot(shear);
    let values = [mean + radius, mean - radius];
    if radius <= 0.0 {
        return (values, None);
    }
    // The radius is the norm of (half, shear) = P strain.
    let p = SMatrix::<f64, 2, 3>::new(0.5, -0.5, 0.0, 0.0, 0.0, 0.5);
    let unit = Vector2::new(half, shear) / radius;
    let gradient = Vector3::new(0.5, 0.5, 0.0) + p.transpose() * unit;
    let hessian =
        p.transpose() * (nalgebra::Matrix2::identity() - unit * unit.transpose()) * p / radius;
    (values, Some((gradient, hessian)))
}

/// Young's modulus and Poisson's ratio of a stiffness alike in every direction, if it is.
fn isotropic(stiffness: &crate::material::Stiffness) -> Option<(f64, f64)> {
    let young = stiffness.along;
    let shear = young / (2.0 * (1.0 + stiffness.poisson));
    let alike = |a: f64, b: f64| (a - b).abs() <= 1e-12 * a.abs().max(b.abs());
    (alike(stiffness.across, young) && alike(stiffness.shear, shear))
        .then_some((young, stiffness.poisson))
}

/// The energy wrinkles release from a strain of a sheet alike in every direction, after
/// Pipkin: the full energy less the relaxed one, half Young's modulus times the greater principal
/// strain squared where it pulls, plus the plane-stress modulus over two times the lesser
/// principal stress's strain squared where that pulls.
fn pipkin(strain: &Vector3<f64>, young: f64, poisson: f64) -> Response {
    let ([greater, lesser], derivatives) = principal(strain);
    let mean = Vector3::new(0.5, 0.5, 0.0);
    // The lesser principal stress over the plane-stress modulus, λ₂ + ν λ₁.
    let lesser_stress = lesser + poisson * greater;
    let (a, b) = ((-greater).max(0.0), (-lesser_stress).max(0.0));
    let (bar, plate) = (0.5 * young, 0.5 * young / (1.0 - poisson * poisson));
    let value = bar * a * a + plate * b * b;
    let (grad_greater, curve) = derivatives.unwrap_or((mean, Matrix3::zeros()));
    // λ₂ + ν λ₁ = (1 + ν) m − (1 − ν) r, with λ₁ = m + r.
    let grad_lesser = mean * (1.0 + poisson) - (grad_greater - mean) * (1.0 - poisson);
    let step = |x: f64| if x > 0.0 { 1.0 } else { 0.0 };
    Response {
        value,
        gradient: grad_greater * (-2.0 * bar * a) + grad_lesser * (-2.0 * plate * b),
        hessian: grad_greater * grad_greater.transpose() * (2.0 * bar * step(a))
            + grad_lesser * grad_lesser.transpose() * (2.0 * plate * step(b))
            + curve * (-2.0 * bar * a + 2.0 * plate * b * (1.0 - poisson)),
    }
}

/// The energy wrinkles release from a strain of any sheet: none where it is taut, the whole
/// energy where it is slack, and the energy of the best wrinkle where it wrinkles.
fn release_any(strain: &Vector3<f64>, stiffness: &Matrix3<f64>) -> Response {
    let stress = stiffness * strain;
    let ([greater, _], _) = principal(strain);
    if greater <= 0.0 {
        return Response {
            value: 0.5 * strain.dot(&stress),
            gradient: stress,
            hessian: *stiffness,
        };
    }
    if stress[0] + stress[1] >= 0.0 && stress[0] * stress[1] >= stress[2] * stress[2] {
        return Response::ZERO;
    }
    // The release is the most over every wrinkle, so its gradient is the envelope's, and its
    // Hessian adds how the best angle turns with the strain.
    let r = release(strain, stiffness, wrinkle_angle(strain, stiffness));
    let turn = if r.angle2 < 0.0 {
        r.mixed * r.mixed.transpose() / r.angle2
    } else {
        Matrix3::zeros()
    };
    Response {
        value: r.value,
        gradient: r.strain,
        hessian: r.strain2 - turn,
    }
}

/// The squared norm the barrier caps: the sum of the squared principal strains, each
/// compressive one weighed by the `share` of the energy the triangle keeps under compression.
///
/// A sheet that keeps its full energy caps its whole strain, and a wrinkling one caps its
/// tension and lets compression run past the cap by as much as it relaxes.
fn limited(strain: &Vector3<f64>, share: f64) -> Response {
    let ([greater, lesser], derivatives) = principal(strain);
    let full = |weight: f64| Response {
        value: weight
            * (strain[0] * strain[0] + strain[1] * strain[1] + 0.5 * strain[2] * strain[2]),
        gradient: Vector3::new(2.0 * strain[0], 2.0 * strain[1], strain[2]) * weight,
        hessian: Matrix3::from_diagonal(&Vector3::new(2.0, 2.0, 1.0)) * weight,
    };
    if lesser >= 0.0 {
        return full(1.0);
    }
    match derivatives {
        Some((grad_greater, curve)) if greater > 0.0 => {
            // λ₁ = m + r pulls and λ₂ = m − r pushes.
            let mean = Vector3::new(0.5, 0.5, 0.0);
            let grad_lesser = mean * 2.0 - grad_greater;
            Response {
                value: greater * greater + share * lesser * lesser,
                gradient: grad_greater * (2.0 * greater) + grad_lesser * (2.0 * share * lesser),
                hessian: grad_greater * grad_greater.transpose() * 2.0
                    + grad_lesser * grad_lesser.transpose() * (2.0 * share)
                    + curve * (2.0 * greater - 2.0 * share * lesser),
            }
        }
        _ => full(share),
    }
}

/// The voigt form of n ⊗ n for the unit vector n at `angle`, and its first and second
/// derivatives in the angle.
fn direction(angle: f64) -> [Vector3<f64>; 3] {
    let (sin, cos) = (2.0 * angle).sin_cos();
    [
        Vector3::new(0.5 * (1.0 + cos), 0.5 * (1.0 - cos), sin),
        Vector3::new(-sin, sin, 2.0 * cos),
        Vector3::new(-2.0 * cos, 2.0 * cos, -4.0 * sin),
    ]
}

/// The energy a wrinkle at `angle` releases, f = σ² / 2k, for the normal stress σ and normal
/// stiffness k along it, with f's derivatives: ∂f/∂θ, ∂²f/∂θ², ∂f/∂ε, ∂²f/∂ε∂θ and ∂²f/∂ε².
struct Release {
    value: f64,
    angle: f64,
    angle2: f64,
    strain: Vector3<f64>,
    mixed: Vector3<f64>,
    strain2: Matrix3<f64>,
    normal: f64,
}

fn release(strain: &Vector3<f64>, stiffness: &Matrix3<f64>, angle: f64) -> Release {
    let [n, n1, n2] = direction(angle);
    let (a, a1, a2) = (stiffness * n, stiffness * n1, stiffness * n2);
    let (s, s1, s2) = (a.dot(strain), a1.dot(strain), a2.dot(strain));
    let k = n.dot(&a);
    let k1 = 2.0 * n1.dot(&a);
    let k2 = 2.0 * (n2.dot(&a) + n1.dot(&a1));
    Release {
        value: s * s / (2.0 * k),
        angle: s * s1 / k - s * s * k1 / (2.0 * k * k),
        angle2: (s1 * s1 + s * s2) / k - 2.0 * s * s1 * k1 / (k * k) - s * s * k2 / (2.0 * k * k)
            + s * s * k1 * k1 / (k * k * k),
        strain: a * (s / k),
        mixed: a * (s1 / k - s * k1 / (k * k)) + a1 * (s / k),
        strain2: a * a.transpose() / k,
        normal: s,
    }
}

/// The direction a strain's wrinkles run across: the one whose compressive stress releases the
/// most energy. A coarse search over directions starts Newton's method on the angle.
fn wrinkle_angle(strain: &Vector3<f64>, stiffness: &Matrix3<f64>) -> f64 {
    const DIRECTIONS: u32 = 36;
    let mut best = (f64::NEG_INFINITY, 0.0);
    for k in 0..DIRECTIONS {
        let angle = std::f64::consts::PI * f64::from(k) / f64::from(DIRECTIONS);
        let r = release(strain, stiffness, angle);
        if r.normal < 0.0 && r.value > best.0 {
            best = (r.value, angle);
        }
    }
    let mut angle = best.1;
    let reach = std::f64::consts::PI / f64::from(DIRECTIONS);
    for _ in 0..50 {
        let r = release(strain, stiffness, angle);
        if r.angle2 >= 0.0 {
            break;
        }
        let step = (-r.angle / r.angle2).clamp(-reach, reach);
        angle += step;
        if step.abs() <= 1e-15 {
            break;
        }
    }
    angle
}

/// The membrane energy of a strain, per area, relaxed where it compresses past the sheet's
/// wrinkling strain, keeping `share` of the full energy there.
///
/// The relaxed energy is the full energy less what wrinkles release from the strain shifted
/// by the wrinkling strain. Up to that compression the sheet resists in full, as paper does
/// before it buckles, and past it the compressive stress stays at the buckling stress.
fn response(strain: &Vector3<f64>, stretching: &Stretching, share: f64) -> Response {
    let stiffness = &stretching.stiffness;
    let stress = stiffness * strain;
    let full = Response {
        value: 0.5 * strain.dot(&stress),
        gradient: stress,
        hessian: *stiffness,
    };
    if share >= 1.0 {
        return full;
    }
    let shifted = strain + Vector3::new(stretching.wrinkling, stretching.wrinkling, 0.0);
    let released = match stretching.isotropic {
        Some((young, poisson)) => pipkin(&shifted, young, poisson),
        None => release_any(&shifted, stiffness),
    };
    full.blend(1.0, &released, share - 1.0)
}

/// The norm of a triangle's Green strain that the barrier caps.
pub(crate) fn strain_norm(rest: &RestTriangle, x: &[Point; 3]) -> f64 {
    limited(&strain(rest, x).0, rest.compression).value.sqrt()
}

/// The stretching energy of a triangle, with its barrier, or `None` past the strain cap.
pub(crate) fn energy(rest: &RestTriangle, x: &[Point; 3], stretching: &Stretching) -> Option<f64> {
    let (strain, _) = strain(rest, x);
    let d = 1.0 - limited(&strain, rest.compression).value / (stretching.cap * stretching.cap);
    let [barrier, _, _] = stretching.barrier.at(d)?;
    let elastic = response(&strain, stretching, rest.compression).value;
    Some(rest.area * (elastic + stretching.barrier_stiffness * barrier))
}

/// The elastic stretching energy of a triangle, without its barrier.
pub(crate) fn elastic_energy(rest: &RestTriangle, x: &[Point; 3], stretching: &Stretching) -> f64 {
    let (strain, _) = strain(rest, x);
    rest.area * response(&strain, stretching, rest.compression).value
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
    let tension = limited(&strain, rest.compression);
    let [_, slope, curvature] = stretching.barrier.at(1.0 - tension.value / cap2)?;
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
    let elastic = response(&strain, stretching, rest.compression);
    let kappa = stretching.barrier_stiffness;
    // The barrier's pull on the squared norm it caps, positive when the barrier is active.
    let pull = -kappa * slope / cap2;
    // The energy's gradient and Hessian in the strain, membrane and barrier together.
    let second = elastic.gradient + tension.gradient * pull;
    let tangent = elastic.hessian
        + tension.hessian * pull
        + tension.gradient * tension.gradient.transpose() * (kappa * curvature / (cap2 * cap2));
    let gradient = jacobian.transpose() * second * rest.area;
    // Each strain component is quadratic in the positions, with a Hessian of 3 by 3 identity
    // blocks, which `second` weighs.
    let mut hessian = jacobian.transpose() * tangent * jacobian;
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

    fn stretching() -> Stretching {
        Stretching::of(&Material::kami())
    }

    fn rest() -> RestTriangle {
        kept(1.0)
    }

    /// The rest triangle, keeping `compression` of its energy where compressed.
    fn kept(compression: f64) -> RestTriangle {
        RestTriangle::new([[0.0, 0.0], [0.01, 0.0], [0.003, 0.008]], compression)
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
    fn a_turned_triangle_is_its_own_nearest_rotation() {
        let x = corners(&placed(0.0));
        assert!(rotation_gap(&rest(), &x).unwrap().abs().max() < 1e-12);
        // Stretched, its nearest rotation keeps the turn and drops the stretch.
        let stretched = corners(&placed(0.01));
        let gap = rotation_gap(&rest(), &stretched).unwrap();
        assert!(gap.abs().max() > 1e-3);
        let column = |k: usize| {
            stretched[0] * rest().shape[0][k]
                + stretched[1] * rest().shape[1][k]
                + stretched[2] * rest().shape[2][k]
        };
        let rotation = nalgebra::Matrix3x2::from_columns(&[column(0), column(1)]) + gap;
        assert!(
            (rotation.transpose() * rotation - nalgebra::Matrix2::identity())
                .abs()
                .max()
                < 1e-12
        );
    }

    /// The rest triangle deformed by `deformation` in the plane, then turned in space.
    fn deformed(deformation: [[f64; 2]; 2]) -> SMatrix<f64, 9, 1> {
        let turn = nalgebra::Rotation3::from_euler_angles(0.3, -0.5, 1.1);
        let corners = [[0.0, 0.0], [0.01, 0.0], [0.003, 0.008]];
        let mut x = SMatrix::<f64, 9, 1>::zeros();
        for (a, c) in corners.iter().enumerate() {
            let [[xx, xy], [yx, yy]] = deformation;
            let p = Vector3::new(
                c[0] + xx * c[0] + xy * c[1],
                c[1] + yx * c[0] + yy * c[1],
                0.001,
            );
            x.fixed_rows_mut::<3>(3 * a).copy_from(&(turn * p));
        }
        x
    }

    /// Stretching for kami, orthotropic with its grain turned.
    fn orthotropic() -> Stretching {
        let mut material = Material::kami();
        material.stiffness.across = 2.5e9;
        material.stiffness.grain = 0.4;
        Stretching::of(&material)
    }

    #[test]
    fn derivatives_match_finite_differences_where_the_sheet_wrinkles() {
        // Taut, wrinkled each way, and slack, with the barrier active in some: on tension, and
        // on compression where the triangle keeps a twentieth of its energy.
        let deformations = [
            [[0.002, 0.0005], [0.0, 0.001]],
            [[0.003, 0.001], [0.0, -0.01]],
            [[-0.02, 0.0], [0.002, 0.0042]],
            [[-0.01, 0.002], [0.0, -0.004]],
        ];
        let cases = [
            (stretching(), kept(0.01)),
            (orthotropic(), kept(0.01)),
            (orthotropic(), kept(0.05)),
        ];
        let mut checked = 0;
        for (stretching, rest) in cases {
            for deformation in deformations {
                let x = deformed(deformation);
                // A twentieth of the energy caps compression past the cap's reach in one.
                let Some(local) = local(&rest, &corners(&x), &stretching) else {
                    continue;
                };
                checked += 1;
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
        assert_eq!(checked, 11);
    }

    #[test]
    fn a_wrinkled_sheet_carries_tension_along_its_wrinkles_alone() {
        // Stretched along x, squeezed hard along y: the isotropic sheet wrinkles across y,
        // and its relaxed energy is a bar's, half Young's modulus times the squared strain.
        let mut material = Material::kami();
        material.wrinkling = 0.0;
        let stretching = Stretching::of(&material);
        let x = corners(&deformed([[0.002, 0.0], [0.0, -0.01]]));
        let along = 0.5 * (1.002f64.powi(2) - 1.0);
        let bar = 0.5 * 4e9 * Material::kami().thickness * along * along * rest().area;
        let relaxed = elastic_energy(&kept(1e-9), &x, &stretching);
        assert!(
            (relaxed - bar).abs() < 1e-6 * bar,
            "{relaxed} is no bar's {bar}"
        );
        // Compressed every way, it is slack, and keeps only its share of the full energy.
        let squeezed = corners(&deformed([[-0.01, 0.0], [0.0, -0.01]]));
        let full = elastic_energy(&kept(1.0), &squeezed, &stretching);
        let share = elastic_energy(&kept(0.01), &squeezed, &stretching);
        assert!((share - 0.01 * full).abs() < 1e-9 * full);
        // The barrier caps tension alone, so a squeeze past the cap still has an energy.
        assert!(energy(&kept(0.01), &squeezed, &stretching).is_some());
    }

    #[test]
    fn a_sheet_resists_compression_in_full_until_it_wrinkles() {
        let mut material = Material::kami();
        material.wrinkling = 1e-4;
        let stretching = Stretching::of(&material);
        let squeeze = |strain: f64| {
            let stretch = (1.0 + 2.0 * strain).sqrt() - 1.0;
            corners(&deformed([[stretch, 0.0], [0.0, stretch]]))
        };
        // Squeezed alike every way, a little: no wrinkles, so the full energy.
        let little = squeeze(-0.5e-4);
        let full = elastic_energy(&kept(1.0), &little, &stretching);
        let kept_energy = elastic_energy(&kept(1e-3), &little, &stretching);
        assert!((kept_energy - full).abs() < 1e-9 * full);
        // Past the wrinkling strain, the stress stays at the buckling stress: each further
        // strain adds the same energy.
        let [a, b, c] =
            [-0.01, -0.02, -0.03].map(|s| elastic_energy(&kept(1e-9), &squeeze(s), &stretching));
        assert!(((c - b) - (b - a)).abs() < 1e-6 * (c - b), "{a} {b} {c}");
    }

    #[test]
    fn strain_past_the_cap_has_no_energy() {
        let x = corners(&placed(0.006));
        assert!(strain_norm(&rest(), &x) > 0.005);
        assert!(energy(&rest(), &x, &stretching()).is_none());
    }
}
