//! Distances between a point and a triangle, between two edges, and between a point and an
//! edge, with exact derivatives.
//!
//! Each distance first finds which features are closest. They are two points, a point and a
//! segment's interior, a point and a triangle's interior, or two segments' interiors. The
//! squared distance between those features has its own smooth formula, whose derivatives the
//! code generator writes. The squared distance is the true one everywhere, so its gradient
//! stays continuous where the closest features change.

#![allow(
    clippy::many_single_char_names,
    reason = "the geometry keeps the names of the references it follows: points a to d, s and t"
)]

use nalgebra::SMatrix;

use crate::energy::Point;
use crate::generated::{
    line_line_distance, point_line_distance, point_plane_distance, point_point_distance,
};

/// A pair of primitives that may touch.
///
/// A point and a triangle list the point, then the triangle's corners. Two edges list the
/// first edge's ends, then the second edge's. A point and an edge list the point, then the
/// edge's ends, and the fourth slot repeats the edge's second end without taking part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Kind {
    PointTriangle,
    EdgeEdge,
    PointEdge,
}

/// The features that are closest, as the stencil slots each formula reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Closest {
    PointPoint([usize; 2]),
    PointLine([usize; 3]),
    PointPlane,
    LineLine,
}

/// The squared distance of a stencil, with its gradient and Hessian over the four vertices.
pub(crate) type Derivatives = (f64, SMatrix<f64, 12, 1>, SMatrix<f64, 12, 12>);

/// The squared distance between the primitives of a stencil.
pub(crate) fn squared(kind: Kind, x: &[Point; 4]) -> f64 {
    let (a, b) = closest_points(kind, x);
    (a - b).norm_squared()
}

/// The squared distance between the primitives of a stencil, with its exact derivatives.
pub(crate) fn derivatives(kind: Kind, x: &[Point; 4]) -> Derivatives {
    let array = |p: &Point| [p[0], p[1], p[2]];
    let closest = classify(kind, x);
    let (value, slots, local_gradient, local_hessian): (f64, Vec<usize>, Vec<f64>, Vec<Vec<f64>>) =
        match closest {
            Closest::PointPoint([i, j]) => {
                let (v, g, h) = point_point_distance(array(&x[i]), array(&x[j]));
                (
                    v,
                    vec![i, j],
                    g.to_vec(),
                    h.iter().map(|r| r.to_vec()).collect(),
                )
            }
            Closest::PointLine([i, j, k]) => {
                let (v, g, h) = point_line_distance(array(&x[i]), array(&x[j]), array(&x[k]));
                (
                    v,
                    vec![i, j, k],
                    g.to_vec(),
                    h.iter().map(|r| r.to_vec()).collect(),
                )
            }
            Closest::PointPlane => {
                let (v, g, h) =
                    point_plane_distance(array(&x[0]), array(&x[1]), array(&x[2]), array(&x[3]));
                (
                    v,
                    vec![0, 1, 2, 3],
                    g.to_vec(),
                    h.iter().map(|r| r.to_vec()).collect(),
                )
            }
            Closest::LineLine => {
                let (v, g, h) =
                    line_line_distance(array(&x[0]), array(&x[1]), array(&x[2]), array(&x[3]));
                (
                    v,
                    vec![0, 1, 2, 3],
                    g.to_vec(),
                    h.iter().map(|r| r.to_vec()).collect(),
                )
            }
        };
    let mut gradient = SMatrix::<f64, 12, 1>::zeros();
    let mut hessian = SMatrix::<f64, 12, 12>::zeros();
    for (a, &sa) in slots.iter().enumerate() {
        for c in 0..3 {
            gradient[3 * sa + c] += local_gradient[3 * a + c];
            for (b, &sb) in slots.iter().enumerate() {
                for d in 0..3 {
                    hessian[(3 * sa + c, 3 * sb + d)] += local_hessian[3 * a + c][3 * b + d];
                }
            }
        }
    }
    (value, gradient, hessian)
}

