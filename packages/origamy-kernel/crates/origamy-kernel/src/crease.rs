//! Creasing open paper along a crease pattern's segments, as `origamy.pattern.crease`
//! does it: the faces the segments, the paper's edges and its creases divide it into.
//!
//! The numbering follows the Python kernel's. The paper's vertices keep their
//! numbers, and new ones follow as each line, in order, meets them from its start.
//! Faces follow the edges in order, each traced with the face on its left.

#![allow(
    clippy::many_single_char_names,
    reason = "the geometry names segments' ends p to s, as the formulas do"
)]

use std::cmp::Ordering;

use rustc_hash::FxHashMap;

use crate::field::{Number, Tower};
use crate::geometry::{Point, float_box, orientation};

/// Why creasing refuses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CreaseError {
    /// A segment runs off the paper: its index among the segments.
    OffPaper(usize),
    /// A face borders a crease twice: a crease ends inside a face.
    Loose,
    /// A face turns right at a corner: a crease ends inside a face.
    Concave,
}

/// The vertices and faces of open paper creased along segments.
///
/// `vertices` and `edges` are the paper's, `outline` its corners counterclockwise,
/// and `segments` the creases to add. Each face lists its corners counterclockwise.
///
/// # Errors
///
/// [`CreaseError`] when a segment runs off the paper, or a crease ends inside a face.
///
/// # Panics
///
/// When the vertices number 2³² or more.
pub fn crease(
    tower: &Tower,
    vertices: &[Point],
    edges: &[[u32; 2]],
    outline: &[Point],
    segments: &[(Point, Point)],
) -> Result<(Vec<Point>, Vec<Vec<u32>>), CreaseError> {
    let mut lines: Vec<(Point, Point)> = edges
        .iter()
        .map(|&[u, v]| (vertices[u as usize].clone(), vertices[v as usize].clone()))
        .collect();
    for (k, (p, q)) in segments.iter().enumerate() {
        if p == q {
            continue;
        }
        let covered = |r: &Point| {
            outline
                .iter()
                .zip(outline.iter().cycle().skip(1))
                .all(|(a, b)| orientation(a, b, r, tower) >= 0)
        };
        if !(covered(p) && covered(q)) {
            return Err(CreaseError::OffPaper(k));
        }
        lines.push((p.clone(), q.clone()));
    }
    let splits = split(&lines, tower);
    let mut all = vertices.to_vec();
    let mut index: FxHashMap<Point, u32> = all
        .iter()
        .enumerate()
        .map(|(i, p)| {
            (
                p.clone(),
                u32::try_from(i).expect("vertices fit in 32 bits"),
            )
        })
        .collect();
    let mut found: Vec<[u32; 2]> = vec![];
    for ((p, q), points) in lines.iter().zip(splits) {
        let direction = q.minus(p, tower);
        let mut ordered: Vec<(Number, Point)> = points
            .into_iter()
            .map(|r| (r.minus(p, tower).dot(&direction, tower), r))
            .collect();
        ordered.sort_by(|(a, _), (b, _)| a.compare(b, tower));
        ordered.dedup_by(|(_, a), (_, b)| a == b);
        let ids: Vec<u32> = ordered
            .into_iter()
            .map(|(_, r)| {
                *index.entry(r.clone()).or_insert_with(|| {
                    all.push(r);
                    u32::try_from(all.len() - 1).expect("vertices fit in 32 bits")
                })
            })
            .collect();
        for pair in ids.windows(2) {
            found.push([pair[0].min(pair[1]), pair[0].max(pair[1])]);
        }
    }
    found.sort_unstable();
    found.dedup();
    let faces = faces(&all, &found, tower)?;
    Ok((all, faces))
}

