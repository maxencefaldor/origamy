//! Contact with thickness: no two pieces of paper come closer than a paper thickness, and no
//! paper goes through the table.
//!
//! Pairs of a point and a triangle, and of two edges, take a barrier on their distance, as in
//! codimensional incremental potential contact. The barrier acts within the activation gap
//! of the pair's thickness. Pairs that share a vertex, or lie that close in the flat sheet,
//! are the sheet itself rather than contact, and take none. Continuous collision detection
//! cuts every Newton update short before any pair would reach its thickness.

mod bvh;
pub(crate) mod ccd;
pub(crate) mod distance;

use nalgebra::{SMatrix, Vector3};

use bvh::{Bounds, Bvh};
use rayon::prelude::*;

use crate::energy::{Barrier, Point};
use crate::generated::edge_cross_squared;
use crate::settings::Contact;
pub(crate) use distance::Kind;

/// Every primitive of a scene's sheets, in the scene's vertex numbering.
#[derive(Clone, Debug)]
pub(crate) struct Primitives {
    pub triangles: Vec<[u32; 3]>,
    pub edges: Vec<[u32; 2]>,
    /// The sheet of each vertex.
    pub sheet: Vec<u32>,
    /// Each vertex's place in its flat sheet, at height zero.
    pub rest: Vec<Point>,
}

/// Hierarchies over a scene's triangles and edges, built for one arrangement of the vertices
/// and fitted to others.
#[derive(Clone, Debug, Default)]
pub(crate) struct Trees {
    triangles: Bvh,
    edges: Bvh,
}

impl Trees {
    /// The hierarchies for the primitives with vertices at positions `x`.
    pub fn new(primitives: &Primitives, x: &[Point]) -> Self {
        let bound = |vertices: &[u32]| {
            vertices
                .iter()
                .map(|&v| (x[v as usize], x[v as usize]))
                .reduce(|(low, high), (l, h)| (low.inf(&l), high.sup(&h)))
                .expect("a primitive has vertices")
        };
        let triangles: Vec<Bounds> = primitives.triangles.iter().map(|t| bound(t)).collect();
        let edges: Vec<Bounds> = primitives.edges.iter().map(|e| bound(e)).collect();
        Self {
            triangles: Bvh::new(&triangles),
            edges: Bvh::new(&edges),
        }
    }
}

/// A pair of primitives that may touch: a point and a triangle's corners, or two edges' ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Pair {
    pub kind: Kind,
    pub vertices: [u32; 4],
}

impl Pair {
    /// The pair's four vertices at positions `x`.
    pub fn at(&self, x: &[Point]) -> [Point; 4] {
        self.vertices.map(|v| x[v as usize])
    }

    /// The pair's stencil for an assembly.
    pub fn stencil(&self) -> [Option<u32>; 4] {
        self.vertices.map(Some)
    }
}

const SHAPE: Barrier = Barrier { onset: 1.0 };

impl Contact {
    /// The barrier of a pair at squared distance `squared` and thickness `thickness`, with its
    /// derivatives in the squared distance; `None` at or below the thickness.
    pub(crate) fn pair(&self, squared: f64, thickness: f64) -> Option<[f64; 3]> {
        let scale = (thickness + self.gap).powi(2) - thickness * thickness;
        let [value, slope, curvature] = SHAPE.at((squared - thickness * thickness) / scale)?;
        Some([
            self.stiffness * value,
            self.stiffness * slope / scale,
            self.stiffness * curvature / (scale * scale),
        ])
    }

    /// The barrier between a vertex and the table, with its derivatives in the vertex's height.
    ///
    /// The clearance is the vertex's height over the table less its half thickness. The
    /// barrier is `None` where the clearance is gone.
    pub(crate) fn table(&self, clearance: f64) -> Option<[f64; 3]> {
        let [value, slope, curvature] = SHAPE.at(clearance / self.gap)?;
        Some([
            self.stiffness * value,
            self.stiffness * slope / self.gap,
            self.stiffness * curvature / (self.gap * self.gap),
        ])
    }

