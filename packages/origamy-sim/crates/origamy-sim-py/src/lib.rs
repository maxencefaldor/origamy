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
    Element, IntoPyArray, PyArray2, PyArrayMethods, PyUntypedArray, PyUntypedArrayMethods,
};
use origamy_sim::{Mesh, Vertex};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

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
#[derive(PartialEq)]
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

/// The compiled core of origamy-sim.
#[pymodule]
mod _core {
    #[pymodule_export]
    use super::PyMesh;
}
