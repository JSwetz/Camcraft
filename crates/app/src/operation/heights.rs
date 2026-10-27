

use super::*;

impl Height {
    /// Resolve against the current world; None when the source is not
    /// available (no stock or model yet, no entity in the slot). The
    /// default setup's world - a caller standing in another setup uses
    /// [`Self::z_for`] (M3.2 stage A).
    #[must_use]
    pub fn z(&self, world: &flecs_ecs::prelude::World) -> Option<f64> {
        self.z_for(world, None)
    }

    /// The same resolve in a declared setup context ([`crate::setup`]).
    #[must_use]
    pub fn z_for(
        &self,
        world: &flecs_ecs::prelude::World,
        setup: Option<&str>,
    ) -> Option<f64> {
        resolve_height(world, setup, *self)
    }

}

/// Resolve one height to a world Z. `Fixed` reads through; the other
/// sources ask the world for the current geometry (model bounds exclude
/// stock and WCS by `solid_world_aabb`'s rule; a slot entity that exists
/// but was never moved reads at the origin, matching `transform_of`'s
/// own default-everything semantics).
/// `setup` is the declared setup of the asking operation (`None` = the
/// first setup, which in a document without setups is the old global
/// lookup exactly - the implicit-setup law of [`crate::setup`]). The
/// model bounds are document-wide: the part is the part whatever setup
/// holds it; only the stock box and the offset planes follow the setup.
#[must_use]
pub fn resolve_height(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    h: Height,
) -> Option<f64> {
    let base = match h.reference {
        HeightReference::Fixed => return (h.offset.is_finite()).then_some(h.offset),
        HeightReference::TopOfStock => model::stock_world_aabb_for(world, setup)?.max.z,
        HeightReference::BottomOfStock => model::stock_world_aabb_for(world, setup)?.min.z,
        HeightReference::TopOfModel => model::solid_world_aabb(world)?.max.z,
        HeightReference::BottomOfModel => model::solid_world_aabb(world)?.min.z,
        HeightReference::WorkOffset(i) => {
            let entity = crate::setup::wcs_entity(world, setup, i)?;
            model::transform_of(world, &entity).unwrap_or_default().pos.z
        }
    };
    (base + h.offset).is_finite().then_some(base + h.offset)
}

/// What a reference is missing, phrased for a diagnostic: the user can
/// act on "no stock yet". None when the source stands.
#[must_use]
pub fn missing_source(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    r: HeightReference,
) -> Option<&'static str> {
    match r {
        HeightReference::Fixed => None,
        HeightReference::TopOfStock | HeightReference::BottomOfStock => {
            model::stock_world_aabb_for(world, setup)
                .is_none()
                .then_some("no stock yet - set it up in Setup > Stock")
        }
        HeightReference::TopOfModel | HeightReference::BottomOfModel => {
            model::solid_world_aabb(world).is_none().then_some("no model loaded")
        }
        HeightReference::WorkOffset(i) => match crate::setup::wcs_entity_name(world, setup, i) {
            None => Some("work offset slot out of range (G54..G59)"),
            Some(name) => model::find_by_name(world, &name)
                .is_none()
                .then_some("that work offset has no origin yet - place it in Setup"),
        },
    }
}

/// The Z the operation's region lives at, resolved: the section/stadium
/// plane for the depth kinds, the skim (or first stage) plane for Face.
/// None when that height's reference has no source.
#[must_use]
pub fn region_z(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    params: &OpParams,
) -> Option<f64> {
    match params {
        OpParams::Face { top_z, cut_z, .. } => top_z
            .map(|t| resolve_height(world, setup, t))
            .unwrap_or_else(|| resolve_height(world, setup, *cut_z)),
        OpParams::Contour { top_z, .. }
        | OpParams::Pocket { top_z, .. }
        | OpParams::Slot { top_z, .. }
        | OpParams::Trace { top_z, .. }
        | OpParams::Drill { top_z, .. }
        | OpParams::Pocket3D { top_z, .. }
        | OpParams::Finish3D { top_z, .. }
        | OpParams::Contour5D { top_z, .. }
        | OpParams::Parallel5D { top_z, .. }
        | OpParams::Swarf { top_z, .. }
        | OpParams::AdvancedSwarf { top_z, .. }
        | OpParams::RotaryContour { top_z, .. }
        | OpParams::RotaryParallel { top_z, .. }
        | OpParams::RotaryOffset { top_z, .. }
        | OpParams::MultiAxisRough { top_z, .. }
        | OpParams::Flow5D { top_z, .. }
        | OpParams::MultiAxisContour { top_z, .. }
        | OpParams::Facing { top_z, .. }
        | OpParams::ProfileRough { top_z, .. }
        | OpParams::ProfileFinish { top_z, .. }
        | OpParams::GrooveRough { top_z, .. }
        | OpParams::GrooveFinish { top_z, .. }
        | OpParams::Part { top_z, .. } => resolve_height(world, setup, *top_z),
    }
}

