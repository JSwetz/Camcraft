//! A field carried between setups (#637 steps A, C): the SAME field - its
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

/// A field seen through a setup change's turn, optionally clipped to the
/// next setup's own stock block (#637 step C).
#[derive(Clone, Debug)]
pub struct CarriedField {
    pub field: SdfField,
    pub turn: LatticeTurn,
    /// The later setup's block `[lo, hi]` in the NEW frame: the carried
    /// solid is intersected with it - `max(d_body, d_block)`, the
    /// field's form of the column body's `lower` clip. `None`: unclipped.
    pub clip: Option<(Point3, Point3)>,
}

/// The exact signed distance to the box `[lo, hi]`: negative inside.
#[must_use]
pub fn box_signed(lo: Point3, hi: Point3, p: Point3) -> f64 {
    let (l, h, q) = ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z], [p.x, p.y, p.z]);
    let mut out2 = 0.0f64;
    let mut inner = f64::NEG_INFINITY;
    for a in 0..3 {
        let d = (l[a] - q[a]).max(q[a] - h[a]); // > 0 outside along this axis
        if d > 0.0 {
            out2 += d * d;
        }
        inner = inner.max(d);
    }
    if out2 > 0.0 { out2.sqrt() } else { inner.min(0.0) }
}

impl CarriedField {
    /// Carry `field` through `turn` (no data moves: the turn is the view).
    #[must_use]
    pub fn new(field: SdfField, turn: LatticeTurn) -> Self {
        CarriedField { field, turn, clip: None }
    }

    /// Clip to the next setup's block (new frame). A second clip
    /// intersects with the first's block, never widens it.
    #[must_use]
    pub fn clipped(mut self, lo: Point3, hi: Point3) -> CarriedField {
        self.clip = Some(match self.clip {
            None => (lo, hi),
            Some((a, b)) => (
                Point3::new(a.x.max(lo.x), a.y.max(lo.y), a.z.max(lo.z)),
                Point3::new(b.x.min(hi.x), b.y.min(hi.y), b.z.min(hi.z)),
            ),
        });
        self
    }

    /// Carry again: the turns compose (`next` after this one).
    #[must_use]
    pub fn then(self, next: LatticeTurn) -> CarriedField {
        // A clip stated in the old frame is the block it was; carrying the
        // body again carries the clip through the same turn.
        let clip = self.clip.map(|(lo, hi)| {
            let (a, b) = (next.apply(lo), next.apply(hi));
            (Point3::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)), Point3::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)))
        });
        CarriedField { field: self.field, turn: self.turn.then(next), clip }
    }

    /// The carried solid's signed distance at a point of the new frame -
    /// intersected with the clip block when one is set.
    #[must_use]
    pub fn distance(&self, p: Point3) -> f64 {
        let d = self.field.distance(self.turn.unapply(p));
        match self.clip {
            Some((lo, hi)) => d.max(box_signed(lo, hi, p)),
            None => d,
        }
    }

    /// The uncut stock's signed distance (the mesh term) in the new frame.
    #[must_use]
    pub fn mesh_distance(&self, p: Point3) -> f64 {
        self.field.mesh_distance(self.turn.unapply(p))
    }

    /// Membership in the new frame: inside the carried solid AND inside
    /// the clip block when one is set.
    #[must_use]
    pub fn inside(&self, p: Point3) -> bool {
        self.field.inside(self.turn.unapply(p)) && self.clip.is_none_or(|(lo, hi)| box_signed(lo, hi, p) < 0.0)
    }
}
