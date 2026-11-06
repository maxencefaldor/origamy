//! The overlay of folded footprints: the cells their sides cut the plane into, and the
//! footprints that cover each.
//!
//! Crossing an edge of the overlay leaves the footprints whose inside lies on the side
//! crossed from, and enters those whose inside lies ahead. Walking from the outside
//! across every edge gives every cell its cover. A cell whose every edge leaves some
//! footprint covers more than any neighbor: a region where the most footprints meet.
//! Every three faces that share area meet in such a region, and so does every pair.
//!
//! Each crease, once folded, lies along sides of its faces' footprints, so it follows
//! edges of the overlay. A footprint whose inside the crease runs through covers the
//! cells on both sides of one of those edges.

use std::collections::VecDeque;

use rustc_hash::FxHashSet;

use crate::arrangement::{Arrangement, Segment};
use crate::field::{Number, Tower};
use crate::geometry::{Point, Polygon, left_of, orientation, sides};

/// The overlay of some footprints, with the creases that lie along their sides.
pub struct Overlay {
    /// The arrangement of the footprints' sides.
    pub arrangement: Arrangement,
    /// The footprints covering each face of the arrangement, sorted.
    pub covers: Vec<Vec<u32>>,
    /// The covers of the regions where the most footprints meet, each cover once.
    pub regions: Vec<Vec<u32>>,
    /// For each crease, the footprints whose inside it runs through, sorted.
    pub crossed: Vec<Vec<u32>>,
    /// For each crease, its line, by index among the arrangement's lines.
    pub crease_lines: Vec<u32>,
}

impl Overlay {
    /// The overlay of footprints, and where each crease runs through them.
    ///
    /// Each crease, as its two ends once folded, must lie along footprints' sides.
    ///
    /// # Panics
    ///
    /// When the footprints and creases number 2³² or more, or a segment is a point.
    #[must_use]
    pub fn new(footprints: &[Polygon], creases: &[(Point, Point)], tower: &Tower) -> Overlay {
        let count = u32::try_from(footprints.len()).expect("footprints fit in 32 bits");
        let mut segments = vec![];
        // A footprint runs counterclockwise, so its inside lies left of each side.
        for (k, shape) in (0..count).zip(footprints) {
            for (p, q) in sides(shape) {
                let side = left_of(p, q, tower);
                segments.push(Segment {
                    start: p.clone(),
                    end: q.clone(),
                    tag: k,
                    side,
                });
            }
        }
        // A crease takes a tag past the footprints' and no side: it changes no cover.
        for (c, (p, q)) in creases.iter().enumerate() {
            let tag = count + u32::try_from(c).expect("creases fit in 32 bits");
            segments.push(Segment {
                start: p.clone(),
                end: q.clone(),
                tag,
                side: 0,
            });
        }
        let arrangement = Arrangement::new(&segments, tower);
        let first = segments.len() - creases.len();
        let crease_lines = arrangement.segment_lines[first..].to_vec();
        let covers = covers(&arrangement, &segments, footprints, tower);
        let mut found = FxHashSet::default();
        let mut regions = vec![];
        for (face, cycle) in arrangement.faces.iter().enumerate() {
            if !arrangement.bounded[face] {
                continue;
            }
            let most = cycle.iter().all(|&h| {
                let left = arrangement.left_side(h, tower);
                arrangement.edges[(h / 2) as usize]
                    .segments
                    .iter()
                    .any(|&s| segments[s as usize].side == left)
            });
            if most && found.insert(covers[face].clone()) {
                regions.push(covers[face].clone());
            }
        }
        let mut crossed = vec![vec![]; creases.len()];
        for (e, edge) in arrangement.edges.iter().enumerate() {
            let on: Vec<usize> = edge
                .segments
                .iter()
                .map(|&s| segments[s as usize].tag)
                .filter(|&tag| tag >= count)
                .map(|tag| (tag - count) as usize)
                .collect();
            if on.is_empty() {
                continue;
            }
            let left = &covers[arrangement.face_of[2 * e] as usize];
            let right = &covers[arrangement.face_of[2 * e + 1] as usize];
            let both = common(left, right);
            for c in on {
                crossed[c].extend_from_slice(&both);
            }
        }
        for through in &mut crossed {
            through.sort_unstable();
            through.dedup();
        }
        Overlay {
            arrangement,
            covers,
            regions,
            crossed,
            crease_lines,
        }
    }

    /// Every pair of footprints that share area, the lesser first, sorted.
    #[must_use]
    pub fn meeting(&self) -> Vec<(u32, u32)> {
        let count = self
            .regions
            .iter()
            .flatten()
            .max()
            .map_or(0, |&k| k as usize + 1);
        let mut holding: Vec<Vec<u32>> = vec![vec![]; count];
        for (r, cover) in (0u32..).zip(&self.regions) {
            for &k in cover {
                holding[k as usize].push(r);
            }
        }
        // Each footprint gathers the greater footprints of its regions, once each.
        let mut seen = vec![u32::MAX; count];
        let (mut pairs, mut greater) = (vec![], vec![]);
        for (a, regions) in (0u32..).zip(&holding) {
            greater.clear();
            for &r in regions {
                for &b in &self.regions[r as usize] {
                    if b > a && seen[b as usize] != a {
                        seen[b as usize] = a;
                        greater.push(b);
                    }
                }
            }
            greater.sort_unstable();
            pairs.extend(greater.iter().map(|&b| (a, b)));
        }
        pairs
    }
}

/// The footprints in both sorted lists.
fn common(first: &[u32], second: &[u32]) -> Vec<u32> {
    let (mut i, mut j, mut both) = (0, 0, vec![]);
    while i < first.len() && j < second.len() {
        match first[i].cmp(&second[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                both.push(first[i]);
                i += 1;
                j += 1;
            }
        }
    }
    both
}

