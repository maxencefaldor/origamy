//! The rest geometry of a sheet: its flat shape, as a triangle mesh.
//!
//! A sheet lies flat at rest, in material coordinates: two numbers per vertex, in meters.
//! Its triangles run counterclockwise there. Every rest quantity of the sheet, such as a
//! triangle's area or an edge's length, comes from this geometry. Every derived table, such
//! as the edges and the dihedrals, comes out in one order, so a mesh built twice is the same.

/// The index of a vertex.
pub type Vertex = u32;

/// An interior edge and the two triangles that meet along it, at the dihedral angle where the
/// sheet bends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dihedral {
    /// The edge's two vertices, the lesser first.
    pub edge: [Vertex; 2],
    /// The vertex opposite the edge in each of its triangles, left then right.
    ///
    /// The left triangle lies on the edge's left, going from its lesser vertex to its greater.
    pub opposite: [Vertex; 2],
    /// The two triangles, in the same order.
    pub triangles: [u32; 2],
}

/// The rest geometry of a sheet, checked, with its edges, dihedrals and boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    rest: Vec<[f64; 2]>,
    triangles: Vec<[Vertex; 3]>,
    edges: Vec<[Vertex; 2]>,
    dihedrals: Vec<Dihedral>,
    boundary: Vec<[Vertex; 2]>,
}

/// Why a mesh cannot be the rest geometry of a sheet.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum MeshError {
    /// The mesh has no triangle.
    #[error("the mesh has no triangles")]
    Empty,
    /// The mesh has more vertices than a vertex index can name.
    #[error("the mesh has {count} vertices, more than a vertex index can name")]
    TooManyVertices {
        /// How many vertices the mesh has.
        count: usize,
    },
    /// A vertex lies at a coordinate that is infinite or not a number.
    #[error("vertex {vertex} lies at a coordinate that is not finite")]
    NotFinite {
        /// The vertex.
        vertex: Vertex,
    },
    /// A triangle names a vertex the mesh does not have.
    #[error("triangle {triangle} names vertex {vertex}, but the mesh has {count} vertices")]
    OutOfRange {
        /// The triangle.
        triangle: usize,
        /// The vertex it names.
        vertex: Vertex,
        /// How many vertices the mesh has.
        count: usize,
    },
    /// A triangle names one vertex twice.
    #[error("triangle {triangle} names one vertex twice")]
    RepeatedVertex {
        /// The triangle.
        triangle: usize,
    },
    /// A triangle runs clockwise, or has no area.
    #[error(
        "triangle {triangle} has area {area} m²: triangles run counterclockwise, with positive area"
    )]
    NotCounterclockwise {
        /// The triangle.
        triangle: usize,
        /// Its signed area, in square meters.
        area: f64,
    },
    /// An edge borders more than two triangles.
    #[error("edge ({0}, {1}) borders {count} triangles, but an edge borders one or two", edge[0], edge[1])]
    NonManifold {
        /// The edge, its lesser vertex first.
        edge: [Vertex; 2],
        /// How many triangles it borders.
        count: usize,
    },
    /// Both triangles along an edge run it in the same direction.
    #[error("both triangles along edge ({0}, {1}) run it in the same direction", edge[0], edge[1])]
    Inconsistent {
        /// The edge, its lesser vertex first.
        edge: [Vertex; 2],
    },
    /// A vertex belongs to no triangle.
    #[error("vertex {vertex} belongs to no triangle")]
    Unused {
        /// The vertex.
        vertex: Vertex,
    },
}

/// A triangle's side, as its triangle runs it.
#[derive(Clone, Copy)]
struct Side {
    from: Vertex,
    to: Vertex,
    triangle: u32,
    opposite: Vertex,
}

impl Side {
    fn key(&self) -> [Vertex; 2] {
        [self.from.min(self.to), self.from.max(self.to)]
    }

    /// Whether the side runs from the edge's lesser vertex to its greater.
    fn forward(&self) -> bool {
        self.from < self.to
    }
}

