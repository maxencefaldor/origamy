//! Python bindings of origamy-kernel, as the module `origamy_kernel._core`.
//!
//! Numbers cross as their numerators and denominator, points as two numbers, and
//! places as six: the matrix row by row, then the translation. The doc comments are
//! the Python docstrings, so they follow the Google style.

#![allow(
    clippy::doc_markdown,
    reason = "the doc comments are Python docstrings, where Google style sections name exceptions bare"
)]

use num_bigint::BigInt;
use origamy_kernel::collapse::{
    CollapseError, Collapsed, Kind, Paper, collapse as collapse_paper, largest as largest_face,
};
use origamy_kernel::crease::{CreaseError, crease as crease_paper};
use origamy_kernel::field::{Number, Tower};
use origamy_kernel::geometry::{Isometry, Point};
use origamy_kernel::int::Int;
use origamy_kernel::lines::{Along, along as creases_along};
use origamy_kernel::sheet::{Pair, Sheet};
use origamy_kernel::stacking::{
    Constraint, Problem, StackError, stack as solve, violations as broken,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// An integer as Python gives it or takes it: 64 bits wide when it fits, which
/// crosses fastest, and of any size otherwise.
struct Integer(Int);

impl<'py> FromPyObject<'_, 'py> for Integer {
    type Error = PyErr;

    fn extract(object: Borrowed<'_, 'py, PyAny>) -> PyResult<Self> {
        if let Ok(small) = object.extract::<i64>() {
            return Ok(Integer(Int::from(small)));
        }
        Ok(Integer(Int::from_big(object.extract::<BigInt>()?)))
    }
}

impl<'py> IntoPyObject<'py> for Integer {
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> PyResult<Self::Output> {
        match self.0 {
            Int::Small(value) => Ok(value.into_pyobject(py)?.into_any()),
            Int::Big(value) => Ok((*value).into_pyobject(py)?.into_any()),
        }
    }
}

/// A number as Python gives it: its numerators and its denominator.
type NumberData = (Vec<Integer>, Integer);

/// A point as two numbers.
type PointData = (NumberData, NumberData);

/// A constraint as Python reads it: its kind, its faces, and its crease if any.
type ConstraintData = (String, Vec<u32>, Option<u32>);

fn number(data: NumberData) -> PyResult<Number> {
    let (numerators, Integer(denominator)) = data;
    if !numerators.len().is_power_of_two() {
        return Err(PyValueError::new_err(
            "a number has a power of two of numerators",
        ));
    }
    if denominator.is_zero() {
        return Err(PyValueError::new_err(
            "a number's denominator is never zero",
        ));
    }
    Ok(Number::new(
        numerators.into_iter().map(|Integer(n)| n).collect(),
        denominator,
    ))
}

fn number_data(number: &Number) -> NumberData {
    (
        number.numerators().iter().cloned().map(Integer).collect(),
        Integer(number.denominator().clone()),
    )
}

fn tower(radicands: Vec<NumberData>) -> PyResult<Tower> {
    Ok(Tower::new(
        radicands.into_iter().map(number).collect::<PyResult<_>>()?,
    ))
}

fn points(data: Vec<PointData>, tower: &Tower) -> PyResult<Vec<Point>> {
    data.into_iter()
        .map(|(x, y)| Ok(Point::new(number(x)?, number(y)?, tower)))
        .collect()
}

fn point_data(point: &Point) -> PointData {
    (number_data(&point.x), number_data(&point.y))
}

fn places(data: Vec<[NumberData; 6]>) -> PyResult<Vec<Isometry>> {
    data.into_iter()
        .map(|entries| {
            let numbers = entries
                .into_iter()
                .map(number)
                .collect::<PyResult<Vec<_>>>()?;
            Ok(Isometry {
                entries: numbers.try_into().expect("six entries"),
            })
        })
        .collect()
}

fn place_data(place: &Isometry) -> [NumberData; 6] {
    place.entries.each_ref().map(number_data)
}

fn check_faces(
    vertices: usize,
    corners: &[Vec<u32>],
    places: usize,
    creases: &[([u32; 2], [u32; 2])],
) -> PyResult<()> {
    let count = u32::try_from(vertices).unwrap_or(u32::MAX);
    if corners.iter().flatten().any(|&v| v >= count) || corners.len() != places {
        return Err(PyValueError::new_err(
            "each face needs a place and vertices of the crease pattern",
        ));
    }
    let faces = u32::try_from(corners.len()).unwrap_or(u32::MAX);
    if creases
        .iter()
        .any(|(edge, two)| edge.iter().any(|&v| v >= count) || two.iter().any(|&f| f >= faces))
    {
        return Err(PyValueError::new_err(
            "each crease needs vertices and faces of the paper",
        ));
    }
    Ok(())
}

