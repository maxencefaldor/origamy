//! Checks of the values parameters take, each naming the parameter it refuses.

/// A parameter outside the values it may take.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[error("{name} is {value}, but it must be {requirement}")]
pub struct ParameterError {
    /// The parameter's name.
    pub name: &'static str,
    /// The value it was given.
    pub value: f64,
    /// What its value must be.
    pub requirement: &'static str,
}

/// Checks that a value meets a requirement, or names the parameter that does not.
pub(crate) fn require(
    name: &'static str,
    value: f64,
    met: bool,
    requirement: &'static str,
) -> Result<(), ParameterError> {
    if met {
        Ok(())
    } else {
        Err(ParameterError {
            name,
            value,
            requirement,
        })
    }
}

/// Checks that a parameter is positive and finite.
pub(crate) fn positive(name: &'static str, value: f64) -> Result<(), ParameterError> {
    require(
        name,
        value,
        value > 0.0 && value.is_finite(),
        "positive and finite",
    )
}

/// Checks that a parameter is finite.
pub(crate) fn finite(name: &'static str, value: f64) -> Result<(), ParameterError> {
    require(name, value, value.is_finite(), "finite")
}

/// Checks that a parameter lies strictly between 0 and 1.
pub(crate) fn fraction(name: &'static str, value: f64) -> Result<(), ParameterError> {
    require(name, value, value > 0.0 && value < 1.0, "between 0 and 1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_check_names_the_parameter_it_refuses() {
        assert!(positive("thickness", 1e-4).is_ok());
        for refused in [0.0, -1.0, f64::INFINITY, f64::NAN] {
            let error = positive("thickness", refused).unwrap_err();
            assert_eq!(error.name, "thickness");
        }
        assert!(finite("grain", -3.0).is_ok());
        assert!(finite("grain", f64::NEG_INFINITY).is_err());
        assert!(fraction("onset", 0.5).is_ok());
        for refused in [0.0, 1.0, f64::NAN] {
            assert!(fraction("onset", refused).is_err());
        }
        assert_eq!(
            positive("thickness", 0.0).unwrap_err().to_string(),
            "thickness is 0, but it must be positive and finite"
        );
    }
}
