//! A sheet of paper: its rest mesh, its material and its creases.

use std::num::NonZeroU32;

use crate::energy::bending::RestPlate;
use crate::energy::crease::Plastic;
use crate::energy::membrane::RestTriangle;
use crate::material::Material;
use crate::mesh::{Mesh, Vertex};
use crate::parameter::{ParameterError, finite, non_negative, positive, require};

/// A sharp crease: a dihedral where the paper has yielded.
///
/// The paper bends there alone, about the dihedral's rest angle, instead of with the plate
/// around it. Past its yield moment, the crease yields further, and its rest angle follows the
/// fold. Released, the crease springs back by its moment over its stiffness. Below its yield
/// moment, the crease ages: its rest angle creeps toward the fold, faster the larger its moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crease {
    /// The moment per length that turns the crease by one radian from rest, in newton meters per
    /// meter per radian.
    pub stiffness: f64,
    /// The moment per length past which the crease yields, in newton meters per meter; infinite
    /// for a crease that never yields.
    pub yield_moment: f64,
    /// The rise of the yield moment per radian the crease flows through, in newton meters per
    /// meter per radian.
    pub hardening: f64,
    /// The moment per length that multiplies the rate of aging by e, in newton meters per meter.
    ///
    /// A crease held at an angle loses this moment for each factor e its hold time grows by.
    pub aging_moment: f64,
    /// The rate of aging, in radians per second; zero for a crease that never ages.
    ///
    /// The rest angle creeps by this rate times the hyperbolic sine of the moment over the aging
    /// moment.
    pub aging_rate: f64,
}

impl Crease {
    /// A crease in kami.
    ///
    /// The stiffness is the bending stiffness of kami over 200 thicknesses, the length that
    /// creased sheets measure, from Lechenault, Thiria and Adda-Bedia, 2014. Paper pressed flat
    /// springs back to 36°, from Farain, 2020, so the yield moment is the stiffness times that
    /// angle. Pressing harder closes a fold no further, so the crease does not harden.
    ///
    /// Paper held folded sets 5.7° deeper for each tenfold of the hold, from Farain, 2020, which
    /// gives the aging moment. Folds measured by Jules et al., 2020, begin to relax after about a
    /// second, which gives the aging rate.
    #[must_use]
    pub const fn kami() -> Self {
        Self {
            stiffness: 9.0e-3,
            yield_moment: 5.65e-3,
            hardening: 0.0,
            aging_moment: 3.89e-4,
            aging_rate: 4.25e-8,
        }
    }

    /// The crease of each of `lines` parallel lines of edges that spread this crease into a
    /// crease band.
    ///
    /// The lines turn in series, each by a `lines`-th of the band's turn, under one moment. Each
    /// line therefore takes `lines` times the stiffness and hardening, and a `lines`-th of the
    /// aging rate, at the same yield and aging moments. With each line resting at a `lines`-th of
    /// the rest angle, the band turns, yields and ages as this sharp crease does.
    #[must_use]
    pub fn band(&self, lines: NonZeroU32) -> Self {
        let n = f64::from(lines.get());
        Self {
            stiffness: n * self.stiffness,
            hardening: n * self.hardening,
            aging_rate: self.aging_rate / n,
            ..*self
        }
    }

    /// Checks that every parameter takes a value it may.
    ///
    /// # Errors
    ///
    /// Returns the first parameter outside its values.
    pub fn check(&self) -> Result<(), ParameterError> {
        positive("crease stiffness", self.stiffness)?;
        require(
            "yield moment",
            self.yield_moment,
            self.yield_moment > 0.0,
            "positive, or infinite for a crease that never yields",
        )?;
        non_negative("hardening", self.hardening)?;
        positive("aging moment", self.aging_moment)?;
        non_negative("aging rate", self.aging_rate)
    }
}

impl Default for Crease {
    /// A crease in kami.
    fn default() -> Self {
        Self::kami()
    }
}

