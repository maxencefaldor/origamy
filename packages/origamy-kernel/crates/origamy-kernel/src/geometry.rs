//! Exact points, lines, isometries and convex polygons, as `origamy.geometry` defines
//! them.
//!
//! Every predicate decides by float estimates where their error bounds leave the
//! answer clear, and by exact arithmetic otherwise.

#![allow(
    clippy::many_single_char_names,
    reason = "an isometry's entries are a to f, as its matrix writes them"
)]

use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use crate::field::{Estimate, Number, Tower};

/// A point of the plane, or a vector, with its coordinates' estimates.
#[derive(Clone, Debug)]
pub struct Point {
    /// The x coordinate.
    pub x: Number,
    /// The y coordinate.
    pub y: Number,
    ex: Estimate,
    ey: Estimate,
}

impl PartialEq for Point {
    fn eq(&self, other: &Point) -> bool {
        self.x == other.x && self.y == other.y
    }
}

impl Eq for Point {}

impl Hash for Point {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.x.hash(state);
        self.y.hash(state);
    }
}

impl Point {
    /// The point with these coordinates.
    #[must_use]
    pub fn new(x: Number, y: Number, tower: &Tower) -> Point {
        let (ex, ey) = (tower.estimate(&x), tower.estimate(&y));
        Point { x, y, ex, ey }
    }

    /// The coordinates as floats.
    #[must_use]
    pub fn floats(&self) -> (f64, f64) {
        (self.ex.value, self.ey.value)
    }

    /// The coordinates' estimates.
    #[must_use]
    pub fn estimates(&self) -> (Estimate, Estimate) {
        (self.ex, self.ey)
    }

    /// The vector from `other` to this point.
    #[must_use]
    pub fn minus(&self, other: &Point, tower: &Tower) -> Point {
        Point::new(self.x.minus(&other.x), self.y.minus(&other.y), tower)
    }

    /// The sum of two vectors.
    #[must_use]
    pub fn plus(&self, other: &Point, tower: &Tower) -> Point {
        Point::new(self.x.plus(&other.x), self.y.plus(&other.y), tower)
    }

    /// The vector scaled by a number.
    #[must_use]
    pub fn scaled(&self, factor: &Number, tower: &Tower) -> Point {
        Point::new(
            self.x.times(factor, tower),
            self.y.times(factor, tower),
            tower,
        )
    }

    /// The dot product.
    #[must_use]
    pub fn dot(&self, other: &Point, tower: &Tower) -> Number {
        self.x
            .times(&other.x, tower)
            .plus(&self.y.times(&other.y, tower))
    }

    /// The z component of the cross product: positive when `other` turns left.
    #[must_use]
    pub fn cross(&self, other: &Point, tower: &Tower) -> Number {
        self.x
            .times(&other.y, tower)
            .minus(&self.y.times(&other.x, tower))
    }
}

/// 1, -1 or 0 as `r` lies left of, right of or on the line from `p` to `q`.
#[must_use]
pub fn orientation(p: &Point, q: &Point, r: &Point, tower: &Tower) -> i32 {
    let estimate =
        q.ex.minus(p.ex)
            .times(r.ey.minus(p.ey))
            .minus(q.ey.minus(p.ey).times(r.ex.minus(p.ex)));
    if let Some(sign) = estimate.sign() {
        return sign;
    }
    if r == p || r == q || p == q {
        return 0;
    }
    let (ux, uy) = (q.x.minus(&p.x), q.y.minus(&p.y));
    let (vx, vy) = (r.x.minus(&p.x), r.y.minus(&p.y));
    ux.times(&vy, tower)
        .minus(&uy.times(&vx, tower))
        .exact_sign(tower)
}

/// The points `p` with `normal · p == offset`, the normal's first nonzero coordinate 1.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Line {
    /// The normal, scaled so equal lines compare equal.
    pub normal: (Number, Number),
    /// The offset along the normal.
    pub offset: Number,
}

