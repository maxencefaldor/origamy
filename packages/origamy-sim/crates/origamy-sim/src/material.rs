//! What a sheet is made of: its thickness, weight and stiffness, all inputs in SI units.

use nalgebra::{Matrix2, Matrix3, Vector3};

use crate::parameter::{ParameterError, finite, fraction, positive, require};

/// The material of a sheet.
///
/// Every field is an input. [`Material::kami`] fills them for the thin paper most origami uses,
/// and is the default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// The sheet's thickness, in meters.
    pub thickness: f64,
    /// The sheet's mass per area, in kilograms per square meter.
    pub mass_per_area: f64,
    /// The stiffness of the sheet in its plane.
    pub stiffness: Stiffness,
    /// The barrier that caps the sheet's strain.
    pub strain_limit: StrainLimit,
}

/// The stiffness of a sheet in its plane, orthotropic about its grain, under plane stress.
///
/// The same stiffness resists stretching, through the thickness, and bending, through the
/// thickness cubed over twelve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stiffness {
    /// Young's modulus along the grain, in pascals.
    pub along: f64,
    /// Young's modulus across the grain, in pascals.
    pub across: f64,
    /// Poisson's ratio: how much a stretch along the grain narrows the sheet across it.
    pub poisson: f64,
    /// The shear modulus in the plane, in pascals.
    pub shear: f64,
    /// The grain's direction in the flat sheet, in radians from the x axis toward the y axis.
    pub grain: f64,
}

/// The barrier that caps a sheet's strain.
///
/// Paper barely stretches, so the barrier keeps every triangle's Green strain below a cap. The
/// barrier pushes back harder the closer the strain comes to the cap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrainLimit {
    /// The greatest norm the Green strain of any triangle may reach: 0.005 caps strain at half
    /// a percent.
    pub cap: f64,
    /// The fraction of the cap at which the barrier starts to push back, between 0 and 1.
    pub onset: f64,
    /// The barrier's energy per volume, in pascals: the larger, the harder it pushes back.
    pub stiffness: f64,
}

impl Material {
    /// Kami, the thin paper most origami uses.
    ///
    /// Kami is 70 µm thick and weighs 60 g/m². Its Young's modulus is 4 gigapascals and its
    /// Poisson's ratio 0.3, with strain capped at half a percent.
    #[must_use]
    pub fn kami() -> Self {
        Self {
            thickness: 70e-6,
            mass_per_area: 0.060,
            stiffness: Stiffness::isotropic(4e9, 0.3),
            strain_limit: StrainLimit {
                cap: 0.005,
                onset: 0.8,
                // Young's modulus times the squared cap: the elastic energy density at the cap.
                stiffness: 1e5,
            },
        }
    }

    /// The bending stiffness of the sheet: its in-plane stiffness times the thickness cubed over
    /// twelve, in newton meters.
    #[must_use]
    pub fn bending(&self) -> Matrix3<f64> {
        self.stiffness.matrix() * (self.thickness.powi(3) / 12.0)
    }

    /// Checks that every parameter takes a value it may.
    ///
    /// # Errors
    ///
    /// Returns the first parameter outside its values.
    pub fn check(&self) -> Result<(), ParameterError> {
        positive("thickness", self.thickness)?;
        positive("mass per area", self.mass_per_area)?;
        self.stiffness.check()?;
        self.strain_limit.check()
    }
}

impl Default for Material {
    /// Kami.
    fn default() -> Self {
        Self::kami()
    }
}

impl StrainLimit {
    /// Checks that every parameter takes a value it may.
    ///
    /// # Errors
    ///
    /// Returns the first parameter outside its values.
    pub fn check(&self) -> Result<(), ParameterError> {
        positive("strain cap", self.cap)?;
        fraction("strain onset", self.onset)?;
        positive("strain stiffness", self.stiffness)
    }
}

impl Stiffness {
    /// The stiffness of a material alike in every direction.
    #[must_use]
    pub fn isotropic(young: f64, poisson: f64) -> Self {
        Self {
            along: young,
            across: young,
            poisson,
            shear: young / (2.0 * (1.0 + poisson)),
            grain: 0.0,
        }
    }

