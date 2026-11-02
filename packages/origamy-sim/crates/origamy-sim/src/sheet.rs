//! A sheet of paper: its rest mesh, its material and its creases.

use crate::energy::bending::RestPlate;
use crate::energy::membrane::RestTriangle;
use crate::material::{Material, ParameterError, finite, positive};
use crate::mesh::{Mesh, Vertex};

/// A sharp crease: a dihedral where the paper has yielded, so that it bends there alone, about a
/// rest angle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crease {
    /// The moment per length that turns the crease by one radian from rest, in newton meters per
    /// meter per radian.
    pub stiffness: f64,
    /// The dihedral angle the crease rests at, in radians, positive toward the sheet's front.
    pub rest_angle: f64,
}

impl Crease {
    /// Checks that every parameter takes a value it may.
    ///
    /// # Errors
    ///
    /// Returns the first parameter outside its values.
    pub fn check(&self) -> Result<(), ParameterError> {
        positive("crease stiffness", self.stiffness)?;
        finite("rest angle", self.rest_angle)?;
        if self.rest_angle.abs() < std::f64::consts::PI {
            Ok(())
        } else {
            Err(ParameterError {
                name: "rest angle",
                value: self.rest_angle,
                requirement: "smaller in size than π",
            })
        }
    }
}

/// Why a sheet cannot take a change.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum SheetError {
    /// The edge has fewer than two triangles, or does not exist.
    #[error("the sheet has no interior edge from vertex {} to vertex {}", .edge[0], .edge[1])]
    NoDihedral {
        /// The edge.
        edge: [Vertex; 2],
    },
    /// A parameter lies outside its values.
    #[error(transparent)]
    Parameter(#[from] ParameterError),
}

/// Where a triangle's bending reads each of its edges: the dihedral there, and the slots of that
/// dihedral's stencil in the triangle's stencil of up to six vertices.
pub(crate) type EdgeStencils = [Option<(u32, [usize; 4])>; 3];

/// A sheet of paper: its rest mesh, its material and its creases.
#[derive(Clone, Debug)]
pub struct Sheet {
    mesh: Mesh,
    material: Material,
    creases: Vec<Option<Crease>>,
    triangles: Vec<RestTriangle>,
    plates: Vec<RestPlate>,
    stencils: Vec<EdgeStencils>,
    lengths: Vec<f64>,
    areas: Vec<f64>,
}

impl Sheet {
    /// A sheet of this mesh and material, with no creases.
    ///
    /// # Errors
    ///
    /// Returns the first parameter of the material outside its values.
    pub fn new(mesh: Mesh, material: Material) -> Result<Self, ParameterError> {
        material.check()?;
        let corners = |t: &[Vertex; 3]| t.map(|v| mesh.rest()[v as usize]);
        let triangles: Vec<_> = mesh
            .triangles()
            .iter()
            .map(|t| RestTriangle::new(corners(t)))
            .collect();
        let plates = mesh
            .triangles()
            .iter()
            .map(|t| RestPlate::new(corners(t)))
            .collect();
        let mut areas = vec![0.0; mesh.vertex_count()];
        for (t, rest) in mesh.triangles().iter().zip(&triangles) {
            for &v in t {
                areas[v as usize] += rest.area / 3.0;
            }
        }
        let lengths = mesh
            .dihedrals()
            .iter()
            .map(|d| {
                let [u, v] = d.edge.map(|v| mesh.rest()[v as usize]);
                (v[0] - u[0]).hypot(v[1] - u[1])
            })
            .collect();
        let stencils = mesh
            .triangles()
            .iter()
            .map(|t| edge_stencils(&mesh, t))
            .collect();
        Ok(Self {
            creases: vec![None; mesh.dihedrals().len()],
            mesh,
            material,
            triangles,
            plates,
            stencils,
            lengths,
            areas,
        })
    }

    /// The sheet's rest mesh.
    #[must_use]
    pub fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    /// The sheet's material.
    #[must_use]
    pub fn material(&self) -> &Material {
        &self.material
    }

    /// Changes the sheet's material.
    ///
    /// # Errors
    ///
    /// Returns the first parameter of the material outside its values.
    pub fn set_material(&mut self, material: Material) -> Result<(), ParameterError> {
        material.check()?;
        self.material = material;
        Ok(())
    }

    /// The crease along an interior edge, if any.
    ///
    /// # Errors
    ///
    /// Returns [`SheetError::NoDihedral`] when the edge is not an interior edge of the sheet.
    pub fn crease(&self, edge: [Vertex; 2]) -> Result<Option<Crease>, SheetError> {
        Ok(self.creases[self.dihedral(edge)?])
    }

