//! Folded paper as the stacking sees it: faces with their places, and what their
//! geometry gives once folded.

use rustc_hash::FxHashMap;

use crate::field::Tower;
use crate::geometry::{Isometry, Point, Polygon, left_of};
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
    /// Whether each face's place turns it over.
    pub flipped: Vec<bool>,
    /// Each crease's two vertices and two faces, in the order the caller gives them.
    pub creases: Vec<([u32; 2], [u32; 2])>,
    /// Each face's corners once folded, by index among `images`, in the order of its
    /// corners on the crease pattern.
    pub corner_images: Vec<Vec<u32>>,
    /// Where corners lie once folded. Two faces along a crease put its ends in one place,
    /// so corners joined by creases share one image.
    pub images: Vec<Point>,
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
        let (corner_images, images) = images_of(&tower, &vertices, &corners, &places, &creases);
        let flipped: Vec<bool> = places.iter().map(|place| place.flips(&tower)).collect();
        let folded: Vec<Polygon> = corner_images
            .iter()
            .zip(&flipped)
            .map(|(face, &flips)| {
                let mut image: Polygon = face.iter().map(|&k| images[k as usize].clone()).collect();
                if flips {
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
        let image = |face: u32, vertex: u32| {
            let at = corners[face as usize]
                .iter()
                .position(|&c| c == vertex)
                .expect("the face holds the crease");
            images[corner_images[face as usize][at] as usize].clone()
        };
        let ends: Vec<(Point, Point)> = creases
            .iter()
            .map(|&([v, w], [f, _])| (image(f, v), image(f, w)))
            .collect();
        let overlay = Overlay::new(&footprints, &ends, &tower);
        let meeting = overlay.meeting();
        let overlapping = overlapping_of(&stacked, &footprint, &meeting);
        Sheet {
            tower,
            vertices,
            corners,
            places,
            flipped,
            creases,
            corner_images,
            images,
            folded,
            footprints,
            footprint,
            stacked,
            overlay,
            meeting,
            overlapping,
        }
    }

    /// The side of a crease's line, as [`crate::geometry::Line::side`] gives it, that
    /// a face along the crease lies on once folded.
    ///
    /// # Panics
    ///
    /// When the crease from `v` to `w` is no side of the face.
    #[must_use]
    pub fn inside(&self, face: u32, v: u32, w: u32) -> i32 {
        let corners = &self.corners[face as usize];
        let at = corners
            .iter()
            .position(|&c| c == v)
            .expect("the face holds the crease");
        // Counterclockwise on the crease pattern, the face lies left of v to w when w
        // follows v; turning over reverses that.
        let forward = (corners[(at + 1) % corners.len()] == w) != self.flipped[face as usize];
        let image = |vertex: u32| {
            let at = corners
                .iter()
                .position(|&c| c == vertex)
                .expect("the face holds the crease");
            &self.images[self.corner_images[face as usize][at] as usize]
        };
        let (start, end) = (image(v), image(w));
        if forward {
            left_of(start, end, &self.tower)
        } else {
            left_of(end, start, &self.tower)
        }
    }

    /// Where a vertex of a face lies once folded.
    ///
    /// # Panics
    ///
    /// When the vertex is no corner of the face.
    #[must_use]
    pub fn image(&self, face: u32, vertex: u32) -> &Point {
        let at = self.corners[face as usize]
            .iter()
            .position(|&c| c == vertex)
            .expect("the face holds the vertex");
        &self.images[self.corner_images[face as usize][at] as usize]
    }

    /// Whether two faces lie in one place.
    #[must_use]
    pub fn same_place(&self, f: u32, g: u32) -> bool {
        self.places[f as usize] == self.places[g as usize]
    }
}

/// Where each face's corners lie once folded, each image found once.
///
/// Corners that creases join share an image: the two faces along a crease put each of
/// its ends in one place. Corners joined by no crease, as across a cut, each have their
/// own.
fn images_of(
    tower: &Tower,
    vertices: &[Point],
    corners: &[Vec<u32>],
    places: &[Isometry],
    creases: &[([u32; 2], [u32; 2])],
) -> (Vec<Vec<u32>>, Vec<Point>) {
    let mut starts = Vec::with_capacity(corners.len());
    let mut total = 0;
    for face in corners {
        starts.push(total);
        total += face.len();
    }
    let at = |face: u32, vertex: u32| {
        starts[face as usize]
            + corners[face as usize]
                .iter()
                .position(|&c| c == vertex)
                .expect("the face holds the crease")
    };
    let mut parent: Vec<usize> = (0..total).collect();
    for &([v, w], [f, g]) in creases {
        for vertex in [v, w] {
            let (a, b) = (
                root(&mut parent, at(f, vertex)),
                root(&mut parent, at(g, vertex)),
            );
            parent[a.max(b)] = a.min(b);
        }
    }
    let mut image_of: Vec<Option<u32>> = vec![None; total];
    let mut images = vec![];
    let mut corner_images = Vec::with_capacity(corners.len());
    for (face, (corners, place)) in corners.iter().zip(places).enumerate() {
        let mut found = Vec::with_capacity(corners.len());
        for (k, &vertex) in corners.iter().enumerate() {
            let r = root(&mut parent, starts[face] + k);
            let index = *image_of[r].get_or_insert_with(|| {
                images.push(place.apply(&vertices[vertex as usize], tower));
                u32::try_from(images.len() - 1).expect("corners fit in 32 bits")
            });
            found.push(index);
        }
        corner_images.push(found);
    }
    (corner_images, images)
}

/// The root of a corner's class, halving the path on the way.
fn root(parent: &mut [usize], mut k: usize) -> usize {
    while parent[k] != k {
        parent[k] = parent[parent[k]];
        k = parent[k];
    }
    k
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

/// Every pair of faces that share area, sorted: faces on one footprint, and faces on
/// two footprints that meet. Each face lies on one footprint, so no pair comes twice.
fn overlapping_of(stacked: &[Vec<u32>], footprint: &[u32], meeting: &[Pair]) -> Vec<Pair> {
    let mut partners: Vec<Vec<u32>> = vec![vec![]; stacked.len()];
    for &(a, b) in meeting {
        partners[a as usize].push(b);
        partners[b as usize].push(a);
    }
    let mut pairs = vec![];
    let mut greater = vec![];
    for (f, &k) in (0u32..).zip(footprint) {
        greater.clear();
        let on = |k: u32| stacked[k as usize].iter().copied().filter(|&g| g > f);
        greater.extend(on(k));
        for &other in &partners[k as usize] {
            greater.extend(on(other));
        }
        greater.sort_unstable();
        pairs.extend(greater.iter().map(|&g| (f, g)));
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::Number;
    use crate::geometry::Line;

    fn point(x: i128, y: i128, tower: &Tower) -> Point {
        Point::new(Number::integer(x), Number::integer(y), tower)
    }

    #[test]
    fn corners_join_through_creases_alone() {
        let tower = Tower::default();
        // Two squares that touch at (1, 0) and share no crease, the right one turned
        // over the line x = 1.
        let vertices = vec![
            point(0, 0, &tower),
            point(1, 0, &tower),
            point(1, 1, &tower),
            point(0, 1, &tower),
            point(2, 0, &tower),
            point(2, -1, &tower),
            point(1, -1, &tower),
        ];
        let corners = vec![vec![0, 1, 2, 3], vec![1, 6, 5, 4]];
        let mirror = Isometry::reflection(
            &Line::through(&point(1, 0, &tower), &point(1, 1, &tower), &tower),
            &tower,
        );
        let sheet = Sheet::new(
            tower.clone(),
            vertices,
            corners,
            vec![Isometry::identity(), mirror],
            vec![],
        );
        assert_eq!(sheet.images.len(), 8);
        assert_eq!(*sheet.image(1, 4), point(0, 0, &tower));
        assert_eq!(*sheet.image(1, 1), point(1, 0, &tower));
        // Across a crease, the two faces share their crease's ends.
        let tower = Tower::default();
        let vertices = vec![
            point(0, 0, &tower),
            point(1, 0, &tower),
            point(1, 1, &tower),
            point(0, 1, &tower),
            point(2, 0, &tower),
            point(2, 1, &tower),
        ];
        let corners = vec![vec![0, 1, 2, 3], vec![1, 4, 5, 2]];
        let mirror = Isometry::reflection(
            &Line::through(&point(1, 0, &tower), &point(1, 1, &tower), &tower),
            &tower,
        );
        let sheet = Sheet::new(
            tower.clone(),
            vertices,
            corners,
            vec![Isometry::identity(), mirror],
            vec![([1, 2], [0, 1])],
        );
        assert_eq!(sheet.images.len(), 6);
        assert_eq!(sheet.overlapping, vec![(0, 1)]);
    }
}
