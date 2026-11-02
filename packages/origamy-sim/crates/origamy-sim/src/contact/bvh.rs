//! A bounding volume hierarchy of boxes, for finding the boxes that overlap another.
//!
//! The hierarchy halves its boxes at the median of their centers, down to leaves of a few
//! boxes. Each split runs along the longest side of the centers' bounds. A query descends only into nodes whose
//! bounds overlap it. A query therefore costs about the logarithm of the count plus what it
//! finds, however large the boxes grow.

use crate::energy::Point;

/// An axis-aligned box: its lowest and its highest corner.
pub(crate) type Bounds = (Point, Point);

/// The most boxes a leaf holds.
const LEAF: usize = 4;

/// A node of the hierarchy: its boxes or its two children.
#[derive(Clone, Copy, Debug)]
enum Node {
    /// The boxes at these places in the hierarchy's order.
    Leaf { start: usize, end: usize },
    /// The indices of two child nodes, both greater than the node's own.
    Inner { left: usize, right: usize },
}

/// A bounding volume hierarchy: a tree over a list of boxes, built for one arrangement of the
/// boxes and fitted to any other.
///
/// Paper keeps its neighbors as it moves, so a tree built once fits later arrangements well.
#[derive(Clone, Debug, Default)]
pub(crate) struct Bvh {
    nodes: Vec<Node>,
    /// The boxes' indices, grouped by leaf.
    order: Vec<usize>,
}

impl Bvh {
    /// The hierarchy over these boxes.
    pub fn new(boxes: &[Bounds]) -> Self {
        let mut hierarchy = Self {
            nodes: Vec::with_capacity(2 * boxes.len() / LEAF + 1),
            order: (0..boxes.len()).collect(),
        };
        if !boxes.is_empty() {
            hierarchy.build(boxes, 0, boxes.len());
        }
        hierarchy
    }

    /// Builds the node over the boxes at places `start..end` of the order, and gives its index.
    fn build(&mut self, boxes: &[Bounds], start: usize, end: usize) -> usize {
        let index = self.nodes.len();
        self.nodes.push(Node::Leaf { start, end });
        let members = &mut self.order[start..end];
        if members.len() <= LEAF {
            return index;
        }
        let center = |i: usize| boxes[i].0 + boxes[i].1;
        let (low, high) = members
            .iter()
            .map(|&i| (center(i), center(i)))
            .reduce(|(low, high), (l, h)| (low.inf(&l), high.sup(&h)))
            .expect("a node holds at least one box");
        let axis = (high - low).imax();
        // Ties fall to the lesser index, so the split is the same on every run.
        let half = members.len() / 2;
        members.select_nth_unstable_by(half, |&a, &b| {
            center(a)[axis].total_cmp(&center(b)[axis]).then(a.cmp(&b))
        });
        let left = self.build(boxes, start, start + half);
        let right = self.build(boxes, start + half, end);
        self.nodes[index] = Node::Inner { left, right };
        index
    }

    /// The hierarchy fitted to boxes in a new arrangement, the same boxes in the same order.
    pub fn fit<'a>(&'a self, boxes: &'a [Bounds]) -> Fitted<'a> {
        let mut bounds = vec![(Point::zeros(), Point::zeros()); self.nodes.len()];
        // Children follow their parents, so a reverse pass fits each node after its children.
        for (index, node) in self.nodes.iter().enumerate().rev() {
            bounds[index] = match *node {
                Node::Leaf { start, end } => self.order[start..end]
                    .iter()
                    .map(|&i| boxes[i])
                    .reduce(|(low, high), (l, h)| (low.inf(&l), high.sup(&h)))
                    .expect("a leaf holds at least one box"),
                Node::Inner { left, right } => {
                    let ((low, high), (l, h)) = (bounds[left], bounds[right]);
                    (low.inf(&l), high.sup(&h))
                }
            };
        }
        Fitted {
            hierarchy: self,
            boxes,
            bounds,
        }
    }
}

/// A hierarchy fitted to an arrangement of its boxes, ready for queries.
pub(crate) struct Fitted<'a> {
    hierarchy: &'a Bvh,
    boxes: &'a [Bounds],
    /// Each node's bounds.
    bounds: Vec<Bounds>,
}

impl Fitted<'_> {
    /// The indices of the boxes that overlap `query`, in increasing order.
    pub fn overlapping(&self, query: &Bounds) -> Vec<usize> {
        let mut found = Vec::new();
        let mut stack = Vec::new();
        if !self.bounds.is_empty() {
            stack.push(0);
        }
        while let Some(index) = stack.pop() {
            if !overlap(&self.bounds[index], query) {
                continue;
            }
            match self.hierarchy.nodes[index] {
                Node::Leaf { start, end } => found.extend(
                    self.hierarchy.order[start..end]
                        .iter()
                        .copied()
                        .filter(|&i| overlap(&self.boxes[i], query)),
                ),
                Node::Inner { left, right } => stack.extend([right, left]),
            }
        }
        found.sort_unstable();
        found
    }
}

/// Whether two boxes overlap, touching included.
pub(crate) fn overlap((low, high): &Bounds, (other_low, other_high): &Bounds) -> bool {
    (0..3).all(|i| low[i] <= other_high[i] && other_low[i] <= high[i])
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn bounds() -> impl Strategy<Value = Bounds> {
        let corner = || (-1.0..1.0f64, -1.0..1.0f64, -1.0..1.0f64);
        (corner(), corner()).prop_map(|((a, b, c), (d, e, f))| {
            let (p, q) = (Point::new(a, b, c), Point::new(d, e, f));
            (p.inf(&q), p.sup(&q) * 0.3 + p.inf(&q) * 0.7)
        })
    }

    proptest! {
        #[test]
        fn a_query_finds_exactly_the_overlapping_boxes(
            built in proptest::collection::vec(bounds(), 0..200),
            moves in proptest::collection::vec(bounds(), 200),
            query in bounds(),
        ) {
            // The hierarchy is built for one arrangement and queried in another.
            let hierarchy = Bvh::new(&built);
            let boxes: Vec<Bounds> = built.iter().zip(&moves).map(|(b, m)| (b.0 + m.0, b.1 + m.1)).collect();
            let expected: Vec<usize> =
                (0..boxes.len()).filter(|&i| overlap(&boxes[i], &query)).collect();
            prop_assert_eq!(hierarchy.fit(&boxes).overlapping(&query), expected);
        }
    }

    #[test]
    fn equal_boxes_still_split() {
        let unit = (Point::zeros(), Point::repeat(1.0));
        let boxes = vec![unit; 100];
        let hierarchy = Bvh::new(&boxes);
        assert!(hierarchy.nodes.len() > 1);
        let fitted = hierarchy.fit(&boxes);
        assert_eq!(fitted.overlapping(&unit).len(), 100);
        let far = (Point::repeat(2.0), Point::repeat(3.0));
        assert!(fitted.overlapping(&far).is_empty());
    }
}