impl Line {
    /// The line through two distinct points.
    ///
    /// # Panics
    ///
    /// When the points are equal.
    #[must_use]
    pub fn through(p: &Point, q: &Point, tower: &Tower) -> Line {
        assert!(p != q, "a line needs two distinct points");
        let (nx, ny) = (p.y.minus(&q.y), q.x.minus(&p.x));
        // The normal scales to a first coordinate of 1, or to (0, 1) for a level line.
        if nx.is_zero() {
            return Line {
                normal: (Number::zero(), Number::one()),
                offset: p.y.clone(),
            };
        }
        if ny.is_zero() {
            return Line {
                normal: (Number::one(), Number::zero()),
                offset: p.x.clone(),
            };
        }
        let slope = ny.over(&nx, tower);
        let offset = p.x.plus(&slope.times(&p.y, tower));
        Line {
            normal: (Number::one(), slope),
            offset,
        }
    }

    /// `normal · p - offset`: zero on the line, its sign each side's.
    #[must_use]
    pub fn value(&self, p: &Point, tower: &Tower) -> Number {
        self.normal
            .0
            .times(&p.x, tower)
            .plus(&self.normal.1.times(&p.y, tower))
            .minus(&self.offset)
    }

    /// 1 or -1 as a point lies on the normal's side or the other, 0 on the line.
    #[must_use]
    pub fn side(&self, p: &Point, tower: &Tower) -> i32 {
        let (nx, ny) = (
            tower.estimate(&self.normal.0),
            tower.estimate(&self.normal.1),
        );
        let estimate = nx
            .times(p.ex)
            .plus(ny.times(p.ey))
            .minus(tower.estimate(&self.offset));
        estimate
            .sign()
            .unwrap_or_else(|| self.value(p, tower).sign(tower))
    }

    /// Whether two lines run parallel, or are one line.
    #[must_use]
    pub fn parallel(&self, other: &Line) -> bool {
        self.normal == other.normal
    }

    /// The point where two lines cross, or `None` when they run parallel: Cramer's rule
    /// on their equations.
    #[must_use]
    pub fn meet(&self, other: &Line, tower: &Tower) -> Option<Point> {
        let ((a, b), (c, d)) = (&self.normal, &other.normal);
        let determinant = a.times(d, tower).minus(&b.times(c, tower));
        if determinant.is_zero() {
            return None;
        }
        let inverse = determinant.inverse(tower);
        let x = self
            .offset
            .times(d, tower)
            .minus(&other.offset.times(b, tower));
        let y = a
            .times(&other.offset, tower)
            .minus(&c.times(&self.offset, tower));
        Some(Point::new(
            x.times(&inverse, tower),
            y.times(&inverse, tower),
            tower,
        ))
    }
}

/// The side of the line from `p` to `q`, as [`Line::side`] gives it, that lies on the
/// left of the direction from `p` to `q`.
///
/// The line's normal points left of that direction, scaled by its first nonzero
/// coordinate: `p.y - q.y`, or else `q.x - p.x`.
#[must_use]
pub fn left_of(p: &Point, q: &Point, tower: &Tower) -> i32 {
    match p.y.compare(&q.y, tower) {
        Ordering::Greater => 1,
        Ordering::Less => -1,
        Ordering::Equal => match q.x.compare(&p.x, tower) {
            Ordering::Greater => 1,
            Ordering::Less => -1,
            Ordering::Equal => 0,
        },
    }
}

/// The map `p ↦ M p + t` with `M` orthogonal: where folding puts a face.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Isometry {
    /// The matrix, row by row, then the translation.
    pub entries: [Number; 6],
}

impl Isometry {
    /// The map that moves nothing.
    #[must_use]
    pub fn identity() -> Isometry {
        let (one, zero) = (Number::one(), Number::zero());
        Isometry {
            entries: [
                one.clone(),
                zero.clone(),
                zero.clone(),
                one,
                zero.clone(),
                zero,
            ],
        }
    }

    /// The reflection across a line.
    #[must_use]
    pub fn reflection(line: &Line, tower: &Tower) -> Isometry {
        let (nx, ny) = &line.normal;
        let two = Number::integer(2);
        let scale = two.over(&nx.times(nx, tower).plus(&ny.times(ny, tower)), tower);
        let (sx, sy) = (scale.times(nx, tower), scale.times(ny, tower));
        let one = Number::one();
        let xy = sx.times(ny, tower).negated();
        Isometry {
            entries: [
                one.minus(&sx.times(nx, tower)),
                xy.clone(),
                xy,
                one.minus(&sy.times(ny, tower)),
                line.offset.times(&sx, tower),
                line.offset.times(&sy, tower),
            ],
        }
    }

