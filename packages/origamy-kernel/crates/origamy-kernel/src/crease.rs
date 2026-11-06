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

use rustc_hash::FxHashMap;

use crate::arrangement::{Segment, Stretches};
use crate::field::{Number, Tower};
use crate::geometry::{Point, orientation};

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
    let segments: Vec<Segment> = (0u32..)
        .zip(&lines)
        .map(|(tag, (start, end))| Segment {
            start,
            end,
            tag,
            side: 0,
        })
        .collect();
    let stretches = Stretches::new(&segments, tower);
    // Each line's edges come in order along it, so each segment's points do too.
    let mut along: Vec<Vec<u32>> = vec![vec![]; lines.len()];
    for edge in &stretches.edges {
        for &s in &edge.segments {
            let points = &mut along[s as usize];
            if points.is_empty() {
                points.push(edge.ends[0]);
            }
            points.push(edge.ends[1]);
        }
    }
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
    let mut number: Vec<Option<u32>> = vec![None; stretches.points.len()];
    let mut found: Vec<[u32; 2]> = vec![];
    for ((p, _), mut points) in lines.iter().zip(along) {
        // A segment's vertices number from its start on.
        if stretches.points[points[0] as usize] != *p {
            points.reverse();
        }
        let ids: Vec<u32> = points
            .into_iter()
            .map(|k| {
                *number[k as usize].get_or_insert_with(|| {
                    let r = &stretches.points[k as usize];
                    *index.entry(r.clone()).or_insert_with(|| {
                        all.push(r.clone());
                        u32::try_from(all.len() - 1).expect("vertices fit in 32 bits")
                    })
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
