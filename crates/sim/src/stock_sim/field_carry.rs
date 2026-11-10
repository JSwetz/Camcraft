//! A field carried between setups (#637 step A): the SAME field - its
//! shell, its closed-form box (#633), its cut layers - viewed through a
//! setup change's rigid turn, never re-sampled. A signed distance field
//! is invariant under rigid motion, so the distance from `p` to the
//! carried solid is the distance from `T^-1 p` to the original: exact,
//! the field analogue of `carry`'s "re-index, never re-sample".
//!
//! Phase 1 keeps `carry`'s scope: the eight lattice turns (`LatticeTurn`,
//! Z kept on +-Z), so a vertical column in the new frame is a vertical
//! column (perhaps reversed) in the field's own - the verdict march
//! stays exact. Every other turn is refused upstream by
//! `CARRY_TURN_REFUSAL`.
//!
//! Provenance: rigid-motion invariance of the Euclidean distance (any
//! geometry text); our own design after #637's research comment.

use super::carry::LatticeTurn;
use super::field::SdfField;
use crate::geometry::Point3;

/// A field seen through a setup change's turn.
#[derive(Clone, Debug)]
pub struct CarriedField {
    pub field: SdfField,
    pub turn: LatticeTurn,
}

impl CarriedField {
    /// Carry `field` through `turn` (no data moves: the turn is the view).
    #[must_use]
    pub fn new(field: SdfField, turn: LatticeTurn) -> Self {
        CarriedField { field, turn }
    }

    /// Carry again: the turns compose (`next` after this one).
    #[must_use]
    pub fn then(self, next: LatticeTurn) -> CarriedField {
        CarriedField { field: self.field, turn: self.turn.then(next) }
    }

    /// The carried solid's signed distance at a point of the new frame.
    #[must_use]
    pub fn distance(&self, p: Point3) -> f64 {
        self.field.distance(self.turn.unapply(p))
    }

    /// The uncut stock's signed distance (the mesh term) in the new frame.
    #[must_use]
    pub fn mesh_distance(&self, p: Point3) -> f64 {
        self.field.mesh_distance(self.turn.unapply(p))
    }

    /// Ray-parity membership in the new frame.
    #[must_use]
    pub fn inside(&self, p: Point3) -> bool {
        self.field.inside(self.turn.unapply(p))
    }
}
