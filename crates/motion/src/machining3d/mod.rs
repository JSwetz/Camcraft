//! Mesh-driven 3D pocketing (M2.1a): the level ladder meets the model.
//!
//! Provenance: Z-level pocketing is standard roughing practice, specced
//! from published CAM literature and general machining knowledge; the
//! region algebra is our Clipper2 wrapper and per-level sectioning our
//! marching-slice cutter (`geometry::slice`). The helical entry follows
//! RS-274's G2/G3-with-Z interpolation vocabulary.
//!
//! ## The level law (why every cut is legal by construction)
//!
//! The model is the FINAL part: inside the pocket footprint, material
//! below the model surface must stay; material above it must go. A
//! cutter standing at level z therefore cuts the footprint minus the
//! model's cross-section strictly above z:
//!
//! ```text
//! cut_target(z) = footprint ∖ slice(model, z + ε)
//! ```
//!
//! `slice(model, z+ε)` = columns whose solid reaches above the plane -
//! the per-level islands, computed, not picked. A column whose surface
//! T(x,y) ≤ z is fair game; the ladder then leaves a staircase of at
//! most `stepdown` above T on anything raised (the rest-material bullet,
//! M2.1's simulation work, is what reads that staircase later; a flat
//! floor at `bottom_z` comes out exact, the last level snapped there by
//! the same `cut_levels` law the 2.5-D pockets use). The ε probe means a
//! shelf within one lattice micron of the plane is cut tangentially -
//! quantization noise, never a silent gouge: the reach law verifies the
//! chords at their Z.
//!
//! Entries at a level's first pass: `Plunge` mirrors the 2-D pocket
//! precedent (an axial dive the flutes are built for); `Ramp` feeds the
//! diagonal from the previous level's plane, earned only when that
//! chord stays inside the previous level's cutter-center area - the
//! deepest cut is the most crowded one, so a chord legal there is
//! legal at every plane the descent crosses; `Helix` orbits a circle of
//! `helix_radius` (≤ tool radius, so successive turns overlap) about a
//! scanline midpoint whose room fits the widened disc at both planes,
//! then links along this level's floor to the first pass. Pass ends hug
//! the walls, so an orbit about one is never legal - the dive lives
//! mid-row instead. A diagonal or an orbit into a wall is a gouge, and
//! a fallback that costs a slower dive is never one. The first level
//! always plunges: its plane is the surface itself.
//!
//! ## The rest pass (M2.1b)
//!
//! `pocket3d_with_rest` runs the ladder, simulates it against the mesh
//! with the dexel grid (`stock_sim`), then runs a half-stepdown ladder
//! aimed at the columns the sim says still stand:
//! `target(z) = footprint ∖ slice(z+ε) ∩ {sim_top > z}`. Entries there
//! are always plunges - the nesting law (a deeper plane's island is a
//! superset of a shallower one's) means a column clear at z is clear at
//! every plane above z, so a dive into rest crosses no part wall. The
//! staircase bound halves per pass, toward the grid's own resolution;
//! the closing report is the sim's verdict, never a second surface.
//!
//! ## Feed modulation (M2.1d)
//!
//! `optimize_feed` re-reads every planned planar feed against the same
//! dexel field and scales its feedrate by the engagement it actually
//! meets: the median engagement over the path's loaded bites is the
//! programmed feed, and bites heavier than the median are slowed
//! toward it, down to `FFO_FLOOR` of it; bites the sim reads as
//! engaging nothing (air, zero-depth skin) stay at the programmed
//! feed and hold no vote in the median. Lighter bites never run
//! faster than the programmed feed - the op's feed *is* the tool
//! limit this build models, and speeding past it needs a
//! spindle/torque curve we do not carry (provenance: the
//! constant-chip-thickness relation feed ∝ 1/engagement from
//! machining-handbook side-milling theory; no vendor implementation
//! inspected). Geometry is byte-identical; only `Move::feed` values
//! change.



mod entry;
mod feed;
mod ladder;
mod params;
mod rest;

#[cfg(test)]
#[allow(unused_imports)]
use {entry::*, feed::*, ladder::*, params::*, rest::*};
#[cfg(test)]
mod tests;

pub use entry::{pocket3d, pocket3d_prev, pocket3d_with_rest, pocket3d_with_rest_prev};
pub use feed::{optimize_feed, FFO_FLOOR};
pub use params::{engagement_step, Entry3D, Pocket3DParams};
pub use rest::material_above;
