//! Python bindings of origamy-sim, as the module `origamy_sim._core`.
//!
//! The Python package `origamy_sim` re-exports every class here, and `_core.pyi` types them.
//! The doc comments are the Python docstrings, so they follow the Google style.

#![allow(
    clippy::doc_markdown,
    reason = "the doc comments are Python docstrings, where Google style sections name exceptions bare"
)]

use numpy::ndarray::Array2;
use numpy::{
    Element, IntoPyArray, PyArray1, PyArray2, PyArrayMethods, PyUntypedArray, PyUntypedArrayMethods,
};
use origamy_sim::{
    Crease, Energy, Material, Mesh, Report, Settings, Sheet, Simulation, Stiffness, Vertex,
};
use pyo3::create_exception;
use pyo3::exceptions::{PyIndexError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

create_exception!(
    origamy_sim,
    SingularError,
    PyRuntimeError,
    "Some motion costs no energy, so a time step has no unique result."
);

/// The rest geometry of a sheet: its flat shape, as a triangle mesh.
///
/// Args:
///     rest: Each vertex's place in the flat sheet, in meters, as an array of shape (n, 2).
///     triangles: Each triangle's three vertices, counterclockwise in the flat sheet, as an
///         integer array of shape (m, 3).
///
/// Raises:
///     TypeError: When an array's values do not convert safely: floats as vertices, say.
///     ValueError: When an array has the wrong shape, or the triangles do not make a sheet.
#[pyclass(name = "Mesh", module = "origamy_sim", frozen, eq)]
#[derive(Clone, PartialEq)]
struct PyMesh(Mesh);

#[pymethods]
impl PyMesh {
    #[new]
    fn new(rest: &Bound<'_, PyAny>, triangles: &Bound<'_, PyAny>) -> PyResult<Self> {
        let rest = rows::<f64, 2>("rest", rest)?;
        let count = rest.len();
        let triangles = rows::<i64, 3>("triangles", triangles)?
            .into_iter()
            .enumerate()
            .map(|(triangle, corners)| {
                let mut vertices = [0; 3];
                for (vertex, corner) in vertices.iter_mut().zip(corners) {
                    *vertex = Vertex::try_from(corner).map_err(|_| {
                        PyValueError::new_err(format!(
                            "triangle {triangle} names vertex {corner}, but the mesh has {count} vertices"
                        ))
                    })?;
                }
                Ok(vertices)
            })
            .collect::<PyResult<_>>()?;
        let mesh =
            Mesh::new(rest, triangles).map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(Self(mesh))
    }

    /// Each vertex's place in the flat sheet, in meters, of shape (n, 2).
    #[getter]
    fn rest<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray2<f64>> {
        array(py, self.0.rest().iter().copied())
    }

    /// Each triangle's three vertices, counterclockwise in the flat sheet, of shape (m, 3).
    #[getter]
    fn triangles<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray2<i64>> {
        array(py, self.0.triangles().iter().map(|t| t.map(i64::from)))
    }

    /// Every edge, its lesser vertex first, in increasing order, of shape (e, 2).
    #[getter]
    fn edges<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray2<i64>> {
        array(py, self.0.edges().iter().map(|e| e.map(i64::from)))
    }

    /// Every interior edge with the vertices opposite it, of shape (k, 4).
    ///
    /// A row holds the edge's lesser vertex, its greater vertex, then the vertex opposite the
    /// edge in the triangle on its left, going from lesser to greater, and the vertex opposite
    /// it in the triangle on its right.
    #[getter]
    fn dihedrals<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray2<i64>> {
        let stencil = |h: &origamy_sim::Dihedral| {
            let ([u, v], [left, right]) = (h.edge, h.opposite);
            [u, v, left, right].map(i64::from)
        };
        array(py, self.0.dihedrals().iter().map(stencil))
    }

    /// Every boundary edge, run counterclockwise round the sheet, of shape (b, 2).
    #[getter]
    fn boundary<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray2<i64>> {
        array(py, self.0.boundary().iter().map(|e| e.map(i64::from)))
    }

    /// The area of the flat sheet, in square meters.
    #[getter]
    fn area(&self) -> f64 {
        self.0.area()
    }

    fn __repr__(&self) -> String {
        format!(
            "<Mesh of {} vertices and {} triangles>",
            self.0.vertex_count(),
            self.0.triangles().len()
        )
    }
}

/// The rows of an array of shape (n, N), cast safely to `T`.
fn rows<T: Element + Copy, const N: usize>(
    name: &str,
    value: &Bound<'_, PyAny>,
) -> PyResult<Vec<[T; N]>> {
    let py = value.py();
    let numpy = py.import("numpy")?;
    let options = PyDict::new(py);
    options.set_item("dtype", numpy::dtype::<T>(py))?;
    options.set_item("casting", "safe")?;
    options.set_item("copy", false)?;
    let array =
        numpy
            .call_method1("asarray", (value,))?
            .call_method("astype", (), Some(&options))?;
    let untyped = array.cast::<PyUntypedArray>()?;
    if untyped.ndim() != 2 || untyped.shape()[1] != N {
        let shape = array.getattr("shape")?.repr()?;
        return Err(PyValueError::new_err(format!(
            "{name} has shape {shape}, but it must have shape (n, {N})"
        )));
    }
    let typed = array.cast::<PyArray2<T>>()?.readonly();
    Ok(typed
        .as_array()
        .rows()
        .into_iter()
        .map(|row| std::array::from_fn(|i| row[i]))
        .collect())
}

/// An array of shape (n, N) holding these rows.
fn array<T: Element, const N: usize>(
    py: Python<'_>,
    rows: impl ExactSizeIterator<Item = [T; N]>,
) -> Bound<'_, PyArray2<T>> {
    let count = rows.len();
    let values = rows.flatten().collect();
    Array2::from_shape_vec((count, N), values)
        .expect("the rows fill the shape exactly")
        .into_pyarray(py)
}

