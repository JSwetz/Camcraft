//! The pivot of a typed part-alignment turn (#606): the Setups page's
//! "Part alignment" angles set the part's orientation (X, then Y, then Z,
//! from as modelled) and turn it about a pivot - the setup's WCS origin
//! by default, or the centre of the parts' bounds.
//!
//! - **About the WCS origin**, the part keeps its relation to the origin:
//!   the point of the part that sits on the origin NOW stays on it, and
//!   the rest of the part turns around it. The origin is the setup's own
//!   WCS (a translation in the setup frame), or the frame's zero when no
//!   WCS is placed - the same reading the post makes.
//! - **About the part centre**, the centre of the parts' bounds stays
//!   where it is (the presets' pivot: a flip keeps its footprint).
//!
//! Provenance: our own design (#606, the maintainer's "it shifts the
//! model relative to the origin"); a turn about a fixed point is
//! standard rigid-body algebra (p' = R (p - q) + o). No vendor material
//! consulted.

use flecs_ecs::prelude::World;

use crate::geometry::{Point3, quat_rotate_point, transform_point};
use crate::model;
use crate::schema::Location;
use crate::setup_align;

/// What a typed turn keeps fixed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TurnPivot {
    /// The part's point on the setup's WCS origin stays on it (#606).
    #[default]
    WcsOrigin,
    /// The centre of the parts' bounds stays put (the presets' pivot).
    PartCentre,
}

impl TurnPivot {
    pub const ALL: [TurnPivot; 2] = [TurnPivot::WcsOrigin, TurnPivot::PartCentre];

    /// The plain words, for the Setups page.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            TurnPivot::WcsOrigin => "WCS origin",
            TurnPivot::PartCentre => "part centre",
        }
    }
}

/// The setup's WCS origin in its setup frame: the WCS entity's position,
/// or zero when none is placed (what the post measures from).
#[must_use]
pub fn wcs_origin(world: &World, setup: &str) -> Point3 {
    crate::setup::wcs_entity(world, Some(setup))
        .and_then(|e| model::transform_of(world, &e))
        .map_or(Point3::ZERO, |l| l.pos)
}

/// The alignment that holds the parts turned by `rot` (from as modelled)
/// about `pivot`. All-zero angles are "as modelled" (`None`) whatever the
/// pivot - the words a stored turn reads back as never hide a shift.
pub fn turned(world: &World, setup: &str, rot: [f64; 4], pivot: TurnPivot) -> Result<Option<Location>, String> {
    if setup_align::is_identity_rot(rot) {
        return Ok(None);
    }
    let c = setup_align::part_centre(world).ok_or("there is no part to align - import a model first")?;
    match pivot {
        TurnPivot::PartCentre => Ok(setup_align::about_centre(rot, c)),
        TurnPivot::WcsOrigin => {
            let o = wcs_origin(world, setup);
            // The document point the current alignment holds on the origin.
            let q = match setup_align::stored(world, setup) {
                Some(a) => {
                    let inv = setup_align::inverse(a);
                    transform_point(o, inv.pos, inv.rot)
                }
                None => o,
            };
            let rq = quat_rotate_point(rot, q);
            let pos = Point3::new(o.x - rq.x, o.y - rq.y, o.z - rq.z);
            if ![pos.x, pos.y, pos.z].iter().all(|v| v.is_finite()) {
                return Err("the turn about the WCS origin is not finite".into());
            }
            Ok(Some(Location { pos, rot }))
        }
    }
}
