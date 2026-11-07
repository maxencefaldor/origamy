//! A physical simulator of paper sheets.
//!
//! The simulator models how sheets of paper bend, crease, stack, slide and hold, under
//! hands, tools, gravity and the table. The simulator knows nothing of origami steps. Its
//! inputs are meshes, materials, moving bodies and forces, in SI units.
//!
//! The design, from the physics of paper to the plan of construction, is in
//! `docs/simulator/design.md`.

#![forbid(unsafe_code)]

pub mod body;
mod contact;
mod energy;
pub mod faces;
mod generated;
pub mod material;
pub mod mesh;
pub mod parameter;
pub mod settings;
pub mod sheet;
pub mod simulation;
mod system;

pub use body::{Body, BodyError};
pub use faces::{Faces, FacesError, Refinement};
pub use material::{Material, Stiffness, StrainLimit};
pub use mesh::{Dihedral, Mesh, MeshError, Vertex};
pub use parameter::ParameterError;
pub use settings::{Air, Contact, Settings, Solver, Table};
pub use sheet::{Crease, Order, Sheet, SheetError};
pub use simulation::{Energy, Report, Simulation, StateError, StepError};
pub use system::SingularError;
