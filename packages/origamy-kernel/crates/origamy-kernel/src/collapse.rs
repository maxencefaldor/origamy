//! A collapse, as `origamy.actions.collapse` does it: set creases to mountain, valley
//! or flat at once, and fold the paper to match.
//!
//! The vertices first meet Maekawa's and Kawasaki's theorems. Each face then lies
//! where the creases between it and the fixed face put it, and the layer solver
//! stacks the faces, keeping the orders the paper had where it did not move.

#![allow(
    clippy::many_single_char_names,
    reason = "an isometry's entries are a to f, as its matrix writes them"
)]

use rustc_hash::{FxHashMap, FxHashSet};

use crate::field::Tower;
use crate::geometry::{Isometry, Line, Point};
use crate::sheet::{Pair, Sheet};
use crate::stacking::{Problem, StackError, stack};

/// Paper before a collapse: its faces where they lie, and how they stack.
pub struct Paper {
    /// The tower the numbers belong to.
    pub tower: Tower,
    /// The crease pattern's vertices.
    pub vertices: Vec<Point>,
    /// Each face's corners, counterclockwise on the crease pattern.
    pub corners: Vec<Vec<u32>>,
    /// Each face's place.
    pub places: Vec<Isometry>,
    /// Each crease's two vertices, the lesser first, and two faces, in the paper's order.
    pub creases: Vec<([u32; 2], [u32; 2])>,
    /// The edges along the paper's own edge.
    pub border: Vec<[u32; 2]>,
    /// The stacking: `(upper, lower)` for every pair of overlapping faces.
    pub above: Vec<Pair>,
}

/// Why a collapse refuses.
#[derive(Clone, Debug)]
pub enum CollapseError {
    /// Maekawa's theorem fails at a vertex: its mountains and valleys.
    Count {
        /// The vertex.
        vertex: u32,
        /// Its mountains.
        mountains: usize,
        /// Its valleys.
        valleys: usize,
    },
    /// Kawasaki's theorem fails at a vertex: the alternate angles' sums, in degrees.
    Angles {
        /// The vertex.
        vertex: u32,
        /// The sums of the odd and of the even angles between its folds.
        sums: (f64, f64),
    },
    /// No stacking exists, or none was found within budget.
    Stack(StackError),
}

/// The paper after a collapse: each face's place, the stacking, and the faces moved.
pub struct Collapsed {
    /// Each face's place.
    pub places: Vec<Isometry>,
    /// The stacking: `(upper, lower)` for every pair of overlapping faces.
    pub above: Vec<Pair>,
    /// The faces that moved.
    pub moved: Vec<u32>,
}

/// The kinds a crease may take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A mountain fold.
    Mountain,
    /// A valley fold.
    Valley,
    /// A crease lying flat.
    Flat,
}

/// Collapse paper: each crease takes its kind in `target`, face `fixed` stays.
///
/// The orders a valley or mountain sets, and the orders the paper keeps where no face
/// moved, are required. The search tries first the orders that move the paper least.
///
/// # Errors
///
/// [`CollapseError`] when a vertex fails Maekawa's or Kawasaki's theorem, or no
/// stacking keeps paper from crossing paper.
///
/// # Panics
///
/// When the faces, creases or pairs number 2³² or more.
pub fn collapse(
    paper: &Paper,
    target: &[Kind],
    fixed: u32,
    budget: i64,
) -> Result<Collapsed, CollapseError> {
    check_vertices(paper, target)?;
    let tower = &paper.tower;
    let places = places(paper, target, fixed);
    let moved: Vec<u32> = (0..paper.places.len())
        .filter(|&f| paper.places[f] != places[f])
        .map(|f| u32::try_from(f).expect("faces fit in 32 bits"))
        .collect();
    let is_moved: FxHashSet<u32> = moved.iter().copied().collect();
    let mut required: Vec<Pair> = paper
        .above
        .iter()
        .copied()
        .filter(|(u, l)| !is_moved.contains(u) && !is_moved.contains(l))
        .collect();
    for (&([_, _], [f, g]), &kind) in paper.creases.iter().zip(target) {
        if kind == Kind::Flat {
            continue;
        }
        let (front, back) = if places[f as usize].flips(tower) {
            (g, f)
        } else {
            (f, g)
        };
        // A valley folds the front inward: the face showing its back lies above.
        required.push(if kind == Kind::Valley {
            (back, front)
        } else {
            (front, back)
        });
    }
    let change: Vec<Isometry> = paper
        .places
        .iter()
        .zip(&places)
        .map(|(old, new)| new.after(&inverse(old, tower), tower))
        .collect();
    let prefer = least_motion(paper, target, &change, tower);
    let sheet = Sheet::new(
        paper.tower.clone(),
        paper.vertices.clone(),
        paper.corners.clone(),
        places,
        paper.creases.clone(),
    );
    let problem = Problem::new(&sheet);
    let above = stack(&problem, &required, &prefer, budget).map_err(CollapseError::Stack)?;
    Ok(Collapsed {
        places: sheet.places.clone(),
        above,
        moved,
    })
}