/// Which features of the stencil's primitives are closest.
fn classify(kind: Kind, x: &[Point; 4]) -> Closest {
    match kind {
        Kind::PointTriangle => match triangle_region(&x[0], &x[1], &x[2], &x[3]) {
            Region::Corner(i) => Closest::PointPoint([0, 1 + i]),
            Region::Side(i, j) => Closest::PointLine([0, 1 + i, 1 + j]),
            Region::Face => Closest::PointPlane,
        },
        Kind::EdgeEdge => {
            let (s, t) = segment_parameters(&x[0], &x[1], &x[2], &x[3]);
            let inside = |u: f64| u > 0.0 && u < 1.0;
            match (inside(s), inside(t)) {
                (true, true) => Closest::LineLine,
                (false, true) => Closest::PointLine([end(s, 0, 1), 2, 3]),
                (true, false) => Closest::PointLine([end(t, 2, 3), 0, 1]),
                (false, false) => Closest::PointPoint([end(s, 0, 1), end(t, 2, 3)]),
            }
        }
        Kind::PointEdge => {
            let t = edge_parameter(&x[0], &x[1], &x[2]);
            if t > 0.0 && t < 1.0 {
                Closest::PointLine([0, 1, 2])
            } else {
                Closest::PointPoint([0, end(t, 1, 2)])
            }
        }
    }
}

/// The parameter, from 0 at `a` to 1 at `b`, of the point of the segment from `a` to `b`
/// closest to `p`.
fn edge_parameter(p: &Point, a: &Point, b: &Point) -> f64 {
    let direction = b - a;
    ((p - a).dot(&direction) / direction.norm_squared()).clamp(0.0, 1.0)
}

/// The slot of a segment's end that a clamped parameter names.
fn end(parameter: f64, start: usize, finish: usize) -> usize {
    if parameter <= 0.0 { start } else { finish }
}

/// The closest points of the stencil's two primitives.
fn closest_points(kind: Kind, x: &[Point; 4]) -> (Point, Point) {
    let weights = closest_weights(kind, x);
    let split = match kind {
        Kind::PointTriangle | Kind::PointEdge => 1,
        Kind::EdgeEdge => 2,
    };
    let first = (0..split).map(|i| x[i] * weights[i]).sum();
    let second = (split..4).map(|i| x[i] * -weights[i]).sum();
    (first, second)
}

/// The weights of the stencil's vertices in the vector between the primitives' closest points.
///
/// The vector runs from the second primitive to the first. The vector is the sum of each
/// vertex times its weight.
///
/// The first primitive's weights are positive and sum to 1, and the second's are negative and
/// sum to −1.
pub(crate) fn closest_weights(kind: Kind, x: &[Point; 4]) -> [f64; 4] {
    match kind {
        Kind::PointTriangle => {
            let [a, b, c] = triangle_weights(&x[0], &x[1], &x[2], &x[3]);
            [1.0, -a, -b, -c]
        }
        Kind::EdgeEdge => {
            let (s, t) = segment_parameters(&x[0], &x[1], &x[2], &x[3]);
            [1.0 - s, s, t - 1.0, -t]
        }
        Kind::PointEdge => {
            let t = edge_parameter(&x[0], &x[1], &x[2]);
            [1.0, t - 1.0, -t, 0.0]
        }
    }
}

/// Where the point closest to `p` lies on a triangle.
enum Region {
    Corner(usize),
    Side(usize, usize),
    Face,
}

/// The region of the triangle `a`, `b`, `c` that holds the point closest to `p`, after
/// Ericson's Real-Time Collision Detection, 5.1.5.
/// Whether a point lies over a triangle's face: closest to the inside of its plane rather
/// than to an edge or a corner.
pub(crate) fn over_face(p: &Point, a: &Point, b: &Point, c: &Point) -> bool {
    matches!(triangle_region(p, a, b, c), Region::Face)
}

