//! A recognised pocket or slot as an operation's source and parameters
//! (#35): what the caller hands `CreateOperation`, so the new operation
//! is one undoable History step whose every field stays editable - the
//! way Detect holes fills a drill.
//!
//! Provenance: our own mapping onto this repo's operation kinds.

use super::{Pocket, SlotEnd};
use crate::operation::{self, FeatureSel, GeoSel, Height, HeightReference, OpParams, OpType, SrcRef};

/// The kind a recognised feature machines as: a slot whose closed ends
/// are all round (or open) rides the Slot cut along its centreline; a
/// square closed end would leave its corners standing under a Slot's
/// round cap, so it - and every pocket - is a 2D Pocket over the floor.
#[must_use]
pub fn op_kind_of(p: &Pocket) -> OpType {
    match &p.slot {
        Some(s) if !s.ends.contains(&SlotEnd::Square) => OpType::Slot,
        _ => OpType::Pocket,
    }
}

/// The source and parameters of the operation `p` machines as,
/// carried from `base` (any kind's params: the caller's seed, its feeds
/// and clearance kept through the registry's retype law).
///
/// A Pocket picks the floor face - its rings are the boundary and its
/// islands, read again at every generate - with its floor at the
/// selection's bottom and its top the walls' height above that, so the
/// depths follow a moved part. A Slot is authored by its centreline
/// ends and width (world XY) and fixed depths, cut into the setup's
/// stock, as a typed Slot is: an open end runs its full width out to
/// the floor's edge, so the round cap overhangs the part there into
/// stock (or air) - never into the part.
///
/// # Errors
/// A feature with no usable depth or width.
pub fn operation_for(p: &Pocket, base: &OpParams) -> Result<(SrcRef, OpParams), String> {
    if !(p.depth.is_finite() && p.depth > 0.0 && p.floor_z.is_finite() && p.top_z.is_finite()) {
        return Err(format!("the feature on '{}' has no usable depth", p.model));
    }
    let kind = op_kind_of(p);
    let mut params = if base.kind() == kind { base.clone() } else { operation::retype_params(base, kind as usize) };
    match (&mut params, &p.slot) {
        (OpParams::Pocket { top_z, bottom_z, .. }, _) => {
            *bottom_z = Height::of(HeightReference::BottomOfSelection, 0.0);
            *top_z = Height::of(HeightReference::BottomOfSelection, p.depth);
            let src = SrcRef::Selection(vec![GeoSel { entity: p.model.clone(), feature: FeatureSel::Face(p.floor) }]);
            Ok((src, params))
        }
        (OpParams::Slot { a, b, width, top_z, bottom_z, .. }, Some(s)) => {
            if !(s.width.is_finite() && s.width > 0.0) {
                return Err(format!("the slot on '{}' has no usable width", p.model));
            }
            *a = s.a;
            *b = s.b;
            *width = s.width;
            *top_z = Height::fixed(p.top_z);
            *bottom_z = Height::fixed(p.floor_z);
            Ok((SrcRef::Stock, params))
        }
        _ => Err(format!("the feature on '{}' maps to no operation kind", p.model)),
    }
}
