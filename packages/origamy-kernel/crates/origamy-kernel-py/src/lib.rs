//! Python bindings of origamy-kernel, as the module `origamy_kernel._core`.
//!
//! Numbers cross as their numerators and denominator, points as two numbers, and
//! places as six: the matrix row by row, then the translation. The doc comments are
//! the Python docstrings, so they follow the Google style.

#![allow(
    clippy::doc_markdown,
    reason = "the doc comments are Python docstrings, where Google style sections name exceptions bare"
)]

use std::sync::OnceLock;

use num_bigint::BigInt;
use origamy_kernel::collapse::{
    CollapseError, Collapsed, Kind, Paper, collapse as collapse_paper, kinds as kinds_of,
    largest as largest_face,
};
use origamy_kernel::crease::{CreaseError, crease as crease_paper};
use origamy_kernel::field::{Number, Tower};
use origamy_kernel::folded::{Folded, Pair, creases as creases_of, edges as edges_of};
use origamy_kernel::geometry::{Isometry, Point};
use origamy_kernel::int::Int;
use origamy_kernel::lines::{Along, along as creases_along};
use origamy_kernel::snap::{Bounds, explain as explain_float};
use origamy_kernel::stacking::{
    Constraint, Problem, StackError, stack as solve, violations as broken,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use rayon::prelude::*;

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

fn check_faces(vertices: usize, corners: &[Vec<u32>], places: usize) -> PyResult<()> {
    let count = u32::try_from(vertices).unwrap_or(u32::MAX);
    let malformed = |face: &Vec<u32>| face.len() < 3 || face.iter().any(|&v| v >= count);
    if corners.is_empty() || corners.len() != places || corners.iter().any(malformed) {
        return Err(PyValueError::new_err(
            "the paper needs a face, and each face a place and three vertices of the crease pattern",
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

/// The exact number that explains each float best: a fraction, from the continued
/// fraction, or `(p + q√r) / d` for small integers, from lattice reduction. The floats
/// find theirs on every core.
///
/// Args:
///     values: The floats to explain.
///     tolerance: How far a candidate may lie from its float.
///     sure: The fit under which a fraction explains a float surely, so that no root
///         is tried.
///     denominator: The largest denominator of a fraction.
///     roots: The integers whose square roots a candidate may hold.
///     scale: The weight of a relation's error against its integers.
///     size: The largest integer of a candidate with a root.
///
/// Returns:
///     For each float, the best candidate's fit, its rational part and its root's
///     coefficient as fractions in lowest terms, and the integer under the root, 1 for
///     a fraction; or `None` when no candidate lies within the tolerance.
///
/// Raises:
///     ValueError: A value is not finite, or a root, the scale or the size is not
///         positive.
#[pyfunction]
#[allow(
    clippy::too_many_arguments,
    reason = "each bound is a documented input"
)]
#[allow(clippy::needless_pass_by_value)]
fn explain(
    py: Python<'_>,
    values: Vec<f64>,
    tolerance: f64,
    sure: f64,
    denominator: u64,
    roots: Vec<u32>,
    scale: i64,
    size: i64,
) -> PyResult<Vec<Option<Explained>>> {
    if values.iter().any(|v| !v.is_finite()) || roots.contains(&0) || scale <= 0 || size <= 0 {
        return Err(PyValueError::new_err(
            "explaining needs finite values, and positive roots, scale and size",
        ));
    }
    let bounds = Bounds {
        tolerance,
        sure,
        denominator,
        roots,
        scale,
        size,
    };
    Ok(py.detach(|| {
        values
            .par_iter()
            .map(|&value| {
                explain_float(value, &bounds)
                    .map(|found| (found.fit, found.rational, found.surd, found.root))
            })
            .collect()
    }))
}

/// What explains a float: its fit, its rational part and its root's coefficient, and
/// the integer under the root.
type Explained = (f64, (BigInt, BigInt), (BigInt, BigInt), u32);

/// What a stacking came to: an outcome's name, the stacking, and the facts of a
/// refusal.
type StackData = (String, Vec<Pair>, Vec<ConstraintData>, Vec<Pair>);

/// What a collapse came to: an outcome's name, the paper after with its stacking, the
/// faces moved, and the facts of a refusal.
type CollapseData = (
    String,
    Option<State>,
    Vec<u32>,
    Option<(u32, usize, usize, f64, f64)>,
    Vec<ConstraintData>,
    Vec<Pair>,
);

/// What creasing came to: an outcome's name, the paper after, and the segment that runs
/// off the paper, if one does.
type CreaseData = (String, Option<State>, Option<usize>);

/// A state of the paper as the kernel holds it: its crease pattern, each face's place,
/// and its stacking.
///
/// The edges and creases come in the order `origamy.state.State` gives them. The kernel
/// folds the faces the first time a question needs them, and keeps them.
///
/// Args:
///     radicands: The tower's radicands, level 1 first.
///     vertices: The crease pattern's vertices.
///     corners: Each face's vertices, counterclockwise on the crease pattern.
///     places: Each face's place.
///     stacking: Which face is above wherever two overlap, as `(upper, lower)`.
///
/// Raises:
///     ValueError: A number or a face is malformed, or there is no face.
#[pyclass(frozen, module = "origamy_kernel._core")]
struct State {
    tower: Tower,
    vertices: Vec<Point>,
    corners: Vec<Vec<u32>>,
    places: Vec<Isometry>,
    creases: Vec<([u32; 2], [u32; 2])>,
    above: Vec<Pair>,
    folded: OnceLock<Folded>,
}

impl State {
    /// The faces folded, the first time a question needs them.
    fn folded(&self) -> &Folded {
        self.folded.get_or_init(|| {
            Folded::new(
                self.tower.clone(),
                self.vertices.clone(),
                self.corners.clone(),
                self.places.clone(),
                self.creases.clone(),
            )
        })
    }

    /// The state of paper the kernel has folded and stacked.
    fn of(folded: Folded, above: Vec<Pair>) -> State {
        State {
            tower: folded.tower.clone(),
            vertices: folded.vertices.clone(),
            corners: folded.corners.clone(),
            places: folded.places.clone(),
            creases: folded.creases.clone(),
            above,
            folded: OnceLock::from(folded),
        }
    }
}

/// Points from data, in a tower.
fn segments_of(data: Vec<(PointData, PointData)>, tower: &Tower) -> PyResult<Vec<(Point, Point)>> {
    data.into_iter()
        .map(|((ax, ay), (bx, by))| {
            Ok((
                Point::new(number(ax)?, number(ay)?, tower),
                Point::new(number(bx)?, number(by)?, tower),
            ))
        })
        .collect()
}

#[pymethods]
impl State {
    #[new]
    #[allow(clippy::needless_pass_by_value)]
    fn new(
        radicands: Vec<NumberData>,
        vertices: Vec<PointData>,
        corners: Vec<Vec<u32>>,
        places: Vec<[NumberData; 6]>,
        stacking: Vec<Pair>,
    ) -> PyResult<State> {
        let tower = tower(radicands)?;
        let vertices = points(vertices, &tower)?;
        let places = self::places(places)?;
        check_faces(vertices.len(), &corners, places.len())?;
        let creases = creases_of(&corners);
        Ok(State {
            tower,
            vertices,
            corners,
            places,
            creases,
            above: stacking,
            folded: OnceLock::new(),
        })
    }

    /// The crease pattern's vertices.
    fn vertices(&self) -> Vec<PointData> {
        self.vertices.iter().map(point_data).collect()
    }

    /// Each face's vertices, counterclockwise on the crease pattern.
    fn corners(&self) -> Vec<Vec<u32>> {
        self.corners.clone()
    }

    /// Each face's place.
    fn places(&self) -> Vec<[NumberData; 6]> {
        self.places.iter().map(place_data).collect()
    }

    /// Each crease's two vertices, the lesser first, and two faces.
    fn creases(&self) -> Vec<(Pair, Pair)> {
        self.creases
            .iter()
            .map(|&([u, v], [f, g])| ((u, v), (f, g)))
            .collect()
    }

    /// Each edge's two vertices, the lesser first, and the faces it borders: two along a
    /// crease, one along the paper's edge.
    fn edges(&self) -> Vec<(Pair, Vec<u32>)> {
        edges_of(&self.corners)
            .into_iter()
            .map(|([u, v], faces)| ((u, v), faces))
            .collect()
    }

    /// The stacking, as `(upper, lower)`.
    fn stacking(&self) -> Vec<Pair> {
        self.above.clone()
    }

    /// Each crease's kind now, as seen from the paper's front: `M` or `V` folded, `F`
    /// flat.
    ///
    /// A fold is a valley when the face under it shows the paper's front.
    fn assignments(&self, py: Python<'_>) -> String {
        py.detach(|| {
            kinds_of(&self.tower, &self.places, &self.creases, &self.above)
                .into_iter()
                .map(|kind| match kind {
                    Kind::Mountain => 'M',
                    Kind::Valley => 'V',
                    Kind::Flat => 'F',
                })
                .collect()
        })
    }

    /// Crease the open paper along segments, folding nothing.
    ///
    /// Every face keeps the first face's place. Faces follow the edges in order, and
    /// the new vertices follow the paper's, as each segment meets them from its start.
    ///
    /// Args:
    ///     outline: The paper's corners, counterclockwise.
    ///     segments: The creases to add, each as two points.
    ///
    /// Returns:
    ///     `("creased", state, None)`, its vertices the paper's then those added, or
    ///     `("off-paper", None, k)` when segment k runs off the paper, `("loose", None,
    ///     None)` when a face borders a crease twice, or `("concave", None, None)` when a
    ///     face is not convex.
    ///
    /// Raises:
    ///     ValueError: A number is malformed.
    #[allow(clippy::needless_pass_by_value)]
    fn crease(
        &self,
        py: Python<'_>,
        outline: Vec<PointData>,
        segments: Vec<(PointData, PointData)>,
    ) -> PyResult<CreaseData> {
        let tower = &self.tower;
        let outline = points(outline, tower)?;
        let segments = segments_of(segments, tower)?;
        let edges: Vec<[u32; 2]> = edges_of(&self.corners)
            .into_iter()
            .map(|(edge, _)| edge)
            .collect();
        Ok(py.detach(
            || match crease_paper(tower, &self.vertices, &edges, &outline, &segments) {
                Ok((all, faces)) => {
                    let state = State {
                        tower: tower.clone(),
                        vertices: all,
                        places: vec![self.places[0].clone(); faces.len()],
                        creases: creases_of(&faces),
                        corners: faces,
                        above: vec![],
                        folded: OnceLock::new(),
                    };
                    ("creased".to_owned(), Some(state), None)
                }
                Err(CreaseError::OffPaper(k)) => ("off-paper".to_owned(), None, Some(k)),
                Err(CreaseError::Loose) => ("loose".to_owned(), None, None),
                Err(CreaseError::Concave) => ("concave".to_owned(), None, None),
            },
        ))
    }

    /// The creases along each line or segment of the crease pattern.
    ///
    /// Args:
    ///     queries: Each line as two of its points, or segment as its two ends, and
    ///         whether it is a segment.
    ///
    /// Returns:
    ///     The creases along each, by their places among the creases, in order.
    ///
    /// Raises:
    ///     ValueError: A number is malformed.
    #[allow(clippy::needless_pass_by_value)]
    fn along(
        &self,
        py: Python<'_>,
        queries: Vec<(PointData, PointData, bool)>,
    ) -> PyResult<Vec<Vec<u32>>> {
        let tower = &self.tower;
        let point = |(x, y): PointData| -> PyResult<Point> {
            Ok(Point::new(number(x)?, number(y)?, tower))
        };
        let queries = queries
            .into_iter()
            .map(|(a, b, segment)| {
                let (p, q) = (point(a)?, point(b)?);
                Ok(if segment {
                    Along::Segment(p, q)
                } else {
                    Along::Line(p, q)
                })
            })
            .collect::<PyResult<Vec<_>>>()?;
        let edges: Vec<[u32; 2]> = self.creases.iter().map(|&(edge, _)| edge).collect();
        Ok(py.detach(|| creases_along(tower, &self.vertices, &edges, &queries)))
    }

    /// The face of largest area on the crease pattern, the first of them when several
    /// tie.
    fn largest(&self, py: Python<'_>) -> u32 {
        py.detach(|| largest_face(&self.vertices, &self.corners, &self.tower))
    }

    /// Every pair of faces that share area once folded, the lesser first, sorted.
    fn overlapping(&self, py: Python<'_>) -> Vec<Pair> {
        py.detach(|| self.folded().overlapping.clone())
    }

    /// The most layers of paper at one point once folded, and a point inside them.
    fn thickest(&self, py: Python<'_>) -> (usize, PointData) {
        let (most, point) = py.detach(|| self.folded().thickest());
        (most, point_data(&point))
    }

    /// Find a stacking of the folded faces: `(upper, lower)` for every overlapping
    /// pair.
    ///
    /// Args:
    ///     fixed: The orders required, as `(upper, lower)`.
    ///     prefer: The orders to try first, as `(upper, lower)`.
    ///     budget: The dead ends the search may meet before it gives up.
    ///
    /// Returns:
    ///     `("stacked", pairs, [], [])` with every pair's order, `("none", [],
    ///     constraints, fixed)` when no stacking exists, or `("budget", [], [], [])`
    ///     when the search ran out of budget.
    #[allow(clippy::needless_pass_by_value)]
    fn stack(&self, py: Python<'_>, fixed: Vec<Pair>, prefer: Vec<Pair>, budget: i64) -> StackData {
        py.detach(|| stack_data(solve(&Problem::new(self.folded()), &fixed, &prefer, budget)))
    }

    /// The constraints a stacking breaks: clauses it meets with no order, and three
    /// faces of a region in a cycle, one cycle for each region.
    ///
    /// Args:
    ///     above: The stacking, as `(upper, lower)`.
    ///
    /// Returns:
    ///     Each constraint broken: its kind, its faces, and its crease if any.
    #[allow(clippy::needless_pass_by_value)]
    fn violations(&self, py: Python<'_>, above: Vec<Pair>) -> Vec<ConstraintData> {
        py.detach(|| {
            broken(&Problem::new(self.folded()), &above)
                .into_iter()
                .map(constraint_data)
                .collect()
        })
    }

    /// Set creases to mountain, valley or flat at once, and fold the paper to match.
    ///
    /// Args:
    ///     target: Each crease's kind after: `M`, `V` or `F`.
    ///     fixed: The face that stays.
    ///     budget: The dead ends the layer solver may meet before it gives up.
    ///
    /// Returns:
    ///     `("collapsed", state, moved, None, [], [])`, with the paper after as a state
    ///     that holds its stacking; or for a refusal, `("count", None, [], (vertex,
    ///     mountains, valleys, 0, 0), [], [])`, `("angles", None, [], (vertex, 0, 0,
    ///     odd, even), [], [])`, `("none", None, [], None, constraints, fixed)`, or
    ///     `("budget", None, [], None, [], [])`.
    ///
    /// Raises:
    ///     ValueError: A kind is malformed, a crease lacks one, or the fixed face is no
    ///         face.
    #[allow(clippy::needless_pass_by_value)]
    fn collapse(
        &self,
        py: Python<'_>,
        target: String,
        fixed: u32,
        budget: i64,
    ) -> PyResult<CollapseData> {
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
        if kinds.len() != self.creases.len() || fixed as usize >= self.corners.len() {
            return Err(PyValueError::new_err(
                "each crease needs a kind, and the fixed face must be a face",
            ));
        }
        let border: Vec<[u32; 2]> = edges_of(&self.corners)
            .into_iter()
            .filter(|(_, faces)| faces.len() == 1)
            .map(|(edge, _)| edge)
            .collect();
        let paper = Paper {
            tower: self.tower.clone(),
            vertices: self.vertices.clone(),
            corners: self.corners.clone(),
            places: self.places.clone(),
            creases: self.creases.clone(),
            border,
            above: self.above.clone(),
        };
        let refused = |name: &str, facts| (name.to_owned(), None, vec![], facts, vec![], vec![]);
        Ok(
            py.detach(|| match collapse_paper(&paper, &kinds, fixed, budget) {
                Ok(Collapsed {
                    folded,
                    above,
                    moved,
                }) => (
                    "collapsed".to_owned(),
                    Some(State::of(folded, above)),
                    moved,
                    None,
                    vec![],
                    vec![],
                ),
                Err(CollapseError::Count {
                    vertex,
                    mountains,
                    valleys,
                }) => refused("count", Some((vertex, mountains, valleys, 0.0, 0.0))),
                Err(CollapseError::Angles {
                    vertex,
                    sums: (odd, even),
                }) => refused("angles", Some((vertex, 0, 0, odd, even))),
                Err(CollapseError::Stack(error)) => {
                    let (name, _, constraints, conflict) = stack_data(Err(error));
                    (name, None, vec![], None, constraints, conflict)
                }
            }),
        )
    }
}

/// The exact kernel of origamy.
#[pymodule]
fn _core(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<State>()?;
    module.add_function(wrap_pyfunction!(explain, module)?)?;
    Ok(())
}
