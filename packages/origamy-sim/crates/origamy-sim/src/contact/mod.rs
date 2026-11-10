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
pub(crate) mod friction;

use nalgebra::{SMatrix, Vector3};

use bvh::{Bounds, Bvh};
use rayon::prelude::*;

use crate::energy::{Barrier, Point};
use crate::generated::edge_cross_squared;
use crate::settings::Contact;
pub(crate) use distance::Kind;
use friction::{Slip, tangent_plane};

/// Every primitive of a scene's sheets and bodies, in the scene's vertex numbering.
///
/// The paper's vertices come first, then the points of the bodies.
#[derive(Clone, Debug)]
pub(crate) struct Primitives {
    /// The paper's triangles.
    pub triangles: Vec<[u32; 3]>,
    /// The paper's edges.
    pub edges: Vec<[u32; 2]>,
    /// The axes of capsules, each from one end to the other.
    pub axes: Vec<[u32; 2]>,
    /// The sheet of each vertex of paper.
    pub sheet: Vec<u32>,
    /// Each vertex's place in its flat sheet, at height zero, and each body point's place when
    /// its body joined the scene.
    pub rest: Vec<Point>,
    /// Each vertex's half thickness, in meters: half its sheet's thickness, or its body's
    /// radius.
    pub half: Vec<f64>,
}

impl Primitives {
    /// Whether a vertex is a body's point rather than paper.
    pub fn is_body(&self, vertex: u32) -> bool {
        vertex as usize >= self.sheet.len()
    }

    /// The thickness a pair keeps between its primitives: the sum of their half thicknesses.
    ///
    /// Two pieces of paper keep the mean of their thicknesses, and paper keeps its half
    /// thickness plus a body's radius from the body.
    pub fn thickness(&self, pair: &Pair) -> f64 {
        let [a, b, c, _] = pair.vertices;
        let other = match pair.kind {
            Kind::PointTriangle | Kind::PointEdge => b,
            Kind::EdgeEdge => c,
        };
        self.half[a as usize] + self.half[other as usize]
    }
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
        let [a, b, c, d] = self.vertices.map(Some);
        match self.kind {
            Kind::PointTriangle | Kind::EdgeEdge => [a, b, c, d],
            Kind::PointEdge => [a, b, c, None],
        }
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
            Kind::PointTriangle | Kind::PointEdge => None,
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

    /// A pair's friction at positions `x` with coefficient `friction`, fixed for a time step.
    /// The friction is `None` where the barrier does not push, or the coefficient is zero.
    pub(crate) fn pair_slip(
        &self,
        pair: &Pair,
        x: &[Point],
        thickness: f64,
        fade: Option<f64>,
        friction: f64,
    ) -> Option<Slip> {
        let points = pair.at(x);
        let squared = distance::squared(pair.kind, &points);
        let [_, slope, _] = self.pair(squared, thickness)?;
        if slope == 0.0 || friction == 0.0 {
            return None;
        }
        let weight = fade.map_or(1.0, |fade| {
            let [start, end, other_start, other_end] = points;
            let cross = (end - start).cross(&(other_end - other_start));
            mollifier(cross.norm_squared(), fade)[0]
        });
        let weights = distance::closest_weights(pair.kind, &points);
        let between: Point = points.iter().zip(weights).map(|(p, w)| p * w).sum();
        let distance = between.norm();
        // The barrier is a function of the squared distance D, so its push along the distance
        // d is its slope in D times 2d.
        let normal = -slope * 2.0 * distance * weight;
        Some(Slip {
            stencil: pair.stencil(),
            weights,
            tangent: tangent_plane(&(between / distance)),
            force: friction * normal,
        })
    }

    /// A vertex's friction with the table, fixed for a time step, from its clearance: `None`
    /// where the table does not push, or has no friction.
    pub(crate) fn table_slip(&self, vertex: u32, clearance: f64, friction: f64) -> Option<Slip> {
        let [_, slope, _] = self.table(clearance)?;
        if slope == 0.0 || friction == 0.0 {
            return None;
        }
        Some(Slip {
            stencil: [Some(vertex), None, None, None],
            weights: [1.0, 0.0, 0.0, 0.0],
            tangent: tangent_plane(&Vector3::z()),
            force: friction * -slope,
        })
    }
}