/// The stiffness of a sheet in its plane, orthotropic about its grain, under plane stress.
///
/// Args:
///     along: Young's modulus along the grain, in pascals.
///     across: Young's modulus across the grain, in pascals.
///     poisson: Poisson's ratio: how much a stretch along the grain narrows the sheet across it.
///     shear: The shear modulus in the plane, in pascals.
///     grain: The grain's direction in the flat sheet, in radians from the x axis.
///
/// Raises:
///     ValueError: When the stiffness is not positive definite.
#[pyclass(name = "Stiffness", module = "origamy_sim", frozen, eq, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
struct PyStiffness(Stiffness);

#[pymethods]
impl PyStiffness {
    #[new]
    #[pyo3(signature = (*, along, across, poisson, shear, grain = 0.0))]
    fn new(along: f64, across: f64, poisson: f64, shear: f64, grain: f64) -> PyResult<Self> {
        let stiffness = Stiffness {
            along,
            across,
            poisson,
            shear,
            grain,
        };
        stiffness.check().map_err(value_error)?;
        Ok(Self(stiffness))
    }

    /// The stiffness of a material alike in every direction.
    ///
    /// Args:
    ///     young: Young's modulus, in pascals.
    ///     poisson: Poisson's ratio.
    #[staticmethod]
    fn isotropic(young: f64, poisson: f64) -> PyResult<Self> {
        let stiffness = Stiffness::isotropic(young, poisson);
        stiffness.check().map_err(value_error)?;
        Ok(Self(stiffness))
    }

    /// Young's modulus along the grain, in pascals.
    #[getter]
    fn along(&self) -> f64 {
        self.0.along
    }

    /// Young's modulus across the grain, in pascals.
    #[getter]
    fn across(&self) -> f64 {
        self.0.across
    }

    /// Poisson's ratio.
    #[getter]
    fn poisson(&self) -> f64 {
        self.0.poisson
    }

    /// The shear modulus in the plane, in pascals.
    #[getter]
    fn shear(&self) -> f64 {
        self.0.shear
    }

    /// The grain's direction in the flat sheet, in radians from the x axis.
    #[getter]
    fn grain(&self) -> f64 {
        self.0.grain
    }

    fn __repr__(&self) -> String {
        let s = self.0;
        format!(
            "Stiffness(along={:?}, across={:?}, poisson={:?}, shear={:?}, grain={:?})",
            s.along, s.across, s.poisson, s.shear, s.grain
        )
    }
}

