//! Meshes of planar faces that meet along their edges: a constrained Delaunay triangulation of
//! the edges, refined until its triangles are small and well shaped.
//!
//! Every edge of every face becomes a chain of mesh edges, so a sheet can bend sharply along
//! it. Ruppert's refinement inserts points until no angle falls under a bound and no triangle
//! exceeds an area. The refinement splits the faces' edges where it must, without gaps.

use spade::{
    AngleLimit, ConstrainedDelaunayTriangulation, Point2, RefinementParameters, Triangulation,
};

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
}

impl Refinement {
    /// Triangles of at most `max_area` square meters, with no angle under 20°.
    #[must_use]
    pub fn of_area(max_area: f64) -> Self {
        Self {
            max_area,
            min_angle: 20f64.to_radians(),
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
    /// The mesh, whose first vertices are the faces' points, in their order.
    pub mesh: Mesh,
    /// The face each triangle of the mesh lies in.
    pub faces: Vec<u32>,
}

impl Faces {
    /// A mesh of faces that meet along their edges, each a counterclockwise polygon of
    /// `points`, in meters, refined as `refinement` asks.
    ///
    /// The mesh keeps every point as a vertex, in order, and every face's edge as a chain of
    /// mesh edges. Its triangles cover exactly the faces.
    ///
    /// # Errors
    ///
    /// Returns a [`FacesError`] when a refinement parameter lies outside its values, or a
    /// point is not finite. Coincident points and faces with too few corners are refused, as
    /// are faces whose edges cross, or that run clockwise or overlap. A refinement that cannot
    /// reach its least angle is refused too.
    pub fn new(
        points: &[[f64; 2]],
        faces: &[Vec<Vertex>],
        refinement: Refinement,
    ) -> Result<Self, FacesError> {
        refinement.check()?;
        let mut cdt = ConstrainedDelaunayTriangulation::<Point2<f64>>::new();
        let mut handles = Vec::with_capacity(points.len());
        for (point, &[x, y]) in points.iter().enumerate() {
            if !(x.is_finite() && y.is_finite()) {
                return Err(FacesError::NotFinite { point });
            }
            let before = cdt.num_vertices();
            let handle = cdt
                .insert(Point2::new(x, y))
                .map_err(|_| FacesError::NotFinite { point })?;
            if cdt.num_vertices() == before {
                return Err(FacesError::Coincident {
                    points: [handle.index(), point],
                });
            }
            handles.push(handle);
        }
        for (face, corners) in faces.iter().enumerate() {
            if corners.len() < 3 || corners.iter().any(|&c| c as usize >= points.len()) {
                return Err(FacesError::Corners {
                    face,
                    count: points.len(),
                });
            }
            if polygon_area(points, corners) <= 0.0 {
                return Err(FacesError::Overlapping { face });
            }
            for (k, &a) in corners.iter().enumerate() {
                let b = corners[(k + 1) % corners.len()];
                let (from, to) = (handles[a as usize], handles[b as usize]);
                if !cdt.can_add_constraint(from, to) {
                    return Err(FacesError::Crossing { face });
                }
                cdt.add_constraint(from, to);
            }
        }
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
                .with_max_additional_vertices(budget),
        );
        if !result.refinement_complete {
            return Err(FacesError::Unrefined);
        }
        // Each triangle lies in the face that holds its center, and the rest of the hull
        // drops out.
        let labels = label(&cdt, points, faces)?;
        // The vertices keep spade's order, which starts with the points, less any unused.
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
        for (v, vertex) in cdt.vertices().enumerate() {
            if used[v] {
                renumber[v] =
                    Vertex::try_from(rest.len()).map_err(|_| MeshError::TooManyVertices {
                        count: cdt.num_vertices(),
                    })?;
                let p = vertex.position();
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
        })
    }
}

/// The face each inner face of the triangulation lies in, or `None` outside every face: the face
/// that holds its center.
fn label(
    cdt: &ConstrainedDelaunayTriangulation<Point2<f64>>,
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
        let meshed = Faces::new(&points, &faces, refinement).unwrap();
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
    fn faces_must_be_counterclockwise_apart_and_well_made() {
        let (mut points, mut faces) = square();
        let refinement = Refinement::of_area(0.1);
        faces[2].reverse();
        assert_eq!(
            Faces::new(&points, &faces, refinement).unwrap_err(),
            FacesError::Overlapping { face: 2 }
        );
        faces[2] = vec![1, 4, 5, 3];
        assert_eq!(
            Faces::new(&points, &faces, refinement).unwrap_err(),
            FacesError::Crossing { face: 2 }
        );
        faces[2] = vec![1, 4];
        assert_eq!(
            Faces::new(&points, &faces, refinement).unwrap_err(),
            FacesError::Corners { face: 2, count: 6 }
        );
        points[5] = points[2];
        assert_eq!(
            Faces::new(&points, &faces, refinement).unwrap_err(),
            FacesError::Coincident { points: [2, 5] }
        );
        assert!(
            Faces::new(&points, &faces, Refinement::of_area(0.0))
                .unwrap_err()
                .to_string()
                .contains("max area")
        );
    }
}