/// Each line's points: its ends, and where other lines cross it, end on it, or
/// overlap it.
fn split(lines: &[(Point, Point)], tower: &Tower) -> Vec<Vec<Point>> {
    let mut points: Vec<Vec<Point>> = lines
        .iter()
        .map(|(p, q)| vec![p.clone(), q.clone()])
        .collect();
    let boxes: Vec<[f64; 4]> = lines
        .iter()
        .map(|(p, q)| float_box(&[p.clone(), q.clone()]))
        .collect();
    let mut order: Vec<usize> = (0..lines.len()).collect();
    order.sort_by(|&a, &b| boxes[a][0].total_cmp(&boxes[b][0]));
    let mut active: Vec<usize> = vec![];
    for &i in &order {
        active.retain(|&j| boxes[j][2] >= boxes[i][0]);
        for &j in &active {
            if boxes[j][1] > boxes[i][3] || boxes[i][1] > boxes[j][3] {
                continue;
            }
            let (p, q) = &lines[i];
            let (r, s) = &lines[j];
            for point in meeting(p, q, r, s, tower) {
                points[i].push(point.clone());
                points[j].push(point);
            }
        }
        active.push(i);
    }
    points
}

/// Where two segments meet: a crossing, an end on the other, or shared stretches.
fn meeting(p: &Point, q: &Point, r: &Point, s: &Point, tower: &Tower) -> Vec<Point> {
    let (o1, o2) = (orientation(p, q, r, tower), orientation(p, q, s, tower));
    if o1 * o2 > 0 {
        return vec![];
    }
    let (o3, o4) = (orientation(r, s, p, tower), orientation(r, s, q, tower));
    if o3 * o4 > 0 {
        return vec![];
    }
    if o1 == 0 && o2 == 0 {
        // Collinear: each end that lies on the other segment splits it.
        let on = |a: &Point, b: &Point, x: &Point| {
            let d = b.minus(a, tower);
            let t = x.minus(a, tower).dot(&d, tower);
            t.sign(tower) >= 0 && t.compare(&d.dot(&d, tower), tower) != Ordering::Greater
        };
        let mut found: Vec<Point> = [p, q]
            .into_iter()
            .filter(|a| on(r, s, a))
            .cloned()
            .collect();
        found.extend([r, s].into_iter().filter(|a| on(p, q, a)).cloned());
        return found;
    }
    vec![match (o1, o2, o3, o4) {
        (0, _, _, _) => r.clone(),
        (_, 0, _, _) => s.clone(),
        (_, _, 0, _) => p.clone(),
        (_, _, _, 0) => q.clone(),
        _ => {
            let d = q.minus(p, tower);
            let e = s.minus(r, tower);
            let t = r
                .minus(p, tower)
                .cross(&e, tower)
                .over(&d.cross(&e, tower), tower);
            p.plus(&d.scaled(&t, tower), tower)
        }
    }]
}

/// The faces the edges divide the paper into, each counterclockwise, in the order the
/// edges first border them.
fn faces(
    vertices: &[Point],
    edges: &[[u32; 2]],
    tower: &Tower,
) -> Result<Vec<Vec<u32>>, CreaseError> {
    let mut around: Vec<Vec<u32>> = vec![vec![]; vertices.len()];
    for &[u, v] in edges {
        around[u as usize].push(v);
        around[v as usize].push(u);
    }
    let half = |v: &Point| -> u8 {
        let y = v.y.sign(tower);
        u8::from(!(y > 0 || (y == 0 && v.x.sign(tower) > 0)))
    };
    let mut position: FxHashMap<(u32, u32), usize> = FxHashMap::default();
    for (v, others) in around.iter_mut().enumerate() {
        let center = &vertices[v];
        let directions: FxHashMap<u32, Point> = others
            .iter()
            .map(|&w| (w, vertices[w as usize].minus(center, tower)))
            .collect();
        others.sort_by(|a, b| {
            let (va, vb) = (&directions[a], &directions[b]);
            half(va)
                .cmp(&half(vb))
                .then_with(|| 0.cmp(&va.cross(vb, tower).sign(tower)))
        });
        for (i, &w) in others.iter().enumerate() {
            position.insert((u32::try_from(v).expect("vertices fit in 32 bits"), w), i);
        }
    }
    let mut seen: rustc_hash::FxHashSet<(u32, u32)> = rustc_hash::FxHashSet::default();
    let mut faces = vec![];
    for &[u, v] in edges {
        for start in [(u, v), (v, u)] {
            if seen.contains(&start) {
                continue;
            }
            let mut walk = vec![];
            let mut here = start;
            while seen.insert(here) {
                walk.push(here.0);
                let (a, b) = here;
                let others = &around[b as usize];
                here = (
                    b,
                    others[(position[&(b, a)] + others.len() - 1) % others.len()],
                );
            }
            let polygon: Vec<&Point> = walk.iter().map(|&i| &vertices[i as usize]).collect();
            if area(&polygon, tower) > 0 {
                check(&polygon, tower)?;
                faces.push(walk);
            }
        }
    }
    Ok(faces)
}