/// The weight that fades two edges' barrier as they turn parallel, with its derivatives.
///
/// The weight is a function of the squared cross product `cross` of the edges' directions. The
/// weight is 1 from `fade` up, and falls smoothly to 0 as the edges turn parallel.
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

/// The pairs whose primitives may come within `gap` of their thickness while the vertices move
/// from `start` to `end`.
///
/// Each primitive's box over the motion grows by its vertices' half thickness and half the gap.
/// A pair is a candidate where the grown boxes overlap. Pairs that share a vertex, or lie
/// that close in one flat sheet, are left out. A body meets paper alone. Without `layers`,
/// paper meets only bodies, and the search skips pairs of paper altogether.
pub(crate) fn candidates(
    primitives: &Primitives,
    trees: &Trees,
    start: &[Point],
    end: &[Point],
    gap: f64,
    layers: bool,
) -> Vec<Pair> {
    let boxes: Vec<Bounds> = start
        .iter()
        .zip(end)
        .enumerate()
        .map(|(v, (a, b))| {
            let grow = Vector3::repeat(primitives.half[v] + 0.5 * gap);
            (a.inf(b) - grow, a.sup(b) + grow)
        })
        .collect();
    let bound = |vertices: &[u32]| {
        vertices
            .iter()
            .map(|&v| boxes[v as usize])
            .reduce(|(low, high), (l, h)| (low.inf(&l), high.sup(&h)))
            .expect("a primitive has vertices")
    };
    // Points, of paper and of bodies, against the paper's triangles.
    let triangle_boxes: Vec<Bounds> = primitives.triangles.iter().map(|t| bound(t)).collect();
    let triangles = trees.triangles.fit(&triangle_boxes);
    let paper =
        u32::try_from(primitives.sheet.len()).expect("a scene counts its vertices in 32 bits");
    let first = if layers { 0 } else { paper as usize };
    let point_pairs: Vec<Vec<Pair>> = boxes[first..]
        .par_iter()
        .enumerate()
        .map(|(v, point)| {
            let v = u32::try_from(v + first).expect("a scene counts its vertices in 32 bits");
            triangles
                .overlapping(point)
                .into_iter()
                .map(|t| primitives.triangles[t])
                .filter(|triangle| !triangle.contains(&v))
                .map(|[a, b, c]| Pair {
                    kind: Kind::PointTriangle,
                    vertices: [v, a, b, c],
                })
                .filter(|pair| !resting(primitives, pair, gap))
                .collect()
        })
        .collect();
    // The paper's edges against each other, each pair once.
    let edge_boxes: Vec<Bounds> = primitives.edges.iter().map(|e| bound(e)).collect();
    let edges = trees.edges.fit(&edge_boxes);
    let paper_edges: &[[u32; 2]] = if layers { &primitives.edges } else { &[] };
    let edge_pairs: Vec<Vec<Pair>> = paper_edges
        .par_iter()
        .enumerate()
        .map(|(e, a)| {
            edges
                .overlapping(&edge_boxes[e])
                .into_iter()
                .filter(|&f| f > e)
                .map(|f| primitives.edges[f])
                .filter(|b| !a.iter().any(|v| b.contains(v)))
                .map(|b| Pair {
                    kind: Kind::EdgeEdge,
                    vertices: [a[0], a[1], b[0], b[1]],
                })
                .filter(|pair| !resting(primitives, pair, gap))
                .collect()
        })
        .collect();
    // The capsules' axes against the paper's edges and points. Bodies are few, so each axis
    // meets every edge and point whose box its box overlaps.
    let mut axis_pairs = Vec::new();
    for &[a, b] in &primitives.axes {
        let axis = bound(&[a, b]);
        axis_pairs.extend(edges.overlapping(&axis).into_iter().map(|e| {
            let [c, d] = primitives.edges[e];
            Pair {
                kind: Kind::EdgeEdge,
                vertices: [c, d, a, b],
            }
        }));
        axis_pairs.extend(
            (0..paper)
                .filter(|&v| bvh::overlap(&boxes[v as usize], &axis))
                .map(|v| Pair {
                    kind: Kind::PointEdge,
                    vertices: [v, a, b, b],
                }),
        );
    }
    // Each pair comes once: a point meets each triangle once, an edge each later edge, and an
    // axis each edge and point. A body's pairs are never the sheet's own shape.
    point_pairs
        .into_iter()
        .flatten()
        .chain(edge_pairs.into_iter().flatten())
        .chain(axis_pairs)
        .collect()
}