/// Whether a point lies over a triangle, its edges and corners included, within a share
/// `slack` of the triangle's size.
pub(crate) fn over(p: &Point, a: &Point, b: &Point, c: &Point, slack: f64) -> bool {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d00, d01, d11) = (ab.dot(&ab), ab.dot(&ac), ac.dot(&ac));
    let (d20, d21) = (ap.dot(&ab), ap.dot(&ac));
    let determinant = d00 * d11 - d01 * d01;
    if determinant <= 0.0 {
        return false;
    }
    let v = (d11 * d20 - d01 * d21) / determinant;
    let w = (d00 * d21 - d01 * d20) / determinant;
    v >= -slack && w >= -slack && v + w <= 1.0 + slack
}

fn triangle_region(p: &Point, a: &Point, b: &Point, c: &Point) -> Region {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(&ap), ac.dot(&ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return Region::Corner(0);
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(&bp), ac.dot(&bp));
    if d3 >= 0.0 && d4 <= d3 {
        return Region::Corner(1);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return Region::Side(0, 1);
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(&cp), ac.dot(&cp));
    if d6 >= 0.0 && d5 <= d6 {
        return Region::Corner(2);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return Region::Side(0, 2);
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return Region::Side(1, 2);
    }
    Region::Face
}

/// The barycentric weights of the point of the triangle `a`, `b`, `c` closest to `p`.
fn triangle_weights(p: &Point, a: &Point, b: &Point, c: &Point) -> [f64; 3] {
    let along = |from: &Point, to: &Point| {
        let direction = to - from;
        ((p - from).dot(&direction) / direction.norm_squared()).clamp(0.0, 1.0)
    };
    match triangle_region(p, a, b, c) {
        Region::Corner(0) => [1.0, 0.0, 0.0],
        Region::Corner(1) => [0.0, 1.0, 0.0],
        Region::Corner(_) => [0.0, 0.0, 1.0],
        Region::Side(0, 1) => {
            let t = along(a, b);
            [1.0 - t, t, 0.0]
        }
        Region::Side(0, _) => {
            let t = along(a, c);
            [1.0 - t, 0.0, t]
        }
        Region::Side(..) => {
            let t = along(b, c);
            [0.0, 1.0 - t, t]
        }
        Region::Face => {
            // The weights of the point's projection, from the areas it makes with the sides.
            let (ab, ac, ap) = (b - a, c - a, p - a);
            let normal = ab.cross(&ac);
            let area = normal.norm_squared();
            let v = ap.cross(&ac).dot(&normal) / area;
            let w = ab.cross(&ap).dot(&normal) / area;
            [1.0 - v - w, v, w]
        }
    }
}

