//! Meshes of planar faces that meet along their edges: a constrained Delaunay triangulation of
//! the edges, refined until its triangles are small and well shaped.
//!
//! Every edge of every face becomes a chain of mesh edges, so a sheet can bend sharply along
//! it. Ruppert's refinement inserts points until no angle falls under a bound and no triangle
//! exceeds an area. The refinement splits the faces' edges where it must, without gaps.
//!
//! Lines inside the faces join the mesh after the refinement, as chains of mesh edges at most
//! the refinement's edge length, with the points that only lines name. Two lines a fraction of
//! a millimeter apart then bound a strip of long thin triangles, where refining would fill the
//! strip with tiny ones.

use spade::handles::FixedVertexHandle;
use spade::{
    AngleLimit, ConstrainedDelaunayTriangulation, Point2, RefinementParameters, Triangulation,
};

/// The triangulation the faces' edges constrain.
type Cdt = ConstrainedDelaunayTriangulation<Point2<f64>>;

use crate::mesh::{Mesh, MeshError, Vertex};
use crate::parameter::{ParameterError, positive, require};

/// How finely a mesh of faces is refined.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Refinement {
    /// The largest area a triangle may have, in square meters.
    pub max_area: f64,
    /// The least angle a triangle may have, in radians.
    ///
    /// Ruppert's refinement ends for bounds up to about 30°, and 20° is the usual choice.
    pub min_angle: f64,
    /// The area under which the refinement leaves a triangle as it is, in square meters.
    ///
    /// Points closer than the edges such triangles span then keep no tiny triangles around
    /// them, at the cost of angles under the bound there. Zero refines every triangle.
    pub min_area: f64,
}

impl Refinement {
    /// Triangles of at most `max_area` square meters, with no angle under 20°.
    #[must_use]
    pub fn of_area(max_area: f64) -> Self {
        Self {
            max_area,
            min_angle: 20f64.to_radians(),
            min_area: 0.0,
        }
    }

    /// Checks that every parameter takes a value it may.
    ///
    /// # Errors
    ///
    /// Returns the first parameter outside its values.
    pub fn check(&self) -> Result<(), ParameterError> {
        positive("max area", self.max_area)?;
        require(
            "min angle",
            self.min_angle,
            self.min_angle > 0.0 && self.min_angle <= 35f64.to_radians(),
            "positive, and at most 35°",
        )?;
        require(
            "min area",
            self.min_area,
            (0.0..self.max_area).contains(&self.min_area),
            "at least zero, and less than the max area",
        )
    }
}

