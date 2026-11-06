//! The exact kernel of origamy.
//!
//! Numbers live in a tower of square roots, as `origamy.field` defines them, and every
//! decision is exact: float estimates with error bounds decide where the bounds leave
//! the answer clear, and exact arithmetic decides the rest. The layer solver finds
//! which face lies above which wherever faces overlap, as `origamy.layers` defines it.

#![forbid(unsafe_code)]

pub mod arrangement;
pub mod collapse;
pub mod crease;
pub mod field;
pub mod geometry;
pub mod int;
pub mod lines;
pub mod overlay;
pub mod sheet;
pub mod stacking;
