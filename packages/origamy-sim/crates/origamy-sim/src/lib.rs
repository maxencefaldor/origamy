//! A physical simulator of paper sheets.
//!
//! origamy-sim simulates how sheets of paper bend, crease, stack, slide and hold, under
//! hands, tools, gravity and the table. It knows nothing of origami steps: its inputs are
//! meshes, materials, moving bodies and forces, in SI units.
//!
//! The design, from the physics of paper to the plan of construction, is in
//! `docs/design.md`.

#![forbid(unsafe_code)]

mod energy;
mod generated;
pub mod material;
pub mod mesh;
pub mod sheet;
pub mod simulation;
mod system;

pub use material::{Material, ParameterError, Stiffness};
pub use mesh::{Dihedral, Mesh, MeshError, Vertex};
pub use sheet::{Crease, Sheet, SheetError};
pub use simulation::{Energy, Report, Settings, Simulation, StateError};
pub use system::SingularError;