    /// The squared cross product of two edges' directions under which their barrier fades, at
    /// the edges' rest lengths; `None` for a point and a triangle.
    pub(crate) fn fade(&self, primitives: &Primitives, pair: &Pair) -> Option<f64> {
        match pair.kind {
            Kind::PointTriangle => None,
            Kind::EdgeEdge => {
                let [a, b, c, d] = pair.at(&primitives.rest);
                Some(self.parallel * (b - a).norm_squared() * (d - c).norm_squared())
            }
        }
    }

    /// The barrier of a pair at positions `x`, faded below `fade`: infinite where the pair
    /// reaches its thickness.
    pub(crate) fn pair_energy(
        &self,
        pair: &Pair,
        x: &[Point],
        thickness: f64,
        fade: Option<f64>,
    ) -> f64 {
        let points = pair.at(x);
        let Some([value, _, _]) = self.pair(distance::squared(pair.kind, &points), thickness)
        else {
            return f64::INFINITY;
        };
        let [weight, _, _] = fade.map_or([1.0, 0.0, 0.0], |fade| {
            let [start, end, other_start, other_end] = points;
            let cross = (end - start).cross(&(other_end - other_start));
            mollifier(cross.norm_squared(), fade)
        });
        value * weight
    }

    /// The gradient and Hessian of a pair's barrier over its four vertices, faded below
    /// `fade`, or `None` where the pair lies beyond the barrier's reach.
    pub(crate) fn pair_derivatives(
        &self,
        pair: &Pair,
        x: &[Point],
        thickness: f64,
        fade: Option<f64>,
    ) -> Option<(SMatrix<f64, 12, 1>, SMatrix<f64, 12, 12>)> {
        let points = pair.at(x);
        if distance::squared(pair.kind, &points) >= (thickness + self.gap).powi(2) {
            return None;
        }
        let (squared, gradient, hessian) = distance::derivatives(pair.kind, &points);
        let [value, slope, curvature] = self.pair(squared, thickness)?;
        let barrier_gradient = gradient * slope;
        let barrier_hessian = gradient * gradient.transpose() * curvature + hessian * slope;
        let Some(fade) = fade else {
            return Some((barrier_gradient, barrier_hessian));
        };
        let [start, end, other_start, other_end] = points.map(|p| [p[0], p[1], p[2]]);
        let (cross, cross_gradient, cross_hessian) =
            edge_cross_squared(start, end, other_start, other_end);
        let [weight, weight_slope, weight_curvature] = mollifier(cross, fade);
        if weight_slope == 0.0 {
            return Some((barrier_gradient, barrier_hessian));
        }
        let cross_gradient = SMatrix::<f64, 12, 1>::from_column_slice(&cross_gradient);
        let cross_hessian = SMatrix::<f64, 12, 12>::from_fn(|i, j| cross_hessian[i][j]);
        let weight_gradient = cross_gradient * weight_slope;
        let weight_hessian = cross_gradient * cross_gradient.transpose() * weight_curvature
            + cross_hessian * weight_slope;
        Some((
            barrier_gradient * weight + weight_gradient * value,
            barrier_hessian * weight
                + weight_gradient * barrier_gradient.transpose()
                + barrier_gradient * weight_gradient.transpose()
                + weight_hessian * value,
        ))
    }
}

/// The weight that fades two edges' barrier as they turn parallel, with its derivatives.
///
/// The weight is a function of the squared cross product `cross` of the edges' directions. It
/// is 1 from `fade` up, and falls smoothly to 0 as the edges turn parallel.
///
/// Parallel edges have no single closest pair of points, so their distance is not smooth there.
/// The points and triangles around them hold them apart instead. The weight follows incremental
/// potential contact: (2 − s) s, with s the cross product over `fade`.
fn mollifier(cross: f64, fade: f64) -> [f64; 3] {
    let s = cross / fade;
    if s >= 1.0 {
        [1.0, 0.0, 0.0]
    } else {
        [(2.0 - s) * s, (2.0 - 2.0 * s) / fade, -2.0 / (fade * fade)]
    }
}