/// The inverse of an isometry: its orthogonal matrix inverts by its transpose.
fn inverse(place: &Isometry, tower: &Tower) -> Isometry {
    let [a, b, c, d, e, f] = &place.entries;
    Isometry {
        entries: [
            a.clone(),
            c.clone(),
            b.clone(),
            d.clone(),
            a.times(e, tower).plus(&c.times(f, tower)).negated(),
            b.times(e, tower).plus(&d.times(f, tower)).negated(),
        ],
    }
}

/// Refuse an assignment that no flat fold meets at some vertex.
fn check_vertices(paper: &Paper, target: &[Kind]) -> Result<(), CollapseError> {
    let tower = &paper.tower;
    let on_border: FxHashSet<u32> = paper.border.iter().flatten().copied().collect();
    let mut order: Vec<u32> = vec![];
    let mut around: FxHashMap<u32, Vec<usize>> = FxHashMap::default();
    for (c, (edge, _)) in paper.creases.iter().enumerate() {
        for &v in edge {
            if !on_border.contains(&v) {
                around.entry(v).or_insert_with(|| {
                    order.push(v);
                    vec![]
                });
                around.get_mut(&v).expect("just added").push(c);
            }
        }
    }
    let away = |c: usize, v: u32| -> &Point {
        let [a, b] = paper.creases[c].0;
        &paper.vertices[if a == v { b } else { a } as usize]
    };
    let half = |p: &Point| -> u8 {
        let y = p.y.sign(tower);
        u8::from(!(y > 0 || (y == 0 && p.x.sign(tower) > 0)))
    };
    for v in order {
        let center = &paper.vertices[v as usize];
        let mut creases = around.remove(&v).expect("each vertex has creases");
        let directions: FxHashMap<usize, Point> = creases
            .iter()
            .map(|&c| (c, away(c, v).minus(center, tower)))
            .collect();
        creases.sort_by(|a, b| {
            let (va, vb) = (&directions[a], &directions[b]);
            half(va)
                .cmp(&half(vb))
                .then_with(|| 0.cmp(&va.cross(vb, tower).sign(tower)))
        });
        let folds: Vec<usize> = creases
            .into_iter()
            .filter(|&c| target[c] != Kind::Flat)
            .collect();
        if folds.is_empty() {
            continue;
        }
        let mountains = folds
            .iter()
            .filter(|&&c| target[c] == Kind::Mountain)
            .count();
        let valleys = folds.len() - mountains;
        if mountains.abs_diff(valleys) != 2 {
            return Err(CollapseError::Count {
                vertex: v,
                mountains,
                valleys,
            });
        }
        let mut turn = Isometry::identity();
        for &c in &folds {
            let [a, b] = paper.creases[c].0;
            let line = Line::through(
                &paper.vertices[a as usize],
                &paper.vertices[b as usize],
                tower,
            );
            turn = Isometry::reflection(&line, tower).after(&turn, tower);
        }
        if turn != Isometry::identity() {
            let (cx, cy) = center.floats();
            let angles: Vec<f64> = folds
                .iter()
                .map(|&c| {
                    let (x, y) = away(c, v).floats();
                    (y - cy).atan2(x - cx).to_degrees().rem_euclid(360.0)
                })
                .collect();
            let gaps: Vec<f64> = (0..angles.len())
                .map(|i| (angles[(i + 1) % angles.len()] - angles[i]).rem_euclid(360.0))
                .collect();
            let odd = gaps.iter().step_by(2).sum();
            let even = gaps.iter().skip(1).step_by(2).sum();
            return Err(CollapseError::Angles {
                vertex: v,
                sums: (odd, even),
            });
        }
    }
    Ok(())
}

