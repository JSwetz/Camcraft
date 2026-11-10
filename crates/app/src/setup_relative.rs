//! Setup-to-setup transforms (#316): how the part moved between two
//! setups (a flip, a turn on the table), and the link that makes one
//! setup a re-clamp of another.
//!
//! Provenance: our own design; re-clamping a part flipped or turned
//! between operations is plain shop practice, and the algebra is
//! standard rigid-body composition (T = A_to * A_from^-1). No vendor
//! implementation inspected.
//!
//! ## The laws here
//!
//! - **One transform, not two.** Each setup stores ONE rigid transform,
//!   its part alignment (#326, document -> setup frame). The transform
//!   between two setups is derived from those two
//!   ([`transform_between`]); nothing here stores a second one.
//! - **On the part, never the work offset.** The work offset is a plain
//!   translation placed in the setup frame (`G54`..`G59` from
//!   [`crate::setup::wcs_slot`]); it says where zero is, not how the
//!   part moved, so no transform is read from it.
//! - **A link carries edits.** A setup placed relative to another keeps
//!   the transform between them when that other setup's alignment
//!   changes ([`carried`]): `AlignPartToSetup` re-places every linked
//!   setup in the same undo unit, or refuses the whole change by name.
//! - **Refuse what is not rigid.** A missing setup, a name that is no
//!   setup, a non-finite or non-unit stored transform: named errors,
//!   never an identity guess.

use flecs_ecs::prelude::World;

use crate::geometry::{Point3, transform_point};
use crate::schema::Location;
use crate::setup;
use crate::setup_align::{self, compose, inverse};

/// Is this transform the identity (a micron, a micro-radian)?
#[must_use]
pub fn is_still(a: Location) -> bool {
    let p = a.pos;
    p.x.abs() <= 1e-9 && p.y.abs() <= 1e-9 && p.z.abs() <= 1e-9 && setup_align::is_identity_rot(a.rot)
}

/// Identity is absence: the stored form of an alignment.
#[must_use]
pub fn normalised(a: Location) -> Option<Location> {
    (!is_still(a)).then_some(a)
}

/// Do two stored alignments hold the part the same way?
#[must_use]
pub fn same(a: Option<Location>, b: Option<Location>) -> bool {
    is_still(compose(a.unwrap_or_default(), inverse(b.unwrap_or_default())))
}

/// The setup a setup is placed relative to (`None`: not linked, or no
/// such setup).
#[must_use]
pub fn relative_to(world: &World, name: &str) -> Option<String> {
    setup::def_of(world, name).and_then(|d| d.relative_to)
}

/// The setups placed directly relative to `parent`, in setup order.
#[must_use]
pub fn dependents(world: &World, parent: &str) -> Vec<String> {
    setup::names(world)
        .into_iter()
        .filter(|n| relative_to(world, n).as_deref() == Some(parent))
        .collect()
}

/// Does following the links up from `from` reach `target` (or is `from`
/// itself)? Bounded by the setup count, so a looped world cannot hang.
#[must_use]
pub fn links_back_to(world: &World, from: &str, target: &str) -> bool {
    let mut at = Some(from.to_owned());
    for _ in 0..=setup::names(world).len() {
        match at {
            Some(n) if n == target => return true,
            Some(n) => at = relative_to(world, &n),
            None => return false,
        }
    }
    true // a loop: treat as reaching, so the caller refuses
}

/// A setup's stored alignment, checked rigid; refused by name.
fn rigid_alignment(world: &World, name: &str) -> Result<Location, String> {
    let def = setup::def_of(world, name).ok_or_else(|| {
        if crate::model::find_by_name(world, name).is_some() {
            format!("'{name}' is not a setup")
        } else {
            format!("there is no setup named '{name}'")
        }
    })?;
    let Some(a) = def.part_alignment else { return Ok(Location::default()) };
    let [x, y, z, w] = a.rot;
    let norm = (x * x + y * y + z * z + w * w).sqrt();
    if !a.pos.is_finite() || !norm.is_finite() || (norm - 1.0).abs() > 1e-9 {
        return Err(format!("setup '{name}' holds the part at a placement that is not a rigid transform"));
    }
    Ok(a)
}

/// The rigid transform that takes the part as it sits in setup `from` to
/// where it sits in setup `to`: `p_to = T * p_from`, with
/// `T = A_to * A_from^-1` from the two part alignments. The identity for
/// `from == to` and for two setups holding the part alike. Composes
/// (A -> C = (B -> C) * (A -> B)) and inverts (B -> A = (A -> B)^-1).
///
/// # Errors
/// A name that is no setup, or a stored alignment that is not rigid -
/// named, never read as the identity.
pub fn transform_between(world: &World, from: &str, to: &str) -> Result<Location, String> {
    let a_from = rigid_alignment(world, from)?;
    let a_to = rigid_alignment(world, to)?;
    let t = compose(a_to, inverse(a_from));
    if !t.pos.is_finite() || t.rot.iter().any(|v| !v.is_finite()) {
        return Err(format!("the transform from setup '{from}' to setup '{to}' is not finite"));
    }
    Ok(t)
}

/// The alignment of a setup turned by `rot` RELATIVE to setup `parent`:
/// the part as `parent` holds it, turned in `parent`'s frame about the
/// centre of its bounds there (so a flip keeps it on the same spot of
/// the table). `Ok(None)` is "as modelled" (the identity overall).
///
/// # Errors
/// `parent` is no rigid setup, or there is no part to turn.
pub fn relative_alignment(world: &World, parent: &str, rot: [f64; 4]) -> Result<Option<Location>, String> {
    let p = rigid_alignment(world, parent)?;
    if setup_align::is_identity_rot(rot) {
        return Ok(normalised(p));
    }
    let c = setup_align::part_centre(world).ok_or_else(|| "there is no part to turn - import a model first".to_owned())?;
    let c_in_parent = transform_point(c, p.pos, p.rot);
    let turn = setup_align::about_centre(rot, c_in_parent).unwrap_or_default();
    Ok(normalised(compose(turn, p)))
}

