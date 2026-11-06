//! The exact planar arrangement of segments: where they cross, the edges between, and
//! the faces they bound.
//!
//! Segments on one line merge into the stretches between their ends and crossings, each
//! stretch an edge that lists the segments covering it. A grid over the segments' boxes
//! finds the pairs that may cross, and exact predicates decide them. Each vertex orders
//! its edges by angle, and walking each edge's next on the left traces every face.

#![allow(
    clippy::many_single_char_names,
    reason = "the geometry names segments' ends a to d, as the formulas do"
)]

use std::cmp::Ordering;

use rayon::prelude::*;
use rustc_hash::FxHashMap;

use crate::field::{Number, Tower};
use crate::geometry::{Line, Point, float_box, left_of, orientation};

/// A segment, with a label and a side its caller cares about.
#[derive(Clone, Debug)]
pub struct Segment {
    /// One end.
    pub start: Point,
    /// The other end.
    pub end: Point,
    /// The caller's label.
    pub tag: u32,
    /// A side of the segment's line, as [`Line::side`] gives it, or 0.
    pub side: i32,
}

/// A stretch of a line between two vertices, and the segments that cover it.
#[derive(Clone, Debug)]
pub struct Edge {
    /// Its two vertices, in order along its line.
    pub ends: [u32; 2],
    /// Its line, by index among the arrangement's lines.
    pub line: u32,
    /// The segments covering it, by index.
    pub segments: Vec<u32>,
    /// The side of its line, as [`Line::side`] gives it, on the left of the way from
    /// its first vertex to its second.
    pub left: i32,
}

/// The stretches that segments make of their lines: an arrangement's vertices and
/// edges, before its faces.
pub struct Stretches {
    /// The vertices: segments' ends and crossings.
    pub points: Vec<Point>,
    /// The lines the segments lie on.
    pub lines: Vec<Line>,
    /// The line each segment lies on, by index among the lines, in the segments' order.
    pub segment_lines: Vec<u32>,
    /// The edges, line by line, each line's in order along it.
    pub edges: Vec<Edge>,
}

/// The arrangement of some segments.
pub struct Arrangement {
    /// The vertices: segments' ends and crossings.
    pub points: Vec<Point>,
    /// The lines the segments lie on.
    pub lines: Vec<Line>,
    /// The line each segment lies on, by index among the lines, in the segments' order.
    pub segment_lines: Vec<u32>,
    /// The edges.
    pub edges: Vec<Edge>,
    /// Each face as its half-edges in order, the face on their left. Half-edge `2e`
    /// runs along edge `e` from its first vertex, and `2e + 1` back.
    pub faces: Vec<Vec<u32>>,
    /// The face on the left of each half-edge.
    pub face_of: Vec<u32>,
    /// Whether each face is bounded: its corners run counterclockwise round area.
    pub bounded: Vec<bool>,
}

impl Arrangement {
    /// The arrangement of segments, none of them a single point.
    ///
    /// # Panics
    ///
    /// When a segment's ends are equal.
    #[must_use]
    pub fn new(segments: &[Segment], tower: &Tower) -> Arrangement {
        let Stretches {
            points,
            lines,
            segment_lines,
            edges,
        } = Stretches::new(segments, tower);
        let (faces, face_of, bounded) = trace(&points, &edges, tower);
        Arrangement {
            points,
            lines,
            segment_lines,
            edges,
            faces,
            face_of,
            bounded,
        }
    }
}