/// Where each face lies, from the creases between it and the fixed face.
fn places(paper: &Paper, target: &[Kind], fixed: u32) -> Vec<Isometry> {
    let tower = &paper.tower;
    let crease_of: FxHashMap<[u32; 2], usize> = paper
        .creases
        .iter()
        .enumerate()
        .map(|(c, (edge, _))| (*edge, c))
        .collect();
    let mut found: Vec<Option<Isometry>> = vec![None; paper.places.len()];
    found[fixed as usize] = Some(paper.places[fixed as usize].clone());
    let mut queue = vec![fixed];
    while let Some(face) = queue.pop() {
        let corners = &paper.corners[face as usize];
        for i in 0..corners.len() {
            let (u, v) = (corners[i], corners[(i + 1) % corners.len()]);
            let Some(&c) = crease_of.get(&[u.min(v), u.max(v)]) else {
                continue;
            };
            let [f, g] = paper.creases[c].1;
            let other = if g == face { f } else { g };
            if found[other as usize].is_some() {
                continue;
            }
            let mut place = found[face as usize]
                .clone()
                .expect("a face in the queue has its place");
            if target[c] != Kind::Flat {
                let [a, b] = paper.creases[c].0;
                let line = Line::through(
                    &paper.vertices[a as usize],
                    &paper.vertices[b as usize],
                    tower,
                );
                place = place.after(&Isometry::reflection(&line, tower), tower);
            }
            found[other as usize] = Some(place);
            queue.push(other);
        }
    }
    found
        .into_iter()
        .map(|place| place.expect("the paper is connected"))
        .collect()
}

/// The orders that move the paper least through itself, for the solver to try first.
///
/// Each face that turns about a fold lands beside the face across it. Faces that
/// overlapped keep their order, reversed where they moved together and the move
/// turned them over.
fn least_motion(paper: &Paper, target: &[Kind], change: &[Isometry], tower: &Tower) -> Vec<Pair> {
    let mut turns = vec![];
    for ((_, [f, g]), &kind) in paper.creases.iter().zip(target) {
        if kind == Kind::Flat {
            continue;
        }
        for (turned, partner) in [(*f, *g), (*g, *f)] {
            if change[turned as usize] != change[partner as usize] {
                turns.push((turned, partner));
            }
        }
    }
    let identity = Isometry::identity();
    let mut prefer = beside(paper, &turns, change);
    for &(upper, lower) in &paper.above {
        let (cu, cl) = (&change[upper as usize], &change[lower as usize]);
        if *cu == identity && *cl == identity {
            continue;
        }
        prefer.push(if cu == cl && cu.flips(tower) {
            (lower, upper)
        } else {
            (upper, lower)
        });
    }
    prefer
}

