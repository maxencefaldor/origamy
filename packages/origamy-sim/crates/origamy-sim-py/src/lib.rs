//! Python bindings of origamy-sim, as the module `origamy_sim._core`.
//!
//! The Python package `origamy_sim` re-exports every class here, and `_core.pyi` types them.
//! Parameters cross as the pydantic models of `origamy_sim.parameters`. The doc comments are
//! the Python docstrings, so they follow the Google style.

#![allow(
    clippy::doc_markdown,
    reason = "the doc comments are Python docstrings, where Google style sections name exceptions bare"
)]

use numpy::ndarray::Array2;
use numpy::{
    Element, IntoPyArray, PyArray1, PyArray2, PyArrayDescrMethods, PyArrayMethods, PyUntypedArray,
    PyUntypedArrayMethods,
};
use origamy_sim::{
    Body, Crease, Energy, Faces, Material, Mesh, Refinement, Report, Settings, Sheet, Simulation,
    StepError, Vertex,
};
use pyo3::create_exception;
use pyo3::exceptions::{PyIndexError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

mod parameters;

use parameters::Model;

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

    /// A `width` by `height` rectangle of `columns` by `rows` cells, each split along its
    /// rising diagonal, with its lower left corner at the origin.
    ///
    /// Vertex `j * (columns + 1) + i` lies at column `i` and row `j`.
    ///
    /// Raises:
    ///     ValueError: When the grid has no cell, too many vertices, or a size that is not
    ///         positive and finite.
    #[staticmethod]
    fn grid(columns: u32, rows: u32, width: f64, height: f64) -> PyResult<Self> {
        Ok(Self(
            Mesh::grid(columns, rows, width, height).map_err(value_error)?,
        ))
    }

    /// A mesh of faces that meet along their edges, refined into small, well-shaped
    /// triangles, and the face each triangle lies in.
    ///
    /// Every edge of every face becomes a chain of mesh edges, so a sheet can bend sharply
    /// along it, as a crease pattern's creases need. The mesh keeps each point as a vertex, in
    /// order, before the vertices the refinement adds.
    ///
    /// Args:
    ///     points: The faces' corners, in meters, of shape (n, 2).
    ///     faces: Each face's corners, as indices of points, counterclockwise.
    ///     refinement: The largest area and the least angle of the triangles.
    ///
    /// Returns:
    ///     The mesh, and the face of each triangle, of shape (m,).
    ///
    /// Raises:
    ///     ValueError: When a point is not finite, or two points coincide. The error also
    ///         names a face with too few corners, faces that cross or overlap, and a
    ///         refinement that cannot reach its angle.
    #[staticmethod]
    fn of_faces<'py>(
        py: Python<'py>,
        points: &Bound<'_, PyAny>,
        faces: Vec<Vec<i64>>,
        refinement: Model<Refinement>,
    ) -> PyResult<(Self, Bound<'py, PyArray1<i64>>)> {
        let points = rows::<f64, 2>("points", points)?;
        let faces = faces
            .into_iter()
            .enumerate()
            .map(|(face, corners)| {
                corners
                    .into_iter()
                    .map(|c| {
                        Vertex::try_from(c).map_err(|_| {
                            PyValueError::new_err(format!("face {face} names point {c}"))
                        })
                    })
                    .collect::<PyResult<Vec<Vertex>>>()
            })
            .collect::<PyResult<Vec<_>>>()?;
        let meshed = Faces::new(&points, &faces, refinement.0).map_err(value_error)?;
        let labels: Vec<i64> = meshed.faces.iter().map(|&f| i64::from(f)).collect();
        Ok((Self(meshed.mesh), labels.into_pyarray(py)))
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
    /// A row holds the edge's lesser vertex and its greater vertex. Then come the vertices
    /// opposite the edge in its left triangle and in its right one. The left triangle lies on
    /// the edge's left, going from its lesser vertex to its greater.
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

/// A value as a NumPy array of `T`, cast safely; integers take integer values alone.
fn cast<'py, T: Element>(name: &str, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let py = value.py();
    let numpy = py.import("numpy")?;
    let array = numpy.call_method1("asarray", (value,))?;
    let target = numpy::dtype::<T>(py);
    let kind: char = array.getattr("dtype")?.getattr("kind")?.extract()?;
    if target.kind() == b'i' && !matches!(kind, 'i' | 'u') {
        let found = array.getattr("dtype")?.str()?;
        return Err(PyTypeError::new_err(format!(
            "{name} must hold integers, but it holds {found}"
        )));
    }
    let options = PyDict::new(py);
    options.set_item("dtype", target)?;
    options.set_item("casting", "safe")?;
    options.set_item("copy", false)?;
    array.call_method("astype", (), Some(&options))
}