/// Checks that a rest angle is finite and smaller in size than a half turn.
fn check_rest_angle(angle: f64) -> Result<(), ParameterError> {
    finite("rest angle", angle)?;
    require(
        "rest angle",
        angle,
        angle.abs() < std::f64::consts::PI,
        "smaller in size than π",
    )
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
    /// A span's stencil is not a triangle of the sheet with a direction beyond its edge.
    #[error(
        "span {stencil:?} needs a triangle of the sheet, its third vertex on the edge's left, \
         and a direction to the edge's right"
    )]
    Span {
        /// The span's stencil.
        stencil: [Vertex; 5],
    },
    /// The sheet has no span of this index.
    #[error("the sheet has no span {span}")]
    NoSpan {
        /// The span.
        span: usize,
    },
    /// The sheet has no triangle of this index.
    #[error("the sheet has no triangle {triangle}")]
    NoTriangle {
        /// The triangle.
        triangle: usize,
    },
    /// The sheet has no vertex of this index.
    #[error("the sheet has no vertex {vertex}")]
    NoVertex {
        /// The vertex.
        vertex: usize,
    },
    /// An order names no triangle.
    #[error("the order of vertex {vertex} names no triangle")]
    EmptyOrder {
        /// The order's point.
        vertex: usize,
    },
    /// A parameter lies outside its values.
    #[error(transparent)]
    Parameter(#[from] ParameterError),
}

/// Where a triangle's bending reads each of its edges.
///
/// Each edge names its dihedral, and the slots of the dihedral's stencil in the triangle's
/// stencil of up to six vertices.
pub(crate) type EdgeStencils = [Option<(u32, [usize; 4])>; 3];

/// A point of the sheet that keeps to one side of some of its triangles while it lies over
/// them.
///
/// Orders keep layers of paper without thickness in their stacking. A penalty pushes the
/// point back where it lies over a triangle's face on the wrong side. An order lapses once a
/// time step ends with its point over none of its triangles. Paper may then pass round an
/// edge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Order {
    /// The point.
    pub point: Vertex,
    /// The triangles it keeps to one side of, by their index in the mesh.
    pub triangles: Vec<u32>,
    /// Whether it keeps to the triangles' fronts, where their normals point, or their backs.
    pub front: bool,
}

/// A crease across a band of the sheet: the turn of the paper beyond the band about an edge
/// on the band's near side.
///
/// The span's angle is the dihedral angle between the triangle at the edge and the plane
/// through the edge parallel to the far side, which a vector from `foot` to `far` spans. The
/// band's own paper then bends however its contacts let it, while the span turns the paper
/// beyond the band as a crease turns its two sides.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Span {
    /// The edge from `start` to `end` with `near` opposite it in a triangle on its left, then
    /// `foot` and `far` beyond the band, on the edge's right.
    pub stencil: [Vertex; 5],
    /// The crease the span turns as, if any; a span without one turns freely.
    pub crease: Option<Crease>,
    /// The rest length of the edge, in meters.
    pub length: f64,
    /// Where the span rests, and how far it has yielded.
    pub plastic: Plastic,
}