/// What a sheet is made of. Every argument defaults to kami, the thin paper most origami uses.
///
/// Args:
///     thickness: The sheet's thickness, in meters.
///     mass_per_area: The sheet's mass per area, in kilograms per square meter.
///     stiffness: The sheet's stiffness in its plane.
///     strain_cap: The greatest norm the Green strain of any triangle may reach.
///     strain_onset: The fraction of the strain cap at which the strain barrier starts to push
///         back, between 0 and 1.
///
/// Raises:
///     ValueError: When a parameter lies outside its values.
#[pyclass(name = "Material", module = "origamy_sim", frozen, eq, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
struct PyMaterial(Material);

#[pymethods]
impl PyMaterial {
    #[new]
    #[pyo3(signature = (*, thickness = None, mass_per_area = None, stiffness = None, strain_cap = None, strain_onset = None))]
    fn new(
        thickness: Option<f64>,
        mass_per_area: Option<f64>,
        stiffness: Option<PyStiffness>,
        strain_cap: Option<f64>,
        strain_onset: Option<f64>,
    ) -> PyResult<Self> {
        Self(Material::kami()).replace(
            thickness,
            mass_per_area,
            stiffness,
            strain_cap,
            strain_onset,
        )
    }

    /// Kami: 70 µm thick, 60 g/m², a Young's modulus of 4 gigapascals and a Poisson's ratio of
    /// 0.3, with strain capped at half a percent.
    #[staticmethod]
    fn kami() -> Self {
        Self(Material::kami())
    }

    /// A copy with some parameters changed.
    ///
    /// Raises:
    ///     ValueError: When a parameter lies outside its values.
    #[pyo3(signature = (*, thickness = None, mass_per_area = None, stiffness = None, strain_cap = None, strain_onset = None))]
    fn replace(
        &self,
        thickness: Option<f64>,
        mass_per_area: Option<f64>,
        stiffness: Option<PyStiffness>,
        strain_cap: Option<f64>,
        strain_onset: Option<f64>,
    ) -> PyResult<Self> {
        let base = self.0;
        let material = Material {
            thickness: thickness.unwrap_or(base.thickness),
            mass_per_area: mass_per_area.unwrap_or(base.mass_per_area),
            stiffness: stiffness.map_or(base.stiffness, |s| s.0),
            strain_cap: strain_cap.unwrap_or(base.strain_cap),
            strain_onset: strain_onset.unwrap_or(base.strain_onset),
        };
        material.check().map_err(value_error)?;
        Ok(Self(material))
    }

    /// The sheet's thickness, in meters.
    #[getter]
    fn thickness(&self) -> f64 {
        self.0.thickness
    }

    /// The sheet's mass per area, in kilograms per square meter.
    #[getter]
    fn mass_per_area(&self) -> f64 {
        self.0.mass_per_area
    }

    /// The sheet's stiffness in its plane.
    #[getter]
    fn stiffness(&self) -> PyStiffness {
        PyStiffness(self.0.stiffness)
    }

    /// The greatest norm the Green strain of any triangle may reach.
    #[getter]
    fn strain_cap(&self) -> f64 {
        self.0.strain_cap
    }

    /// The fraction of the strain cap at which the strain barrier starts to push back.
    #[getter]
    fn strain_onset(&self) -> f64 {
        self.0.strain_onset
    }

    fn __repr__(&self) -> String {
        let m = self.0;
        format!(
            "Material(thickness={:?}, mass_per_area={:?}, stiffness={}, strain_cap={:?}, strain_onset={:?})",
            m.thickness,
            m.mass_per_area,
            PyStiffness(m.stiffness).__repr__(),
            m.strain_cap,
            m.strain_onset
        )
    }
}

/// A sharp crease: a dihedral where the paper has yielded, so that it bends there alone, about a
/// rest angle.
///
/// Args:
///     stiffness: The moment per length that turns the crease by one radian from rest, in newton
///         meters per meter per radian.
///     rest_angle: The dihedral angle the crease rests at, in radians, positive toward the
///         sheet's front.
///
/// Raises:
///     ValueError: When a parameter lies outside its values.
#[pyclass(name = "Crease", module = "origamy_sim", frozen, eq, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
struct PyCrease(Crease);