/// The pairs whose primitives may come within `margin` of each other while the vertices move
/// from `start` to `end`.
///
/// A pair is a candidate where its boxes over the motion, grown by `margin`, overlap. Pairs
/// that share a vertex, or lie within `margin` of each other in one flat sheet, are left out.
pub(crate) fn candidates(
    primitives: &Primitives,
    trees: &Trees,
    start: &[Point],
    end: &[Point],
    margin: f64,
) -> Vec<Pair> {
    let boxes: Vec<Bounds> = start
        .iter()
        .zip(end)
        .map(|(a, b)| (a.inf(b), a.sup(b)))
        .collect();
    let bound = |vertices: &[u32]| {
        let (low, high) = vertices
            .iter()
            .map(|&v| boxes[v as usize])
            .reduce(|(low, high), (l, h)| (low.inf(&l), high.sup(&h)))
            .expect("a primitive has vertices");
        let grow = Vector3::repeat(margin);
        (low - grow, high + grow)
    };
    // Points against triangles.
    let triangle_boxes: Vec<Bounds> = primitives.triangles.iter().map(|t| bound(t)).collect();
    let triangles = trees.triangles.fit(&triangle_boxes);
    let point_pairs: Vec<Vec<Pair>> = boxes
        .par_iter()
        .enumerate()
        .map(|(v, point)| {
            let v = u32::try_from(v).expect("a scene counts its vertices in 32 bits");
            triangles
                .overlapping(point)
                .into_iter()
                .map(|t| primitives.triangles[t])
                .filter(|triangle| !triangle.contains(&v))
                .map(|[a, b, c]| Pair {
                    kind: Kind::PointTriangle,
                    vertices: [v, a, b, c],
                })
                .collect()
        })
        .collect();
    // Edges against edges, each pair once: an edge's own box meets the others' grown boxes.
    let edge_boxes: Vec<Bounds> = primitives.edges.iter().map(|e| bound(e)).collect();
    let edges = trees.edges.fit(&edge_boxes);
    let edge_pairs: Vec<Vec<Pair>> = primitives
        .edges
        .par_iter()
        .enumerate()
        .map(|(e, a)| {
            let own = bound(a);
            let own = (
                own.0 + Vector3::repeat(margin),
                own.1 - Vector3::repeat(margin),
            );
            edges
                .overlapping(&own)
                .into_iter()
                .filter(|&f| f > e)
                .map(|f| primitives.edges[f])
                .filter(|b| !a.iter().any(|v| b.contains(v)))
                .map(|b| Pair {
                    kind: Kind::EdgeEdge,
                    vertices: [a[0], a[1], b[0], b[1]],
                })
                .collect()
        })
        .collect();
    let mut pairs: Vec<Pair> = point_pairs
        .into_iter()
        .chain(edge_pairs)
        .flatten()
        .collect();
    pairs.sort_unstable();
    pairs.dedup();
    pairs.retain(|pair| !resting(primitives, pair, margin));
    pairs
}