fn constraint_data(constraint: Constraint) -> ConstraintData {
    (
        constraint.kind.to_owned(),
        constraint.faces,
        constraint.crease,
    )
}

fn stack_data(
    result: Result<Vec<Pair>, StackError>,
) -> (String, Vec<Pair>, Vec<ConstraintData>, Vec<Pair>) {
    match result {
        Ok(pairs) => ("stacked".to_owned(), pairs, vec![], vec![]),
        Err(StackError::Budget) => ("budget".to_owned(), vec![], vec![], vec![]),
        Err(StackError::NoStacking { constraints, fixed }) => (
            "none".to_owned(),
            vec![],
            constraints.into_iter().map(constraint_data).collect(),
            fixed,
        ),
    }
}

/// Find a stacking of folded faces: `(upper, lower)` for every overlapping pair.
///
/// Args:
///     radicands: The tower's radicands, level 1 first.
///     vertices: The crease pattern's vertices.
///     corners: Each face's vertices, counterclockwise on the crease pattern.
///     places: Each face's place.
///     creases: Each crease's two vertices and two faces.
///     fixed: The orders required, as `(upper, lower)`.
///     prefer: The orders to try first, as `(upper, lower)`.
///     budget: The dead ends the search may meet before it gives up.
///
/// Returns:
///     `("stacked", pairs, [], [])` with every pair's order, `("none", [], constraints,
///     fixed)` when no stacking exists, or `("budget", [], [], [])` when the search ran
///     out of budget.
///
/// Raises:
///     ValueError: A number, a face or a crease is malformed.
#[pyfunction]
#[pyo3(signature = (radicands, vertices, corners, places, creases, fixed, prefer, budget))]
#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::needless_pass_by_value
)]
fn stack(
    py: Python<'_>,
    radicands: Vec<NumberData>,
    vertices: Vec<PointData>,
    corners: Vec<Vec<u32>>,
    places: Vec<[NumberData; 6]>,
    creases: Vec<([u32; 2], [u32; 2])>,
    fixed: Vec<Pair>,
    prefer: Vec<Pair>,
    budget: i64,
) -> PyResult<(String, Vec<Pair>, Vec<ConstraintData>, Vec<Pair>)> {
    let tower = tower(radicands)?;
    let vertices = points(vertices, &tower)?;
    let places = self::places(places)?;
    check_faces(vertices.len(), &corners, places.len(), &creases)?;
    Ok(py.detach(|| {
        let sheet = Sheet::new(tower, vertices, corners, places, creases);
        let problem = Problem::new(&sheet);
        stack_data(solve(&problem, &fixed, &prefer, budget))
    }))
}

/// The constraints a stacking breaks: clauses it meets with no order, and three faces
/// of a region in a cycle, one cycle for each region.
///
/// Args:
///     radicands: The tower's radicands, level 1 first.
///     vertices: The crease pattern's vertices.
///     corners: Each face's vertices, counterclockwise on the crease pattern.
///     places: Each face's place.
///     creases: Each crease's two vertices and two faces.
///     above: The stacking, as `(upper, lower)`.
///
/// Returns:
///     Each constraint broken: its kind, its faces, and its crease if any.
///
/// Raises:
///     ValueError: A number, a face or a crease is malformed.
#[pyfunction]
#[allow(clippy::needless_pass_by_value)]
fn violations(
    py: Python<'_>,
    radicands: Vec<NumberData>,
    vertices: Vec<PointData>,
    corners: Vec<Vec<u32>>,
    places: Vec<[NumberData; 6]>,
    creases: Vec<([u32; 2], [u32; 2])>,
    above: Vec<Pair>,
) -> PyResult<Vec<ConstraintData>> {
    let tower = tower(radicands)?;
    let vertices = points(vertices, &tower)?;
    let places = self::places(places)?;
    check_faces(vertices.len(), &corners, places.len(), &creases)?;
    Ok(py.detach(|| {
        let sheet = Sheet::new(tower, vertices, corners, places, creases);
        broken(&Problem::new(&sheet), &above)
            .into_iter()
            .map(constraint_data)
            .collect()
    }))
}