/// The rows of an array of shape (n, N), cast safely to `T`.
fn rows<T: Element + Copy, const N: usize>(
    name: &str,
    value: &Bound<'_, PyAny>,
) -> PyResult<Vec<[T; N]>> {
    let array = cast::<T>(name, value)?;
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

/// The entries of a scalar or an array of shape (n,), as 64-bit integers.
fn entries(name: &str, value: &Bound<'_, PyAny>) -> PyResult<Vec<i64>> {
    let array = cast::<i64>(name, value)?;
    let untyped = array.cast::<PyUntypedArray>()?;
    if untyped.ndim() > 1 {
        let shape = array.getattr("shape")?.repr()?;
        return Err(PyValueError::new_err(format!(
            "{name} has shape {shape}, but it must be a scalar or have shape (n,)"
        )));
    }
    array
        .call_method0("ravel")?
        .call_method0("tolist")?
        .extract()
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

/// A sheet of paper: its rest mesh, its material, its sharp creases and its rest angles.
///
/// Args:
///     mesh: The sheet's rest mesh.
///     material: What the sheet is made of; kami unless given.
///
/// Raises:
///     TypeError: When the material is not a `Material`.
///     ValueError: When a parameter of the material lies outside its values.
#[pyclass(name = "Sheet", module = "origamy_sim", from_py_object)]
#[derive(Clone)]
struct PySheet(Sheet);

#[pymethods]
impl PySheet {
    #[new]
    #[pyo3(signature = (mesh, material = None))]
    fn new(mesh: &PyMesh, material: Option<Model<Material>>) -> PyResult<Self> {
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
    fn material(&self) -> Model<Material> {
        Model(*self.0.material())
    }

    #[setter]
    fn set_material(&mut self, material: Model<Material>) -> PyResult<()> {
        self.0.set_material(material.0).map_err(value_error)
    }

    /// The crease along an interior edge, or None where the paper is plain.
    ///
    /// Raises:
    ///     ValueError: When the edge is not an interior edge of the sheet.
    fn crease(&self, edge: [Vertex; 2]) -> PyResult<Option<Model<Crease>>> {
        Ok(self.0.crease(edge).map_err(value_error)?.map(Model))
    }

    /// Makes an interior edge a sharp crease, or with None plain paper again.
    ///
    /// Raises:
    ///     ValueError: When the edge is not an interior edge of the sheet, or the crease's
    ///         stiffness lies outside its values.
    fn set_crease(&mut self, edge: [Vertex; 2], crease: Option<Model<Crease>>) -> PyResult<()> {
        self.0
            .set_crease(edge, crease.map(|c| c.0))
            .map_err(value_error)
    }

    /// The dihedral angle an interior edge rests at, in radians: zero where the paper rests
    /// flat, positive toward the sheet's front.
    ///
    /// Raises:
    ///     ValueError: When the edge is not an interior edge of the sheet.
    fn rest_angle(&self, edge: [Vertex; 2]) -> PyResult<f64> {
        self.0.rest_angle(edge).map_err(value_error)
    }

    /// The rotation the crease along an interior edge has yielded and crept through, in
    /// radians; zero for plain paper.
    ///
    /// Raises:
    ///     ValueError: When the edge is not an interior edge of the sheet.
    fn plastic_rotation(&self, edge: [Vertex; 2]) -> PyResult<f64> {
        self.0.plastic_rotation(edge).map_err(value_error)
    }

    /// Sets the dihedral angle an interior edge rests at.
    ///
    /// Along a sharp crease, the crease rests there. Elsewhere the plate around the edge
    /// takes the rest curvature the angle gives, so paper can rest curved.
    ///
    /// Raises:
    ///     ValueError: When the edge is not an interior edge of the sheet, or the angle is not
    ///         finite or reaches a half turn.
    fn set_rest_angle(&mut self, edge: [Vertex; 2], angle: f64) -> PyResult<()> {
        self.0.set_rest_angle(edge, angle).map_err(value_error)
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
    /// The work creases take as they yield over the time step, dissipated or stored by
    /// hardening.
    yielding: f64,
    /// The energy of contact's barrier, between sheets and with the table.
    contact: f64,
    /// The work friction does against the slip of contacts over the time step.
    friction: f64,
    /// The work air drag does against the paper over the time step.
    drag: f64,
    /// The potential energy of the point forces, zero at the origin, and the work pressure
    /// does over the time step.
    loads: f64,
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
            yielding: e.yielding,
            contact: e.contact,
            friction: e.friction,
            drag: e.drag,
            loads: e.loads,
            gravity: e.gravity,
            kinetic: e.kinetic,
        }
    }
}

#[pymethods]
impl PyEnergy {
    /// The sum of every term, in joules.
    #[getter]
    fn total(&self) -> f64 {
        Energy {
            stretching: self.stretching,
            strain_barrier: self.strain_barrier,
            bending: self.bending,
            creases: self.creases,
            yielding: self.yielding,
            contact: self.contact,
            friction: self.friction,
            drag: self.drag,
            loads: self.loads,
            gravity: self.gravity,
            kinetic: self.kinetic,
        }
        .total()
    }

    fn __repr__(&self) -> String {
        format!(
            "Energy(stretching={:?}, strain_barrier={:?}, bending={:?}, creases={:?}, yielding={:?}, contact={:?}, friction={:?}, drag={:?}, loads={:?}, gravity={:?}, kinetic={:?})",
            self.stretching,
            self.strain_barrier,
            self.bending,
            self.creases,
            self.yielding,
            self.contact,
            self.friction,
            self.drag,
            self.loads,
            self.gravity,
            self.kinetic
        )
    }
}

/// What a time step did.
#[pyclass(name = "Report", module = "origamy_sim", frozen, eq, get_all)]
#[derive(Clone, Copy, PartialEq)]
struct PyReport {
    /// Whether the step reached its tolerance, in every time step it took.
    converged: bool,
    /// The Newton iterations the step took, over every time step and every lag of friction.
    iterations: usize,
    /// The time steps the step took: 1, or more where a time step that did not converge was
    /// halved.
    substeps: usize,
    /// The largest vertex move of the last Newton update, in meters: the residual.
    update: f64,
    /// The energy after the step.
    energy: PyEnergy,
    /// The greatest norm of any triangle's Green strain after the step.
    greatest_strain: f64,
    /// The pairs of primitives, and the vertices on the table, that contact pushes apart.
    contacts: usize,
    /// The least gap beyond the thickness within contact's reach, in meters; infinite where
    /// nothing is that close.
    least_gap: f64,
    /// The simulated time after the step, in seconds.
    time: f64,
}

impl From<Report> for PyReport {
    fn from(r: Report) -> Self {
        Self {
            converged: r.converged,
            iterations: r.iterations,
            substeps: r.substeps,
            update: r.update,
            energy: r.energy.into(),
            greatest_strain: r.greatest_strain,
            contacts: r.contacts,
            least_gap: r.least_gap,
            time: r.time,
        }
    }
}

#[pymethods]
impl PyReport {
    fn __repr__(&self) -> String {
        format!(
            "Report(converged={}, iterations={}, substeps={}, update={:?}, energy={}, greatest_strain={:?}, contacts={}, least_gap={:?}, time={:?})",
            if self.converged { "True" } else { "False" },
            self.iterations,
            self.substeps,
            self.update,
            self.energy.__repr__(),
            self.greatest_strain,
            self.contacts,
            self.least_gap,
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
///     settings: The world, contact and the solver; the defaults unless given.
///
/// Raises:
///     TypeError: When the settings are not `Settings`.
///     ValueError: When a setting lies outside its values.
#[pyclass(name = "Simulation", module = "origamy_sim")]
struct PySimulation(Simulation);

#[pymethods]
impl PySimulation {
    #[new]
    #[pyo3(signature = (sheets, settings = None))]
    fn new(sheets: Vec<PySheet>, settings: Option<Model<Settings>>) -> PyResult<Self> {
        let settings = settings.map_or_else(Settings::default, |s| s.0);
        let sheets = sheets.into_iter().map(|s| s.0).collect();
        Ok(Self(
            Simulation::new(sheets, settings).map_err(value_error)?,
        ))
    }

    /// The world, contact and the solver.
    #[getter]
    fn settings(&self) -> Model<Settings> {
        Model(*self.0.settings())
    }

    #[setter]
    fn set_settings(&mut self, settings: Model<Settings>) -> PyResult<()> {
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
    fn set_material(&mut self, sheet: usize, material: Model<Material>) -> PyResult<()> {
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
    ///     ValueError: When the edge is not an interior edge of the sheet, or the crease's
    ///         stiffness lies outside its values.
    fn set_crease(
        &mut self,
        sheet: usize,
        edge: [Vertex; 2],
        crease: Option<Model<Crease>>,
    ) -> PyResult<()> {
        self.check(sheet)?;
        self.0
            .sheet_mut(sheet)
            .set_crease(edge, crease.map(|c| c.0))
            .map_err(value_error)
    }

    /// Sets the dihedral angle an interior edge of a sheet rests at, as `Sheet.set_rest_angle`
    /// does. Changing a crease's rest angle folds it.
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such sheet.
    ///     ValueError: When the edge is not an interior edge of the sheet, or the angle is not
    ///         finite or reaches a half turn.
    fn set_rest_angle(&mut self, sheet: usize, edge: [Vertex; 2], angle: f64) -> PyResult<()> {
        self.check(sheet)?;
        self.0
            .sheet_mut(sheet)
            .set_rest_angle(edge, angle)
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

    /// Holds vertices of a sheet where they lie, or with `held=False` lets them go.
    ///
    /// A hold takes a vertex from any body that grips it.
    ///
    /// Args:
    ///     sheet: The sheet.
    ///     vertices: A vertex, or an integer array of vertices.
    ///     held: Whether to hold the vertices or let them go.
    ///
    /// Raises:
    ///     TypeError: When the vertices are not integers.
    ///     IndexError: When the simulation has no such sheet, or the sheet no such vertex.
    #[pyo3(signature = (sheet, vertices, *, held = true))]
    fn hold(&mut self, sheet: usize, vertices: &Bound<'_, PyAny>, held: bool) -> PyResult<()> {
        for vertex in self.vertices(sheet, vertices)? {
            self.0.hold(sheet, vertex, held);
        }
        Ok(())
    }

    /// Moves held vertices of a sheet, each to its position, over the next time step, as a
    /// hand carries them.
    ///
    /// The vertices move within the step, so collision detection covers their paths, and the
    /// paper around them follows. The step converges only once every vertex reaches its
    /// position.
    ///
    /// Args:
    ///     sheet: The sheet.
    ///     vertices: A vertex, or an integer array of n vertices.
    ///     positions: Where each vertex goes, in meters, of shape (n, 3).
    ///
    /// Raises:
    ///     TypeError: When the vertices are not integers.
    ///     IndexError: When the simulation has no such sheet, or the sheet no such vertex.
    ///     ValueError: When a vertex is not held, a body grips it, or a position is not finite.
    ///         A refused move changes nothing.
    fn move_held(
        &mut self,
        sheet: usize,
        vertices: &Bound<'_, PyAny>,
        positions: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let moves = self.pairs(sheet, vertices, "positions", positions)?;
        self.0.move_held(sheet, &moves).map_err(value_error)
    }

    /// The force each hold exerted on its vertex at the end of the last time step, of shape
    /// (n, 3).
    ///
    /// The forces are in newtons, and zero where a vertex is free.
    ///
    /// The force balances every other force on the vertex, to within the forces the solver's
    /// tolerance leaves.
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such sheet.
    fn hold_forces<'py>(
        &self,
        py: Python<'py>,
        sheet: usize,
    ) -> PyResult<Bound<'py, PyArray2<f64>>> {
        self.check(sheet)?;
        Ok(array(py, self.0.hold_forces(sheet).iter().copied()))
    }

    /// The point force on each vertex of a sheet, in newtons, of shape (n, 3).
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such sheet.
    fn forces<'py>(&self, py: Python<'py>, sheet: usize) -> PyResult<Bound<'py, PyArray2<f64>>> {
        self.check(sheet)?;
        Ok(array(py, self.0.forces(sheet).iter().copied()))
    }

    /// Sets the point forces on vertices of a sheet, in newtons, each until set again.
    ///
    /// Args:
    ///     sheet: The sheet.
    ///     vertices: A vertex, or an integer array of n vertices.
    ///     forces: The force on each vertex, in newtons, of shape (n, 3).
    ///
    /// Raises:
    ///     TypeError: When the vertices are not integers.
    ///     IndexError: When the simulation has no such sheet, or the sheet no such vertex.
    ///     ValueError: When a force is not finite. A refused force changes nothing.
    fn set_forces(
        &mut self,
        sheet: usize,
        vertices: &Bound<'_, PyAny>,
        forces: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let forces = self.pairs(sheet, vertices, "forces", forces)?;
        self.0.set_forces(sheet, &forces).map_err(value_error)
    }

    /// The pressure on a sheet's front, in pascals.
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such sheet.
    fn pressure(&self, sheet: usize) -> PyResult<f64> {
        self.check(sheet)?;
        Ok(self.0.pressure(sheet))
    }

    /// Sets the pressure on a sheet's front, in pascals, until set again.
    ///
    /// The pressure pushes each patch of the sheet against its normal at the start of each
    /// time step; a negative pressure pulls.
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such sheet.
    ///     ValueError: When the pressure is not finite.
    fn set_pressure(&mut self, sheet: usize, pressure: f64) -> PyResult<()> {
        self.check(sheet)?;
        self.0.set_pressure(sheet, pressure).map_err(value_error)
    }

    /// Adds a rigid body, and returns its index.
    ///
    /// The body is a sphere round one point, or a capsule round the segment between two. Its
    /// points stay where they are until `move_body` carries them.
    ///
    /// Args:
    ///     body: The body's radius and friction.
    ///     points: The sphere's center, or the capsule's two ends, in meters, of shape (1, 3)
    ///         or (2, 3).
    ///
    /// Raises:
    ///     ValueError: When the points are neither one nor two, or not finite. The error also
    ///         names a capsule whose ends coincide. A body may not lie closer to paper than its
    ///         radius and half the paper's thickness.
    fn add_body(&mut self, body: Model<Body>, points: &Bound<'_, PyAny>) -> PyResult<usize> {
        let points = rows::<f64, 3>("points", points)?;
        self.0.add_body(body.0, &points).map_err(value_error)
    }

    /// Each body of the simulation, in the order they joined it.
    #[getter]
    fn bodies(&self) -> Vec<Model<Body>> {
        self.0.bodies().iter().copied().map(Model).collect()
    }

    /// Where a body's points lie, in meters: a sphere's center, or a capsule's two ends.
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such body.
    fn body_points<'py>(
        &self,
        py: Python<'py>,
        body: usize,
    ) -> PyResult<Bound<'py, PyArray2<f64>>> {
        self.check_body(body)?;
        Ok(array(py, self.0.body_points(body).iter().copied()))
    }

    /// Moves a body's points over the next time step, as a hand carries it.
    ///
    /// The body moves within the step, so collision detection covers its path, and paper in
    /// its way moves aside. The step converges only once the body reaches its points. A body
    /// its hand holds by a spring stops instead where the spring balances the paper's push.
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such body.
    ///     ValueError: When the points are not as many as the body's, or not finite, or a
    ///         capsule's ends coincide.
    fn move_body(&mut self, body: usize, points: &Bound<'_, PyAny>) -> PyResult<()> {
        self.check_body(body)?;
        let points = rows::<f64, 3>("points", points)?;
        self.0.move_body(body, &points).map_err(value_error)
    }

    /// The force a body exerted on the paper at the end of the last time step, in newtons, of
    /// shape (3,).
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such body.
    fn body_force<'py>(&self, py: Python<'py>, body: usize) -> PyResult<Bound<'py, PyArray1<f64>>> {
        self.check_body(body)?;
        Ok(PyArray1::from_slice(py, &self.0.body_force(body)))
    }

    /// Ties vertices of a sheet to a body, which then carries them as it moves, as a pinch does.
    ///
    /// The vertices are held, and each time step carries them by the motion of the body's
    /// hand. A sphere carries them as its center moves. A capsule keeps each vertex's share of
    /// the way along its axis. The vertex's offset from the axis turns as the axis turns, by
    /// the least rotation. A vertex another body grips moves to this one.
    ///
    /// Args:
    ///     body: The body.
    ///     sheet: The sheet.
    ///     vertices: A vertex, or an integer array of vertices.
    ///
    /// Raises:
    ///     TypeError: When the vertices are not integers.
    ///     IndexError: When the simulation has no such body or sheet, or the sheet no such
    ///         vertex.
    fn grip(&mut self, body: usize, sheet: usize, vertices: &Bound<'_, PyAny>) -> PyResult<()> {
        self.check_body(body)?;
        let vertices = self.vertices(sheet, vertices)?;
        self.0.grip(body, sheet, &vertices);
        Ok(())
    }

    /// Lets go of every vertex a body grips.
    ///
    /// Each vertex falls free with the velocity the body last gave it.
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such body.
    fn release(&mut self, body: usize) -> PyResult<()> {
        self.check_body(body)?;
        self.0.release(body);
        Ok(())
    }

    /// The body that grips each vertex of a sheet, or -1 where none does, of shape (n,).
    ///
    /// Raises:
    ///     IndexError: When the simulation has no such sheet.
    fn grips<'py>(&self, py: Python<'py>, sheet: usize) -> PyResult<Bound<'py, PyArray1<i64>>> {
        self.check(sheet)?;
        let grips = self.0.grips(sheet).iter().map(|grip| {
            grip.map_or(-1, |body| {
                i64::try_from(body).expect("a scene counts its bodies in 64 bits")
            })
        });
        Ok(PyArray1::from_iter(py, grips))
    }

    /// The dihedral angle along each interior edge of a sheet, in radians.
    ///
    /// The angles follow the order of the mesh's dihedrals. An angle is zero where the sheet
    /// lies flat, and positive where it folds toward its front.
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
    /// A time step whose solve does not converge retries as two of half its length, down to
    /// the solver's shortest time step.
    ///
    /// Returns:
    ///     What the step did, with whether it reached its tolerance.
    ///
    /// Raises:
    ///     SingularError: When some motion costs no energy, so the step has no unique result.
    ///     ValueError: When paper starts closer than its thickness to itself or to the table.
    fn step(&mut self, py: Python<'_>) -> PyResult<PyReport> {
        let simulation = &mut self.0;
        let report = py
            .detach(|| simulation.step())
            .map_err(|error| match error {
                StepError::Singular(_) => SingularError::new_err(error.to_string()),
                StepError::Overlapping => PyValueError::new_err(error.to_string()),
            })?;
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

    fn check_body(&self, body: usize) -> PyResult<()> {
        let count = self.0.bodies().len();
        if body < count {
            Ok(())
        } else {
            Err(PyIndexError::new_err(format!(
                "the simulation has {count} bodies, so no body {body}"
            )))
        }
    }

    /// Vertices of a sheet, from a vertex or an integer array of them.
    fn vertices(&self, sheet: usize, vertices: &Bound<'_, PyAny>) -> PyResult<Vec<usize>> {
        self.check(sheet)?;
        let count = self.0.positions(sheet).len();
        entries("vertices", vertices)?
            .into_iter()
            .map(|vertex| {
                usize::try_from(vertex)
                    .ok()
                    .filter(|&v| v < count)
                    .ok_or_else(|| {
                        PyIndexError::new_err(format!(
                            "the sheet has {count} vertices, so no vertex {vertex}"
                        ))
                    })
            })
            .collect()
    }

    /// Vertices of a sheet, each with its row of `rows`, named `name`.
    fn pairs(
        &self,
        sheet: usize,
        vertices: &Bound<'_, PyAny>,
        name: &str,
        values: &Bound<'_, PyAny>,
    ) -> PyResult<Vec<(usize, [f64; 3])>> {
        let vertices = self.vertices(sheet, vertices)?;
        let values = rows::<f64, 3>(name, values)?;
        if vertices.len() != values.len() {
            return Err(PyValueError::new_err(format!(
                "{} vertices were given, but {} {name}",
                vertices.len(),
                values.len()
            )));
        }
        Ok(vertices.into_iter().zip(values).collect())
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
    use super::{PyEnergy, PyMesh, PyReport, PySheet, PySimulation, SingularError};
}
