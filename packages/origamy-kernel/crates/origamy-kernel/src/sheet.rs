//! Folded paper as the stacking sees it: faces with their places, and what their
//! geometry gives once folded.

use rustc_hash::FxHashMap;

use crate::field::Tower;
use crate::geometry::{Isometry, Line, Point, Polygon};
use crate::overlay::Overlay;

/// A pair of faces, the lesser first.
pub type Pair = (u32, u32);

/// Faces of paper, each where its place puts it.
pub struct Sheet {
    /// The tower the numbers belong to.
    pub tower: Tower,
    /// The crease pattern's vertices.
    pub vertices: Vec<Point>,
    /// Each face's corners, counterclockwise on the crease pattern.
    pub corners: Vec<Vec<u32>>,
    /// Each face's place.
    pub places: Vec<Isometry>,
    /// Each crease's two vertices and two faces, in the order the caller gives them.
    pub creases: Vec<([u32; 2], [u32; 2])>,
    /// Each face once folded, its corners counterclockwise.
    pub folded: Vec<Polygon>,
    /// Each region a face covers once folded, faces stacked exactly sharing one.
    pub footprints: Vec<Polygon>,
    /// The index of each face's footprint.
    pub footprint: Vec<u32>,
    /// The faces on each footprint, in order.
    pub stacked: Vec<Vec<u32>>,
    /// The overlay of the footprints, with the creases once folded.
    pub overlay: Overlay,
    /// Every pair of footprints that share area.
    pub meeting: Vec<Pair>,
    /// Every pair of faces that share area once folded, sorted.
    pub overlapping: Vec<Pair>,
}

impl Sheet {
    /// The sheet of these faces, with everything their geometry gives once folded.
    ///
    /// # Panics
    ///
    /// When the faces number 2³² or more, or a crease's ends meet once folded.
    #[must_use]
    pub fn new(
        tower: Tower,
        vertices: Vec<Point>,
        corners: Vec<Vec<u32>>,
        places: Vec<Isometry>,
        creases: Vec<([u32; 2], [u32; 2])>,
    ) -> Sheet {
        let folded: Vec<Polygon> = corners
            .iter()
            .zip(&places)
            .map(|(face, place)| {
                let mut image: Polygon = face
                    .iter()
                    .map(|&v| place.apply(&vertices[v as usize], &tower))
                    .collect();
                if place.flips(&tower) {
                    image.reverse();
                }
                image
            })
            .collect();
        let (footprints, footprint) = footprints_of(&folded);
        let mut stacked = vec![vec![]; footprints.len()];
        for (face, &k) in footprint.iter().enumerate() {
            stacked[k as usize].push(u32::try_from(face).expect("faces fit in 32 bits"));
        }
        let ends: Vec<(Point, Point)> = creases
            .iter()
            .map(|&([v, w], [f, _])| {
                let place = &places[f as usize];
                (
                    place.apply(&vertices[v as usize], &tower),
                    place.apply(&vertices[w as usize], &tower),
                )
            })
            .collect();
        let overlay = Overlay::new(&footprints, &ends, &tower);
        let meeting = overlay.meeting();
        let overlapping = overlapping_of(&stacked, &meeting);
        Sheet {
            tower,
            vertices,
            corners,
            places,
            creases,
            folded,
            footprints,
            footprint,
            stacked,
            overlay,
            meeting,
            overlapping,
        }
    }

    /// Where a vertex of a face lies once folded.
    #[must_use]
    pub fn folded_point(&self, face: u32, vertex: u32) -> Point {
        self.places[face as usize].apply(&self.vertices[vertex as usize], &self.tower)
    }

    /// The side of a line that a face lies on once folded, 0 when it lies on it.
    #[must_use]
    pub fn side(&self, face: u32, line: &Line) -> i32 {
        self.folded[face as usize]
            .iter()
            .map(|p| line.side(p, &self.tower))
            .find(|&s| s != 0)
            .unwrap_or(0)
    }

    /// Whether two faces lie in one place.
    #[must_use]
    pub fn same_place(&self, f: u32, g: u32) -> bool {
        self.places[f as usize] == self.places[g as usize]
    }
}

/// Faces with the same corners once folded share a footprint, numbered by first face.
fn footprints_of(folded: &[Polygon]) -> (Vec<Polygon>, Vec<u32>) {
    let mut index: FxHashMap<Vec<&Point>, u32> = FxHashMap::default();
    let mut footprints = vec![];
    let mut footprint = Vec::with_capacity(folded.len());
    for polygon in folded {
        let mut key: Vec<&Point> = polygon.iter().collect();
        key.sort_by(|p, q| {
            p.floats()
                .partial_cmp(&q.floats())
                .expect("estimates are finite")
                .then_with(|| order(p, q))
        });
        let next = u32::try_from(footprints.len()).expect("footprints fit in 32 bits");
        let k = *index.entry(key).or_insert_with(|| {
            footprints.push(polygon.clone());
            next
        });
        footprint.push(k);
    }
    (footprints, footprint)
}

/// A total order of points, for sorting exactly equal estimates apart.
fn order(p: &Point, q: &Point) -> std::cmp::Ordering {
    format!("{:?}", (&p.x, &p.y)).cmp(&format!("{:?}", (&q.x, &q.y)))
}

fn overlapping_of(stacked: &[Vec<u32>], meeting: &[Pair]) -> Vec<Pair> {
    let mut pairs = vec![];
    for group in stacked {
        for (i, &f) in group.iter().enumerate() {
            for &g in &group[i + 1..] {
                pairs.push((f.min(g), f.max(g)));
            }
        }
    }
    for &(a, b) in meeting {
        for &f in &stacked[a as usize] {
            for &g in &stacked[b as usize] {
                pairs.push((f.min(g), f.max(g)));
            }
        }
    }
    pairs.sort_unstable();
    pairs.dedup();
    pairs
}
