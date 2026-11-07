//! The exact corners of a crease pattern that a file gives as floats, as
//! `origamy.pattern` reads them.
//!
//! A corner whose coordinates both snap surely takes them. Any other corner lies where
//! two lines of creases cross, each through exact corners; corners found so let more
//! be found. Snapping within a looser fit then finds the corners left, and lets
//! crossings find more, until none is left.
//!
//! A snapped number may lie in a field whose roots no level of the tower holds yet,
//! which adds levels. Corners and slopes build their numbers in the order the reading
//! meets them, so the tower's levels come in that order. Crossings run on every core,
//! and a crossing that needs a new level runs again, in order, once the level is added.

use std::cmp::Ordering;

use num_bigint::BigInt;
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use smallvec::smallvec;

use crate::field::{Number, Tower};
use crate::geometry::{Line, Point};
use crate::int::Int;
use crate::snap::{Bounds, Explanation, Field, explain};

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

/// Where a file's floats lie: the lowest leftmost corner of the box around them, and
/// the box's longer side, which reading scales to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// The box's lowest leftmost corner.
    pub origin: (f64, f64),
    /// The box's longer side.
    pub side: f64,
}

/// The exact point of each corner of some segments given as floats, in the tower grown
/// by the roots they need, and each segment's two corners.
///
/// Each end scales to the frame. Its key, rounded to ten decimal places as Python's
/// `round` rounds, makes one corner written twice read as one, and corners number in
/// the order the segments first meet them. Straight runs and crossings measure keys,
/// and snapping explains each corner's floats as first met.
///
/// # Errors
///
/// [`Unexplained`] names a corner that snaps to no exact point, where no two lines of
/// creases through exact corners cross.
///
/// # Panics
///
/// When the corners number 2³² or more.
pub fn read(
    segments: &[[(f64, f64); 2]],
    frame: &Frame,
    reading: &Reading,
    tower: &mut Tower,
) -> Result<(Vec<Point>, Vec<[u32; 2]>), Unexplained> {
    let (keys, floats, ends) = corners(segments, frame);
    let mut order: Vec<usize> = (0..keys.len()).collect();
    order.sort_by(|&a, &b| by_floats(keys[a], keys[b]));
    let edges: Vec<(u32, u32)> = ends.iter().map(|&[u, v]| (u, v)).collect();
    let mut reader = Reader::new(
        order.iter().map(|&i| keys[i]).collect(),
        order.iter().map(|&i| floats[i]).collect(),
        &ranked(&order, &edges),
        reading,
        tower,
    );
    let exact = reader.exact(tower)?;
    let mut points: Vec<Option<Point>> = vec![None; keys.len()];
    for (point, &i) in exact.into_iter().zip(&order) {
        points[i] = Some(point);
    }
    Ok((points.into_iter().flatten().collect(), ends))
}

/// Each corner's key and first floats, scaled to the frame, and each segment's corners.
fn corners(segments: &[[(f64, f64); 2]], frame: &Frame) -> CornerData {
    let ((left, bottom), side) = (frame.origin, frame.side);
    let mut index: FxHashMap<(u64, u64), u32> = FxHashMap::default();
    let (mut keys, mut floats) = (vec![], vec![]);
    let ends = segments
        .iter()
        .map(|segment| {
            segment.map(|(x, y)| {
                let float = ((x - left) / side, (y - bottom) / side);
                // Adding 0 turns -0 into 0, which Python's keys take as one.
                let key = (rounded(float.0) + 0.0, rounded(float.1) + 0.0);
                *index
                    .entry((key.0.to_bits(), key.1.to_bits()))
                    .or_insert_with(|| {
                        keys.push(key);
                        floats.push(float);
                        u32::try_from(keys.len() - 1).expect("corners fit in 32 bits")
                    })
            })
        })
        .collect();
    (keys, floats, ends)
}

/// Corners' keys and floats, and segments' corners.
type CornerData = (Vec<(f64, f64)>, Vec<(f64, f64)>, Vec<[u32; 2]>);

/// A unit in the last place of a key: keys round to ten decimal places.
const KEY: f64 = 1e-10;

