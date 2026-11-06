//! The exact corners of a crease pattern that a file gives as floats, as
//! `origamy.pattern` reads them.
//!
//! A corner whose coordinates both snap surely takes them. Any other corner lies where
//! two lines of creases cross, each through exact corners; corners found so let more
//! be found. Snapping within a looser fit then finds the corners left, and lets
//! crossings find more, until none is left.
//!
//! A snapped number may hold a square root that no level of the tower holds, which
//! adds a level. Corners and slopes build their numbers in the order the reading meets
//! them, so the tower's levels come in that order. Crossings run on every core, and a
//! crossing that needs a new level runs again, in order, once the level is added.

use std::cmp::Ordering;

use num_bigint::BigInt;
use rayon::prelude::*;
use smallvec::smallvec;

use crate::field::{Number, Tower};
use crate::geometry::{Line, Point};
use crate::int::Int;
use crate::snap::{Bounds, Explanation, explain};

/// The inputs of reading, besides the corners.
#[derive(Clone, Debug)]
pub struct Reading {
    /// The bounds on snapping a coordinate, whose sure fit is the fit reading takes
    /// first.
    pub bounds: Bounds,
    /// The fit reading loosens to once crossings find no more corners, and the fit a
    /// snapped slope needs.
    pub fit: f64,
    /// How far apart two float points may lie and still be one.
    pub near: f64,
}

/// A corner that snaps to no exact point, where no two lines of creases cross.
#[derive(Clone, Debug, PartialEq)]
pub struct Unexplained {
    /// A coordinate of the corner that snaps to no exact number.
    pub value: f64,
    /// The corner, as the file gives it.
    pub corner: (f64, f64),
}

/// The exact point of each corner, in the tower grown by the roots they need.
///
/// `keys` are the corners as floats rounded so that one corner written twice reads as
/// one, and `floats` are the corners as the file first gives them. `edges` are the
/// creases between corners, as indices. Straight runs and crossings measure keys, and
/// snapping explains floats.
///
/// # Errors
///
/// [`Unexplained`] names a corner that snaps to no exact point, where no two lines of
/// creases through exact corners cross.
///
/// # Panics
///
/// When `keys` and `floats` differ in length, or an edge names no corner.
pub fn read(
    keys: &[(f64, f64)],
    floats: &[(f64, f64)],
    edges: &[(u32, u32)],
    reading: &Reading,
    tower: &mut Tower,
) -> Result<Vec<Point>, Unexplained> {
    assert_eq!(keys.len(), floats.len(), "each corner has a key and floats");
    let mut order: Vec<usize> = (0..keys.len()).collect();
    order.sort_by(|&a, &b| by_floats(keys[a], keys[b]));
    let mut reader = Reader::new(
        order.iter().map(|&i| keys[i]).collect(),
        order.iter().map(|&i| floats[i]).collect(),
        &ranked(&order, edges),
        reading,
        tower,
    );
    let exact = reader.exact(tower)?;
    let mut points: Vec<Option<Point>> = vec![None; keys.len()];
    for (point, &i) in exact.into_iter().zip(&order) {
        points[i] = Some(point);
    }
    Ok(points.into_iter().flatten().collect())
}

/// Floats in order by x, then by y.
fn by_floats(p: (f64, f64), q: (f64, f64)) -> Ordering {
    p.0.total_cmp(&q.0).then(p.1.total_cmp(&q.1))
}

/// The edges between corners renumbered by their rank in `order`.
fn ranked(order: &[usize], edges: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut rank = vec![0; order.len()];
    for (r, &i) in order.iter().enumerate() {
        rank[i] = u32::try_from(r).expect("corners fit in 32 bits");
    }
    edges
        .iter()
        .map(|&(u, v)| (rank[u as usize], rank[v as usize]))
        .collect()
}

/// The roots that snapped numbers may hold, each once a level of the tower holds it.
struct Roots(Vec<(u32, Option<Number>)>);

impl Roots {
    fn new(roots: &[u32], tower: &Tower) -> Roots {
        let mut found = Roots(roots.iter().map(|&r| (r, None)).collect());
        found.refresh(tower);
        found
    }

    /// The root of an integer, or `Err` with the integer when no level holds it yet.
    fn get(&self, integer: u32) -> Result<&Number, u32> {
        self.0
            .iter()
            .find(|(r, _)| *r == integer)
            .and_then(|(_, root)| root.as_ref())
            .ok_or(integer)
    }