/// Why faces cannot make a mesh.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum FacesError {
    /// A parameter lies outside its values.
    #[error(transparent)]
    Parameter(#[from] ParameterError),
    /// A point lies at a coordinate that is infinite or not a number.
    #[error("point {point} lies at a coordinate that is not finite")]
    NotFinite {
        /// The point.
        point: usize,
    },
    /// Two points coincide.
    #[error("points {0} and {1} coincide", points[0], points[1])]
    Coincident {
        /// The two points.
        points: [usize; 2],
    },
    /// A face has fewer than three corners, or names a point that does not exist.
    #[error("face {face} needs at least 3 corners among the {count} points")]
    Corners {
        /// The face.
        face: usize,
        /// How many points there are.
        count: usize,
    },
    /// A face's edge crosses another face's edge.
    #[error("an edge of face {face} crosses another face's edge")]
    Crossing {
        /// The face.
        face: usize,
    },
    /// A face runs clockwise, or covers part of another face.
    #[error("face {face} runs clockwise or overlaps another face")]
    Overlapping {
        /// The face.
        face: usize,
    },
    /// A line names a point that does not exist, or runs from a point to itself.
    #[error("line {line} needs two distinct points among the {count} points")]
    Line {
        /// The line.
        line: usize,
        /// How many points there are.
        count: usize,
    },
    /// The refinement could not reach its least angle within its budget of points.
    #[error("the refinement stopped short of its least angle")]
    Unrefined,
    /// The refined mesh is not a mesh a sheet can take.
    #[error(transparent)]
    Mesh(#[from] MeshError),
}

/// A mesh of faces, with the face each triangle lies in.
#[derive(Clone, Debug, PartialEq)]
pub struct Faces {
    /// The mesh, whose first vertices are the points, in their order.
    pub mesh: Mesh,
    /// The face each triangle of the mesh lies in.
    pub faces: Vec<u32>,
    /// The vertices along each line, from its first point to its second.
    pub lines: Vec<Vec<Vertex>>,
}

impl Faces {
    /// A mesh of faces that meet along their edges, each a counterclockwise polygon of
    /// `points`, in meters, refined as `refinement` asks.
    ///
    /// The mesh keeps every point as a vertex, in order, and every face's edge as a chain of
    /// mesh edges. Its triangles cover exactly the faces. Each of `lines`, between two points,
    /// then joins the mesh as a chain of mesh edges no longer than the refinement's edge
    /// length, the side of an equilateral triangle of the largest area. The points that no face
    /// names join with the lines, after the refinement. Lines that cross split at the crossing.
    ///
    /// # Errors
    ///
    /// Returns a [`FacesError`] when a refinement parameter lies outside its values, or a
    /// point is not finite. Coincident points and faces with too few corners are refused, as
    /// are faces whose edges cross, or that run clockwise or overlap. A line must join two
    /// distinct points. A refinement that cannot reach its least angle is refused too.
    pub fn new(
        points: &[[f64; 2]],
        faces: &[Vec<Vertex>],
        lines: &[[Vertex; 2]],
        refinement: Refinement,
    ) -> Result<Self, FacesError> {
        refinement.check()?;
        if let Some(line) = lines
            .iter()
            .position(|&[a, b]| a == b || a.max(b) as usize >= points.len())
        {
            return Err(FacesError::Line {
                line,
                count: points.len(),
            });
        }
        let (mut cdt, handles) = triangulate(points, faces)?;
        // The budget lets the refinement fill the faces at the area bound several times over.
        let area: f64 = faces.iter().map(|f| polygon_area(points, f).abs()).sum();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the budget is a positive count, capped"
        )]
        let budget = (8.0 * area / refinement.max_area).min(1e8) as usize + 10 * points.len();
        let result = cdt.refine(
            RefinementParameters::new()
                .with_angle_limit(AngleLimit::from_rad(refinement.min_angle))
                .with_max_allowed_area(refinement.max_area)
                .with_min_required_area(refinement.min_area)
                .with_max_additional_vertices(budget),
        );
        if !result.refinement_complete {
            return Err(FacesError::Unrefined);
        }
        // The points no face names join now, so the refinement leaves them be.
        let (handles, points_of) = insert_rest(&mut cdt, points, &handles)?;
        let length = (4.0 * refinement.max_area / 3f64.sqrt()).sqrt();
        let chains: Vec<Vec<usize>> = lines
            .iter()
            .map(|&[a, b]| join(&mut cdt, handles[a as usize], handles[b as usize], length))
            .collect();
        // Each triangle lies in the face that holds its center, and the rest of the hull
        // drops out.
        let labels = label(&cdt, points, faces)?;
        // The points come first, in their order, then the refinement's points, less any unused.
        let mut used = vec![false; cdt.num_vertices()];
        let mut triangles = Vec::new();
        let mut owners = Vec::new();
        for face in cdt.fixed_inner_faces() {
            if let Some(owner) = labels[face.index()] {
                let corners = cdt.face(face).vertices().map(|v| v.fix().index());
                for &c in &corners {
                    used[c] = true;
                }
                triangles.push(corners);
                owners.push(owner);
            }
        }
        let mut renumber = vec![0; used.len()];
        let mut rest = Vec::new();
        let first = handles.iter().map(FixedVertexHandle::index);
        let others = (0..used.len()).filter(|&v| points_of.get(v).is_none_or(Option::is_none));
        for v in first.chain(others) {
            if used[v] {
                renumber[v] =
                    Vertex::try_from(rest.len()).map_err(|_| MeshError::TooManyVertices {
                        count: cdt.num_vertices(),
                    })?;
                let p = cdt.vertex(FixedVertexHandle::from_index(v)).position();
                rest.push([p.x, p.y]);
            }
        }
        let triangles = triangles
            .into_iter()
            .map(|t| t.map(|v| renumber[v]))
            .collect();
        Ok(Self {
            mesh: Mesh::new(rest, triangles)?,
            faces: owners,
            lines: chains
                .into_iter()
                .map(|chain| chain.into_iter().map(|v| renumber[v]).collect())
                .collect(),
        })
    }
}