/// A float rounded to ten decimal places, as Python's `round(x, 10)` rounds it: to the
/// nearest decimal, ties to even, then to the nearest float.
fn rounded(value: f64) -> f64 {
    const SCALE: u128 = 10_000_000_000;
    if !value.is_finite() {
        return value;
    }
    let bits = value.abs().to_bits();
    let exponent = i64::try_from(bits >> 52).expect("eleven bits");
    let fraction = bits & ((1 << 52) - 1);
    let (mantissa, power) = if exponent == 0 {
        (fraction, -1074)
    } else {
        (fraction | 1 << 52, exponent - 1075)
    };
    // A whole float, past 2^52, rounds to itself.
    if power >= 0 {
        return value;
    }
    let shift = power.unsigned_abs();
    let scaled = u128::from(mantissa) * SCALE;
    let nearest = if shift >= 128 {
        // The float lies under 2^-75, so it rounds to 0.
        0
    } else {
        let (whole, rest, half) = (
            scaled >> shift,
            scaled & ((1 << shift) - 1),
            1 << (shift - 1),
        );
        whole + u128::from(rest > half || (rest == half && whole & 1 == 1))
    };
    #[allow(
        clippy::cast_precision_loss,
        reason = "the integers are exact under 2^53"
    )]
    let magnitude = if nearest < 1 << 53 {
        nearest as f64 / SCALE as f64
    } else {
        crate::snap::ratio(&BigInt::from(nearest), &BigInt::from(SCALE))
    };
    magnitude.copysign(value)
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

/// The fields that snapped numbers may lie in, each with the products of its roots once
/// levels of the tower hold them.
struct Fields(Vec<(Field, Option<Vec<Number>>)>);

impl Fields {
    fn new(fields: &[Field], tower: &Tower) -> Fields {
        let mut found = Fields(fields.iter().map(|field| (field.clone(), None)).collect());
        found.refresh(tower);
        found
    }

    /// The products of a field's roots, or `Err` with the field when no level holds
    /// one of its roots yet.
    fn get(&self, field: usize) -> Result<&[Number], usize> {
        self.0[field].1.as_deref().ok_or(field)
    }

    /// Add the levels a field's roots need, and find the roots the others hold.
    fn grow(&mut self, field: usize, tower: &mut Tower) {
        let mut products = vec![Number::one()];
        for radicand in &self.0[field].0.0 {
            let root = combination(radicand, &products, tower).sqrt(tower);
            let above: Vec<Number> = products.iter().map(|p| p.times(&root, tower)).collect();
            products.extend(above);
        }
        self.0[field].1 = Some(products);
        self.refresh(tower);
    }

    fn refresh(&mut self, tower: &Tower) {
        for (field, products) in &mut self.0 {
            if products.is_none() {
                *products = roots(field, tower);
            }
        }
    }
}

/// The products of a field's roots, when levels of the tower hold every root.
fn roots(field: &Field, tower: &Tower) -> Option<Vec<Number>> {
    let mut products = vec![Number::one()];
    for radicand in &field.0 {
        let root = combination(radicand, &products, tower).root(tower)?;
        let above: Vec<Number> = products.iter().map(|p| p.times(&root, tower)).collect();
        products.extend(above);
    }
    Some(products)
}

/// The sum of integers times numbers.
fn combination(integers: &[i64], numbers: &[Number], tower: &Tower) -> Number {
    integers
        .iter()
        .zip(numbers)
        .fold(Number::zero(), |sum, (&integer, number)| {
            sum.plus(&Number::integer(i128::from(integer)).times(number, tower))
        })
}