    /// Add the level an integer's root needs, and find the roots it holds.
    fn grow(&mut self, integer: u32, tower: &mut Tower) {
        let root = Number::integer(i128::from(integer)).sqrt(tower);
        if let Some(entry) = self.0.iter_mut().find(|(r, _)| *r == integer) {
            entry.1 = Some(root);
        }
        self.refresh(tower);
    }

    fn refresh(&mut self, tower: &Tower) {
        for (integer, root) in &mut self.0 {
            if root.is_none() {
                *root = Number::integer(i128::from(*integer)).root(tower);
            }
        }
    }
}

/// A candidate as a number, or `Err` with the integer under its root when no level
/// holds that root yet.
fn number(candidate: &Explanation, roots: &Roots, tower: &Tower) -> Result<Number, u32> {
    let rational = fraction(&candidate.rational);
    if candidate.surd.0 == BigInt::ZERO {
        return Ok(rational);
    }
    let root = roots.get(candidate.root)?;
    Ok(rational.plus(&fraction(&candidate.surd).times(root, tower)))
}

fn fraction((numerator, denominator): &(BigInt, BigInt)) -> Number {
    Number::new(
        smallvec![Int::from_big(numerator.clone())],
        Int::from_big(denominator.clone()),
    )
}

/// The corners in order by key, their creases, and what snapping found of them.
struct Reader<'r> {
    keys: Vec<(f64, f64)>,
    floats: Vec<(f64, f64)>,
    /// Each corner's neighbors along creases, in order.
    near: Vec<Vec<u32>>,
    reading: &'r Reading,
    /// Each corner's coordinates' best candidates, x then y.
    candidates: Vec<[Option<Explanation>; 2]>,
    /// Each corner's coordinates' numbers, once built.
    numbers: Vec<[Option<Number>; 2]>,
    roots: Roots,
}