/// Every height of one params set, resolved at once for a generation or
/// a panel pass. Err carries the field name and the source that is
/// missing, so the refusal is actionable by name - never a guessed z.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedHeights {
    /// Rapids plane, every kind.
    pub clearance: f64,
    /// Depth kinds: section top. Face: None (it has [`Self::face_top`]).
    pub top: Option<f64>,
    /// Depth kinds: last level. Face: the finished cut plane.
    pub bottom: Option<f64>,
    /// Face: first stage when stepped, None for one-level facing.
    pub face_top: Option<f64>,
    /// Contour resolves Some; pocket-family None hops at clearance.
    pub retract: Option<f64>,
}

impl ResolvedHeights {
    /// The height-plane Zs of one kind: (top, bottom/cut) as the panels
    /// and verify want them, from the resolved bag.
    #[must_use]
    pub fn depth(&self) -> (Option<f64>, Option<f64>) {
        if self.face_top.is_some() || self.top.is_some() {
            (self.top.or(self.face_top), self.bottom)
        } else {
            (self.face_top, self.bottom)
        }
    }
}

/// Resolve every height in `params`; Err is "<field> (From: <ref> <offset>):
/// <why>" - each refusal names the height and its source.
pub fn resolve_params(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    params: &OpParams,
) -> Result<ResolvedHeights, String> {
    let req = |what: &str, h: Height| -> Result<f64, String> {
        resolve_height(world, setup, h).ok_or_else(|| unresolved_msg(what, h, world, setup))
    };
    let opt = |what: &str, h: Option<Height>| -> Result<Option<f64>, String> {
        h.map(|v| req(what, v)).transpose()
    };
    let (clearance, top, bottom, face_top, retract) = match params {
        OpParams::Face { clearance_z, cut_z, top_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            None,
            Some(req("Cut Z", *cut_z)?),
            opt("Top Z", *top_z)?,
            None,
        ),
        OpParams::Contour { clearance_z, retract_z, top_z, bottom_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            Some(req("Top of Part Z", *top_z)?),
            Some(req("Bottom of Part Z", *bottom_z)?),
            None,
            Some(req("Retract Z", *retract_z)?),
        ),
        OpParams::Pocket { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::Slot { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::Trace { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::Pocket3D { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::Finish3D { clearance_z, retract_z, top_z, bottom_z, .. }
        // The 5-axis kinds are the finish's height shape exactly: the
        // same four named heights, the same optional hop.
        | OpParams::Contour5D { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::Parallel5D { clearance_z, retract_z, top_z, bottom_z, .. }
        // The swarf family's too - stage D adds no height to the shape,
        // nor does rotary: the generators read these world planes into
        // axial stations themselves (stage E).
        | OpParams::Swarf { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::AdvancedSwarf { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::RotaryContour { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::RotaryParallel { clearance_z, retract_z, top_z, bottom_z, .. }
        // The stamped ladder's four are the pocket family's, the
        // optional hop among them (M5.1 stage E3), and flow's are
        // the finish's four the same way (M5.1 I).
        | OpParams::MultiAxisRough { clearance_z, retract_z, top_z, bottom_z, .. }
        // The drive-curve contour names the same four, and its
        // top/bottom are the window the picked chain must stand in
        // (M5.1 J).
        | OpParams::Flow5D { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::MultiAxisContour { clearance_z, retract_z, top_z, bottom_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            Some(req("Top of Part Z", *top_z)?),
            Some(req("Bottom of Part Z", *bottom_z)?),
            None,
            opt("Retract Z", *retract_z)?,
        ),
        // The wrap ladder names its own rapids plane - one infeed and
        // a tool radius outside the skin - so there is no hop height
        // to resolve; the four-number shape minus the retract is all
        // it has (M5.1 stage E2).
        OpParams::RotaryOffset { clearance_z, top_z, bottom_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            Some(req("Top of Part Z", *top_z)?),
            Some(req("Bottom of Part Z", *bottom_z)?),
            None,
            None,
        ),
        // The facing ladder is the wrap ladder's height shape spoken
        // on the axis instead of the skin: the same three, no hop -
        // the station rapid names itself one infeed clear of the
        // profile (M5.2 stage A).
        OpParams::Facing { clearance_z, top_z, bottom_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            Some(req("Top of Part Z", *top_z)?),
            Some(req("Facing Plane Z", *bottom_z)?),
            None,
            None,
        ),
        // The profile window stands the same way on the axis - three
        // heights, no hop; the rapids plane of the ride names itself
        // a tolerance clear of the rungs' reach (M5.2 stage B).
        OpParams::ProfileRough { clearance_z, top_z, bottom_z, .. }
        | OpParams::ProfileFinish { clearance_z, top_z, bottom_z, .. }
        // The blade windows state the same trio: three heights, no
        // hop - the rapids ride the whole bar's radius, a law and not
        // a height (M5.2 stage C).
        | OpParams::GrooveRough { clearance_z, top_z, bottom_z, .. }
        | OpParams::GrooveFinish { clearance_z, top_z, bottom_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            Some(req("Top of Part Z", *top_z)?),
            Some(req("Bottom of Part Z", *bottom_z)?),
            None,
            None,
        ),
        // Parting names its bottom for what it is - the plane the
        // blade sinks through, facing's law read as a sever (M5.2 C).
        OpParams::Part {
            clearance_z,
            top_z,
            bottom_z,
            ..
        } => (
            req("Clearance Z", *clearance_z)?,
            Some(req("Top of Part Z", *top_z)?),
            Some(req("Parting Plane Z", *bottom_z)?),
            None,
            None,
        ),
        // The drill bottom is an override of the whole pattern, not a
        // requirement: each hole's own floor is the default depth, so an
        // unresolved (None) bottom is the normal shape, not a failure.
        OpParams::Drill { clearance_z, retract_z, top_z, bottom_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            Some(req("Top of Part Z", *top_z)?),
            opt("Bottom Z", *bottom_z)?,
            None,
            opt("Retract Z", *retract_z)?,
        ),
    };
    Ok(ResolvedHeights { clearance, top, bottom, face_top, retract })
}

/// The resolved depth pair for the verify context and the level list:
/// (top, bottom), Face folded to (cut, cut-or-top) - None parts stay
/// None, never guessed.
#[must_use]
pub fn resolve_map(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    params: &OpParams,
) -> (Option<f64>, Option<f64>) {
    match resolve_params(world, setup, params) {
        Ok(r) => {
            if matches!(params, OpParams::Face { .. }) {
                let cut = r.bottom;
                (cut, cut)
            } else {
                (r.top, r.bottom)
            }
        }
        Err(_) => (None, None),
    }
}

/// Ordering sanity on resolved values: an operation whose top is not
/// above its bottom (or whose finished cut is not below its first stage)
/// is refused at command creation. A height that cannot resolve is not
/// here's business: `resolve_params` refuses it at generation, by name.
#[must_use]
pub fn depth_sane(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    params: &OpParams,
) -> bool {
    let Ok(r) = resolve_params(world, setup, params) else {
        return true; // unresolved is refused elsewhere, never a false lie here
    };
    match params {
        OpParams::Face { .. } => match (r.face_top, r.bottom) {
            (Some(t), Some(c)) => t >= c,
            _ => true,
        },
        _ => match (r.top, r.bottom) {
            (Some(t), Some(b)) => t > b,
            _ => true,
        },
    }
}