impl Stretches {
    /// The stretches of segments, none of them a single point.
    ///
    /// # Panics
    ///
    /// When a segment's ends are equal.
    #[must_use]
    pub fn new(segments: &[Segment], tower: &Tower) -> Stretches {
        // Every end and crossing once, by an id in the order first met.
        let mut seen = Seen::default();
        let ends: Vec<[u32; 2]> = segments
            .iter()
            .map(|s| [seen.id(&s.start), seen.id(&s.end)])
            .collect();
        let (distinct, copies) = distinct(&ends, &seen.points, tower);
        let through: Vec<Line> = distinct
            .par_iter()
            .map(|&[a, b]| Line::through(&seen.points[a as usize], &seen.points[b as usize], tower))
            .collect();
        let mut lines: Vec<Line> = vec![];
        let mut line_index: FxHashMap<Line, u32> = FxHashMap::default();
        let line_of: Vec<u32> = through
            .into_iter()
            .map(|line| {
                let next = u32::try_from(lines.len()).expect("lines fit in 32 bits");
                *line_index.entry(line).or_insert_with_key(|line| {
                    lines.push(line.clone());
                    next
                })
            })
            .collect();
        let mut on_line: Vec<Vec<u32>> = vec![vec![]; lines.len()];
        for (&[a, b], &l) in distinct.iter().zip(&line_of) {
            on_line[l as usize].extend([a, b]);
        }
        for (i, j, point) in crossings(&distinct, &seen.points, &line_of, &lines, tower) {
            let id = seen.id(&point);
            on_line[line_of[i] as usize].push(id);
            on_line[line_of[j] as usize].push(id);
        }
        let mut by_line: Vec<Vec<u32>> = vec![vec![]; lines.len()];
        for (s, &l) in line_of.iter().enumerate() {
            by_line[l as usize].push(u32::try_from(s).expect("segments fit in 32 bits"));
        }
        let points = &seen.points;
        // Each line's stretches, on every core.
        let stretches: Vec<Vec<Stretch>> = lines
            .par_iter()
            .zip(on_line)
            .zip(&by_line)
            .map(|((line, stops), on)| {
                stretches_along(line, stops, on, (&distinct, &copies), points, tower)
            })
            .collect();
        // The arrangement numbers its vertices as its edges first meet them.
        let mut number = vec![u32::MAX; points.len()];
        let mut numbered: Vec<Point> = vec![];
        let mut vertex = |id: u32| -> u32 {
            let slot = &mut number[id as usize];
            if *slot == u32::MAX {
                *slot = u32::try_from(numbered.len()).expect("points fit in 32 bits");
                numbered.push(points[id as usize].clone());
            }
            *slot
        };
        let mut edges = vec![];
        for (l, found) in stretches.into_iter().enumerate() {
            for ([from, to], segments, left) in found {
                edges.push(Edge {
                    ends: [vertex(from), vertex(to)],
                    line: u32::try_from(l).expect("lines fit in 32 bits"),
                    segments,
                    left,
                });
            }
        }
        let mut segment_lines = vec![0; segments.len()];
        for (k, copies) in copies.iter().enumerate() {
            for &s in copies {
                segment_lines[s as usize] = line_of[k];
            }
        }
        Stretches {
            points: numbered,
            lines,
            segment_lines,
            edges,
        }
    }
}

/// A stretch of a line: its ends' ids in order along the line, the segments covering
/// it, and the side of the line on its left.
type Stretch = ([u32; 2], Vec<u32>, i32);

/// The stretches of a line between its stops: the ends and crossings on it, by id.
/// `on` lists the distinct segments on the line, which `segments` gives with their
/// copies.
fn stretches_along(
    line: &Line,
    mut stops: Vec<u32>,
    on: &[u32],
    (distinct, copies): (&[[u32; 2]], &[Vec<u32>]),
    points: &[Point],
    tower: &Tower,
) -> Vec<Stretch> {
    let along = |id: &u32| {
        let p = &points[*id as usize];
        if line.normal.1.is_zero() { &p.y } else { &p.x }
    };
    stops.sort_unstable();
    stops.dedup();
    let by_id: Vec<u32> = stops.clone();
    stops.sort_by(|a, b| along(a).compare(along(b), tower));
    let mut position = vec![0; stops.len()];
    for (k, &id) in stops.iter().enumerate() {
        position[by_id.binary_search(&id).expect("a stop")] = k;
    }
    let at = |id: u32| position[by_id.binary_search(&id).expect("a stop")];
    // Each segment covers the stretches from its first stop to its last.
    let mut starting: Vec<Vec<u32>> = vec![vec![]; stops.len()];
    let mut ending: Vec<Vec<u32>> = vec![vec![]; stops.len()];
    for &s in on {
        let [a, b] = distinct[s as usize];
        let (a, b) = (at(a), at(b));
        starting[a.min(b)].push(s);
        ending[a.max(b)].push(s);
    }
    let mut active: Vec<u32> = vec![];
    let mut found = vec![];
    for k in 0..stops.len() {
        active.retain(|s| !ending[k].contains(s));
        active.extend(&starting[k]);
        if k + 1 < stops.len() && !active.is_empty() {
            let mut covering: Vec<u32> = active
                .iter()
                .flat_map(|&s| copies[s as usize].iter().copied())
                .collect();
            covering.sort_unstable();
            let (from, to) = (stops[k], stops[k + 1]);
            let left = left_of(&points[from as usize], &points[to as usize], tower);
            found.push(([from, to], covering, left));
        }
    }
    found
}