impl Mesh {
    /// The mesh with these rest positions, in meters, and these triangles.
    ///
    /// # Errors
    ///
    /// Returns a [`MeshError`] naming the first fault: a mesh with no triangle, or a
    /// coordinate that is not finite. A triangle may name a missing or repeated vertex, or run
    /// clockwise. An edge may border more than two triangles, or be run one way by both. A
    /// vertex may be unused.
    pub fn new(rest: Vec<[f64; 2]>, triangles: Vec<[Vertex; 3]>) -> Result<Self, MeshError> {
        if triangles.is_empty() {
            return Err(MeshError::Empty);
        }
        let count = rest.len();
        if Vertex::try_from(count).is_err() {
            return Err(MeshError::TooManyVertices { count });
        }
        if let Some(vertex) = rest
            .iter()
            .position(|p| !(p[0].is_finite() && p[1].is_finite()))
        {
            return Err(MeshError::NotFinite {
                vertex: index(vertex),
            });
        }
        let mut used = vec![false; count];
        for (triangle, corners) in triangles.iter().enumerate() {
            for &vertex in corners {
                if vertex as usize >= count {
                    return Err(MeshError::OutOfRange {
                        triangle,
                        vertex,
                        count,
                    });
                }
                used[vertex as usize] = true;
            }
            let [a, b, c] = *corners;
            if a == b || b == c || c == a {
                return Err(MeshError::RepeatedVertex { triangle });
            }
            let area = signed_area(rest[a as usize], rest[b as usize], rest[c as usize]);
            if area.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
                return Err(MeshError::NotCounterclockwise { triangle, area });
            }
        }
        if let Some(vertex) = used.iter().position(|&u| !u) {
            return Err(MeshError::Unused {
                vertex: index(vertex),
            });
        }
        let (edges, dihedrals, boundary) = topology(&triangles)?;
        Ok(Self {
            rest,
            triangles,
            edges,
            dihedrals,
            boundary,
        })
    }

    /// Each vertex's place in the flat sheet, in meters.
    #[must_use]
    pub fn rest(&self) -> &[[f64; 2]] {
        &self.rest
    }

    /// The triangles, each counterclockwise in the flat sheet.
    #[must_use]
    pub fn triangles(&self) -> &[[Vertex; 3]] {
        &self.triangles
    }

    /// Every edge, its lesser vertex first, in increasing order.
    #[must_use]
    pub fn edges(&self) -> &[[Vertex; 2]] {
        &self.edges
    }

    /// Every interior edge with its two triangles, in the order of [`Mesh::edges`].
    #[must_use]
    pub fn dihedrals(&self) -> &[Dihedral] {
        &self.dihedrals
    }

    /// Every edge on the sheet's boundary, run as its triangle runs it.
    ///
    /// The boundary turns counterclockwise round the sheet and clockwise round its holes.
    #[must_use]
    pub fn boundary(&self) -> &[[Vertex; 2]] {
        &self.boundary
    }

    /// How many vertices the mesh has.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.rest.len()
    }

    /// The area of a triangle at rest, in square meters.
    ///
    /// # Panics
    ///
    /// Panics when the mesh has no such triangle.
    #[must_use]
    pub fn rest_area(&self, triangle: usize) -> f64 {
        let [a, b, c] = self.triangles[triangle];
        signed_area(
            self.rest[a as usize],
            self.rest[b as usize],
            self.rest[c as usize],
        )
    }

    /// The area of the whole sheet, in square meters.
    #[must_use]
    pub fn area(&self) -> f64 {
        (0..self.triangles.len()).map(|t| self.rest_area(t)).sum()
    }
}

/// The edges, dihedrals and boundary of consistently oriented triangles.
type Topology = (Vec<[Vertex; 2]>, Vec<Dihedral>, Vec<[Vertex; 2]>);

fn topology(triangles: &[[Vertex; 3]]) -> Result<Topology, MeshError> {
    let mut sides: Vec<Side> = triangles
        .iter()
        .enumerate()
        .flat_map(|(triangle, &[a, b, c])| {
            let triangle = index(triangle);
            [(a, b, c), (b, c, a), (c, a, b)].map(|(from, to, opposite)| Side {
                from,
                to,
                triangle,
                opposite,
            })
        })
        .collect();
    // Sorting by edge, then running direction, then triangle fixes every order below.
    sides.sort_unstable_by_key(|side| (side.key(), !side.forward(), side.triangle));
    let mut edges = Vec::new();
    let mut dihedrals = Vec::new();
    let mut boundary = Vec::new();
    for group in sides.chunk_by(|a, b| a.key() == b.key()) {
        let edge = group[0].key();
        edges.push(edge);
        match *group {
            [side] => boundary.push([side.from, side.to]),
            [left, right] if left.forward() && !right.forward() => dihedrals.push(Dihedral {
                edge,
                opposite: [left.opposite, right.opposite],
                triangles: [left.triangle, right.triangle],
            }),
            [_, _] => return Err(MeshError::Inconsistent { edge }),
            _ => {
                return Err(MeshError::NonManifold {
                    edge,
                    count: group.len(),
                });
            }
        }
    }
    Ok((edges, dihedrals, boundary))
}

/// The area of the triangle `a`, `b`, `c`: positive when it runs counterclockwise.
fn signed_area(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]))
}