/// Crease open paper along segments: its new vertices and its faces after.
///
/// Args:
///     radicands: The tower's radicands, level 1 first.
///     vertices: The paper's vertices.
///     edges: The paper's edges, each as two vertices.
///     outline: The paper's corners, counterclockwise.
///     segments: The creases to add, each as two points.
///
/// Returns:
///     `("creased", added, faces, None)`, with the vertices added after the paper's, or
///     `("off-paper", [], [], k)` when segment k runs off the paper, `("loose", [], [],
///     None)` when a face borders a crease twice, or `("concave", [], [], None)` when a
///     face is not convex.
///
/// Raises:
///     ValueError: A number or an edge is malformed.
#[pyfunction]
#[allow(clippy::type_complexity, clippy::needless_pass_by_value)]
fn crease(
    py: Python<'_>,
    radicands: Vec<NumberData>,
    vertices: Vec<PointData>,
    edges: Vec<[u32; 2]>,
    outline: Vec<PointData>,
    segments: Vec<(PointData, PointData)>,
) -> PyResult<(String, Vec<PointData>, Vec<Vec<u32>>, Option<usize>)> {
    let tower = tower(radicands)?;
    let vertices = points(vertices, &tower)?;
    let count = u32::try_from(vertices.len()).unwrap_or(u32::MAX);
    if edges.iter().flatten().any(|&v| v >= count) {
        return Err(PyValueError::new_err(
            "each edge needs vertices of the paper",
        ));
    }
    let outline = points(outline, &tower)?;
    let segments = segments
        .into_iter()
        .map(|((ax, ay), (bx, by))| {
            Ok((
                Point::new(number(ax)?, number(ay)?, &tower),
                Point::new(number(bx)?, number(by)?, &tower),
            ))
        })
        .collect::<PyResult<Vec<_>>>()?;
    Ok(py.detach(
        || match crease_paper(&tower, &vertices, &edges, &outline, &segments) {
            Ok((all, faces)) => (
                "creased".to_owned(),
                all[vertices.len()..].iter().map(point_data).collect(),
                faces,
                None,
            ),
            Err(CreaseError::OffPaper(k)) => ("off-paper".to_owned(), vec![], vec![], Some(k)),
            Err(CreaseError::Loose) => ("loose".to_owned(), vec![], vec![], None),
            Err(CreaseError::Concave) => ("concave".to_owned(), vec![], vec![], None),
        },
    ))
}

/// The creases along each line or segment of the crease pattern.
///
/// Args:
///     radicands: The tower's radicands, level 1 first.
///     vertices: The crease pattern's vertices.
///     edges: The creases, each as two vertices.
///     queries: Each line or segment as two points, and whether it is a segment.
///
/// Returns:
///     The creases along each, by their indices in `edges`, in order.
///
/// Raises:
///     ValueError: A number or an edge is malformed.
#[pyfunction]
#[allow(clippy::needless_pass_by_value)]
fn along(
    py: Python<'_>,
    radicands: Vec<NumberData>,
    vertices: Vec<PointData>,
    edges: Vec<[u32; 2]>,
    queries: Vec<(PointData, PointData, bool)>,
) -> PyResult<Vec<Vec<u32>>> {
    let tower = tower(radicands)?;
    let vertices = points(vertices, &tower)?;
    let count = u32::try_from(vertices.len()).unwrap_or(u32::MAX);
    if edges
        .iter()
        .any(|&[u, v]| u >= count || v >= count || u == v)
    {
        return Err(PyValueError::new_err(
            "each crease needs two vertices of the crease pattern",
        ));
    }
    let queries = queries
        .into_iter()
        .map(|((ax, ay), (bx, by), segment)| {
            let (p, q) = (
                Point::new(number(ax)?, number(ay)?, &tower),
                Point::new(number(bx)?, number(by)?, &tower),
            );
            Ok(if segment {
                Along::Segment(p, q)
            } else {
                Along::Line(p, q)
            })
        })
        .collect::<PyResult<Vec<_>>>()?;
    Ok(py.detach(|| creases_along(&tower, &vertices, &edges, &queries)))
}

/// The face of largest area on the crease pattern, the first of them when several tie.
///
/// Args:
///     radicands: The tower's radicands, level 1 first.
///     vertices: The crease pattern's vertices.
///     corners: Each face's vertices, counterclockwise on the crease pattern.
///
/// Returns:
///     The face's index.
///
/// Raises:
///     ValueError: A number or a face is malformed, or there is no face.
#[pyfunction]
#[allow(clippy::needless_pass_by_value)]
fn largest(
    py: Python<'_>,
    radicands: Vec<NumberData>,
    vertices: Vec<PointData>,
    corners: Vec<Vec<u32>>,
) -> PyResult<u32> {
    let tower = tower(radicands)?;
    let vertices = points(vertices, &tower)?;
    let count = u32::try_from(vertices.len()).unwrap_or(u32::MAX);
    if corners.is_empty() || corners.iter().flatten().any(|&v| v >= count) {
        return Err(PyValueError::new_err(
            "each face needs vertices of the crease pattern, and there is a face",
        ));
    }
    Ok(py.detach(|| largest_face(&vertices, &corners, &tower)))
}

