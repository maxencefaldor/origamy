//! The creases along lines and segments of a crease pattern, as `origamy.actions.along`
//! finds them.
//!
//! The creases group by the line through each, and each group sorts along its line.
//! A segment then takes the run of its line's creases that lie between its ends.

use std::cmp::Ordering;

use rayon::prelude::*;
use rustc_hash::FxHashMap;

use crate::field::{Number, Tower};
use crate::geometry::{Line, Point};

/// A line or a segment of the crease pattern to find creases along.
pub enum Along {
    /// Every crease on the line through two points.
    Line(Point, Point),
    /// Every crease on a segment, between its two ends.
    Segment(Point, Point),
}

/// The creases along each line or segment, by their indices in `edges`, in order.
///
/// `edges` are the creases, each as two vertices. A line or segment with two equal
/// points has no crease along it. The edges find their lines, and the queries their
/// creases, on every core.
///
/// # Panics
///
/// When the edges number 2³² or more, or an edge's two vertices are equal.
#[must_use]
pub fn along(
    tower: &Tower,
    vertices: &[Point],
    edges: &[[u32; 2]],
    queries: &[Along],
) -> Vec<Vec<u32>> {
    let placed: Vec<(Line, Number, Number)> = edges
        .par_iter()
        .map(|&[u, v]| {
            let (p, q) = (&vertices[u as usize], &vertices[v as usize]);
            let line = Line::through(p, q, tower);
            let (a, b) = (place(&line, p), place(&line, q));
            if a.compare(&b, tower) == Ordering::Greater {
                (line, b, a)
            } else {
                (line, a, b)
            }
        })
        .collect();
    let mut groups: FxHashMap<Line, Vec<(Number, Number, u32)>> = FxHashMap::default();
    for (index, (line, low, high)) in (0u32..).zip(placed) {
        groups.entry(line).or_default().push((low, high, index));
    }
    groups
        .par_iter_mut()
        .for_each(|(_, group)| group.sort_by(|x, y| x.0.compare(&y.0, tower)));
    queries
        .par_iter()
        .map(|query| {
            let (p, q, segment) = match query {
                Along::Line(p, q) => (p, q, false),
                Along::Segment(p, q) => (p, q, true),
            };
            if p == q {
                return vec![];
            }
            let line = Line::through(p, q, tower);
            let Some(group) = groups.get(&line) else {
                return vec![];
            };
            let mut found: Vec<u32> = if segment {
                let (a, b) = (place(&line, p), place(&line, q));
                let (low, high) = if a.compare(&b, tower) == Ordering::Greater {
                    (b, a)
                } else {
                    (a, b)
                };
                let start = group
                    .partition_point(|(start, _, _)| start.compare(&low, tower) == Ordering::Less);
                group[start..]
                    .iter()
                    .take_while(|(start, _, _)| start.compare(&high, tower) != Ordering::Greater)
                    .filter(|(_, end, _)| end.compare(&high, tower) != Ordering::Greater)
                    .map(|&(_, _, index)| index)
                    .collect()
            } else {
                group.iter().map(|&(_, _, index)| index).collect()
            };
            found.sort_unstable();
            found
        })
        .collect()
}

/// Where a point of a line lies along it: its y on an upright line, its x otherwise.
fn place(line: &Line, p: &Point) -> Number {
    if line.normal.1.is_zero() {
        p.y.clone()
    } else {
        p.x.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: i128, y: i128, tower: &Tower) -> Point {
        Point::new(Number::integer(x), Number::integer(y), tower)
    }

    #[test]
    fn a_segment_takes_the_creases_between_its_ends() {
        let tower = Tower::new(vec![]);
        let vertices: Vec<Point> = [(0, 0), (1, 0), (2, 0), (3, 0), (0, 1)]
            .iter()
            .map(|&(x, y)| point(x, y, &tower))
            .collect();
        let edges = [[2, 3], [0, 1], [1, 2], [0, 4]];
        let queries = [
            Along::Segment(point(0, 0, &tower), point(2, 0, &tower)),
            Along::Segment(point(3, 0, &tower), point(1, 0, &tower)),
            Along::Line(point(5, 0, &tower), point(7, 0, &tower)),
            Along::Segment(point(0, 3, &tower), point(0, -1, &tower)),
            Along::Segment(point(1, 1, &tower), point(2, 2, &tower)),
        ];
        let found = along(&tower, &vertices, &edges, &queries);
        assert_eq!(
            found,
            vec![vec![1, 2], vec![0, 2], vec![0, 1, 2], vec![3], vec![]]
        );
    }
}