impl<'r> Reader<'r> {
    fn new(
        keys: Vec<(f64, f64)>,
        floats: Vec<(f64, f64)>,
        edges: &[(u32, u32)],
        reading: &'r Reading,
        tower: &Tower,
    ) -> Reader<'r> {
        let mut near = vec![vec![]; keys.len()];
        for &(u, v) in edges {
            near[u as usize].push(v);
            near[v as usize].push(u);
        }
        for neighbors in &mut near {
            neighbors.sort_unstable();
            neighbors.dedup();
        }
        let mut values: Vec<f64> = floats.iter().flat_map(|&(x, y)| [x, y]).collect();
        values.sort_by(f64::total_cmp);
        values.dedup();
        let explained: Vec<Option<Explanation>> = values
            .par_iter()
            .map(|&value| explain(value, &reading.bounds))
            .collect();
        let candidate = |value: f64| {
            let i = values
                .binary_search_by(|v| v.total_cmp(&value))
                .expect("every coordinate is explained");
            explained[i].clone()
        };
        let candidates = floats
            .iter()
            .map(|&(x, y)| [candidate(x), candidate(y)])
            .collect();
        Reader {
            numbers: vec![[None, None]; keys.len()],
            keys,
            floats,
            near,
            reading,
            candidates,
            roots: Roots::new(&reading.bounds.roots, tower),
        }
    }

    /// The exact point of each corner, in order by key.
    fn exact(&mut self, tower: &mut Tower) -> Result<Vec<Point>, Unexplained> {
        let mut exact: Vec<Option<Point>> = vec![None; self.keys.len()];
        let mut left: Vec<usize> = (0..self.keys.len()).collect();
        let mut fit = self.reading.bounds.sure;
        while !left.is_empty() {
            let mut found = vec![];
            for &corner in &left {
                if let Some(x) = self.snapped(corner, 0, fit, tower)
                    && let Some(y) = self.snapped(corner, 1, fit, tower)
                {
                    found.push((corner, Point::new(x, y, tower)));
                }
            }
            if found.is_empty() {
                found = self.crossings(&left, &exact, tower);
            }
            if !found.is_empty() {
                for (corner, point) in found {
                    exact[corner] = Some(point);
                }
                left.retain(|&corner| exact[corner].is_none());
            } else if fit < self.reading.fit {
                fit = self.reading.fit;
            } else {
                let corner = left[0];
                let (x, y) = self.floats[corner];
                let value = if self.snapped(corner, 0, fit, tower).is_none() {
                    x
                } else {
                    y
                };
                return Err(Unexplained {
                    value,
                    corner: (x, y),
                });
            }
        }
        Ok(exact.into_iter().flatten().collect())
    }

    /// The number a coordinate snaps to within a fit, built the first time it is
    /// asked for, whether or not it fits.
    fn snapped(
        &mut self,
        corner: usize,
        axis: usize,
        fit: f64,
        tower: &mut Tower,
    ) -> Option<Number> {
        let candidate = self.candidates[corner][axis].as_ref()?;
        if self.numbers[corner][axis].is_none() {
            let built = loop {
                match number(candidate, &self.roots, tower) {
                    Ok(built) => break built,
                    Err(integer) => self.roots.grow(integer, tower),
                }
            };
            self.numbers[corner][axis] = Some(built);
        }
        if candidate.fit <= fit {
            self.numbers[corner][axis].clone()
        } else {
            None
        }
    }

    /// The corners left that lie where two lines of creases cross, with their points.
    fn crossings(
        &mut self,
        left: &[usize],
        exact: &[Option<Point>],
        tower: &mut Tower,
    ) -> Vec<(usize, Point)> {
        let outcomes: Vec<Result<Option<Point>, u32>> = {
            let (reader, tower) = (&*self, &*tower);
            left.par_iter()
                .map(|&corner| reader.crossing(corner, exact, tower))
                .collect()
        };
        let mut found = vec![];
        for (&corner, mut outcome) in left.iter().zip(outcomes) {
            while let Err(integer) = outcome {
                self.roots.grow(integer, tower);
                outcome = self.crossing(corner, exact, tower);
            }
            if let Ok(Some(point)) = outcome {
                found.push((corner, point));
            }
        }
        found
    }

    /// Where two lines of creases through exact corners cross at a corner, when they
    /// cross within `near` of its key; or `Err` with an integer whose root no level of
    /// the tower holds yet.
    ///
    /// A line runs from the corner along creases that continue straight, in both
    /// directions, through two exact corners on its way. Failing two such lines, a
    /// crease to an exact corner gives a line too, when its slope snaps to an exact
    /// number.
    fn crossing(
        &self,
        corner: usize,
        exact: &[Option<Point>],
        tower: &Tower,
    ) -> Result<Option<Point>, u32> {
        let mut first: Option<Line> = None;
        let mut meet = |line: Line| -> Option<Option<Point>> {
            match &first {
                Some(other) if other.parallel(&line) => None,
                Some(other) => {
                    let point = other.meet(&line, tower)?;
                    let ((x, y), (kx, ky)) = (point.floats(), self.keys[corner]);
                    let near = (x - kx).abs().max((y - ky).abs()) <= self.reading.near;
                    Some(near.then_some(point))
                }
                None => {
                    first = Some(line);
                    None
                }
            }
        };
        let neighbors = &self.near[corner];
        for &start in neighbors {
            let start = start as usize;
            let opposite = neighbors
                .iter()
                .map(|&n| n as usize)
                .find(|&other| self.straight_on(start, corner, other));
            let mut known: [Option<&Point>; 2] = [None, None];
            for direction in std::iter::once(start).chain(opposite) {
                self.straight(corner, direction, |on| {
                    if let Some(point) = &exact[on] {
                        lowest(&mut known, point);
                    }
                });
            }
            if let [Some(p), Some(q)] = known
                && let Some(found) = meet(Line::through(p, q, tower))
            {
                return Ok(found);
            }
        }
        for &start in neighbors {
            let Some(point) = &exact[start as usize] else {
                continue;
            };
            let ((kx, ky), (sx, sy)) = (self.keys[corner], self.keys[start as usize]);
            let (dx, dy) = (kx - sx, ky - sy);
            let length = dx.hypot(dy);
            if length == 0.0 {
                continue;
            }
            let steep = dy.abs() > dx.abs();
            let bounds = Bounds {
                tolerance: self.reading.bounds.tolerance / length,
                ..self.reading.bounds.clone()
            };
            let Some(candidate) = explain(if steep { dx / dy } else { dy / dx }, &bounds) else {
                continue;
            };
            let slope = number(&candidate, &self.roots, tower)?;
            if candidate.fit > self.reading.fit {
                continue;
            }
            let along = if steep {
                Point::new(slope, Number::one(), tower)
            } else {
                Point::new(Number::one(), slope, tower)
            };
            let line = Line::through(point, &point.plus(&along, tower), tower);
            if let Some(found) = meet(line) {
                return Ok(found);
            }
        }
        Ok(None)
    }

    /// Visit the corners from a corner's neighbor on, while the creases run straight.
    fn straight(&self, corner: usize, start: usize, mut visit: impl FnMut(usize)) {
        let (mut previous, mut current) = (corner, Some(start));
        while let Some(on) = current {
            visit(on);
            let next = self.near[on]
                .iter()
                .map(|&n| n as usize)
                .find(|&n| self.straight_on(previous, on, n));
            (previous, current) = (on, next);
        }
    }

    /// Whether the path from corner a through b to c runs straight on at b.
    fn straight_on(&self, a: usize, b: usize, c: usize) -> bool {
        let (a, b, c) = (self.keys[a], self.keys[b], self.keys[c]);
        let (ux, uy) = (b.0 - a.0, b.1 - a.1);
        let (vx, vy) = (c.0 - b.0, c.1 - b.1);
        let lengths = ux.hypot(uy) * vx.hypot(vy);
        lengths > 0.0
            && ux * vx + uy * vy > 0.0
            && (ux * vy - uy * vx).abs() <= self.reading.near * lengths
    }
}