    /// Makes an interior edge a sharp crease, changes its crease, or with `None` makes it plain
    /// paper again.
    ///
    /// # Errors
    ///
    /// Returns [`SheetError::NoDihedral`] when the edge is not an interior edge of the sheet,
    /// and [`SheetError::Parameter`] when a parameter of the crease lies outside its values.
    pub fn set_crease(
        &mut self,
        edge: [Vertex; 2],
        crease: Option<Crease>,
    ) -> Result<(), SheetError> {
        let index = self.dihedral(edge)?;
        if let Some(crease) = &crease {
            crease.check()?;
        }
        self.creases[index] = crease;
        Ok(())
    }

    /// The index of the dihedral along an edge, given in either order.
    fn dihedral(&self, [a, b]: [Vertex; 2]) -> Result<usize, SheetError> {
        let edge = [a.min(b), a.max(b)];
        self.mesh
            .dihedrals()
            .binary_search_by_key(&edge, |d| d.edge)
            .map_err(|_| SheetError::NoDihedral { edge: [a, b] })
    }

    pub(crate) fn creases(&self) -> &[Option<Crease>] {
        &self.creases
    }

    pub(crate) fn rest_triangles(&self) -> &[RestTriangle] {
        &self.triangles
    }

    pub(crate) fn plates(&self) -> &[RestPlate] {
        &self.plates
    }

    pub(crate) fn stencils(&self) -> &[EdgeStencils] {
        &self.stencils
    }

    /// The rest length of each dihedral's edge, in meters.
    pub(crate) fn lengths(&self) -> &[f64] {
        &self.lengths
    }

    /// Each vertex's share of the sheet's area, in square meters: a third of each triangle round
    /// it.
    pub(crate) fn areas(&self) -> &[f64] {
        &self.areas
    }

    /// A triangle's bending stencil: its corners, then the vertex across the edge opposite each
    /// corner, where there is one.
    pub(crate) fn bending_stencil(&self, triangle: usize) -> [Option<Vertex>; 6] {
        let corners = self.mesh.triangles()[triangle];
        let mut stencil = [None; 6];
        for (slot, &corner) in corners.iter().enumerate() {
            stencil[slot] = Some(corner);
        }
        for (i, edge) in self.stencils[triangle].iter().enumerate() {
            if let Some((d, _)) = edge {
                let dihedral = &self.mesh.dihedrals()[*d as usize];
                let far = dihedral.opposite.iter().find(|&&v| v != corners[i]);
                stencil[3 + i] = far.copied();
            }
        }
        stencil
    }
}

/// Where a triangle's bending reads each of its edges.
fn edge_stencils(mesh: &Mesh, corners: &[Vertex; 3]) -> EdgeStencils {
    [0, 1, 2].map(|i| {
        let (from, to) = (corners[(i + 1) % 3], corners[(i + 2) % 3]);
        let edge = [from.min(to), from.max(to)];
        let index = mesh
            .dihedrals()
            .binary_search_by_key(&edge, |d| d.edge)
            .ok()?;
        let dihedral = &mesh.dihedrals()[index];
        let ([start, end], [left, right]) = (dihedral.edge, dihedral.opposite);
        let slot = |vertex: Vertex| corners.iter().position(|&c| c == vertex).unwrap_or(3 + i);
        Some((
            u32::try_from(index).ok()?,
            [start, end, left, right].map(slot),
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Sheet {
        let mesh = Mesh::new(
            vec![[0.0, 0.0], [0.1, 0.0], [0.1, 0.1], [0.0, 0.1]],
            vec![[0, 1, 2], [0, 2, 3]],
        )
        .unwrap();
        Sheet::new(mesh, Material::kami()).unwrap()
    }

    #[test]
    fn each_triangle_reads_its_diagonal_across_from_the_far_corner() {
        let sheet = square();
        // Triangle 0 is (0, 1, 2): its diagonal from 2 to 0 lies opposite corner 1, and the far
        // vertex across it is 3.
        assert_eq!(
            sheet.bending_stencil(0),
            [Some(0), Some(1), Some(2), None, Some(3), None]
        );
        let (_, slots) = sheet.stencils()[0][1].unwrap();
        // The dihedral's stencil is [0, 2, 3, 1]: the edge, its left vertex 3, its right vertex 1.
        assert_eq!(slots, [0, 2, 4, 1]);
        assert!((sheet.areas().iter().sum::<f64>() - 0.01).abs() < 1e-15);
    }

    #[test]
    fn a_crease_lies_along_an_interior_edge() {
        let mut sheet = square();
        let crease = Crease {
            stiffness: 0.01,
            rest_angle: 1.0,
        };
        sheet.set_crease([2, 0], Some(crease)).unwrap();
        assert_eq!(sheet.crease([0, 2]), Ok(Some(crease)));
        assert_eq!(
            sheet
                .set_crease([0, 1], Some(crease))
                .unwrap_err()
                .to_string(),
            "the sheet has no interior edge from vertex 0 to vertex 1"
        );
        let flat = Crease {
            rest_angle: 4.0,
            ..crease
        };
        assert!(matches!(
            sheet.set_crease([0, 2], Some(flat)),
            Err(SheetError::Parameter(_))
        ));
    }
}
