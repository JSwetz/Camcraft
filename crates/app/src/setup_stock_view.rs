//! The incoming stock of a setup as the viewport draws it (#406): the
//! body the setups before it leave (`stock_sim::incoming_body`), its
//! sheet as triangles in the viewport's (active setup's) frame, and its
//! box and volume - one program sim for both.
//!
//! Provenance: our own design over #317's carry and #320's stock source;
//! showing the stock a later setup starts from is a plain CAM workflow
//! (behaviour only). No vendor implementation inspected.
//!
//! ## The laws here
//!
//! - **Measured once, by the caller's cache.** One call runs the program
//!   sim; the UI keys the answer by document version and never asks per
//!   frame.
//! - **The viewport's frame.** The body stands in the frame of the setup
//!   that receives it (its part alignment); the viewport draws in the
//!   ACTIVE setup's frame ([`crate::setup_align::view_pose`]), so the
//!   sheet is carried by `A_active * A_setup^-1` - identity when the two
//!   are the same setup or neither is aligned.
//! - **Nothing to draw is a refusal, not an empty mesh.** A body with no
//!   material, or one whose sheet holds no facet, is named.

use flecs_ecs::prelude::World;

use crate::geometry::{Point3, Triangle, transform_point};
use crate::schema::Location;
use crate::setup_align;
use crate::setup_stock::Incoming;

/// What a setup starts from, for the eye: the measured box and volume
/// (in the setup's own frame, as the page and the sheet print them) and
/// the body's sheet as triangle soup in the viewport's frame.
#[derive(Debug, Clone)]
pub struct IncomingShape {
    pub incoming: Incoming,
    pub mesh: Vec<Triangle>,
}

/// The rigid map from `setup`'s frame into the frame the viewport draws
/// in (the active setup's).
#[must_use]
pub fn to_view(world: &World, setup: &str) -> Location {
    let from = setup_align::alignment(world, Some(setup)).unwrap_or_default();
    let view = setup_align::alignment(world, crate::setup::active(world).as_deref()).unwrap_or_default();
    setup_align::compose(view, setup_align::inverse(from))
}

/// The incoming stock of `setup`, measured and meshed.
///
/// # Errors
/// Every refusal of the program sim (`stock_sim::incoming_body`), a
/// body with no material left, and a sheet with no facet to draw.
pub fn incoming_shape(world: &World, setup: &str) -> Result<IncomingShape, String> {
    let body = crate::stock_sim::incoming_body(world, setup)?;
    let (lo, hi) = body.bounds().ok_or_else(|| format!("no material reaches setup '{setup}'"))?;
    let incoming = Incoming { min: [lo.x, lo.y, lo.z], max: [hi.x, hi.y, hi.z], volume: body.volume() };
    let sheet = body.display().surface(&[]);
    let map = to_view(world, setup);
    let at = |i: u32| -> Option<Point3> {
        let p = *sheet.verts.get(usize::try_from(i).ok()?)?;
        Some(transform_point(p, map.pos, map.rot))
    };
    let mut mesh = Vec::with_capacity(sheet.tris.len());
    for t in &sheet.tris {
        // An index outside the vertex list is a sheet bug: skip the facet
        // rather than draw a wrong one (never a panic on a display path).
        if let (Some(a), Some(b), Some(c)) = (at(t[0]), at(t[1]), at(t[2])) {
            mesh.push(Triangle::new(a, b, c));
        }
    }
    // The sheet is the top only; close it into a block (hand check of
    // #519: the bare sheet lay buried inside the part).
    for t in crate::setup_stock_shell::floor_and_walls(&body) {
        let m = |p: Point3| transform_point(p, map.pos, map.rot);
        mesh.push(Triangle::new(m(t.a), m(t.b), m(t.c)));
    }
    if mesh.is_empty() {
        return Err(format!("the stock reaching setup '{setup}' has no surface to draw"));
    }
    Ok(IncomingShape { incoming, mesh })
}