/// The footprints covering each face, sorted.
fn covers(
    arrangement: &Arrangement,
    segments: &[Segment],
    footprints: &[Polygon],
    tower: &Tower,
) -> Vec<Vec<u32>> {
    let faces = arrangement.faces.len();
    let mut covers: Vec<Option<Vec<u32>>> = vec![None; faces];
    let mut queue = VecDeque::new();
    // A face that is not bounded lies outside its piece of the overlay: it is the
    // outside, or a hole within some footprints, found at a point of its boundary.
    for (face, cover) in covers.iter_mut().enumerate() {
        if !arrangement.bounded[face] {
            *cover = Some(cover_beside(
                arrangement,
                arrangement.faces[face][0],
                segments,
                footprints,
                tower,
            ));
            queue.push_back(face);
        }
    }
    while let Some(face) = queue.pop_front() {
        let cover = covers[face]
            .clone()
            .expect("a face in the queue has its cover");
        for &h in &arrangement.faces[face] {
            let other = arrangement.face_of[(h ^ 1) as usize] as usize;
            if covers[other].is_some() {
                continue;
            }
            let left = arrangement.left_side(h, tower);
            let mut next = cover.clone();
            for &s in &arrangement.edges[(h / 2) as usize].segments {
                let segment = &segments[s as usize];
                if segment.side == 0 {
                    continue;
                }
                if segment.side == left {
                    next.retain(|&k| k != segment.tag);
                } else if let Err(at) = next.binary_search(&segment.tag) {
                    next.insert(at, segment.tag);
                }
            }
            covers[other] = Some(next);
            queue.push_back(other);
        }
    }
    covers
        .into_iter()
        .map(|cover| cover.expect("every face is reached"))
        .collect()
}

/// The footprints covering the area just left of a half-edge, at its middle.
fn cover_beside(
    arrangement: &Arrangement,
    half: u32,
    segments: &[Segment],
    footprints: &[Polygon],
    tower: &Tower,
) -> Vec<u32> {
    let (tail, head) = (
        &arrangement.points[arrangement.tail(half) as usize],
        &arrangement.points[arrangement.head(half) as usize],
    );
    let half_number = Number::integer(1).over(&Number::integer(2), tower);
    let middle = tail.plus(&head.minus(tail, tower).scaled(&half_number, tower), tower);
    let left = arrangement.left_side(half, tower);
    let edge = &arrangement.edges[(half / 2) as usize];
    let mut cover = vec![];
    for (k, shape) in footprints.iter().enumerate() {
        let tag = u32::try_from(k).expect("footprints fit in 32 bits");
        let strictly = sides(shape).all(|(p, q)| orientation(p, q, &middle, tower) > 0);
        // On the footprint's boundary, the middle lies on a side along the edge.
        let beside = edge
            .segments
            .iter()
            .any(|&s| segments[s as usize].tag == tag && segments[s as usize].side == left);
        if strictly || beside {
            cover.push(tag);
        }
    }
    cover
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: i128, y: i128, tower: &Tower) -> Point {
        Point::new(Number::integer(x), Number::integer(y), tower)
    }

    /// A rectangle from its lower left corner to its upper right one, counterclockwise.
    fn rectangle(r: [i128; 4], tower: &Tower) -> Polygon {
        let [left, bottom, right, top] = r;
        vec![
            point(left, bottom, tower),
            point(right, bottom, tower),
            point(right, top, tower),
            point(left, top, tower),
        ]
    }

    fn rectangles() -> impl proptest::strategy::Strategy<Value = Vec<[i128; 4]>> {
        proptest::collection::vec(
            (0..6i128, 0..6i128, 1..4i128, 1..4i128).prop_map(|(x, y, w, h)| [x, y, x + w, y + h]),
            1..6,
        )
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn footprints_meet_where_their_rectangles_share_area(shapes in rectangles()) {
            let tower = Tower::default();
            let mut distinct = shapes.clone();
            distinct.sort_unstable();
            distinct.dedup();
            let footprints: Vec<Polygon> = distinct.iter().map(|&r| rectangle(r, &tower)).collect();
            let overlay = Overlay::new(&footprints, &[], &tower);
            let mut expected = vec![];
            for (i, a) in distinct.iter().enumerate() {
                for (j, b) in distinct.iter().enumerate().skip(i + 1) {
                    if a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3] {
                        expected.push((u32::try_from(i).unwrap(), u32::try_from(j).unwrap()));
                    }
                }
            }
            prop_assert_eq!(overlay.meeting(), expected);
        }

        #[test]
        fn a_side_runs_through_the_rectangles_it_crosses_inside(shapes in rectangles(), pick in 0..64usize) {
            let tower = Tower::default();
            let mut distinct = shapes.clone();
            distinct.sort_unstable();
            distinct.dedup();
            let footprints: Vec<Polygon> = distinct.iter().map(|&r| rectangle(r, &tower)).collect();
            // A crease along the bottom side of one rectangle.
            let k = pick % distinct.len();
            let [left, bottom, right, _] = distinct[k];
            let crease = (point(left, bottom, &tower), point(right, bottom, &tower));
            let overlay = Overlay::new(&footprints, &[crease], &tower);
            let expected: Vec<u32> = distinct
                .iter()
                .enumerate()
                .filter(|(_, b)| b[1] < bottom && bottom < b[3] && b[0] < right && left < b[2])
                .map(|(j, _)| u32::try_from(j).unwrap())
                .collect();
            prop_assert_eq!(&overlay.crossed[0], &expected);
        }
    }
}