/// Whether a pair lies within `margin` in its flat sheet, so that its closeness is the sheet's
/// own shape rather than contact.
fn resting(primitives: &Primitives, pair: &Pair, margin: f64) -> bool {
    let sheet = primitives.sheet[pair.vertices[0] as usize];
    pair.vertices
        .iter()
        .all(|&v| primitives.sheet[v as usize] == sheet)
        && distance::squared(pair.kind, &pair.at(&primitives.rest)) < margin * margin
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::energy::testing::check;

    /// Two unit squares of two triangles each, the second `height` above the first.
    fn stack(height: f64) -> (Primitives, Vec<Point>) {
        let corners = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let mut rest = Vec::new();
        let mut x = Vec::new();
        for (sheet, z) in [(0, 0.0), (1, height)] {
            for c in corners {
                rest.push(Point::new(c[0], c[1], 0.0));
                x.push(Point::new(c[0], c[1], z));
            }
            let _ = sheet;
        }
        let triangles = vec![[0, 1, 2], [0, 2, 3], [4, 5, 6], [4, 6, 7]];
        let edges = vec![
            [0, 1],
            [0, 2],
            [0, 3],
            [1, 2],
            [2, 3],
            [4, 5],
            [4, 6],
            [4, 7],
            [5, 6],
            [6, 7],
        ];
        let sheet = vec![0, 0, 0, 0, 1, 1, 1, 1];
        (
            Primitives {
                triangles,
                edges,
                sheet,
                rest,
            },
            x,
        )
    }

    #[test]
    fn stacked_sheets_are_candidates_and_a_sheet_is_not_its_own() {
        let (primitives, x) = stack(0.05);
        let trees = Trees::new(&primitives, &x);
        let pairs = candidates(&primitives, &trees, &x, &x, 0.1);
        // No pair of one sheet lies within the margin in the flat sheet.
        assert!(pairs.iter().all(|p| !resting(&primitives, p, 0.1)));
        // Each corner over each triangle below, and the reverse, is a candidate.
        assert!(pairs.contains(&Pair {
            kind: Kind::PointTriangle,
            vertices: [4, 0, 1, 2]
        }));
        assert!(pairs.contains(&Pair {
            kind: Kind::PointTriangle,
            vertices: [0, 4, 5, 6]
        }));
        // Sheets half a meter apart are never candidates of each other.
        let apart = stack(0.5).1;
        let across = |p: &Pair| {
            p.vertices
                .iter()
                .any(|&v| primitives.sheet[v as usize] == 1)
        };
        let both = |p: &Pair| {
            p.vertices
                .iter()
                .any(|&v| primitives.sheet[v as usize] == 0)
                && across(p)
        };
        assert!(
            !candidates(&primitives, &trees, &apart, &apart, 0.1)
                .iter()
                .any(both)
        );
    }

    #[test]
    fn nearly_parallel_edges_fade_their_barrier_smoothly() {
        // Two edges crossing a thickness apart, 0.02 rad from parallel: within the fade.
        let (h, contact) = (70e-6, Contact::default());
        let x = SMatrix::<f64, 12, 1>::from_column_slice(&[
            0.0,
            0.0,
            0.0,
            0.01,
            0.0,
            0.0,
            0.002,
            -0.0001,
            h + 1e-6,
            0.012,
            0.0001,
            h + 1.2e-6,
        ]);
        let points = |x: &SMatrix<f64, 12, 1>| -> Vec<Point> {
            (0..4)
                .map(|k| x.fixed_rows::<3>(3 * k).into_owned())
                .collect()
        };
        let pair = Pair {
            kind: Kind::EdgeEdge,
            vertices: [0, 1, 2, 3],
        };
        let fade = Some(1e-3 * 1e-4 * 1e-4);
        let weight = |x: &SMatrix<f64, 12, 1>| {
            let p = points(x);
            mollifier((p[1] - p[0]).cross(&(p[3] - p[2])).norm_squared(), 1e-11)[0]
        };
        assert!(weight(&x) > 0.5 && weight(&x) < 1.0);
        let (_, hessian) = contact
            .pair_derivatives(&pair, &points(&x), h, fade)
            .unwrap();
        check(
            x,
            |x| contact.pair_energy(&pair, &points(x), h, fade),
            |x| {
                contact
                    .pair_derivatives(&pair, &points(x), h, fade)
                    .unwrap()
                    .0
            },
            &hessian,
            1e-10,
            1e-4,
        );
        // Parallel edges take no barrier, though they lie as close.
        let mut parallel = points(&x);
        parallel[3] = parallel[2] + (parallel[1] - parallel[0]);
        assert!(contact.pair_energy(&pair, &parallel, h, fade).abs() < f64::MIN_POSITIVE);
    }

    #[test]
    fn the_barrier_reaches_only_to_the_gap() {
        let contact = Contact {
            gap: 1e-5,
            stiffness: 1e-6,
            ..Contact::default()
        };
        let h = 7e-5;
        assert_eq!(contact.pair((h + 1e-5) * (h + 1e-5), h), Some([0.0; 3]));
        assert!(contact.pair((h + 5e-6) * (h + 5e-6), h).unwrap()[0] > 0.0);
        assert_eq!(contact.pair(h * h, h), None);
        assert_eq!(contact.table(1e-5), Some([0.0; 3]));
        assert!(contact.table(1e-6).unwrap()[0] > 0.0);
        assert_eq!(contact.table(0.0), None);
    }
}