    /// Where the map puts a point.
    #[must_use]
    pub fn apply(&self, p: &Point, tower: &Tower) -> Point {
        let [xx, xy, yx, yy, tx, ty] = &self.entries;
        Point::new(
            xx.times(&p.x, tower).plus(&xy.times(&p.y, tower)).plus(tx),
            yx.times(&p.x, tower).plus(&yy.times(&p.y, tower)).plus(ty),
            tower,
        )
    }

    /// The composition: `other` first, then this map.
    #[must_use]
    pub fn after(&self, other: &Isometry, tower: &Tower) -> Isometry {
        let [a, b, c, d, e, f] = &self.entries;
        let [p, q, r, s, t, u] = &other.entries;
        let dot = |x: &Number, y: &Number, z: &Number, w: &Number| {
            x.times(y, tower).plus(&z.times(w, tower))
        };
        Isometry {
            entries: [
                dot(a, p, b, r),
                dot(a, q, b, s),
                dot(c, p, d, r),
                dot(c, q, d, s),
                dot(a, t, b, u).plus(e),
                dot(c, t, d, u).plus(f),
            ],
        }
    }

    /// Whether the map turns the paper over.
    #[must_use]
    pub fn flips(&self, tower: &Tower) -> bool {
        let [a, b, c, d, _, _] = &self.entries;
        a.times(d, tower).minus(&b.times(c, tower)).sign(tower) < 0
    }
}

/// A convex polygon, its corners counterclockwise.
pub type Polygon = Vec<Point>;

/// The sides of a polygon, each from a corner to the next.
pub fn sides(polygon: &[Point]) -> impl Iterator<Item = (&Point, &Point)> {
    polygon.iter().zip(polygon.iter().cycle().skip(1))
}

/// The smallest box around some points, in floats grown to hold every rounding.
#[must_use]
pub fn float_box(points: &[Point]) -> [f64; 4] {
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in points {
        b[0] = b[0].min(p.ex.value - p.ex.error);
        b[1] = b[1].min(p.ey.value - p.ey.error);
        b[2] = b[2].max(p.ex.value + p.ex.error);
        b[3] = b[3].max(p.ey.value + p.ey.error);
    }
    b
}

/// Whether two boxes, as [`float_box`] gives them, meet or touch.
#[must_use]
pub fn bounds_meet(a: &[f64; 4], b: &[f64; 4]) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

/// Whether two convex polygons of positive area share some area: unless a side of one
/// has all of the other on or beyond it.
#[must_use]
pub fn overlaps(first: &[Point], second: &[Point], tower: &Tower) -> bool {
    !apart(first, second, tower) && !apart(second, first, tower)
}

fn apart(polygon: &[Point], other: &[Point], tower: &Tower) -> bool {
    sides(polygon).any(|(p, q)| other.iter().all(|r| orientation(p, q, r, tower) <= 0))
}

/// Whether a point lies inside a convex polygon or on its boundary.
#[must_use]
pub fn covers(polygon: &[Point], point: &Point, tower: &Tower) -> bool {
    sides(polygon).all(|(p, q)| orientation(p, q, point, tower) >= 0)
}

/// The intersection of two convex polygons, possibly empty or without area.
#[must_use]
pub fn intersection(first: &[Point], second: &[Point], tower: &Tower) -> Polygon {
    if first.iter().all(|p| covers(second, p, tower)) {
        return first.to_vec();
    }
    if second.iter().all(|p| covers(first, p, tower)) {
        return second.to_vec();
    }
    let mut result = first.to_vec();
    for (p, q) in sides(second) {
        if result.is_empty() {
            break;
        }
        let signs: Vec<i32> = result.iter().map(|r| orientation(p, q, r, tower)).collect();
        let along = q.minus(p, tower);
        result = clipped(
            &result,
            &signs,
            |r| along.cross(&r.minus(p, tower), tower),
            tower,
        );
    }
    result
}