/// The orders that land each turned face beside the face across its fold: with the
/// paper that moved as that face did, and that the turned face did not overlap
/// before, the turned face takes that face's order.
fn beside(paper: &Paper, turns: &[(u32, u32)], change: &[Isometry]) -> Vec<Pair> {
    if paper.above.is_empty() {
        return vec![];
    }
    let overlapped: FxHashSet<Pair> = paper
        .above
        .iter()
        .map(|&(u, l)| (u.min(l), u.max(l)))
        .collect();
    let mut touching: FxHashMap<u32, Vec<(u32, u32)>> = FxHashMap::default();
    for &(upper, lower) in &paper.above {
        touching.entry(upper).or_default().push((upper, lower));
        touching.entry(lower).or_default().push((upper, lower));
    }
    let mut found = vec![];
    for &(turned, partner) in turns {
        for &(upper, lower) in touching.get(&partner).map_or(&[][..], Vec::as_slice) {
            let other = if upper == partner { lower } else { upper };
            if change[other as usize] == change[partner as usize]
                && !overlapped.contains(&(turned.min(other), turned.max(other)))
            {
                found.push(if upper == partner {
                    (turned, other)
                } else {
                    (other, turned)
                });
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crease::crease;
    use crate::field::Number;
    use crate::stacking::{Problem, violations};

    fn point(x: i128, y: i128, tower: &Tower) -> Point {
        Point::new(Number::integer(x), Number::integer(y), tower)
    }

    /// A segment as its two ends' integer coordinates.
    type Ends = ((i128, i128), (i128, i128));

    /// Open paper, a square of side 4 creased along segments.
    fn paper(segments: &[Ends]) -> Paper {
        let tower = Tower::default();
        let square = vec![
            point(0, 0, &tower),
            point(4, 0, &tower),
            point(4, 4, &tower),
            point(0, 4, &tower),
        ];
        let segments: Vec<(Point, Point)> = segments
            .iter()
            .map(|&((a, b), (c, d))| (point(a, b, &tower), point(c, d, &tower)))
            .collect();
        let (vertices, corners) = crease(
            &tower,
            &square,
            &[[0, 1], [1, 2], [2, 3], [0, 3]],
            &square,
            &segments,
        )
        .unwrap();
        let mut sides: FxHashMap<[u32; 2], Vec<u32>> = FxHashMap::default();
        for (f, face) in corners.iter().enumerate() {
            for (k, &u) in face.iter().enumerate() {
                let v = face[(k + 1) % face.len()];
                sides
                    .entry([u.min(v), u.max(v)])
                    .or_default()
                    .push(u32::try_from(f).unwrap());
            }
        }
        let mut creases: Vec<([u32; 2], [u32; 2])> = sides
            .iter()
            .filter(|(_, faces)| faces.len() == 2)
            .map(|(&edge, faces)| (edge, [faces[0], faces[1]]))
            .collect();
        creases.sort_unstable();
        let mut border: Vec<[u32; 2]> = sides
            .iter()
            .filter(|(_, faces)| faces.len() == 1)
            .map(|(&edge, _)| edge)
            .collect();
        border.sort_unstable();
        let places = vec![Isometry::identity(); corners.len()];
        Paper {
            tower,
            vertices,
            corners,
            places,
            creases,
            border,
            above: vec![],
        }
    }

    #[test]
    fn a_valley_puts_the_moved_face_on_top() {
        let paper = paper(&[((2, 0), (2, 4))]);
        let done = collapse(&paper, &[Kind::Valley], 0, 100).unwrap();
        assert_eq!(done.moved, vec![1]);
        assert_eq!(done.above, vec![(1, 0)]);
        let mountain = collapse(&paper, &[Kind::Mountain], 0, 100).unwrap();
        assert_eq!(mountain.above, vec![(0, 1)]);
    }

    #[test]
    fn folding_in_four_stacks_validly() {
        let paper = paper(&[((2, 0), (2, 4)), ((0, 2), (4, 2))]);
        // The vertical crease folds as a valley at both halves, and the horizontal one
        // as a valley at its left half: three valleys and one mountain.
        let two = Number::integer(2);
        let kinds: Vec<Kind> = paper
            .creases
            .iter()
            .map(|([u, v], _)| {
                let (p, q) = (&paper.vertices[*u as usize], &paper.vertices[*v as usize]);
                let left = p.x.compare(&two, &paper.tower).is_lt()
                    || q.x.compare(&two, &paper.tower).is_lt();
                if p.x == q.x || left {
                    Kind::Valley
                } else {
                    Kind::Mountain
                }
            })
            .collect();
        let done = collapse(&paper, &kinds, 0, 100).unwrap();
        assert_eq!(done.above.len(), 6);
        let sheet = Sheet::new(
            paper.tower.clone(),
            paper.vertices.clone(),
            paper.corners.clone(),
            done.places,
            paper.creases.clone(),
        );
        assert!(violations(&Problem::new(&sheet), &done.above).is_empty());
    }

    #[test]
    fn four_valleys_at_a_vertex_break_maekawa() {
        let paper = paper(&[((2, 0), (2, 4)), ((0, 2), (4, 2))]);
        let kinds = vec![Kind::Valley; paper.creases.len()];
        let refused = collapse(&paper, &kinds, 0, 100).err().unwrap();
        assert!(matches!(
            refused,
            CollapseError::Count {
                mountains: 0,
                valleys: 4,
                ..
            }
        ));
    }

    #[test]
    fn creases_meeting_at_uneven_angles_break_kawasaki() {
        let paper = paper(&[((2, 0), (2, 4)), ((0, 0), (2, 2)), ((2, 2), (4, 2))]);
        let kinds: Vec<Kind> = (0..paper.creases.len())
            .map(|k| if k == 0 { Kind::Mountain } else { Kind::Valley })
            .collect();
        let refused = collapse(&paper, &kinds, 0, 100).err().unwrap();
        assert!(
            matches!(refused, CollapseError::Angles { .. }),
            "{refused:?}"
        );
    }
}
