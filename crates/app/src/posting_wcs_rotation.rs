//! The rotated-work-offset clause of the verification gate (#315). A
//! work offset may carry a rotation (`SetWcsOrigin`, the 3-2-1
//! alignment of #187), but the post speaks translation only: the
//! stream carries `OriginX/Y/Z` and every shipped script subtracts that
//! origin from world coordinates. A rotated offset would therefore post
//! a program turned against the part. Until the rotation is carried
//! (baked into the coordinates or a control cycle - #315 step 3), the
//! post refuses it by name; an offset within [`ROT_TOL_RAD`] of the
//! identity posts exactly as before.
//!
//! Provenance: our own design notes (the G54 register is translation
//! only - public RS-274 / ISO 6983 work-offset semantics); no vendor
//! material consulted.

use flecs_ecs::prelude::World;

use crate::model;
use crate::operation::{HeightReference, OpParams};
use crate::posting::HeightRefs;
use crate::schema::WCS_SLOTS;

/// Rotation angle below which an offset counts as unrotated: 1e-9 rad
/// turns a point 1 m out by 1 nm - far under any machine resolution,
/// and far over the float noise of a normalized identity quaternion.
pub(crate) const ROT_TOL_RAD: f64 = 1e-9;

/// The work-offset slot an operation posts to: the first WCS slot its
/// height grammar references, else slot 0 (`G54`). The one law
/// [`crate::posting::wcs_context`] and this clause both read.
pub(crate) fn wcs_slot(params: &OpParams) -> usize {
    params
        .heights()
        .into_iter()
        .find_map(|h| match h.reference {
            HeightReference::WorkOffset(i) if i < WCS_SLOTS.len() => Some(i),
            _ => None,
        })
        .unwrap_or(0)
}

/// Angle (rad, in [0, pi]) and unit axis of a unit quaternion (x, y, z, w).
/// `atan2` rather than `acos(w)`, which is ill-conditioned near identity.
fn angle_axis(q: [f64; 4]) -> (f64, [f64; 3]) {
    let s = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt();
    if !s.is_finite() || s <= 0.0 {
        return (0.0, [0.0, 0.0, 1.0]);
    }
    let sign = if q[3] < 0.0 { -1.0 } else { 1.0 };
    let angle = 2.0 * s.atan2(q[3].abs());
    (angle, [sign * q[0] / s, sign * q[1] / s, sign * q[2] / s])
}

/// The axis as the user reads it: `X`/`Y`/`Z` (signed) when it is a
/// principal axis, else its components.
fn axis_word(a: [f64; 3]) -> String {
    for (i, w) in ["X", "Y", "Z"].iter().enumerate() {
        if (a[i].abs() - 1.0).abs() < 1e-9 {
            return if a[i] < 0.0 { format!("-{w}") } else { (*w).to_string() };
        }
    }
    format!("({:.4}, {:.4}, {:.4})", a[0], a[1], a[2])
}

/// One line per rotated work-offset entity a posted operation uses,
/// naming the setup, the G-word, the rotation and the operations.
/// Empty when every offset in the program is unrotated.
#[must_use]
pub(super) fn rotated_wcs_problems(world: &World) -> Vec<String> {
    // (entity name, line prefix, ops), in first-use program order.
    let mut hits: Vec<(String, String, Vec<String>)> = Vec::new();
    for (name, e) in crate::setup::operation_order(world) {
        let Some(op) = model::operation_of(world, &e) else {
            continue;
        };
        if op.suppressed {
            continue; // not in the program (#110)
        }
        let setup = op.setup.as_deref();
        let slot = wcs_slot(&op.params);
        let Some(wcs) = crate::setup::wcs_entity(world, setup, slot) else {
            continue; // never placed: the identity at the origin
        };
        let Some(loc) = model::transform_of(world, &wcs) else {
            continue;
        };
        let (angle, axis) = angle_axis(loc.rot);
        if angle <= ROT_TOL_RAD {
            continue;
        }
        let entity = crate::setup::wcs_entity_name(world, setup, slot).unwrap_or_default();
        let label = model::operation_label(world, &name, &e);
        if let Some(hit) = hits.iter_mut().find(|h| h.0 == entity) {
            hit.2.push(label);
            continue;
        }
        let word = WCS_SLOTS.get(slot).copied().unwrap_or("G54");
        let owner = match crate::setup::effective(world, setup) {
            Some(s) => format!("setup {s}: its work offset {word}"),
            None => format!("work offset {word}"),
        };
        let prefix = format!(
            "{owner} is rotated {:.3} deg about {}; posting a rotated work offset is not \
             supported yet - re-align it without rotation or post from an unrotated setup",
            angle.to_degrees(),
            axis_word(axis)
        );
        hits.push((entity, prefix, vec![label]));
    }
    hits.into_iter()
        .map(|(_, prefix, ops)| format!("{prefix} (operations: {})", ops.join(", ")))
        .collect()
}