/// A candidate as a number, or `Err` with its field when no level holds one of the
/// field's roots yet.
fn number(candidate: &Explanation, fields: &Fields, tower: &Tower) -> Result<Number, usize> {
    let denominator = Int::from_big(candidate.denominator.clone());
    let Some(field) = candidate.field else {
        return Ok(Number::new(
            smallvec![Int::from_big(candidate.numerators[0].clone())],
            denominator,
        ));
    };
    let products = fields.get(field)?;
    let sum = candidate.numerators.iter().zip(products).fold(
        Number::zero(),
        |sum, (numerator, product)| {
            let term = Number::new(smallvec![Int::from_big(numerator.clone())], Int::ONE);
            sum.plus(&term.times(product, tower))
        },
    );
    Ok(sum.times(&Number::new(smallvec![Int::ONE], denominator), tower))
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
    fields: Fields,
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
            fields: Fields::new(&reading.bounds.fields, tower),
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
                match number(candidate, &self.fields, tower) {
                    Ok(built) => break built,
                    Err(field) => self.fields.grow(field, tower),
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
        let outcomes: Vec<Result<Option<Point>, usize>> = {
            let (reader, tower) = (&*self, &*tower);
            left.par_iter()
                .map(|&corner| reader.crossing(corner, exact, tower))
                .collect()
        };
        let mut found = vec![];
        for (&corner, mut outcome) in left.iter().zip(outcomes) {
            while let Err(field) = outcome {
                self.fields.grow(field, tower);
                outcome = self.crossing(corner, exact, tower);
            }
            if let Ok(Some(point)) = outcome {
                found.push((corner, point));
            }
        }
        found
    }

    /// Where two lines of creases through exact corners cross at a corner, when they
    /// cross within `near` of its key; or `Err` with a field one of whose roots no
    /// level of the tower holds yet.
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
    ) -> Result<Option<Point>, usize> {
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
            // Keys hold ten decimal places, so a slope, at most 1, measures its value to
            // a unit in their last place over the run.
            let bounds = Bounds {
                tolerance: self.reading.bounds.tolerance / length,
                spread: KEY / dx.abs().max(dy.abs()),
                ..self.reading.bounds.clone()
            };
            let Some(candidate) = explain(if steep { dx / dy } else { dy / dx }, &bounds) else {
                continue;
            };
            let slope = number(&candidate, &self.fields, tower)?;
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
                spread: 0.0,
                sure: 1e-8,
                denominator: 1_000_000,
                fields: vec![
                    Field(vec![vec![2]]),
                    Field(vec![vec![3]]),
                    Field(vec![vec![5]]),
                    Field(vec![vec![6]]),
                    Field(vec![vec![2], vec![2, 1]]),
                ],
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

    const UNIT: Frame = Frame {
        origin: (0.0, 0.0),
        side: 1.0,
    };

    #[test]
    fn corners_snap_and_add_the_roots_they_need() {
        let half = 2f64.sqrt() / 2.0;
        let segments = [[(0.0, 0.0), (half, 0.5)], [(half, 0.5), (1.0, 1.0)]];
        let mut tower = Tower::new(vec![]);
        let (points, ends) = read(&segments, &UNIT, &reading(), &mut tower).unwrap();
        assert_eq!(ends, [[0, 1], [1, 2]]);
        assert_eq!(tower.radicands(), &[Number::integer(2)]);
        let root_half = Number::new(smallvec![Int::ZERO, Int::ONE], Int::from(2i64));
        assert_eq!(points[1], Point::new(root_half, rational(1, 2), &tower));
        assert_eq!(points[2], Point::new(Number::one(), Number::one(), &tower));
    }

    #[test]
    fn corners_of_eleven_and_a_quarter_degrees_add_nested_roots() {
        // cos 22.5° / 2 is √(2 + √2) / 4, which needs √2 and then √(2 + √2).
        let quarter = (2.0 + 2f64.sqrt()).sqrt() / 4.0;
        let segments = [[(0.0, 0.0), (quarter, 0.5)]];
        let mut tower = Tower::new(vec![]);
        let (points, _) = read(&segments, &UNIT, &reading(), &mut tower).unwrap();
        let nested = Number::new(smallvec![Int::from(2i64), Int::ONE], Int::ONE);
        assert_eq!(tower.radicands(), &[Number::integer(2), nested]);
        let x = Number::new(
            smallvec![Int::ZERO, Int::ZERO, Int::ONE, Int::ZERO],
            Int::from(4i64),
        );
        assert_eq!(points[1], Point::new(x, rational(1, 2), &tower));
    }

    #[test]
    fn a_corner_written_twice_reads_as_one() {
        let segments = [[(0.0, 0.0), (2.0, 1.0)], [(2.0 + 1e-12, 1.0), (2.0, 2.0)]];
        let frame = Frame {
            origin: (0.0, 0.0),
            side: 2.0,
        };
        let mut tower = Tower::new(vec![]);
        let (points, ends) = read(&segments, &frame, &reading(), &mut tower).unwrap();
        assert_eq!(ends, [[0, 1], [1, 2]]);
        assert_eq!(points[1], Point::new(Number::one(), rational(1, 2), &tower));
    }

    #[test]
    fn a_corner_no_number_explains_lies_where_lines_of_creases_cross() {
        // Two diagonals of a square of side 1/3, shifted by 1/7: their crossing at
        // (1/7 + 1/6, 1/6) holds a coordinate with denominator 42, within the fit of
        // many fractions; a pinch of noise leaves the snapped fraction unsure.
        let (a, b) = (1.0 / 7.0, 1.0 / 7.0 + 1.0 / 3.0);
        let middle = (1.0 / 7.0 + 1.0 / 6.0 + 3e-10, 1.0 / 6.0 + 3e-10);
        let corners = [(a, 0.0), (b, 1.0 / 3.0), (b, 0.0), (a, 1.0 / 3.0)];
        let segments = corners.map(|corner| [corner, middle]);
        let mut tower = Tower::new(vec![]);
        let reading = Reading {
            bounds: Bounds {
                tolerance: 1e-12,
                ..reading().bounds
            },
            ..reading()
        };
        let (points, ends) = read(&segments, &UNIT, &reading, &mut tower).unwrap();
        let x = rational(1, 7).plus(&rational(1, 6));
        assert_eq!(ends[0], [0, 1]);
        assert_eq!(points[1], Point::new(x, rational(1, 6), &tower));
    }

    #[test]
    fn a_corner_nothing_explains_is_named() {
        let segments = [[(0.0, 0.0), (0.123_456_789, 0.5)]];
        let mut tower = Tower::new(vec![]);
        let outcome = read(&segments, &UNIT, &reading(), &mut tower);
        assert_eq!(
            outcome,
            Err(Unexplained {
                value: 0.123_456_789,
                corner: (0.123_456_789, 0.5)
            })
        );
    }

    #[test]
    fn keys_round_as_python_rounds() {
        // Python's round(x, 10) for each float, ties among them to even.
        let cases = [
            (0.000_488_281_25, 0.000_488_281_2),
            (0.001_464_843_75, 0.001_464_843_8),
            (0.1 + 0.2, 0.3),
            (2.0 / 3.0, 0.666_666_666_7),
            (0.123_456_789_012_345, 0.123_456_789),
            (1e-11, 0.0),
            (5e-11, 1e-10),
            (1.5e-10, 1e-10),
            (2.5e-10, 3e-10),
            (0.999_999_999_95, 0.999_999_999_9),
            (0.299_999_999_95, 0.299_999_999_9),
            (0.123_456_789_05, 0.123_456_789_1),
            (0.618_033_988_749_894_9, 0.618_033_988_7),
            (1.0, 1.0),
            (1e300, 1e300),
            (-2.0 / 3.0, -0.666_666_666_7),
        ];
        for (value, expected) in cases {
            assert_eq!(rounded(value).to_bits(), f64::to_bits(expected), "{value}");
        }
        assert_eq!(rounded(-1e-12).to_bits(), (-0.0f64).to_bits());
    }

    proptest::proptest! {
        #[test]
        fn rounding_matches_exact_arithmetic(value in 0.0f64..2e6) {
            let (n, d) = crate::snap::exact(value);
            let scaled = n * BigInt::from(10_000_000_000u64);
            let (whole, rest) = num_integer::Integer::div_rem(&scaled, &d);
            let twice = rest * 2;
            let up = twice > d || (twice == d && num_integer::Integer::is_odd(&whole));
            let nearest = whole + BigInt::from(u8::from(up));
            let expected = crate::snap::ratio(&nearest, &BigInt::from(10_000_000_000u64));
            proptest::prop_assert_eq!(rounded(value).to_bits(), expected.to_bits());
        }
    }
}