#[pymethods]
impl PyCrease {
    #[new]
    #[pyo3(signature = (*, stiffness, rest_angle))]
    fn new(stiffness: f64, rest_angle: f64) -> PyResult<Self> {
        let crease = Crease {
            stiffness,
            rest_angle,
        };
        crease.check().map_err(value_error)?;
        Ok(Self(crease))
    }

    /// The moment per length that turns the crease by one radian, in newton meters per meter.
    #[getter]
    fn stiffness(&self) -> f64 {
        self.0.stiffness
    }

    /// The dihedral angle the crease rests at, in radians.
    #[getter]
    fn rest_angle(&self) -> f64 {
        self.0.rest_angle
    }

    fn __repr__(&self) -> String {
        format!(
            "Crease(stiffness={:?}, rest_angle={:?})",
            self.0.stiffness, self.0.rest_angle
        )
    }
}

/// A sheet of paper: its rest mesh, its material and its sharp creases.
///
/// Args:
///     mesh: The sheet's rest mesh.
///     material: What the sheet is made of; kami unless given.
#[pyclass(name = "Sheet", module = "origamy_sim", from_py_object)]
#[derive(Clone)]
struct PySheet(Sheet);

#[pymethods]
impl PySheet {
    #[new]
    #[pyo3(signature = (mesh, material = None))]
    fn new(mesh: &PyMesh, material: Option<PyMaterial>) -> PyResult<Self> {
        let material = material.map_or_else(Material::kami, |m| m.0);
        Ok(Self(
            Sheet::new(mesh.0.clone(), material).map_err(value_error)?,
        ))
    }

    /// The sheet's rest mesh.
    #[getter]
    fn mesh(&self) -> PyMesh {
        PyMesh(self.0.mesh().clone())
    }

    /// What the sheet is made of.
    #[getter]
    fn material(&self) -> PyMaterial {
        PyMaterial(*self.0.material())
    }

    #[setter]
    fn set_material(&mut self, material: PyMaterial) -> PyResult<()> {
        self.0.set_material(material.0).map_err(value_error)
    }

    /// The crease along an interior edge, or None where the paper is plain.
    ///
    /// Raises:
    ///     ValueError: When the edge is not an interior edge of the sheet.
    fn crease(&self, edge: [Vertex; 2]) -> PyResult<Option<PyCrease>> {
        Ok(self.0.crease(edge).map_err(value_error)?.map(PyCrease))
    }

    /// Makes an interior edge a sharp crease, or with None plain paper again.
    ///
    /// Raises:
    ///     ValueError: When the edge is not an interior edge of the sheet.
    fn set_crease(&mut self, edge: [Vertex; 2], crease: Option<PyCrease>) -> PyResult<()> {
        self.0
            .set_crease(edge, crease.map(|c| c.0))
            .map_err(value_error)
    }

    fn __repr__(&self) -> String {
        let creases = (0..self.0.mesh().dihedrals().len())
            .filter(|&d| {
                let edge = self.0.mesh().dihedrals()[d].edge;
                matches!(self.0.crease(edge), Ok(Some(_)))
            })
            .count();
        format!(
            "<Sheet of {} vertices with {creases} creases>",
            self.0.mesh().vertex_count()
        )
    }
}

/// The settings of a simulation: the world it runs in and how its solver works.
///
/// Args:
///     gravity: The acceleration of gravity, in meters per second squared.
///     time_step: The length of a time step, in seconds; infinite for quasi-static steps, which
///         find the rest the forces lead to.
///     tolerance: The largest vertex move of a Newton update at which a time step has
///         converged, in meters.
///     iteration_limit: The most Newton iterations a time step may take.
///     sufficient_decrease: The fraction of the predicted decrease of energy a line search
///         requires, between 0 and 1.
///     shortest_update: The shortest fraction of a Newton update a line search tries, between
///         0 and 1.
///
/// Raises:
///     ValueError: When a setting lies outside its values.
#[pyclass(name = "Settings", module = "origamy_sim", frozen, eq, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
struct PySettings(Settings);