/// Every setup linked (directly or through others) below `parent`, with
/// its alignment before and after `parent`'s changes from `prev` to
/// `next` - each keeps the transform it had to the setup it is linked
/// to. Parents come before their children; a setup whose alignment would
/// not change is left out. Loops are cut (each setup visited once).
#[must_use]
pub fn carried(
    world: &World,
    parent: &str,
    prev: Option<Location>,
    next: Option<Location>,
) -> Vec<(String, Option<Location>, Option<Location>)> {
    let mut out = Vec::new();
    let mut seen = vec![parent.to_owned()];
    let mut queue = vec![(parent.to_owned(), prev, next)];
    while let Some((p, p_prev, p_next)) = queue.pop() {
        for child in dependents(world, &p) {
            if seen.contains(&child) {
                continue;
            }
            seen.push(child.clone());
            let c_prev = setup_align::stored(world, &child);
            let rel = compose(c_prev.unwrap_or_default(), inverse(p_prev.unwrap_or_default()));
            let moved = compose(rel, p_next.unwrap_or_default());
            let c_next = normalised(Location { rot: unit(moved.rot), ..moved });
            if !same(c_prev, c_next) {
                out.push((child.clone(), c_prev, c_next));
            }
            queue.push((child, c_prev, c_next));
        }
    }
    out
}

/// Re-normalise a product of unit quaternions (float drift only).
fn unit(q: [f64; 4]) -> [f64; 4] {
    let n = q.iter().map(|v| v * v).sum::<f64>().sqrt();
    if n.is_finite() && n > 0.0 { q.map(|v| v / n) } else { q }
}

/// The plain words for how a linked setup sits against the setup it is
/// placed relative to: "Same as Setup1", "Flip over X from Setup1", or
/// the three angles. `None` for a setup that is not linked.
#[must_use]
pub fn describe(world: &World, name: &str) -> Option<String> {
    let parent = relative_to(world, name)?;
    let Ok(t) = transform_between(world, &parent, name) else {
        return Some(format!("placed relative to {parent} (the transform cannot be read)"));
    };
    if is_still(t) {
        return Some(format!("Same as {parent}"));
    }
    let words = match setup_align::preset_of(t.rot) {
        Some(p) if p != setup_align::AlignPreset::AsModelled => p.label().to_owned(),
        Some(_) => "Moved".to_owned(),
        None => {
            let [x, y, z] = setup_align::angles_of(Some(t));
            format!("Turned X {x}°, Y {y}°, Z {z}°")
        }
    };
    Some(format!("{words} from {parent}"))
}

/// The turn (about X, then Y, then Z) a linked setup holds against its
/// parent, in degrees - the typed-angle draft of the page.
#[must_use]
pub fn angles(world: &World, name: &str) -> [f64; 3] {
    relative_to(world, name)
        .and_then(|p| transform_between(world, &p, name).ok())
        .map_or([0.0; 3], |t| setup_align::angles_of(Some(t)))
}

/// The rotation part of a turn given as angles, refused when not finite.
///
/// # Errors
/// A non-finite angle.
pub fn rotation(deg: [f64; 3]) -> Result<[f64; 4], String> {
    setup_align::rotation_from_angles(deg).ok_or_else(|| "the turn angles must be finite numbers".to_owned())
}

/// A point carried from one setup's frame into another's.
///
/// # Errors
/// As [`transform_between`].
pub fn carry_point(world: &World, from: &str, to: &str, p: Point3) -> Result<Point3, String> {
    let t = transform_between(world, from, to)?;
    Ok(transform_point(p, t.pos, t.rot))
}

/// Where a setup's WCS (work offset) is drawn while the viewport shows
/// the ACTIVE setup's frame (#558). A WCS is setup furniture: placed in
/// its own setup's frame, so a point fixed on the part there. Another
/// setup's WCS therefore goes through the same rigid transform the part
/// goes through between the two frames ([`transform_between`]) - it stays
/// on the same spot of the part, wherever the active setup holds it.
///
/// `Ok(Some(pose))`: draw it there. `Ok(None)`: the name is no setup's
/// WCS, or the document has no active setup - the caller draws the
/// stored pose (every document before #326, and every WCS of the active
/// setup itself, map through the identity).
///
/// # Errors
/// The owning setup's transform to the active one is refused (no rigid
/// alignment) - the caller must not draw it at a guessed place.
pub fn wcs_view_pose(world: &World, wcs_name: &str, stored: Location) -> Result<Option<Location>, String> {
    let Some(active) = setup::active(world) else {
        return Ok(None);
    };
    let owners: Vec<String> = setup::names(world)
        .into_iter()
        .filter(|s| setup::wcs_entity_name(world, Some(s)) == wcs_name)
        .collect();
    if owners.is_empty() || owners.contains(&active) {
        return Ok(None); // not a setup's WCS, or the active setup's own
    }
    // A WCS two setups share sits in each one's frame alike only when they
    // hold the part alike; map from the first owner, refused by name else.
    let t = transform_between(world, &owners[0], &active).inspect_err(|e| {
        tracing::debug!(wcs = wcs_name, "not drawn in setup '{active}' frame: {e}");
    })?;
    Ok(Some(compose(t, stored)))
}