/// A sheet of paper: its rest mesh, its material and its creases.
#[derive(Clone, Debug)]
pub struct Sheet {
    mesh: Mesh,
    material: Material,
    creases: Vec<Option<Crease>>,
    spans: Vec<Span>,
    rest_angles: Vec<f64>,
    /// The rotation each crease has yielded and crept through, in radians.
    rotations: Vec<f64>,
    triangles: Vec<RestTriangle>,
    /// The compression share each triangle keeps in place of its material's, if any.
    compressions: Vec<Option<f64>>,
    plates: Vec<RestPlate>,
    stencils: Vec<EdgeStencils>,
    lengths: Vec<f64>,
    areas: Vec<f64>,
    orders: Vec<Order>,
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
            .map(|t| RestTriangle::new(corners(t), material.compression))
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
            compressions: vec![None; mesh.triangles().len()],
            creases: vec![None; mesh.dihedrals().len()],
            spans: Vec::new(),
            rest_angles: vec![0.0; mesh.dihedrals().len()],
            rotations: vec![0.0; mesh.dihedrals().len()],
            mesh,
            material,
            triangles,
            plates,
            stencils,
            lengths,
            areas,
            orders: Vec::new(),
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
        for (rest, share) in self.triangles.iter_mut().zip(&self.compressions) {
            rest.compression = share.unwrap_or(material.compression);
        }
        Ok(())
    }

    /// The orders that have not lapsed.
    #[must_use]
    pub fn orders(&self) -> &[Order] {
        &self.orders
    }

    /// Replaces the sheet's orders.
    ///
    /// # Errors
    ///
    /// Returns [`SheetError::NoVertex`] when an order's point is no vertex of the sheet,
    /// [`SheetError::EmptyOrder`] when an order names no triangle, and
    /// [`SheetError::NoTriangle`] when it names one the sheet lacks. A refused list leaves
    /// the orders as they were.
    pub fn set_orders(&mut self, orders: Vec<Order>) -> Result<(), SheetError> {
        for order in &orders {
            let vertex = order.point as usize;
            if vertex >= self.mesh.vertex_count() {
                return Err(SheetError::NoVertex { vertex });
            }
            if order.triangles.is_empty() {
                return Err(SheetError::EmptyOrder { vertex });
            }
            if let Some(&t) = order
                .triangles
                .iter()
                .find(|&&t| t as usize >= self.triangles.len())
            {
                return Err(SheetError::NoTriangle {
                    triangle: t as usize,
                });
            }
        }
        self.orders = orders;
        Ok(())
    }

    /// Keeps only the orders `keep` accepts.
    pub(crate) fn retain_orders(&mut self, keep: impl FnMut(&Order) -> bool) {
        self.orders.retain(keep);
    }

    /// The share of its full stretching energy a triangle keeps where it is compressed.
    ///
    /// # Errors
    ///
    /// Returns [`SheetError::NoTriangle`] when the sheet has no such triangle.
    pub fn compression(&self, triangle: usize) -> Result<f64, SheetError> {
        self.triangles
            .get(triangle)
            .map(|rest| rest.compression)
            .ok_or(SheetError::NoTriangle { triangle })
    }

    /// Sets the share of its full stretching energy a triangle keeps where it is compressed,
    /// or with `None` returns it to its material's.
    ///
    /// Paper crumples where creases cross, and a crumpled triangle barely resists
    /// compression.
    ///
    /// # Errors
    ///
    /// Returns [`SheetError::NoTriangle`] when the sheet has no such triangle, and a parameter
    /// error when the share is not above 0 and at most 1.
    pub fn set_compression(
        &mut self,
        triangle: usize,
        share: Option<f64>,
    ) -> Result<(), SheetError> {
        if triangle >= self.triangles.len() {
            return Err(SheetError::NoTriangle { triangle });
        }
        if let Some(share) = share {
            require(
                "compression",
                share,
                share > 0.0 && share <= 1.0,
                "above 0, and at most 1",
            )?;
        }
        self.compressions[triangle] = share;
        self.triangles[triangle].compression = share.unwrap_or(self.material.compression);
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
    /// Plain paper has flowed through no rotation, so a crease made there starts unhardened.
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
        } else {
            self.rotations[index] = 0.0;
        }
        self.creases[index] = crease;
        Ok(())
    }

    /// The rotation the crease along an interior edge has yielded and crept through, in radians;
    /// zero for plain paper.
    ///
    /// # Errors
    ///
    /// Returns [`SheetError::NoDihedral`] when the edge is not an interior edge of the sheet.
    pub fn plastic_rotation(&self, edge: [Vertex; 2]) -> Result<f64, SheetError> {
        Ok(self.rotations[self.dihedral(edge)?])
    }

    /// The dihedral angle an interior edge rests at, in radians.
    ///
    /// The angle is zero where the paper rests flat, and positive toward the sheet's front.
    ///
    /// # Errors
    ///
    /// Returns [`SheetError::NoDihedral`] when the edge is not an interior edge of the sheet.
    pub fn rest_angle(&self, edge: [Vertex; 2]) -> Result<f64, SheetError> {
        Ok(self.rest_angles[self.dihedral(edge)?])
    }

    /// Sets the dihedral angle an interior edge rests at.
    ///
    /// Along a sharp crease, the crease rests there. Elsewhere the plate around the edge takes
    /// the rest curvature the angle gives, so paper can rest curved.
    ///
    /// # Errors
    ///
    /// Returns [`SheetError::NoDihedral`] when the edge is not an interior edge of the sheet.
    /// Returns [`SheetError::Parameter`] when the angle is not finite or reaches a half turn.
    pub fn set_rest_angle(&mut self, edge: [Vertex; 2], angle: f64) -> Result<(), SheetError> {
        let index = self.dihedral(edge)?;
        check_rest_angle(angle)?;
        self.rest_angles[index] = angle;
        Ok(())
    }

    /// Adds a span across a band: from the triangle `[start, end, near]` to the paper beyond,
    /// as yet with no crease, so it turns freely. Returns its index.
    ///
    /// The vector from `foot` to `far` gives the far paper's direction away from the band.
    /// The span's angle is the dihedral angle of the stencil `[start, end, near, r]`, where
    /// `r` lies at `start` plus that vector. The angle is zero where the sheet lies flat, and
    /// positive toward the sheet's front.
    ///
    /// # Errors
    ///
    /// Returns [`SheetError::Span`] unless `[start, end, near]` is a triangle of the sheet with
    /// `near` on the left of the edge from `start` to `end`, and the vector from `foot` to
    /// `far` points to its right in the flat sheet.
    pub fn add_span(&mut self, stencil: [Vertex; 5]) -> Result<usize, SheetError> {
        let [start, end, near, foot, far] = stencil;
        let rest = self.mesh.rest();
        let triangle =
            self.mesh.triangles().iter().any(|t| {
                (0..3).any(|k| [t[k], t[(k + 1) % 3], t[(k + 2) % 3]] == [start, end, near])
            });
        let inside = [foot, far].iter().all(|&v| (v as usize) < rest.len());
        if !triangle || !inside {
            return Err(SheetError::Span { stencil });
        }
        let [a, b, f, r] = [start, end, foot, far].map(|v| rest[v as usize]);
        let right = (b[0] - a[0]) * (r[1] - f[1]) - (b[1] - a[1]) * (r[0] - f[0]);
        if right >= 0.0 {
            return Err(SheetError::Span { stencil });
        }
        self.spans.push(Span {
            stencil,
            crease: None,
            length: (b[0] - a[0]).hypot(b[1] - a[1]),
            plastic: Plastic {
                rest: 0.0,
                rotation: 0.0,
            },
        });
        Ok(self.spans.len() - 1)
    }

    /// The crease a span turns as, if any.
    ///
    /// # Errors
    ///
    /// Returns [`SheetError::NoSpan`] when the sheet has no such span.
    pub fn span_crease(&self, span: usize) -> Result<Option<Crease>, SheetError> {
        Ok(self.span(span)?.crease)
    }

    /// Gives a span a crease to turn as, changes it, or with `None` lets the span turn freely.
    ///
    /// A span without a crease has flowed through no rotation, so a crease given it starts
    /// unhardened.
    ///
    /// # Errors
    ///
    /// Returns [`SheetError::NoSpan`] when the sheet has no such span, and
    /// [`SheetError::Parameter`] when a parameter of the crease lies outside its values.
    pub fn set_span_crease(
        &mut self,
        span: usize,
        crease: Option<Crease>,
    ) -> Result<(), SheetError> {
        self.span(span)?;
        if let Some(crease) = &crease {
            crease.check()?;
        } else {
            self.spans[span].plastic.rotation = 0.0;
        }
        self.spans[span].crease = crease;
        Ok(())
    }

    /// The angle a span rests at, in radians.
    ///
    /// # Errors
    ///
    /// Returns [`SheetError::NoSpan`] when the sheet has no such span.
    pub fn span_rest_angle(&self, span: usize) -> Result<f64, SheetError> {
        Ok(self.span(span)?.plastic.rest)
    }

    /// Sets the angle a span rests at.
    ///
    /// # Errors
    ///
    /// Returns [`SheetError::NoSpan`] when the sheet has no such span, and
    /// [`SheetError::Parameter`] when the angle is not finite or reaches a half turn.
    pub fn set_span_rest_angle(&mut self, span: usize, angle: f64) -> Result<(), SheetError> {
        self.span(span)?;
        check_rest_angle(angle)?;
        self.spans[span].plastic.rest = angle;
        Ok(())
    }

    /// The span of an index.
    fn span(&self, span: usize) -> Result<&Span, SheetError> {
        self.spans.get(span).ok_or(SheetError::NoSpan { span })
    }

    pub(crate) fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// Sets where a span rests, and how far it has yielded, after a time step.
    pub(crate) fn set_span_plastic(&mut self, span: usize, state: Plastic) {
        self.spans[span].plastic = state;
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

    /// Where a dihedral rests, and how far its crease has yielded.
    pub(crate) fn plastic(&self, dihedral: usize) -> Plastic {
        Plastic {
            rest: self.rest_angles[dihedral],
            rotation: self.rotations[dihedral],
        }
    }

    /// Sets where a dihedral's crease rests, and how far it has yielded, after a time step.
    pub(crate) fn set_plastic(&mut self, dihedral: usize, state: Plastic) {
        self.rest_angles[dihedral] = state.rest;
        self.rotations[dihedral] = state.rotation;
    }

    /// The rest half-angle sines of a triangle's edges, zero where an edge takes no part in its
    /// plate energy.
    pub(crate) fn rest_sines(&self, triangle: usize) -> [f64; 3] {
        self.stencils[triangle]
            .map(|edge| edge.map_or(0.0, |(d, _)| (0.5 * self.rest_angles[d as usize]).sin()))
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
        let crease = Crease::kami();
        sheet.set_crease([2, 0], Some(crease)).unwrap();
        sheet.set_rest_angle([0, 2], 1.0).unwrap();
        assert_eq!(sheet.rest_angle([2, 0]), Ok(1.0));
        assert_eq!(sheet.crease([0, 2]), Ok(Some(crease)));
        assert_eq!(
            sheet
                .set_crease([0, 1], Some(crease))
                .unwrap_err()
                .to_string(),
            "the sheet has no interior edge from vertex 0 to vertex 1"
        );
        assert!(matches!(
            sheet.set_rest_angle([0, 2], 4.0),
            Err(SheetError::Parameter(_))
        ));
        assert!(matches!(
            sheet.set_crease(
                [0, 2],
                Some(Crease {
                    stiffness: -1.0,
                    ..Crease::kami()
                })
            ),
            Err(SheetError::Parameter(_))
        ));
    }

    #[test]
    fn plain_paper_has_yielded_through_no_rotation() {
        let mut sheet = square();
        sheet.set_crease([0, 2], Some(Crease::kami())).unwrap();
        let yielded = Plastic {
            rest: 2.5,
            rotation: 2.5,
        };
        sheet.set_plastic(0, yielded);
        assert_eq!(sheet.plastic_rotation([2, 0]), Ok(2.5));
        // A changed crease keeps its rotation; plain paper drops it.
        sheet.set_crease([0, 2], Some(Crease::default())).unwrap();
        assert_eq!(sheet.plastic(0), yielded);
        sheet.set_crease([0, 2], None).unwrap();
        assert_eq!(sheet.plastic_rotation([0, 2]), Ok(0.0));
        assert_eq!(sheet.rest_angle([0, 2]), Ok(2.5));
    }
}