/// Points met so far, each once, by id.
#[derive(Default)]
struct Seen {
    points: Vec<Point>,
    ids: FxHashMap<Point, u32>,
}

impl Seen {
    /// A point's id, new if it is.
    fn id(&mut self, point: &Point) -> u32 {
        if let Some(&id) = self.ids.get(point) {
            return id;
        }
        let id = u32::try_from(self.points.len()).expect("points fit in 32 bits");
        self.points.push(point.clone());
        self.ids.insert(point.clone(), id);
        id
    }
}

impl Arrangement {
    /// The vertex a half-edge leaves from.
    #[must_use]
    pub fn tail(&self, half: u32) -> u32 {
        self.edges[(half / 2) as usize].ends[(half % 2) as usize]
    }

    /// The vertex a half-edge arrives at.
    #[must_use]
    pub fn head(&self, half: u32) -> u32 {
        self.edges[(half / 2) as usize].ends[1 - (half % 2) as usize]
    }

    /// The side of its line that a half-edge has on its left, as [`Line::side`] gives
    /// it.
    #[must_use]
    pub fn left_side(&self, half: u32) -> i32 {
        let left = self.edges[(half / 2) as usize].left;
        if half % 2 == 0 { left } else { -left }
    }
}

/// Each pair of segments on different lines that meet, and the point where they do.
fn crossings(
    segments: &[[u32; 2]],
    points: &[Point],
    line_of: &[u32],
    lines: &[Line],
    tower: &Tower,
) -> Vec<(usize, usize, Point)> {
    let ends = |s: usize| {
        let [a, b] = segments[s];
        (&points[a as usize], &points[b as usize])
    };
    let boxes: Vec<[f64; 4]> = (0..segments.len())
        .map(|s| {
            let (a, b) = ends(s);
            float_box(&[a.clone(), b.clone()])
        })
        .collect();
    let all = boxes.iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |b, s| {
            [
                b[0].min(s[0]),
                b[1].min(s[1]),
                b[2].max(s[2]),
                b[3].max(s[3]),
            ]
        },
    );
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let size = ((segments.len() as f64).sqrt().ceil() as usize).clamp(1, 4096);
    #[allow(clippy::cast_precision_loss)]
    let (width, height) = (
        ((all[2] - all[0]) / size as f64).max(f64::MIN_POSITIVE),
        ((all[3] - all[1]) / size as f64).max(f64::MIN_POSITIVE),
    );
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let cell = |x: f64, y: f64| -> (usize, usize) {
        (
            ((x - all[0]) / width).floor().clamp(0.0, (size - 1) as f64) as usize,
            ((y - all[1]) / height)
                .floor()
                .clamp(0.0, (size - 1) as f64) as usize,
        )
    };
    let mut grid: Vec<Vec<u32>> = vec![vec![]; size * size];
    for (s, b) in boxes.iter().enumerate() {
        let (x0, y0) = cell(b[0], b[1]);
        let (x1, y1) = cell(b[2], b[3]);
        for x in x0..=x1 {
            for y in y0..=y1 {
                grid[x * size + y].push(u32::try_from(s).expect("segments fit in 32 bits"));
            }
        }
    }
    // Cells decide their pairs on every core, and their crossings come in cell order.
    let found: Vec<Vec<(usize, usize, Point)>> = grid
        .par_iter()
        .enumerate()
        .map(|(c, members)| {
            let mut found = vec![];
            for (k, &i) in members.iter().enumerate() {
                for &j in &members[k + 1..] {
                    let (i, j) = (i as usize, j as usize);
                    let (a, b) = (&boxes[i], &boxes[j]);
                    if a[0] > b[2] || b[0] > a[2] || a[1] > b[3] || b[1] > a[3] {
                        continue;
                    }
                    // Each pair is decided in one cell: the one holding its boxes' meeting
                    // corner.
                    let (x, y) = cell(a[0].max(b[0]), a[1].max(b[1]));
                    if x * size + y != c
                        || line_of[i] == line_of[j]
                        || lines[line_of[i] as usize].parallel(&lines[line_of[j] as usize])
                    {
                        continue;
                    }
                    let lines = (&lines[line_of[i] as usize], &lines[line_of[j] as usize]);
                    if let Some(point) = meet(ends(i), ends(j), lines, tower) {
                        found.push((i, j, point));
                    }
                }
            }
            found
        })
        .collect();
    found.into_iter().flatten().collect()
}