/// What a collapse came to: an outcome's name, the places and stacking, the faces
/// moved, and the facts of a refusal.
type CollapseData = (
    String,
    Vec<[NumberData; 6]>,
    Vec<Pair>,
    Vec<u32>,
    Option<(u32, usize, usize, f64, f64)>,
    Vec<ConstraintData>,
    Vec<Pair>,
);

/// Set creases to mountain, valley or flat at once, and fold the paper to match.
///
/// Args:
///     radicands: The tower's radicands, level 1 first.
///     vertices: The crease pattern's vertices.
///     corners: Each face's vertices, counterclockwise on the crease pattern.
///     places: Each face's place.
///     creases: Each crease's two vertices, the lesser first, and two faces.
///     border: The edges along the paper's own edge.
///     above: The paper's stacking, as `(upper, lower)`.
///     target: Each crease's kind after: `M`, `V` or `F`.
///     fixed: The face that stays.
///     budget: The dead ends the layer solver may meet before it gives up.
///
/// Returns:
///     `("collapsed", places, above, moved, None, [], [])`; or for a refusal,
///     `("count", ..., (vertex, mountains, valleys, 0, 0), ...)`, `("angles", ...,
///     (vertex, 0, 0, odd, even), ...)`, `("none", ..., constraints, fixed)`, or
///     `("budget", ...)`.
///
/// Raises:
///     ValueError: A number, a face, a crease or a kind is malformed.
#[pyfunction]
#[pyo3(signature = (radicands, vertices, corners, places, creases, border, above, target, fixed, budget))]
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
fn collapse(
    py: Python<'_>,
    radicands: Vec<NumberData>,
    vertices: Vec<PointData>,
    corners: Vec<Vec<u32>>,
    places: Vec<[NumberData; 6]>,
    creases: Vec<([u32; 2], [u32; 2])>,
    border: Vec<[u32; 2]>,
    above: Vec<Pair>,
    target: String,
    fixed: u32,
    budget: i64,
) -> PyResult<CollapseData> {
    let tower = tower(radicands)?;
    let vertices = points(vertices, &tower)?;
    let places = self::places(places)?;
    check_faces(vertices.len(), &corners, places.len(), &creases)?;
    let kinds = target
        .chars()
        .map(|c| match c {
            'M' => Ok(Kind::Mountain),
            'V' => Ok(Kind::Valley),
            'F' => Ok(Kind::Flat),
            _ => Err(PyValueError::new_err(format!(
                "a crease is M, V or F, not {c}"
            ))),
        })
        .collect::<PyResult<Vec<_>>>()?;
    if kinds.len() != creases.len() || fixed as usize >= corners.len() {
        return Err(PyValueError::new_err(
            "each crease needs a kind, and the fixed face must be a face",
        ));
    }
    let paper = Paper {
        tower,
        vertices,
        corners,
        places,
        creases,
        border,
        above,
    };
    Ok(
        py.detach(|| match collapse_paper(&paper, &kinds, fixed, budget) {
            Ok(Collapsed {
                places,
                above,
                moved,
            }) => (
                "collapsed".to_owned(),
                places.iter().map(place_data).collect(),
                above,
                moved,
                None,
                vec![],
                vec![],
            ),
            Err(CollapseError::Count {
                vertex,
                mountains,
                valleys,
            }) => (
                "count".to_owned(),
                vec![],
                vec![],
                vec![],
                Some((vertex, mountains, valleys, 0.0, 0.0)),
                vec![],
                vec![],
            ),
            Err(CollapseError::Angles {
                vertex,
                sums: (odd, even),
            }) => (
                "angles".to_owned(),
                vec![],
                vec![],
                vec![],
                Some((vertex, 0, 0, odd, even)),
                vec![],
                vec![],
            ),
            Err(CollapseError::Stack(error)) => {
                let (name, _, constraints, fixed) = stack_data(Err(error));
                (name, vec![], vec![], vec![], None, constraints, fixed)
            }
        }),
    )
}

/// The exact kernel of origamy.
#[pymodule]
fn _core(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(stack, module)?)?;
    module.add_function(wrap_pyfunction!(violations, module)?)?;
    module.add_function(wrap_pyfunction!(crease, module)?)?;
    module.add_function(wrap_pyfunction!(along, module)?)?;
    module.add_function(wrap_pyfunction!(largest, module)?)?;
    module.add_function(wrap_pyfunction!(collapse, module)?)?;
    Ok(())
}
