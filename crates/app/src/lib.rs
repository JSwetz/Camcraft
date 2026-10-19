//! HSMWorks clean-room app layer.
//!
//! Provenance: independent implementation specced solely from public
//! documentation of commercial CAM workflows and published technical
//! literature. No HSMWorks source, binaries, or assets were consulted.

pub mod cl;
pub mod commands;
pub mod drilling;
pub mod geo2d;
pub mod geometry;
pub mod machining2d;
pub mod model;
pub mod operation;
pub mod pick;
pub mod post;
pub mod posting;
pub mod project;
pub mod schema;
pub mod sim;
pub mod stock;
pub mod tools;
pub mod units;
pub mod verify;
pub mod world;