/// The corners on the kept side, where `signs` is positive, and where sides cross.
fn clipped(
    polygon: &[Point],
    signs: &[i32],
    value: impl Fn(&Point) -> Number,
    tower: &Tower,
) -> Polygon {
    let mut result: Polygon = vec![];
    for (i, p) in polygon.iter().enumerate() {
        let j = (i + 1) % polygon.len();
        if signs[i] >= 0 {
            result.push(p.clone());
        }
        if signs[i] * signs[j] < 0 {
            let (vp, vq) = (value(p), value(&polygon[j]));
            let share = vp.over(&vp.minus(&vq), tower);
            result.push(p.plus(&polygon[j].minus(p, tower).scaled(&share, tower), tower));
        }
    }
    let mut distinct: Polygon = vec![];
    for (i, p) in result.iter().enumerate() {
        if *p != result[(i + result.len() - 1) % result.len()] {
            distinct.push(p.clone());
        }
    }
    distinct
}

/// Twice the signed area of a polygon: positive when its corners run counterclockwise.
#[must_use]
pub fn twice_area<'p>(polygon: impl Iterator<Item = &'p Point> + Clone, tower: &Tower) -> Number {
    let mut total = Number::zero();
    for (p, q) in polygon.clone().zip(polygon.cycle().skip(1)) {
        total = total.plus(&p.cross(q, tower));
    }
    total
}

/// Whether the segment from `p` to `q` runs through a polygon's inside.
#[must_use]
pub fn through_inside(polygon: &[Point], p: &Point, q: &Point, tower: &Tower) -> bool {
    let (mut left, mut right) = (false, false);
    for corner in polygon {
        match orientation(p, q, corner, tower) {
            1 => left = true,
            -1 => right = true,
            _ => {}
        }
    }
    left && right
        && !sides(polygon)
            .any(|(a, b)| orientation(a, b, p, tower) <= 0 && orientation(a, b, q, tower) <= 0)
}

/// Whether two segments on one line share a stretch of positive length.
#[must_use]
pub fn collinear_overlap(first: (&Point, &Point), second: (&Point, &Point), tower: &Tower) -> bool {
    let (p, q) = first;
    if orientation(p, q, second.0, tower) != 0 || orientation(p, q, second.1, tower) != 0 {
        return false;
    }
    let direction = q.minus(p, tower);
    let length = direction.dot(&direction, tower);
    let mut b = [
        direction.dot(&second.0.minus(p, tower), tower),
        direction.dot(&second.1.minus(p, tower), tower),
    ];
    if b[0].compare(&b[1], tower).is_gt() {
        b.swap(0, 1);
    }
    let low = if b[0].sign(tower) > 0 {
        b[0].clone()
    } else {
        Number::zero()
    };
    let high = if b[1].compare(&length, tower).is_lt() {
        b[1].clone()
    } else {
        length
    };
    low.compare(&high, tower).is_lt()
}

/// The signed area, in floats: positive when the corners run counterclockwise.
#[must_use]
pub fn float_area(polygon: &[(f64, f64)]) -> f64 {
    polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .map(|(&(x, y), &(u, v))| x * v - y * u)
        .sum::<f64>()
        / 2.0
}

/// A convex polygon clipped to a convex window, both counterclockwise, in floats.
#[must_use]
pub fn clip_floats(polygon: &[(f64, f64)], window: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut result = polygon.to_vec();
    for (&(px, py), &(qx, qy)) in window.iter().zip(window.iter().cycle().skip(1)) {
        if result.is_empty() {
            break;
        }
        let side: Vec<f64> = result
            .iter()
            .map(|&(x, y)| (qx - px) * (y - py) - (qy - py) * (x - px))
            .collect();
        let mut kept = vec![];
        for (i, &(x, y)) in result.iter().enumerate() {
            let j = (i + 1) % result.len();
            if side[i] >= 0.0 {
                kept.push((x, y));
            }
            if side[i] * side[j] < 0.0 {
                let t = side[i] / (side[i] - side[j]);
                let (u, v) = result[j];
                kept.push((x + t * (u - x), y + t * (v - y)));
            }
        }
        result = kept;
    }
    result
}