fn area(polygon: &[&Point], tower: &Tower) -> i32 {
    let mut total = Number::zero();
    for (p, q) in polygon.iter().zip(polygon.iter().cycle().skip(1)) {
        total = total.plus(&p.cross(q, tower));
    }
    total.sign(tower)
}

/// Refuse a face a loose crease leaves: one it borders twice, or one not convex.
fn check(polygon: &[&Point], tower: &Tower) -> Result<(), CreaseError> {
    let distinct: rustc_hash::FxHashSet<&Point> = polygon.iter().copied().collect();
    if distinct.len() < polygon.len() {
        return Err(CreaseError::Loose);
    }
    let n = polygon.len();
    if (0..n).any(|i| {
        orientation(
            polygon[(i + n - 1) % n],
            polygon[i],
            polygon[(i + 1) % n],
            tower,
        ) < 0
    }) {
        return Err(CreaseError::Concave);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: i128, y: i128, tower: &Tower) -> Point {
        Point::new(Number::integer(x), Number::integer(y), tower)
    }

    fn square(tower: &Tower) -> (Vec<Point>, Vec<[u32; 2]>) {
        let vertices = vec![
            point(0, 0, tower),
            point(2, 0, tower),
            point(2, 2, tower),
            point(0, 2, tower),
        ];
        (vertices, vec![[0, 1], [1, 2], [2, 3], [0, 3]])
    }

    #[test]
    fn two_diagonals_cut_a_square_into_four_triangles() {
        let tower = Tower::default();
        let (vertices, edges) = square(&tower);
        let segments = [
            (point(0, 0, &tower), point(2, 2, &tower)),
            (point(2, 0, &tower), point(0, 2, &tower)),
        ];
        let (all, faces) = crease(&tower, &vertices, &edges, &vertices, &segments).unwrap();
        assert_eq!(all.len(), 5);
        assert_eq!(all[4], point(1, 1, &tower));
        assert_eq!(faces.len(), 4);
        assert!(
            faces
                .iter()
                .all(|face| face.len() == 3 && face.contains(&4))
        );
    }

    #[test]
    fn a_crease_off_the_paper_is_refused() {
        let tower = Tower::default();
        let (vertices, edges) = square(&tower);
        let segments = [(point(0, 0, &tower), point(3, 3, &tower))];
        assert_eq!(
            crease(&tower, &vertices, &edges, &vertices, &segments),
            Err(CreaseError::OffPaper(0))
        );
    }

    #[test]
    fn a_crease_ending_inside_the_paper_is_loose() {
        let tower = Tower::default();
        let (vertices, edges) = square(&tower);
        let segments = [(point(0, 1, &tower), point(1, 1, &tower))];
        assert_eq!(
            crease(&tower, &vertices, &edges, &vertices, &segments),
            Err(CreaseError::Loose)
        );
    }
}