    /// The stiffness as a matrix in the axes of the flat sheet, in pascals.
    ///
    /// The matrix maps the strain (εxx, εyy, γxy), with γxy twice the shear strain, to the
    /// stress (σxx, σyy, σxy).
    #[must_use]
    pub fn matrix(&self) -> Matrix3<f64> {
        let across_poisson = self.poisson * self.across / self.along;
        let scale = 1.0 / (1.0 - self.poisson * across_poisson);
        let grain = Matrix3::new(
            self.along * scale,
            self.poisson * self.across * scale,
            0.0,
            self.poisson * self.across * scale,
            self.across * scale,
            0.0,
            0.0,
            0.0,
            self.shear,
        );
        // Each column of `turn` is a unit strain of the sheet, written in the grain's axes.
        let (sin, cos) = self.grain.sin_cos();
        let axes = Matrix2::new(cos, -sin, sin, cos);
        let in_grain = |strain: Matrix2<f64>| {
            let turned = axes.transpose() * strain * axes;
            Vector3::new(turned[(0, 0)], turned[(1, 1)], 2.0 * turned[(0, 1)])
        };
        let turn = Matrix3::from_columns(&[
            in_grain(Matrix2::new(1.0, 0.0, 0.0, 0.0)),
            in_grain(Matrix2::new(0.0, 0.0, 0.0, 1.0)),
            in_grain(Matrix2::new(0.0, 0.5, 0.5, 0.0)),
        ]);
        turn.transpose() * grain * turn
    }

    /// Checks that the stiffness is positive definite.
    ///
    /// # Errors
    ///
    /// Returns the first parameter outside its values.
    pub fn check(&self) -> Result<(), ParameterError> {
        positive("Young's modulus along the grain", self.along)?;
        positive("Young's modulus across the grain", self.across)?;
        positive("shear modulus", self.shear)?;
        finite("grain direction", self.grain)?;
        require(
            "Poisson's ratio",
            self.poisson,
            self.poisson.abs() < (self.along / self.across).sqrt(),
            "smaller in size than the square root of the ratio of Young's moduli",
        )
    }
}

impl Default for Stiffness {
    /// Kami's: isotropic, with a Young's modulus of 4 gigapascals and a Poisson's ratio of 0.3.
    fn default() -> Self {
        Material::kami().stiffness
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kami_bends_with_the_stiffness_of_paper() {
        let kami = Material::kami();
        kami.check().unwrap();
        // D = E h³ / 12 (1 − ν²), 1.26e-4 N·m for kami.
        let bending = kami.bending();
        let expected = 4e9 * 70e-6_f64.powi(3) / (12.0 * (1.0 - 0.09));
        assert!((bending[(0, 0)] / expected - 1.0).abs() < 1e-12);
        assert!((bending[(0, 1)] / (0.3 * expected) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn an_isotropic_stiffness_ignores_the_grain() {
        let mut stiffness = Stiffness::isotropic(4e9, 0.3);
        let flat = stiffness.matrix();
        stiffness.grain = 0.7;
        assert!((stiffness.matrix() - flat).abs().max() < 1e-3);
    }

    #[test]
    fn turning_the_grain_a_quarter_swaps_along_and_across() {
        let stiffness = Stiffness {
            along: 6e9,
            across: 3e9,
            poisson: 0.2,
            shear: 1.5e9,
            grain: std::f64::consts::FRAC_PI_2,
        };
        let turned = stiffness.matrix();
        let straight = Stiffness {
            grain: 0.0,
            ..stiffness
        }
        .matrix();
        assert!((turned[(0, 0)] - straight[(1, 1)]).abs() < 1e-3);
        assert!((turned[(1, 1)] - straight[(0, 0)]).abs() < 1e-3);
        assert!((turned[(2, 2)] - straight[(2, 2)]).abs() < 1e-3);
    }

    #[test]
    fn a_stiffness_must_be_positive_definite() {
        let mut stiffness = Stiffness::isotropic(4e9, 0.3);
        stiffness.poisson = 1.2;
        assert_eq!(stiffness.check().unwrap_err().name, "Poisson's ratio");
        let mut material = Material::kami();
        material.thickness = -1.0;
        assert_eq!(
            material.check().unwrap_err().to_string(),
            "thickness is -1, but it must be positive and finite"
        );
    }
}
