//! A physical simulator of paper sheets.
//!
//! origamy-sim simulates how sheets of paper bend, crease, stack, slide and hold, under
//! hands, tools, gravity and the table. It knows nothing of origami steps: its inputs are
//! meshes, materials, moving bodies and forces, in SI units.
//!
//! The design, from the physics of paper to the plan of construction, is in
//! `docs/design.md`.

#![forbid(unsafe_code)]

pub mod mesh;

pub use mesh::{Dihedral, Mesh, MeshError, Vertex};
