//! Continuous collision detection: the largest fraction of a motion that keeps every pair at
//! least its thickness apart.
//!
//! Additive collision detection, from codimensional incremental potential contact, advances
//! each pair by steps no longer than its gap divided by the fastest relative motion, so it
//! never steps past a contact. The detection is conservative: it may stop early, never late.

use super::{Pair, distance};
use crate::energy::Point;

/// The fraction of the gap each pair keeps at the end of its motion.
const KEEP: f64 = 0.1;

/// The most advances one pair takes before it stops where it is.
const ADVANCES: usize = 1000;

/// The fraction, at most 1, of the motion from `x` along `displacement` that keeps the pair at
/// least `thickness` apart.
pub(crate) fn pair(pair: &Pair, x: &[Point], displacement: &[Point], thickness: f64) -> f64 {
    let mut points = pair.at(x);
    let mut moves = pair.at(displacement);
    let mean = moves.iter().sum::<Point>() / 4.0;
    for m in &mut moves {
        *m -= mean;
    }
    let length = |p: &Point| p.norm();
    let reach = match pair.kind {
        distance::Kind::PointTriangle => {
            length(&moves[0])
                + length(&moves[1])
                    .max(length(&moves[2]))
                    .max(length(&moves[3]))
        }
        distance::Kind::EdgeEdge => {
            length(&moves[0]).max(length(&moves[1])) + length(&moves[2]).max(length(&moves[3]))
        }
    };
    let gap = |points: &[Point; 4]| distance::squared(pair.kind, points).sqrt() - thickness;
    let first = gap(&points);
    if first <= 0.0 {
        return 0.0;
    }
    if reach == 0.0 {
        return 1.0;
    }
    let least = KEEP * first;
    let mut fraction = 0.0;
    let mut advance = (1.0 - KEEP) * first / reach;
    for _ in 0..ADVANCES {
        for (p, m) in points.iter_mut().zip(&moves) {
            *p += m * advance;
        }
        let now = gap(&points);
        if fraction > 0.0 && now < least {
            return fraction;
        }
        fraction += advance;
        if fraction >= 1.0 {
            return 1.0;
        }
        advance = 0.9 * now / reach;
    }
    fraction
}

/// The fraction, at most 1, of a vertex's fall of `drop` toward the table that keeps its half
/// thickness above the table, from a height `height` over the table.
pub(crate) fn table(height: f64, drop: f64, half_thickness: f64) -> f64 {
    let gap = height - half_thickness;
    if gap <= 0.0 {
        return 0.0;
    }
    if drop <= 0.0 {
        return 1.0;
    }
    ((1.0 - KEEP) * gap / drop).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contact::distance::Kind;
    use proptest::prelude::*;

    fn point() -> impl Strategy<Value = Point> {
        (-1.0..1.0f64, -1.0..1.0f64, -1.0..1.0f64).prop_map(|(a, b, c)| Point::new(a, b, c))
    }

    proptest! {
        #[test]
        fn no_pair_passes_its_thickness_before_the_safe_fraction(
            start in proptest::array::uniform4(point()),
            moves in proptest::array::uniform4(point()),
            edges in proptest::bool::ANY,
        ) {
            let kind = if edges { Kind::EdgeEdge } else { Kind::PointTriangle };
            let thickness = 0.05;
            let x: Vec<Point> = start.to_vec();
            let displacement: Vec<Point> = moves.iter().map(|m| m * 3.0).collect();
            let pair = Pair { kind, vertices: [0, 1, 2, 3] };
            prop_assume!(distance::squared(kind, &pair.at(&x)) > thickness * thickness);
            let safe = super::pair(&pair, &x, &displacement, thickness);
            prop_assert!((0.0..=1.0).contains(&safe));
            for k in 0..=400 {
                let t = safe * f64::from(k) / 400.0;
                let points: Vec<Point> = x.iter().zip(&displacement).map(|(p, m)| p + m * t).collect();
                let gap = distance::squared(kind, &pair.at(&points)).sqrt() - thickness;
                prop_assert!(gap > 0.0, "the pair passes its thickness at {t} before {safe}");
            }
        }
    }

    #[test]
    fn a_point_falling_on_a_triangle_stops_short_of_it() {
        let x = [
            Point::new(0.2, 0.2, 1.0),
            Point::new(0.0, 0.0, 0.0),
            Point::new(1.0, 0.0, 0.0),
            Point::new(0.0, 1.0, 0.0),
        ];
        let fall = [
            Point::new(0.0, 0.0, -2.0),
            Point::zeros(),
            Point::zeros(),
            Point::zeros(),
        ];
        let pair = Pair {
            kind: Kind::PointTriangle,
            vertices: [0, 1, 2, 3],
        };
        let safe = super::pair(&pair, &x, &fall, 0.1);
        // The point reaches the thickness after 0.45 of the fall.
        assert!(safe > 0.3 && safe < 0.45, "{safe}");
        assert!((table(1.0, 2.0, 0.1) - 0.405).abs() < 1e-12);
    }
}