/// Where two segments on crossing lines meet, if they do inside one of them: a meeting
/// at both their ends is no news.
fn meet(
    first: (&Point, &Point),
    second: (&Point, &Point),
    (l, m): (&Line, &Line),
    tower: &Tower,
) -> Option<Point> {
    let ((a, b), (c, d)) = (first, second);
    if a == c || a == d || b == c || b == d {
        return None;
    }
    let (o1, o2) = (orientation(a, b, c, tower), orientation(a, b, d, tower));
    if o1 * o2 > 0 {
        return None;
    }
    let (o3, o4) = (orientation(c, d, a, tower), orientation(c, d, b, tower));
    if o3 * o4 > 0 {
        return None;
    }
    Some(match (o1, o2, o3, o4) {
        (0, _, _, _) => c.clone(),
        (_, 0, _, _) => d.clone(),
        (_, _, 0, _) => a.clone(),
        (_, _, _, 0) => b.clone(),
        _ => l.meet(m, tower)?,
    })
}

/// The distinct segments, each with its ends' ids in order, and the copies of each:
/// segments with the same ends are one stretch of paper's edge, met once.
fn distinct(ends: &[[u32; 2]], points: &[Point], tower: &Tower) -> (Vec<[u32; 2]>, Vec<Vec<u32>>) {
    let mut distinct: Vec<[u32; 2]> = vec![];
    let mut copies: Vec<Vec<u32>> = vec![];
    let mut index: FxHashMap<[u32; 2], u32> = FxHashMap::default();
    for (s, &[a, b]) in ends.iter().enumerate() {
        let next = u32::try_from(distinct.len()).expect("segments fit in 32 bits");
        let k = *index.entry([a.min(b), a.max(b)]).or_insert_with(|| {
            distinct.push(if before(&points[a as usize], &points[b as usize], tower) {
                [a, b]
            } else {
                [b, a]
            });
            copies.push(vec![]);
            next
        });
        copies[k as usize].push(u32::try_from(s).expect("segments fit in 32 bits"));
    }
    (distinct, copies)
}

/// Whether a point comes before another, by x and then by y.
fn before(p: &Point, q: &Point, tower: &Tower) -> bool {
    p.x.compare(&q.x, tower)
        .then_with(|| p.y.compare(&q.y, tower))
        == Ordering::Less
}

/// Order directions counterclockwise from east.
fn by_angle(center: &Point, first: &Point, second: &Point, tower: &Tower) -> Ordering {
    let half = |p: &Point| -> u8 {
        let y = p.y.compare(&center.y, tower);
        u8::from(!(y.is_gt() || (y.is_eq() && p.x.compare(&center.x, tower).is_gt())))
    };
    half(first)
        .cmp(&half(second))
        .then_with(|| 0.cmp(&orientation(center, first, second, tower)))
}

type Traced = (Vec<Vec<u32>>, Vec<u32>, Vec<bool>);