/// Whether a pair lies within half a circle round its reach in its flat sheet, so that its
/// closeness is the sheet's own shape rather than contact.
///
/// The reach is the pair's thickness plus `gap`. A sheet folded flat round no other layer
/// curls in half a circle that wide, so points of the curl that far apart in the flat sheet
/// meet closer than the thickness.
fn resting(primitives: &Primitives, pair: &Pair, gap: f64) -> bool {
    if pair.vertices.iter().any(|&v| primitives.is_body(v)) {
        return false;
    }
    let sheet = primitives.sheet[pair.vertices[0] as usize];
    if pair
        .vertices
        .iter()
        .any(|&v| primitives.sheet[v as usize] != sheet)
    {
        return false;
    }
    let reach = std::f64::consts::FRAC_PI_2 * (primitives.thickness(pair) + gap);
    let rest = pair.at(&primitives.rest);
    // Primitives whose boxes lie that far apart in the flat sheet lie at least as far apart.
    let (first, second) = match pair.kind {
        Kind::PointTriangle => (&rest[..1], &rest[1..]),
        Kind::PointEdge => (&rest[..1], &rest[1..3]),
        Kind::EdgeEdge => (&rest[..2], &rest[2..]),
    };
    let (low, high) = bounds(first);
    let (other_low, other_high) = bounds(second);
    let apart = (other_low - high)
        .sup(&(low - other_high))
        .sup(&Point::zeros());
    apart.norm_squared() < reach * reach && distance::squared(pair.kind, &rest) < reach * reach
}

/// The box round some points.
fn bounds(points: &[Point]) -> Bounds {
    points
        .iter()
        .fold((points[0], points[0]), |(low, high), p| {
            (low.inf(p), high.sup(p))
        })
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
                axes: Vec::new(),
                sheet,
                rest,
                half: vec![0.0; 8],
            },
            x,
        )
    }

    #[test]
    fn stacked_sheets_are_candidates_and_a_sheet_is_not_its_own() {
        let (primitives, x) = stack(0.05);
        let trees = Trees::new(&primitives, &x);
        let pairs = candidates(&primitives, &trees, &x, &x, 0.1, true);
        // No pair of one sheet lies within the gap in the flat sheet.
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
            !candidates(&primitives, &trees, &apart, &apart, 0.1, true)
                .iter()
                .any(both)
        );
        // Without contact between layers, no pair of paper is a candidate.
        assert!(candidates(&primitives, &trees, &x, &x, 0.1, false).is_empty());
    }

    #[test]
    fn a_curl_one_thickness_across_is_the_sheet_itself() {
        // Two points of one sheet, a thickness apart in the flat sheet and then half a
        // circle round it, meet across a curl one thickness across.
        let thickness = 0.1;
        let pair = |apart: f64| {
            let primitives = Primitives {
                triangles: Vec::new(),
                edges: vec![[0, 1], [2, 3]],
                axes: Vec::new(),
                sheet: vec![0; 4],
                rest: vec![
                    Point::new(0.0, 0.0, 0.0),
                    Point::new(0.0, 1.0, 0.0),
                    Point::new(apart, 0.0, 0.0),
                    Point::new(apart, 1.0, 0.0),
                ],
                half: vec![0.5 * thickness; 4],
            };
            let pair = Pair {
                kind: Kind::EdgeEdge,
                vertices: [0, 1, 2, 3],
            };
            resting(&primitives, &pair, 0.0)
        };
        let half_circle = std::f64::consts::FRAC_PI_2 * thickness;
        assert!(pair(1.01 * thickness));
        assert!(pair(0.99 * half_circle));
        assert!(!pair(1.01 * half_circle));
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