#[pymethods]
impl PySettings {
    #[new]
    #[pyo3(signature = (*, gravity = None, time_step = None, tolerance = None, iteration_limit = None, sufficient_decrease = None, shortest_update = None))]
    fn new(
        gravity: Option<[f64; 3]>,
        time_step: Option<f64>,
        tolerance: Option<f64>,
        iteration_limit: Option<usize>,
        sufficient_decrease: Option<f64>,
        shortest_update: Option<f64>,
    ) -> PyResult<Self> {
        Self(Settings::default()).replace(
            gravity,
            time_step,
            tolerance,
            iteration_limit,
            sufficient_decrease,
            shortest_update,
        )
    }

    /// A copy with some settings changed.
    ///
    /// Raises:
    ///     ValueError: When a setting lies outside its values.
    #[pyo3(signature = (*, gravity = None, time_step = None, tolerance = None, iteration_limit = None, sufficient_decrease = None, shortest_update = None))]
    fn replace(
        &self,
        gravity: Option<[f64; 3]>,
        time_step: Option<f64>,
        tolerance: Option<f64>,
        iteration_limit: Option<usize>,
        sufficient_decrease: Option<f64>,
        shortest_update: Option<f64>,
    ) -> PyResult<Self> {
        let base = self.0;
        let settings = Settings {
            gravity: gravity.unwrap_or(base.gravity),
            time_step: time_step.unwrap_or(base.time_step),
            tolerance: tolerance.unwrap_or(base.tolerance),
            iteration_limit: iteration_limit.unwrap_or(base.iteration_limit),
            sufficient_decrease: sufficient_decrease.unwrap_or(base.sufficient_decrease),
            shortest_update: shortest_update.unwrap_or(base.shortest_update),
        };
        settings.check().map_err(value_error)?;
        Ok(Self(settings))
    }

    /// The acceleration of gravity, in meters per second squared.
    #[getter]
    fn gravity(&self) -> (f64, f64, f64) {
        let [x, y, z] = self.0.gravity;
        (x, y, z)
    }

    /// The length of a time step, in seconds.
    #[getter]
    fn time_step(&self) -> f64 {
        self.0.time_step
    }

    /// The largest vertex move of a Newton update at which a time step has converged, in meters.
    #[getter]
    fn tolerance(&self) -> f64 {
        self.0.tolerance
    }

    /// The most Newton iterations a time step may take.
    #[getter]
    fn iteration_limit(&self) -> usize {
        self.0.iteration_limit
    }

    /// The fraction of the predicted decrease of energy a line search requires.
    #[getter]
    fn sufficient_decrease(&self) -> f64 {
        self.0.sufficient_decrease
    }

    /// The shortest fraction of a Newton update a line search tries.
    #[getter]
    fn shortest_update(&self) -> f64 {
        self.0.shortest_update
    }

    fn __repr__(&self) -> String {
        let s = self.0;
        format!(
            "Settings(gravity={:?}, time_step={:?}, tolerance={:?}, iteration_limit={}, sufficient_decrease={:?}, shortest_update={:?})",
            self.gravity(),
            s.time_step,
            s.tolerance,
            s.iteration_limit,
            s.sufficient_decrease,
            s.shortest_update
        )
    }
}

/// The energy of a simulation, term by term, in joules.
#[pyclass(name = "Energy", module = "origamy_sim", frozen, eq, get_all)]
#[derive(Clone, Copy, PartialEq)]
struct PyEnergy {
    /// The elastic energy of stretching.
    stretching: f64,
    /// The energy of the barrier that caps strain.
    strain_barrier: f64,
    /// The elastic energy of bending, away from sharp creases.
    bending: f64,
    /// The elastic energy of sharp creases.
    creases: f64,
    /// The potential energy of gravity, zero at the origin.
    gravity: f64,
    /// The kinetic energy.
    kinetic: f64,
}

impl From<Energy> for PyEnergy {
    fn from(e: Energy) -> Self {
        Self {
            stretching: e.stretching,
            strain_barrier: e.strain_barrier,
            bending: e.bending,
            creases: e.creases,
            gravity: e.gravity,
            kinetic: e.kinetic,
        }
    }
}

#[pymethods]
impl PyEnergy {
    fn __repr__(&self) -> String {
        format!(
            "Energy(stretching={:?}, strain_barrier={:?}, bending={:?}, creases={:?}, gravity={:?}, kinetic={:?})",
            self.stretching,
            self.strain_barrier,
            self.bending,
            self.creases,
            self.gravity,
            self.kinetic
        )
    }
}