/// Every face, as half-edges each with the face on its left.
fn trace(points: &[Point], edges: &[Edge], tower: &Tower) -> Traced {
    let mut leaving: Vec<Vec<u32>> = vec![vec![]; points.len()];
    for (e, edge) in edges.iter().enumerate() {
        let e = u32::try_from(e).expect("edges fit in 32 bits");
        leaving[edge.ends[0] as usize].push(2 * e);
        leaving[edge.ends[1] as usize].push(2 * e + 1);
    }
    let tail = |h: u32| edges[(h / 2) as usize].ends[(h % 2) as usize];
    let head = |h: u32| edges[(h / 2) as usize].ends[1 - (h % 2) as usize];
    leaving.par_iter_mut().enumerate().for_each(|(v, list)| {
        let center = &points[v];
        list.sort_by(|&a, &b| {
            by_angle(
                center,
                &points[head(a) as usize],
                &points[head(b) as usize],
                tower,
            )
        });
    });
    let mut rank = vec![0u32; 2 * edges.len()];
    for list in &leaving {
        for (k, &h) in list.iter().enumerate() {
            rank[h as usize] = u32::try_from(k).expect("degrees fit in 32 bits");
        }
    }
    // The face on the left of u→v goes on with the edge at v just clockwise of v→u.
    let next = |h: u32| -> u32 {
        let v = head(h) as usize;
        let list = &leaving[v];
        let back = rank[(h ^ 1) as usize] as usize;
        list[(back + list.len() - 1) % list.len()]
    };
    let mut face_of = vec![u32::MAX; 2 * edges.len()];
    let mut faces = vec![];
    let mut bounded = vec![];
    for start in 0..u32::try_from(2 * edges.len()).expect("half-edges fit in 32 bits") {
        if face_of[start as usize] != u32::MAX {
            continue;
        }
        let face = u32::try_from(faces.len()).expect("faces fit in 32 bits");
        let mut cycle = vec![];
        let mut h = start;
        loop {
            face_of[h as usize] = face;
            cycle.push(h);
            h = next(h);
            if h == start {
                break;
            }
        }
        let corners: Vec<&Point> = cycle.iter().map(|&h| &points[tail(h) as usize]).collect();
        bounded.push(area_sign(&corners, tower) > 0);
        faces.push(cycle);
    }
    (faces, face_of, bounded)
}

/// The sign of a polygon's area: positive when its corners run counterclockwise.
fn area_sign(corners: &[&Point], tower: &Tower) -> i32 {
    let mut estimate = crate::field::Estimate::exact(0.0);
    for (p, q) in corners.iter().zip(corners.iter().cycle().skip(1)) {
        let ((px, py), (qx, qy)) = (p.estimates(), q.estimates());
        estimate = estimate.plus(px.times(qy).minus(py.times(qx)));
    }
    if let Some(sign) = estimate.sign() {
        return sign;
    }
    let mut total = Number::zero();
    for (p, q) in corners.iter().zip(corners.iter().cycle().skip(1)) {
        total = total.plus(&p.cross(q, tower));
    }
    total.sign(tower)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: i128, y: i128, tower: &Tower) -> Point {
        Point::new(Number::integer(x), Number::integer(y), tower)
    }

    fn segment(a: (i128, i128), b: (i128, i128), tag: u32, tower: &Tower) -> Segment {
        Segment {
            start: point(a.0, a.1, tower),
            end: point(b.0, b.1, tower),
            tag,
            side: 0,
        }
    }

    #[test]
    fn a_square_with_its_diagonals_has_four_faces_inside() {
        let tower = Tower::default();
        let segments = [
            segment((0, 0), (2, 0), 0, &tower),
            segment((2, 0), (2, 2), 0, &tower),
            segment((2, 2), (0, 2), 0, &tower),
            segment((0, 2), (0, 0), 0, &tower),
            segment((0, 0), (2, 2), 1, &tower),
            segment((2, 0), (0, 2), 1, &tower),
        ];
        let arrangement = Arrangement::new(&segments, &tower);
        assert_eq!(arrangement.points.len(), 5);
        assert_eq!(arrangement.edges.len(), 8);
        assert_eq!(arrangement.bounded.iter().filter(|&&b| b).count(), 4);
        assert_eq!(arrangement.faces.len(), 5);
    }

    #[test]
    fn overlapping_segments_share_their_common_stretch() {
        let tower = Tower::default();
        let segments = [
            segment((0, 0), (3, 0), 0, &tower),
            segment((1, 0), (4, 0), 1, &tower),
        ];
        let arrangement = Arrangement::new(&segments, &tower);
        let covers: Vec<Vec<u32>> = arrangement
            .edges
            .iter()
            .map(|e| e.segments.clone())
            .collect();
        assert_eq!(covers, vec![vec![0], vec![0, 1], vec![1]]);
    }
}