/// A count that [`Mesh::new`] has already checked fits a vertex index.
fn index(value: usize) -> Vertex {
    Vertex::try_from(value).expect("the mesh has already checked its counts")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// A unit square, split along its rising diagonal.
    fn square() -> Mesh {
        Mesh::new(
            vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            vec![[0, 1, 2], [0, 2, 3]],
        )
        .unwrap()
    }

    /// A rectangle of `columns` by `rows` unit squares, each split along its rising diagonal.
    fn grid(columns: u32, rows: u32) -> Mesh {
        let at = |i: u32, j: u32| j * (columns + 1) + i;
        let rest = (0..=rows)
            .flat_map(|j| (0..=columns).map(move |i| [f64::from(i), f64::from(j)]))
            .collect();
        let triangles = (0..rows)
            .flat_map(|j| {
                (0..columns).flat_map(move |i| {
                    [
                        [at(i, j), at(i + 1, j), at(i + 1, j + 1)],
                        [at(i, j), at(i + 1, j + 1), at(i, j + 1)],
                    ]
                })
            })
            .collect();
        Mesh::new(rest, triangles).unwrap()
    }

    #[test]
    fn a_square_has_one_dihedral_along_its_diagonal() {
        let mesh = square();
        assert_eq!(mesh.edges(), [[0, 1], [0, 2], [0, 3], [1, 2], [2, 3]]);
        assert_eq!(
            mesh.dihedrals(),
            [Dihedral {
                edge: [0, 2],
                opposite: [3, 1],
                triangles: [1, 0],
            }]
        );
        assert_eq!(mesh.boundary(), [[0, 1], [3, 0], [1, 2], [2, 3]]);
        assert!((mesh.area() - 1.0).abs() < 1e-15);
    }

    #[test]
    fn the_left_opposite_vertex_lies_left_of_its_edge() {
        let mesh = grid(3, 2);
        for dihedral in mesh.dihedrals() {
            let [u, v] = dihedral.edge.map(|i| mesh.rest()[i as usize]);
            let [left, right] = dihedral.opposite.map(|i| mesh.rest()[i as usize]);
            assert!(signed_area(u, v, left) > 0.0);
            assert!(signed_area(u, v, right) < 0.0);
        }
    }

    #[test]
    fn bad_meshes_say_why() {
        let corners = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let cases = [
            (corners.clone(), vec![], MeshError::Empty),
            (
                corners.clone(),
                vec![[0, 1, 3]],
                MeshError::OutOfRange {
                    triangle: 0,
                    vertex: 3,
                    count: 3,
                },
            ),
            (
                corners.clone(),
                vec![[0, 1, 1]],
                MeshError::RepeatedVertex { triangle: 0 },
            ),
            (
                corners.clone(),
                vec![[0, 2, 1]],
                MeshError::NotCounterclockwise {
                    triangle: 0,
                    area: -0.5,
                },
            ),
            (
                vec![[0.0, 0.0], [1.0, 0.0], [0.0, f64::NAN]],
                vec![[0, 1, 2]],
                MeshError::NotFinite { vertex: 2 },
            ),
            (
                vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [5.0, 5.0]],
                vec![[0, 1, 2]],
                MeshError::Unused { vertex: 3 },
            ),
        ];
        for (rest, triangles, error) in cases {
            assert_eq!(Mesh::new(rest, triangles), Err(error));
        }
    }

    #[test]
    fn an_edge_borders_at_most_two_triangles() {
        // Three triangles share the edge from 0 to 1: a fan folded over itself.
        let rest = vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0], [0.5, 2.0], [0.2, 3.0]];
        let error = Mesh::new(rest, vec![[0, 1, 2], [0, 1, 3], [0, 1, 4]]);
        assert_eq!(
            error,
            Err(MeshError::NonManifold {
                edge: [0, 1],
                count: 3
            })
        );
    }

    #[test]
    fn two_triangles_run_their_edge_in_opposite_directions() {
        let rest = vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0], [0.5, 2.0]];
        let error = Mesh::new(rest, vec![[0, 1, 2], [0, 1, 3]]);
        assert_eq!(error, Err(MeshError::Inconsistent { edge: [0, 1] }));
    }

    proptest! {
        #[test]
        fn a_grid_has_the_topology_of_a_disc(columns in 1u32..12, rows in 1u32..12) {
            let mesh = grid(columns, rows);
            let (c, r) = (columns as usize, rows as usize);
            prop_assert_eq!(mesh.vertex_count(), (c + 1) * (r + 1));
            prop_assert_eq!(mesh.triangles().len(), 2 * c * r);
            prop_assert_eq!(mesh.edges().len(), c * (r + 1) + r * (c + 1) + c * r);
            prop_assert_eq!(mesh.boundary().len(), 2 * (c + r));
            prop_assert_eq!(mesh.dihedrals().len(), mesh.edges().len() - mesh.boundary().len());
            // Euler's formula for a disc: vertices - edges + triangles = 1.
            let euler = mesh.vertex_count() + mesh.triangles().len() - mesh.edges().len();
            prop_assert_eq!(euler, 1);
            prop_assert!((mesh.area() - f64::from(columns * rows)).abs() < 1e-9);
        }

        #[test]
        fn building_a_mesh_twice_gives_the_same_mesh(columns in 1u32..8, rows in 1u32..8) {
            prop_assert_eq!(grid(columns, rows), grid(columns, rows));
        }
    }
}