/// What a time step did.
#[pyclass(name = "Report", module = "origamy_sim", frozen, eq, get_all)]
#[derive(Clone, Copy, PartialEq)]
struct PyReport {
    /// Whether the step reached its tolerance.
    converged: bool,
    /// The Newton iterations the step took.
    iterations: usize,
    /// The largest vertex move of the last Newton update, in meters: the residual.
    update: f64,
    /// The energy after the step.
    energy: PyEnergy,
    /// The greatest norm of any triangle's Green strain after the step.
    greatest_strain: f64,
    /// The simulated time after the step, in seconds.
    time: f64,
}

impl From<Report> for PyReport {
    fn from(r: Report) -> Self {
        Self {
            converged: r.converged,
            iterations: r.iterations,
            update: r.update,
            energy: r.energy.into(),
            greatest_strain: r.greatest_strain,
            time: r.time,
        }
    }
}

#[pymethods]
impl PyReport {
    fn __repr__(&self) -> String {
        format!(
            "Report(converged={}, iterations={}, update={:?}, energy={}, greatest_strain={:?}, time={:?})",
            if self.converged { "True" } else { "False" },
            self.iterations,
            self.update,
            self.energy.__repr__(),
            self.greatest_strain,
            self.time
        )
    }
}

/// Sheets in space, stepped through time. Each sheet starts flat on the plane z = 0, still.
///
/// The simulation keeps its own copies of the sheets; change their materials and creases
/// through it.
///
/// Args:
///     sheets: The sheets.
///     settings: The world and the solver; the defaults unless given.
#[pyclass(name = "Simulation", module = "origamy_sim")]
struct PySimulation(Simulation);

#[pymethods]
impl PySimulation {
    #[new]
    #[pyo3(signature = (sheets, settings = None))]
    fn new(sheets: Vec<PySheet>, settings: Option<PySettings>) -> PyResult<Self> {
        let settings = settings.map_or_else(Settings::default, |s| s.0);
        let sheets = sheets.into_iter().map(|s| s.0).collect();
        Ok(Self(
            Simulation::new(sheets, settings).map_err(value_error)?,
        ))
    }

    /// The world and the solver.
    #[getter]
    fn settings(&self) -> PySettings {
        PySettings(*self.0.settings())
    }

    #[setter]
    fn set_settings(&mut self, settings: PySettings) -> PyResult<()> {
        self.0.set_settings(settings.0).map_err(value_error)
    }

    /// The simulated time, in seconds.
    #[getter]
    fn time(&self) -> f64 {
        self.0.time()
    }

    /// A copy of a sheet, with its material and creases as they are now.
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such sheet.
    fn sheet(&self, sheet: usize) -> PyResult<PySheet> {
        self.check(sheet)?;
        Ok(PySheet(self.0.sheets()[sheet].clone()))
    }

    /// Changes a sheet's material.
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such sheet.
    fn set_material(&mut self, sheet: usize, material: PyMaterial) -> PyResult<()> {
        self.check(sheet)?;
        self.0
            .sheet_mut(sheet)
            .set_material(material.0)
            .map_err(value_error)
    }

    /// Makes an interior edge of a sheet a sharp crease, or with None plain paper again.
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such sheet.
    ///     ValueError: When the edge is not an interior edge of the sheet.
    fn set_crease(
        &mut self,
        sheet: usize,
        edge: [Vertex; 2],
        crease: Option<PyCrease>,
    ) -> PyResult<()> {
        self.check(sheet)?;
        self.0
            .sheet_mut(sheet)
            .set_crease(edge, crease.map(|c| c.0))
            .map_err(value_error)
    }

