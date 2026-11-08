//! The work-offset slot law (#196): which `G54`..`G59` word an
//! operation posts at, and the two refusals a per-setup pick brings.
//!
//! - **The pick governs.** A setup that picked its program offset
//!   (`SetupDef::work_offset`, set by `SetSetupWorkOffset`) posts every
//!   one of its operations at that word, measured from that slot's
//!   placement in the setup. A setup that picked none keeps the
//!   inferred law every older document posted with: the first slot the
//!   operation's heights reference, else `G54`.
//! - **A height on another slot is a conflict, refused by name.** In a
//!   picked setup, an operation whose heights are measured from a
//!   DIFFERENT slot would, under the old law, have posted at that slot;
//!   the pick silently overriding it is an offset the user did not
//!   choose for that operation, so the gate names both and asks.
//! - **One program, one register per word.** A single NC file in which
//!   two setups post the same word with different placements would run
//!   the second setup in the first one's zero - the controller holds one
//!   `G54`. The one-file post refuses it by name; per-operation files
//!   are exempt (each file is one setup's, and its offset is set for it).
//!
//! Provenance: our own design notes over RS-274 / ISO 6983 work-offset
//! semantics (G54..G59 select one of six controller-held zero registers);
//! no vendor material consulted.

use flecs_ecs::prelude::World;

use crate::model;
use crate::operation::{HeightReference, OpParams};
use crate::posting::HeightRefs;
use crate::schema::{Location, WCS_SLOTS};

/// Placement difference below which two offsets are the same zero:
/// 1 nm, far under any machine resolution.
const SAME_MM: f64 = 1e-6;

/// The first WCS slot an operation's heights reference - the inferred
/// law's answer before its `G54` default.
fn height_slot(params: &OpParams) -> Option<usize> {
    params.heights().into_iter().find_map(|h| match h.reference {
        HeightReference::WorkOffset(i) if i < WCS_SLOTS.len() => Some(i),
        _ => None,
    })
}

/// The work-offset slot an operation of the `setup` context posts at:
/// the setup's pick, else the first slot its heights reference, else
/// slot 0 (`G54`). THE law: [`crate::posting::wcs_context`] and every
/// clause of the gate read it.
pub(crate) fn wcs_slot(world: &World, setup: Option<&str>, params: &OpParams) -> usize {
    picked(world, setup)
        .or_else(|| height_slot(params))
        .unwrap_or(0)
}

/// The pick of the effective setup of a declared context.
fn picked(world: &World, setup: Option<&str>) -> Option<usize> {
    let s = crate::setup::effective(world, setup)?;
    crate::setup::work_offset(world, &s)
}

fn word(slot: usize) -> &'static str {
    WCS_SLOTS.get(slot).copied().unwrap_or("G54")
}

/// One line per posted operation of a picked setup whose heights are
/// measured from a different slot than the pick (module header).
#[must_use]
pub(super) fn slot_problems(world: &World) -> Vec<String> {
    let mut out = Vec::new();
    for (name, e) in crate::setup::operation_order(world) {
        let Some(op) = model::operation_of(world, &e) else {
            continue;
        };
        if op.suppressed {
            continue; // not in the program (#110)
        }
        let setup = op.setup.as_deref();
        let (Some(pick), Some(h)) = (picked(world, setup), height_slot(&op.params)) else {
            continue;
        };
        if pick == h {
            continue;
        }
        let s = crate::setup::effective(world, setup).unwrap_or_default();
        out.push(format!(
            "{}: its heights are measured from {} but setup {s} posts at {} - measure them \
             from {} or pick {} for the setup",
            model::operation_label(world, &name, &e),
            word(h),
            word(pick),
            word(pick),
            word(h)
        ));
    }
    out
}

/// The placement a setup's slot is measured from (unplaced = the origin,
/// the law `wcs_context` reads).
fn placement(world: &World, setup: Option<&str>, slot: usize) -> Location {
    crate::setup::wcs_entity(world, setup, slot)
        .and_then(|e| model::transform_of(world, &e))
        .unwrap_or_default()
}

fn same_zero(a: &Location, b: &Location) -> bool {
    let pos = (a.pos.x - b.pos.x).abs() <= SAME_MM
        && (a.pos.y - b.pos.y).abs() <= SAME_MM
        && (a.pos.z - b.pos.z).abs() <= SAME_MM;
    // q and -q are one rotation.
    let rot = |s: f64| (0..4).all(|i| (a.rot[i] - s * b.rot[i]).abs() <= 1e-9);
    pos && (rot(1.0) || rot(-1.0))
}

/// The one-file clause (module header): one line per word that two
/// setups of the program post at from different placements. Empty when
/// every word means one zero.
#[must_use]
pub(super) fn shared_word_problems(world: &World) -> Vec<String> {
    // (slot, setup, placement), first use in program order.
    let mut seen: Vec<(usize, String, Location)> = Vec::new();
    let mut out: Vec<String> = Vec::new();
    for (_, e) in crate::setup::operation_order(world) {
        let Some(op) = model::operation_of(world, &e) else {
            continue;
        };
        if op.suppressed {
            continue;
        }
        let setup = op.setup.as_deref();
        let slot = wcs_slot(world, setup, &op.params);
        let name = crate::setup::effective(world, setup).unwrap_or_default();
        let here = placement(world, setup, slot);
        match seen.iter().find(|(s, n, _)| *s == slot && *n != name) {
            Some((_, first, there)) if !same_zero(there, &here) => {
                let line = format!(
                    "setups {first} and {name} both post at {} from different placements \
                     ({first}: X{:.3} Y{:.3} Z{:.3}, {name}: X{:.3} Y{:.3} Z{:.3}) - one program \
                     holds one {} register; pick another work offset for {name} on the Setups \
                     page, or post one file per operation",
                    word(slot),
                    there.pos.x,
                    there.pos.y,
                    there.pos.z,
                    here.pos.x,
                    here.pos.y,
                    here.pos.z,
                    word(slot)
                );
                if !out.contains(&line) {
                    out.push(line);
                }
            }
            _ => {}
        }
        if !seen.iter().any(|(s, n, _)| *s == slot && *n == name) {
            seen.push((slot, name, here));
        }
    }
    out
}