/// Keep the two lowest distinct points by their floats, as a point arrives.
fn lowest<'p>(known: &mut [Option<&'p Point>; 2], point: &'p Point) {
    let floats = point.floats();
    let below = |other: Option<&Point>| {
        other.is_none_or(|other| by_floats(floats, other.floats()) == Ordering::Less)
    };
    if known.iter().flatten().any(|&other| other == point) {
        return;
    }
    if below(known[0]) {
        known[1] = known[0];
        known[0] = Some(point);
    } else if below(known[1]) {
        known[1] = Some(point);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading() -> Reading {
        Reading {
            bounds: Bounds {
                tolerance: 1e-9,
                sure: 1e-8,
                denominator: 1_000_000,
                roots: vec![2, 3, 5, 6],
                scale: 1_000_000_000_000,
                size: 10_000,
            },
            fit: 1e-4,
            near: 1e-7,
        }
    }

    fn rational(numerator: i128, denominator: i128) -> Number {
        Number::new(smallvec![Int::from(numerator)], Int::from(denominator))
    }

    #[test]
    fn corners_snap_and_add_the_roots_they_need() {
        let half = 2f64.sqrt() / 2.0;
        let keys = [(0.0, 0.0), (half, 0.5), (1.0, 1.0)];
        let mut tower = Tower::new(vec![]);
        let points = read(&keys, &keys, &[(0, 1), (1, 2)], &reading(), &mut tower).unwrap();
        assert_eq!(tower.radicands(), &[Number::integer(2)]);
        let root_half = Number::new(smallvec![Int::ZERO, Int::ONE], Int::from(2i64));
        assert_eq!(points[1], Point::new(root_half, rational(1, 2), &tower));
        assert_eq!(points[2], Point::new(Number::one(), Number::one(), &tower));
    }

    #[test]
    fn a_corner_no_number_explains_lies_where_lines_of_creases_cross() {
        // Two diagonals of a square of side 1/3, shifted by 1/7: their crossing at
        // (1/7 + 1/6, 1/6) holds a coordinate with denominator 42, within the fit of
        // many fractions; a pinch of noise leaves the snapped fraction unsure.
        let (a, b) = (1.0 / 7.0, 1.0 / 7.0 + 1.0 / 3.0);
        let middle = (1.0 / 7.0 + 1.0 / 6.0 + 3e-10, 1.0 / 6.0 + 3e-10);
        let keys = [(a, 0.0), (b, 1.0 / 3.0), (b, 0.0), (a, 1.0 / 3.0), middle];
        let edges = [(0, 4), (4, 1), (2, 4), (4, 3)];
        let mut tower = Tower::new(vec![]);
        let reading = Reading {
            bounds: Bounds {
                tolerance: 1e-12,
                ..reading().bounds
            },
            ..reading()
        };
        let points = read(&keys, &keys, &edges, &reading, &mut tower).unwrap();
        let x = rational(1, 7).plus(&rational(1, 6));
        assert_eq!(points[4], Point::new(x, rational(1, 6), &tower));
    }

    #[test]
    fn a_corner_nothing_explains_is_named() {
        let keys = [(0.0, 0.0), (0.123_456_789, 0.5)];
        let mut tower = Tower::new(vec![]);
        let outcome = read(&keys, &keys, &[(0, 1)], &reading(), &mut tower);
        assert_eq!(
            outcome,
            Err(Unexplained {
                value: 0.123_456_789,
                corner: (0.123_456_789, 0.5)
            })
        );
    }
}