    /// Where each vertex of a sheet lies, in meters, of shape (n, 3).
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such sheet.
    fn positions<'py>(&self, py: Python<'py>, sheet: usize) -> PyResult<Bound<'py, PyArray2<f64>>> {
        self.check(sheet)?;
        Ok(array(py, self.0.positions(sheet).iter().copied()))
    }

    /// Moves every vertex of a sheet.
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such sheet.
    ///     ValueError: When the positions do not fit the sheet or stretch it past its cap.
    fn set_positions(&mut self, sheet: usize, positions: &Bound<'_, PyAny>) -> PyResult<()> {
        self.check(sheet)?;
        let positions = rows::<f64, 3>("positions", positions)?;
        self.0.set_positions(sheet, &positions).map_err(value_error)
    }

    /// Each vertex's velocity, in meters per second, of shape (n, 3).
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such sheet.
    fn velocities<'py>(
        &self,
        py: Python<'py>,
        sheet: usize,
    ) -> PyResult<Bound<'py, PyArray2<f64>>> {
        self.check(sheet)?;
        Ok(array(py, self.0.velocities(sheet).iter().copied()))
    }

    /// Sets every vertex's velocity.
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such sheet.
    ///     ValueError: When the velocities do not fit the sheet.
    fn set_velocities(&mut self, sheet: usize, velocities: &Bound<'_, PyAny>) -> PyResult<()> {
        self.check(sheet)?;
        let velocities = rows::<f64, 3>("velocities", velocities)?;
        self.0
            .set_velocities(sheet, &velocities)
            .map_err(value_error)
    }

    /// Whether each vertex of a sheet is held where it lies, of shape (n,).
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such sheet.
    fn held<'py>(&self, py: Python<'py>, sheet: usize) -> PyResult<Bound<'py, PyArray1<bool>>> {
        self.check(sheet)?;
        Ok(PyArray1::from_slice(py, self.0.held(sheet)))
    }

    /// Holds a vertex of a sheet where it lies, or with `held=False` lets it go.
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such sheet, or the sheet no such vertex.
    #[pyo3(signature = (sheet, vertex, *, held = true))]
    fn hold(&mut self, sheet: usize, vertex: usize, held: bool) -> PyResult<()> {
        self.check(sheet)?;
        let count = self.0.positions(sheet).len();
        if vertex >= count {
            return Err(PyIndexError::new_err(format!(
                "the sheet has {count} vertices, so no vertex {vertex}"
            )));
        }
        self.0.hold(sheet, vertex, held);
        Ok(())
    }

    /// The dihedral angle along each interior edge of a sheet, in the order of its mesh's
    /// dihedrals, in radians: zero where flat, positive where it folds toward its front.
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such sheet.
    fn fold_angles<'py>(
        &self,
        py: Python<'py>,
        sheet: usize,
    ) -> PyResult<Bound<'py, PyArray1<f64>>> {
        self.check(sheet)?;
        Ok(PyArray1::from_vec(py, self.0.fold_angles(sheet)))
    }

    /// The simulation's energy now, term by term.
    fn energy(&self) -> PyEnergy {
        self.0.energy().into()
    }

    /// Advances the simulation by one time step.
    ///
    /// Returns:
    ///     What the step did, with whether it reached its tolerance.
    ///
    /// Raises:
    ///     SingularError: When some motion costs no energy, so the step has no unique result.
    fn step(&mut self, py: Python<'_>) -> PyResult<PyReport> {
        let simulation = &mut self.0;
        let report = py
            .detach(|| simulation.step())
            .map_err(|error| SingularError::new_err(error.to_string()))?;
        Ok(report.into())
    }

    fn __repr__(&self) -> String {
        format!(
            "<Simulation of {} sheets at {:?} s>",
            self.0.sheets().len(),
            self.0.time()
        )
    }
}

impl PySimulation {
    /// Checks that the simulation has a sheet.
    fn check(&self, sheet: usize) -> PyResult<()> {
        let count = self.0.sheets().len();
        if sheet < count {
            Ok(())
        } else {
            Err(PyIndexError::new_err(format!(
                "the simulation has {count} sheets, so no sheet {sheet}"
            )))
        }
    }
}

/// A core error as a Python `ValueError` with the same message.
fn value_error(error: impl std::error::Error) -> PyErr {
    PyValueError::new_err(error.to_string())
}

/// The compiled core of origamy-sim.
#[pymodule]
mod _core {
    #[pymodule_export]
    use super::{
        PyCrease, PyEnergy, PyMaterial, PyMesh, PyReport, PySettings, PySheet, PySimulation,
        PyStiffness, SingularError,
    };
}