/// The constrained Delaunay triangulation of the faces' edges, and the handle of each point a
/// face names.
fn triangulate(
    points: &[[f64; 2]],
    faces: &[Vec<Vertex>],
) -> Result<(Cdt, Vec<Option<FixedVertexHandle>>), FacesError> {
    if let Some(point) = points.iter().position(|p| !p.iter().all(|x| x.is_finite())) {
        return Err(FacesError::NotFinite { point });
    }
    let mut named = vec![false; points.len()];
    for (face, corners) in faces.iter().enumerate() {
        if corners.len() < 3 || corners.iter().any(|&c| c as usize >= points.len()) {
            return Err(FacesError::Corners {
                face,
                count: points.len(),
            });
        }
        for &c in corners {
            named[c as usize] = true;
        }
    }
    let mut cdt = Cdt::new();
    let mut handles = vec![None; points.len()];
    let mut inserted = Vec::new();
    for (point, &[x, y]) in points.iter().enumerate() {
        if !named[point] {
            continue;
        }
        let handle = cdt
            .insert(Point2::new(x, y))
            .map_err(|_| FacesError::NotFinite { point })?;
        if handle.index() < inserted.len() {
            return Err(FacesError::Coincident {
                points: [inserted[handle.index()], point],
            });
        }
        inserted.push(point);
        handles[point] = Some(handle);
    }
    for (face, corners) in faces.iter().enumerate() {
        if polygon_area(points, corners) <= 0.0 {
            return Err(FacesError::Overlapping { face });
        }
        for (k, &a) in corners.iter().enumerate() {
            let b = corners[(k + 1) % corners.len()];
            let (Some(from), Some(to)) = (handles[a as usize], handles[b as usize]) else {
                unreachable!("every corner of a face is a named point");
            };
            if !cdt.can_add_constraint(from, to) {
                return Err(FacesError::Crossing { face });
            }
            cdt.add_constraint(from, to);
        }
    }
    Ok((cdt, handles))
}

/// Inserts the points no face names, whose handles are `None`, after the refinement.
///
/// Returns every point's handle, and the point each vertex of the triangulation is, if any.
fn insert_rest(
    cdt: &mut Cdt,
    points: &[[f64; 2]],
    handles: &[Option<FixedVertexHandle>],
) -> Result<(Vec<FixedVertexHandle>, Vec<Option<usize>>), FacesError> {
    let mut points_of: Vec<Option<usize>> = vec![None; cdt.num_vertices()];
    for (point, handle) in handles.iter().enumerate() {
        if let Some(handle) = handle {
            points_of[handle.index()] = Some(point);
        }
    }
    let mut all = Vec::with_capacity(points.len());
    for (point, handle) in handles.iter().enumerate() {
        if let Some(handle) = handle {
            all.push(*handle);
            continue;
        }
        let [x, y] = points[point];
        let handle = cdt
            .insert(Point2::new(x, y))
            .map_err(|_| FacesError::NotFinite { point })?;
        points_of.resize(cdt.num_vertices(), None);
        if let Some(other) = points_of[handle.index()] {
            return Err(FacesError::Coincident {
                points: [other, point],
            });
        }
        points_of[handle.index()] = Some(point);
        all.push(handle);
    }
    Ok((all, points_of))
}

