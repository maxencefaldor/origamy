//! The core's parameters as the pydantic models of `origamy_sim.parameters`.
//!
//! The models validate in Python, and the core checks the values again, so a model built
//! without validation still meets the core's checks.

use origamy_sim::{Contact, Crease, Material, Settings, Solver, Stiffness, StrainLimit, Table};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// A core value that crosses into Python as its model, and back.
#[derive(Clone, Copy)]
pub struct Model<T>(pub T);

/// A core value with a model in `origamy_sim.parameters`.
pub trait Parameters: Sized {
    /// The model's class name.
    const MODEL: &'static str;

    /// Reads the value from its model's fields.
    fn read(model: &Bound<'_, PyAny>) -> PyResult<Self>;

    /// Writes the value into its model's fields.
    fn write(&self, fields: &Bound<'_, PyDict>) -> PyResult<()>;
}

/// The model class of a value.
fn class<T: Parameters>(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    py.import("origamy_sim.parameters")?.getattr(T::MODEL)
}

/// A field of a model.
fn field<'py, T: FromPyObjectOwned<'py>>(model: &Bound<'py, PyAny>, name: &str) -> PyResult<T> {
    model.getattr(name)?.extract().map_err(Into::into)
}

impl<'py, T: Parameters> FromPyObject<'_, 'py> for Model<T> {
    type Error = PyErr;

    fn extract(object: Borrowed<'_, 'py, PyAny>) -> PyResult<Self> {
        if !object.is_instance(&class::<T>(object.py())?)? {
            return Err(PyTypeError::new_err(format!(
                "'{}' object is not a {}",
                object.get_type().name()?,
                T::MODEL
            )));
        }
        T::read(&object).map(Model)
    }
}

impl<'py, T: Parameters> IntoPyObject<'py> for Model<T> {
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> PyResult<Self::Output> {
        let fields = PyDict::new(py);
        self.0.write(&fields)?;
        class::<T>(py)?.call((), Some(&fields))
    }
}

impl Parameters for Stiffness {
    const MODEL: &'static str = "Stiffness";

    fn read(model: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            along: field(model, "along")?,
            across: field(model, "across")?,
            poisson: field(model, "poisson")?,
            shear: field(model, "shear")?,
            grain: field(model, "grain")?,
        })
    }

    fn write(&self, fields: &Bound<'_, PyDict>) -> PyResult<()> {
        fields.set_item("along", self.along)?;
        fields.set_item("across", self.across)?;
        fields.set_item("poisson", self.poisson)?;
        fields.set_item("shear", self.shear)?;
        fields.set_item("grain", self.grain)
    }
}

impl Parameters for StrainLimit {
    const MODEL: &'static str = "StrainLimit";

    fn read(model: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            cap: field(model, "cap")?,
            onset: field(model, "onset")?,
            stiffness: field(model, "stiffness")?,
        })
    }

    fn write(&self, fields: &Bound<'_, PyDict>) -> PyResult<()> {
        fields.set_item("cap", self.cap)?;
        fields.set_item("onset", self.onset)?;
        fields.set_item("stiffness", self.stiffness)
    }
}

impl Parameters for Material {
    const MODEL: &'static str = "Material";

    fn read(model: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            thickness: field(model, "thickness")?,
            mass_per_area: field(model, "mass_per_area")?,
            stiffness: field::<Model<Stiffness>>(model, "stiffness")?.0,
            strain_limit: field::<Model<StrainLimit>>(model, "strain_limit")?.0,
        })
    }

    fn write(&self, fields: &Bound<'_, PyDict>) -> PyResult<()> {
        fields.set_item("thickness", self.thickness)?;
        fields.set_item("mass_per_area", self.mass_per_area)?;
        fields.set_item("stiffness", Model(self.stiffness))?;
        fields.set_item("strain_limit", Model(self.strain_limit))
    }
}

impl Parameters for Crease {
    const MODEL: &'static str = "Crease";

    fn read(model: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            stiffness: field(model, "stiffness")?,
        })
    }

    fn write(&self, fields: &Bound<'_, PyDict>) -> PyResult<()> {
        fields.set_item("stiffness", self.stiffness)
    }
}

impl Parameters for Solver {
    const MODEL: &'static str = "Solver";

    fn read(model: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            tolerance: field(model, "tolerance")?,
            iteration_limit: field(model, "iteration_limit")?,
            trust_radius: field(model, "trust_radius")?,
            sufficient_decrease: field(model, "sufficient_decrease")?,
            backtracking: field(model, "backtracking")?,
            shortest_update: field(model, "shortest_update")?,
        })
    }

    fn write(&self, fields: &Bound<'_, PyDict>) -> PyResult<()> {
        fields.set_item("tolerance", self.tolerance)?;
        fields.set_item("iteration_limit", self.iteration_limit)?;
        fields.set_item("trust_radius", self.trust_radius)?;
        fields.set_item("sufficient_decrease", self.sufficient_decrease)?;
        fields.set_item("backtracking", self.backtracking)?;
        fields.set_item("shortest_update", self.shortest_update)
    }
}

impl Parameters for Contact {
    const MODEL: &'static str = "Contact";

    fn read(model: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            gap: field(model, "gap")?,
            stiffness: field(model, "stiffness")?,
        })
    }

    fn write(&self, fields: &Bound<'_, PyDict>) -> PyResult<()> {
        fields.set_item("gap", self.gap)?;
        fields.set_item("stiffness", self.stiffness)
    }
}

impl Parameters for Table {
    const MODEL: &'static str = "Table";

    fn read(model: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            height: field(model, "height")?,
        })
    }

    fn write(&self, fields: &Bound<'_, PyDict>) -> PyResult<()> {
        fields.set_item("height", self.height)
    }
}

impl Parameters for Settings {
    const MODEL: &'static str = "Settings";

    fn read(model: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            gravity: field(model, "gravity")?,
            time_step: field(model, "time_step")?,
            solver: field::<Model<Solver>>(model, "solver")?.0,
            contact: field::<Model<Contact>>(model, "contact")?.0,
            table: field::<Option<Model<Table>>>(model, "table")?.map(|t| t.0),
        })
    }

    fn write(&self, fields: &Bound<'_, PyDict>) -> PyResult<()> {
        let [x, y, z] = self.gravity;
        fields.set_item("gravity", (x, y, z))?;
        fields.set_item("time_step", self.time_step)?;
        fields.set_item("solver", Model(self.solver))?;
        fields.set_item("contact", Model(self.contact))?;
        fields.set_item("table", self.table.map(Model))
    }
}