/// The parameters s and t in [0, 1] of the closest points of the segments from `a` to `b` and
/// from `c` to `d`.
///
/// The method follows Ericson's Real-Time Collision Detection, 5.1.9. Nearly parallel
/// segments take their closest points at ends.
fn segment_parameters(a: &Point, b: &Point, c: &Point, d: &Point) -> (f64, f64) {
    let (u, v, w) = (b - a, d - c, a - c);
    let (uu, vv, uv, uw, vw) = (u.dot(&u), v.dot(&v), u.dot(&v), u.dot(&w), v.dot(&w));
    let denominator = uu * vv - uv * uv;
    let mut s = if denominator > 1e-12 * uu * vv {
        ((uv * vw - vv * uw) / denominator).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut t = (uv * s + vw) / vv;
    if t < 0.0 {
        t = 0.0;
        s = (-uw / uu).clamp(0.0, 1.0);
    } else if t > 1.0 {
        t = 1.0;
        s = ((uv - uw) / uu).clamp(0.0, 1.0);
    }
    (s, t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::energy::testing::check;
    use proptest::prelude::*;

    fn stencil(x: &SMatrix<f64, 12, 1>) -> [Point; 4] {
        [0, 1, 2, 3].map(|k| x.fixed_rows::<3>(3 * k).into_owned())
    }

    /// The smallest squared distance between points sampled densely on both primitives.
    fn sampled(kind: Kind, x: &[Point; 4]) -> f64 {
        let n = 60;
        let mut best = f64::INFINITY;
        for i in 0..=n {
            for j in 0..=n {
                let (s, t) = (f64::from(i) / f64::from(n), f64::from(j) / f64::from(n));
                let d = match kind {
                    Kind::PointTriangle if s + t <= 1.0 => {
                        (x[0] - (x[1] + (x[2] - x[1]) * s + (x[3] - x[1]) * t)).norm_squared()
                    }
                    Kind::PointTriangle => continue,
                    Kind::EdgeEdge => {
                        ((x[0] + (x[1] - x[0]) * s) - (x[2] + (x[3] - x[2]) * t)).norm_squared()
                    }
                    Kind::PointEdge => (x[0] - (x[1] + (x[2] - x[1]) * s)).norm_squared(),
                };
                best = best.min(d);
            }
        }
        best
    }

    fn point() -> impl Strategy<Value = Point> {
        (-1.0..1.0f64, -1.0..1.0f64, -1.0..1.0f64).prop_map(|(a, b, c)| Point::new(a, b, c))
    }

    proptest! {
        #[test]
        fn the_distance_is_the_least_over_both_primitives(a in point(), b in point(), c in point(), d in point()) {
            let x = [a, b, c, d];
            for kind in [Kind::PointTriangle, Kind::EdgeEdge, Kind::PointEdge] {
                let exact = squared(kind, &x);
                prop_assert!(exact <= sampled(kind, &x) + 1e-12);
                prop_assert!(sampled(kind, &x) - exact < 2e-3);
                let (value, _, _) = derivatives(kind, &x);
                prop_assert!((value - exact).abs() <= 1e-9 * exact.max(1.0));
            }
        }
    }

    #[test]
    fn each_closest_feature_has_exact_derivatives() {
        // The cases are a point over the face, beside a side and past a corner. Then come edges
        // crossing, end on edge and end to end. Last is a point beside an edge, and past its end.
        let cases = [
            (Kind::PointTriangle, [0.2, 0.2, 0.5], 0),
            (Kind::PointTriangle, [0.5, -0.4, 0.3], 1),
            (Kind::PointTriangle, [-0.5, -0.4, 0.3], 2),
            (Kind::EdgeEdge, [0.3, -0.5, 0.4], 3),
            (Kind::EdgeEdge, [1.6, -0.5, 0.4], 4),
            (Kind::EdgeEdge, [1.6, 1.4, 0.4], 5),
            (Kind::PointEdge, [0.4, 0.3, -0.2], 6),
            (Kind::PointEdge, [1.3, 0.3, -0.2], 7),
        ];
        for (kind, [p, q, r], case) in cases {
            let x = match kind {
                Kind::PointTriangle => [p, q, r, 0.0, 0.0, 0.0, 1.0, 0.1, 0.0, 0.1, 1.0, 0.05],
                Kind::PointEdge => [p, q, r, 0.0, 0.0, 0.0, 1.0, 0.2, 0.1, 1.0, 0.2, 0.1],
                Kind::EdgeEdge => [
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                    0.2,
                    0.0,
                    p,
                    q,
                    r,
                    p - 0.1,
                    q + 1.0,
                    r - 0.2,
                ],
            };
            let x = SMatrix::<f64, 12, 1>::from_column_slice(&x);
            let (_, _, hessian) = derivatives(kind, &stencil(&x));
            check(
                x,
                |x| squared(kind, &stencil(x)),
                |x| derivatives(kind, &stencil(x)).1,
                &hessian,
                1e-7,
                1e-5,
            );
            assert!(case < 8);
        }
    }
}
