//! Lengths the same on every processor.
//!
//! The C library's `hypot` rounds differently on different processors, so a simulation
//! that used it would fold differently from one machine to the next. Sums of squares and
//! square roots round the same everywhere.

/// The length of the vector `(x, y)`.
pub(crate) fn hypot(x: f64, y: f64) -> f64 {
    (x * x + y * y).sqrt()
}

/// The length of the vector `(x, y, z)`.
pub(crate) fn norm(x: f64, y: f64, z: f64) -> f64 {
    (x * x + y * y + z * z).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_are_pythagorean() {
        assert!((hypot(3.0, 4.0) - 5.0).abs() < f64::EPSILON);
        assert!((norm(2.0, 3.0, 6.0) - 7.0).abs() < f64::EPSILON);
    }
}
