//! Contact with thickness: no two pieces of paper come closer than a paper thickness, and no
//! paper goes through the table.
//!
//! Pairs of a point and a triangle, and of two edges, take a barrier on their distance, as in
//! codimensional incremental potential contact. The barrier acts within the activation gap
//! of the pair's thickness. Pairs that share a vertex, or lie that close in the flat sheet,
//! are the sheet itself rather than contact, and take none. Continuous collision detection
//! cuts every Newton update short before any pair would reach its thickness.

pub(crate) mod ccd;
pub(crate) mod distance;

use std::collections::HashMap;

use nalgebra::{SMatrix, Vector3};

use crate::energy::{Barrier, Point};
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

    /// The gradient and Hessian of a pair's barrier over its four vertices, or `None` where the
    /// pair lies beyond the barrier's reach.
    pub(crate) fn pair_derivatives(
        &self,
        pair: &Pair,
        x: &[Point],
        thickness: f64,
    ) -> Option<(SMatrix<f64, 12, 1>, SMatrix<f64, 12, 12>)> {
        let points = pair.at(x);
        if distance::squared(pair.kind, &points) >= (thickness + self.gap).powi(2) {
            return None;
        }
        let (squared, gradient, hessian) = distance::derivatives(pair.kind, &points);
        let [_, slope, curvature] = self.pair(squared, thickness)?;
        Some((
            gradient * slope,
            gradient * gradient.transpose() * curvature + hessian * slope,
        ))
    }
}

/// The pairs whose primitives may come within `margin` of each other while the vertices move
/// from `start` to `end`.
///
/// A pair is a candidate where its boxes over the motion, grown by `margin`, overlap. Pairs
/// that share a vertex, or lie within `margin` of each other in one flat sheet, are left out.
pub(crate) fn candidates(
    primitives: &Primitives,
    start: &[Point],
    end: &[Point],
    margin: f64,
) -> Vec<Pair> {
    let boxes: Vec<(Point, Point)> = start
        .iter()
        .zip(end)
        .map(|(a, b)| (a.inf(b), a.sup(b)))
        .collect();
    let bound = |vertices: &[u32]| {
        let mut low = boxes[vertices[0] as usize].0;
        let mut high = boxes[vertices[0] as usize].1;
        for &v in &vertices[1..] {
            low = low.inf(&boxes[v as usize].0);
            high = high.sup(&boxes[v as usize].1);
        }
        let grow = Vector3::repeat(margin);
        (low - grow, high + grow)
    };
    let cell = cell_size(primitives, margin);
    let mut pairs = Vec::new();
    // Points against triangles.
    let triangle_boxes: Vec<_> = primitives.triangles.iter().map(|t| bound(t)).collect();
    let grid = Grid::new(cell, &triangle_boxes);
    for (v, point) in boxes.iter().enumerate() {
        let v = u32::try_from(v).expect("a scene counts its vertices in 32 bits");
        for t in grid.near(point) {
            let triangle = primitives.triangles[t];
            if !triangle.contains(&v) && overlap(point, &triangle_boxes[t]) {
                pairs.push(Pair {
                    kind: Kind::PointTriangle,
                    vertices: [v, triangle[0], triangle[1], triangle[2]],
                });
            }
        }
    }
    // Edges against edges, each pair once.
    let edge_boxes: Vec<_> = primitives.edges.iter().map(|e| bound(e)).collect();
    let grid = Grid::new(cell, &edge_boxes);
    for (e, (low, high)) in edge_boxes.iter().enumerate() {
        let shrunk = (
            low + Vector3::repeat(margin),
            high - Vector3::repeat(margin),
        );
        for f in grid.near(&shrunk) {
            let (a, b) = (primitives.edges[e], primitives.edges[f]);
            if f > e && !a.iter().any(|v| b.contains(v)) && overlap(&shrunk, &edge_boxes[f]) {
                pairs.push(Pair {
                    kind: Kind::EdgeEdge,
                    vertices: [a[0], a[1], b[0], b[1]],
                });
            }
        }
    }
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

/// Whether two boxes overlap.
fn overlap((low, high): &(Point, Point), (other_low, other_high): &(Point, Point)) -> bool {
    (0..3).all(|i| low[i] <= other_high[i] && other_low[i] <= high[i])
}

/// A cell size for the broad phase: the mean rest edge length, and no less than twice the margin.
fn cell_size(primitives: &Primitives, margin: f64) -> f64 {
    let total: f64 = primitives
        .edges
        .iter()
        .map(|[a, b]| (primitives.rest[*a as usize] - primitives.rest[*b as usize]).norm())
        .sum();
    #[expect(clippy::cast_precision_loss, reason = "a mean needs no exact count")]
    let mean = total / primitives.edges.len().max(1) as f64;
    mean.max(2.0 * margin).max(f64::MIN_POSITIVE)
}

/// A uniform grid of boxes, for finding the boxes near another.
struct Grid {
    cell: f64,
    cells: HashMap<[i64; 3], Vec<usize>>,
}

impl Grid {
    fn new(cell: f64, boxes: &[(Point, Point)]) -> Self {
        let mut grid = Self {
            cell,
            cells: HashMap::new(),
        };
        for (i, (low, high)) in boxes.iter().enumerate() {
            for key in grid.keys(low, high) {
                grid.cells.entry(key).or_default().push(i);
            }
        }
        grid
    }

    /// The cells a box covers.
    fn keys(&self, low: &Point, high: &Point) -> Vec<[i64; 3]> {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "cells index a bounded scene"
        )]
        let index = |value: f64| (value / self.cell).floor() as i64;
        let (a, b) = (
            [0, 1, 2].map(|i| index(low[i])),
            [0, 1, 2].map(|i| index(high[i])),
        );
        let mut keys = Vec::new();
        for i in a[0]..=b[0] {
            for j in a[1]..=b[1] {
                for k in a[2]..=b[2] {
                    keys.push([i, j, k]);
                }
            }
        }
        keys
    }

    /// The boxes sharing a cell with a box, each once, in increasing order.
    fn near(&self, (low, high): &(Point, Point)) -> Vec<usize> {
        let mut found: Vec<usize> = self
            .keys(low, high)
            .iter()
            .filter_map(|key| self.cells.get(key))
            .flatten()
            .copied()
            .collect();
        found.sort_unstable();
        found.dedup();
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let pairs = candidates(&primitives, &x, &x, 0.1);
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
            !candidates(&primitives, &apart, &apart, 0.1)
                .iter()
                .any(both)
        );
    }

    #[test]
    fn the_barrier_reaches_only_to_the_gap() {
        let contact = Contact {
            gap: 1e-5,
            stiffness: 1e-6,
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