/// Joins a line from `from` to `to` to the triangulation, as a chain of constraint edges no
/// longer than `length`, and returns the vertices along it.
///
/// The chain's points are spread evenly along the line, and a chain that crosses another
/// splits both at the crossing.
fn join(cdt: &mut Cdt, from: FixedVertexHandle, to: FixedVertexHandle, length: f64) -> Vec<usize> {
    let [a, b] = [from, to].map(|v| cdt.vertex(v).position());
    let span = (b.x - a.x).hypot(b.y - a.y);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a positive count of pieces, from a finite ratio, capped"
    )]
    let pieces = (span / length).ceil().clamp(1.0, 1e6) as u32;
    let mut stops = vec![from];
    for k in 1..pieces {
        let share = f64::from(k) / f64::from(pieces);
        let point = Point2::new(a.x + share * (b.x - a.x), a.y + share * (b.y - a.y));
        stops.push(
            cdt.insert(point)
                .expect("a point between finite points is finite"),
        );
    }
    stops.push(to);
    let mut chain = vec![from.index()];
    for pair in stops.windows(2) {
        for edge in cdt.add_constraint_and_split(pair[0], pair[1], |p| p) {
            chain.push(cdt.directed_edge(edge).to().fix().index());
        }
    }
    chain
}

/// The face each inner face of the triangulation lies in, or `None` outside every face: the face
/// that holds its center.
fn label(
    cdt: &Cdt,
    points: &[[f64; 2]],
    faces: &[Vec<Vertex>],
) -> Result<Vec<Option<u32>>, FacesError> {
    let bounds: Vec<[f64; 4]> = faces
        .iter()
        .map(|corners| {
            corners.iter().fold(
                [
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ],
                |[x0, y0, x1, y1], &c| {
                    let [x, y] = points[c as usize];
                    [x0.min(x), y0.min(y), x1.max(x), y1.max(y)]
                },
            )
        })
        .collect();
    let mut labels: Vec<Option<u32>> = vec![None; cdt.num_all_faces()];
    for triangle in cdt.inner_faces() {
        let [a, b, c] = triangle.positions();
        let center = [(a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0];
        let mut holders = faces.iter().enumerate().filter(|(f, corners)| {
            let [x0, y0, x1, y1] = bounds[*f];
            (x0..=x1).contains(&center[0])
                && (y0..=y1).contains(&center[1])
                && holds(points, corners, center)
        });
        if let Some((face, _)) = holders.next() {
            if let Some((other, _)) = holders.next() {
                return Err(FacesError::Overlapping {
                    face: other.max(face),
                });
            }
            labels[triangle.fix().index()] =
                Some(u32::try_from(face).expect("faces are counted in 32 bits"));
        }
    }
    Ok(labels)
}

/// Whether a polygon of `points` holds a point strictly inside it, by its winding number.
fn holds(points: &[[f64; 2]], corners: &[Vertex], [x, y]: [f64; 2]) -> bool {
    let mut winding = 0;
    for (k, &a) in corners.iter().enumerate() {
        let [x0, y0] = points[a as usize];
        let [x1, y1] = points[corners[(k + 1) % corners.len()] as usize];
        let side = (x1 - x0) * (y - y0) - (x - x0) * (y1 - y0);
        if y0 <= y && y < y1 && side > 0.0 {
            winding += 1;
        } else if y1 <= y && y < y0 && side < 0.0 {
            winding -= 1;
        }
    }
    winding != 0
}

/// The signed area of a polygon of `points`, positive counterclockwise.
fn polygon_area(points: &[[f64; 2]], corners: &[Vertex]) -> f64 {
    let n = corners.len();
    0.5 * (0..n)
        .map(|k| {
            let [x0, y0] = points[corners[k] as usize];
            let [x1, y1] = points[corners[(k + 1) % n] as usize];
            x0 * y1 - x1 * y0
        })
        .sum::<f64>()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit square split along its diagonal into two triangular faces, and a square face.
    fn square() -> (Vec<[f64; 2]>, Vec<Vec<Vertex>>) {
        (
            vec![
                [0.0, 0.0],
                [1.0, 0.0],
                [1.0, 1.0],
                [0.0, 1.0],
                [2.0, 0.0],
                [2.0, 1.0],
            ],
            vec![vec![0, 1, 2], vec![0, 2, 3], vec![1, 4, 5, 2]],
        )
    }

    #[test]
    fn faces_mesh_into_small_well_shaped_triangles_that_follow_their_edges() {
        let (points, faces) = square();
        let refinement = Refinement::of_area(0.01);
        let meshed = Faces::new(&points, &faces, &[], refinement).unwrap();
        let mesh = &meshed.mesh;
        // The points come first, in order.
        assert_eq!(mesh.rest()[..points.len()], points[..]);
        // Each face's triangles cover its area exactly.
        let mut areas = [0.0; 3];
        for (t, &face) in mesh.triangles().iter().zip(&meshed.faces) {
            let [a, b, c] = t.map(|v| mesh.rest()[v as usize]);
            areas[face as usize] +=
                0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]));
            // Every triangle is small and well shaped.
            let sides: Vec<f64> = [[a, b], [b, c], [c, a]]
                .iter()
                .map(|[p, q]| (q[0] - p[0]).hypot(q[1] - p[1]))
                .collect();
            let area = 0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]));
            assert!(
                area <= refinement.max_area * (1.0 + 1e-9),
                "a triangle of area {area}"
            );
            for k in 0..3 {
                // The angle opposite side k, from the law of sines and twice the area.
                let sine = 2.0 * area / (sides[(k + 1) % 3] * sides[(k + 2) % 3]);
                assert!(sine >= refinement.min_angle.sin() - 1e-9 || sine > 0.99);
            }
        }
        for (face, expected) in areas.iter().zip([0.5, 0.5, 1.0]) {
            assert!((face - expected).abs() < 1e-12, "a face covers {face}");
        }
        // Every mesh edge between two faces lies on a face's edge.
        for d in mesh.dihedrals() {
            let [f, g] = d.triangles.map(|t| meshed.faces[t as usize]);
            if f != g {
                let [p, q] = d.edge.map(|v| mesh.rest()[v as usize]);
                let on_diagonal = (p[0] - p[1]).abs() < 1e-12 && (q[0] - q[1]).abs() < 1e-12;
                let on_side = (p[0] - 1.0).abs() < 1e-12 && (q[0] - 1.0).abs() < 1e-12;
                assert!(
                    on_diagonal || on_side,
                    "edge {p:?} {q:?} parts faces {f} and {g}"
                );
            }
        }
    }

    #[test]
    fn lines_join_the_mesh_as_chains_without_refinement_around_them() {
        // The right face of the square has a strip a millimeter wide along its left edge, and
        // two lines crossing at its middle.
        let (mut points, mut faces) = square();
        points.extend([
            [1.001, 0.0],
            [1.002, 0.0],
            [1.25, 0.0],
            [1.75, 0.0],
            [1.001, 1.0],
            [1.002, 1.0],
            [1.25, 1.0],
            [1.75, 1.0],
        ]);
        faces[2] = vec![1, 6, 7, 8, 9, 4, 5, 13, 12, 11, 10, 2];
        let lines = [[6, 10], [7, 11], [8, 13], [9, 12]];
        let refinement = Refinement::of_area(0.01);
        let meshed = Faces::new(&points, &faces, &lines, refinement).unwrap();
        let rest = meshed.mesh.rest();
        let edges = meshed.mesh.edges();
        let length = (4.0 * refinement.max_area / 3f64.sqrt()).sqrt();
        for (chain, [a, b]) in meshed.lines.iter().zip(lines) {
            assert_eq!((chain[0], chain[chain.len() - 1]), (a, b));
            let [p, q] = [a, b].map(|v| rest[v as usize]);
            for pair in chain.windows(2) {
                let edge = [pair[0].min(pair[1]), pair[0].max(pair[1])];
                assert!(edges.binary_search(&edge).is_ok(), "{edge:?} is no edge");
                let [u, v] = [pair[0], pair[1]].map(|w| rest[w as usize]);
                assert!((v[0] - u[0]).hypot(v[1] - u[1]) <= length * (1.0 + 1e-9));
                let off = (q[0] - p[0]) * (v[1] - p[1]) - (q[1] - p[1]) * (v[0] - p[0]);
                assert!(off.abs() < 1e-12, "vertex {v:?} lies off line {a}-{b}");
            }
        }
        // The refinement adds points to the strip only near its ends, where its points lie
        // close together. The crossing lines share their crossing.
        let inside = |&[x, y]: &[f64; 2]| {
            (1.001 + 1e-9..1.002 - 1e-9).contains(&x) && (0.05..0.95).contains(&y)
        };
        assert!(!rest.iter().any(inside));
        let crossing = |chain: &[Vertex]| {
            chain
                .iter()
                .copied()
                .find(|&v| (rest[v as usize][1] - 0.5).abs() < 1e-12)
        };
        assert_eq!(crossing(&meshed.lines[2]), crossing(&meshed.lines[3]));
        assert!(crossing(&meshed.lines[2]).is_some());
        let mut area = 0.0;
        for (t, &face) in meshed.mesh.triangles().iter().zip(&meshed.faces) {
            let [a, b, c] = t.map(|v| rest[v as usize]);
            if face == 2 {
                area += 0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]));
            }
        }
        assert!((area - 1.0).abs() < 1e-12, "the right face covers {area}");
        assert_eq!(
            Faces::new(&points, &faces, &[[3, 3]], refinement).unwrap_err(),
            FacesError::Line { line: 0, count: 14 }
        );
    }

    #[test]
    fn points_only_lines_name_join_with_their_lines_after_the_refinement() {
        // A strip a tenth of a millimeter wide, inside the left face, along its diagonal:
        // two lines between points no face names.
        let (mut points, faces) = square();
        points.extend([[0.2, 0.1], [0.9, 0.8], [0.2, 0.1001], [0.9, 0.8001]]);
        let lines = [[6, 7], [8, 9]];
        let refinement = Refinement::of_area(0.01);
        let meshed = Faces::new(&points, &faces, &lines, refinement).unwrap();
        let alone = Faces::new(&points[..6], &faces, &[], refinement).unwrap();
        let rest = meshed.mesh.rest();
        // The points keep their order, first, and the lines run between them.
        assert_eq!(rest[..points.len()], points[..]);
        for (chain, [a, b]) in meshed.lines.iter().zip(lines) {
            assert_eq!((chain[0], chain[chain.len() - 1]), (a, b));
        }
        // The refinement ran as it would without them, so the strip holds no tiny triangles.
        let added = rest.len() - points.len();
        let chained: usize = meshed.lines.iter().map(|chain| chain.len() - 2).sum();
        assert_eq!(added, alone.mesh.rest().len() - 6 + chained);
    }

    #[test]
    fn faces_must_be_counterclockwise_apart_and_well_made() {
        let (mut points, mut faces) = square();
        let refinement = Refinement::of_area(0.1);
        faces[2].reverse();
        assert_eq!(
            Faces::new(&points, &faces, &[], refinement).unwrap_err(),
            FacesError::Overlapping { face: 2 }
        );
        faces[2] = vec![1, 4, 5, 3];
        assert_eq!(
            Faces::new(&points, &faces, &[], refinement).unwrap_err(),
            FacesError::Crossing { face: 2 }
        );
        faces[2] = vec![1, 4];
        assert_eq!(
            Faces::new(&points, &faces, &[], refinement).unwrap_err(),
            FacesError::Corners { face: 2, count: 6 }
        );
        faces[2] = vec![1, 4, 5, 2];
        points[5] = points[2];
        assert_eq!(
            Faces::new(&points, &faces, &[], refinement).unwrap_err(),
            FacesError::Coincident { points: [2, 5] }
        );
        // A point that only a line names may not coincide with another either.
        points[5] = [2.0, 1.0];
        points.push(points[0]);
        assert_eq!(
            Faces::new(&points, &faces, &[[6, 2]], refinement).unwrap_err(),
            FacesError::Coincident { points: [0, 6] }
        );
        assert!(
            Faces::new(&points, &faces, &[], Refinement::of_area(0.0))
                .unwrap_err()
                .to_string()
                .contains("max area")
        );
    }
}
