//! Drilling: hole recognition from the model's facets, and the cycle
//! generator that turns a hole list into a toolpath carrying both the
//! fully expanded move chain and a [`Cycle`] record per hole.
//!
//! Provenance: cycle semantics (drill, chip-break peck, deep-hole with
//! full retract and dwell, rigid tap, bore and back-bore, spot dimple)
//! follow the public RS-274 / ISO 6983 canned-cycle vocabulary; what a
//! hole IS - a closed cylindrical wall in the facet mesh - is general
//! machining knowledge applied to our own mesh model. No vendor data,
//! no post processors, no HSMWorks material consulted.
//!
//! Law of two truths: `moves` is the physical truth (verify, display,
//! simulation, the .cl dump all read it); `cycles` is the post's truth
//! (one canned block per hole, not a re-derivation from the chain).
//! They are produced together, from the same lattice-snapped numbers,
//! so they agree - and [`tests::agreement_moves_and_cycles_describe_the_same_holes`]
//! pins that: per cycle, starting at `first_move`, the moves that stay
//! on the hole's XY are its group, its first is a cut (positioning
//! rapids precede the group), and the group's deepest point is
//! `bottom_z`.
//!
//! Holes are Z-axis only: the whole codebase plunges along Z (2.5-D),
//! and recognition finds walls, not axes.




mod cycle;
mod hole;
mod recognize;

#[cfg(test)]
#[allow(unused_imports)]
use {cycle::*, hole::*, recognize::*};
#[cfg(test)]
mod tests;

pub use cycle::drill;
pub use hole::{auto_kind, CycleChoice, CycleParams, Hole, CHIP_BREAK, SPOT_DEPTH, THROUGH_CLEAR};
pub use recognize::recognize;
