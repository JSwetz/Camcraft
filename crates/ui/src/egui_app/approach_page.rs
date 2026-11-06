//! The rest of the "Lead in / Lead out" tab (M11.3 B/C/D, user review
//! 2026-11-05: "Entry - Arc should have an option of setting radius and
//! length of the arc. Entry - Ramp lacks all usual settings ... All entry
//! methods are missing extension values ... plunge data ... engagement"):
//! the entry's own shape numbers, the entry feeds and the engagement
//! limit, and - on the contour - the lead-out shape, extensions, vertical
//! lead-in, overlap and the axis-aligned start. Draws into the draft
//! params; the page's Apply rides the command history like every field.
//!
//! Provenance: our own UI over the standard entry/lead parameter
//! vocabulary of published CAM practice (the motion crate's
//! `approach.rs` names the sources); no vendor material or text.

use egui::Ui;

use camcraft_app::links::Approach;
use camcraft_app::machining2d::Lead;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::units::Unit;

use super::pages::{combo, field, num, num_feed, num_plain};

const LEAD_OUTS: [&str; 6] = ["Same as lead-in", "None", "Linear", "Arc", "Tangential", "Perpendicular"];

/// The entry arm's own numbers (`arm` - the first entry, or the level
/// descent the user set), then the plunge height, the entry feeds and the
/// engagement limit every ladder reads.
pub(super) fn entry_shape_editor(ui: &mut Ui, u: Unit, arm: Option<Entry3D>, a: &mut Approach) {
    match arm {
        Some(Entry3D::Ramp) => {
            field(ui, "Entry ramp angle (deg, 0 = one straight chord)");
            num_plain(ui, &mut a.ramp_angle);
            field(ui, "Max ramp leg length (0 = no limit)");
            num(ui, u, &mut a.ramp_max_len);
            ui.checkbox(&mut a.ramp_to_depth, "Ramp from the top of the cut every level");
            ui.label("set an angle and the ramp zig-zags along the first cut, never steeper");
        }
        Some(Entry3D::Helix) => {
            field(ui, "Helix minimum radius (0 = only the set radius)");
            num(ui, u, &mut a.helix_min_radius);
            field(ui, "Entry helix pitch (0 = one turn per helix diameter)");
            num(ui, u, &mut a.helix_pitch);
            field(ui, "Entry helix angle (deg, 0 = by pitch)");
            num_plain(ui, &mut a.helix_angle);
            ui.checkbox(&mut a.helix_cw, "Helix clockwise (G2)");
            ui.label("the helix closes on the first cut's start where it fits there; a ramp, then a plunge, stand in");
        }
        Some(Entry3D::Arc) => {
            field(ui, "Arc entry radius (0 = tool diameter)");
            num(ui, u, &mut a.arc_radius);
            field(ui, "Arc sweep (deg, 0 = cover the drop)");
            num_plain(ui, &mut a.arc_sweep);
            ui.label("a shorter sweep covers less of the drop; the rest feeds straight down at the arc's start");
        }
        Some(Entry3D::Plunge) | None => {}
    }
    field(ui, "Plunge height above material (0 = feed from the traverse plane)");
    num(ui, u, &mut a.plunge_height);
    ui.label("rapid down to this height over the material, feed only the rest");
    field(ui, "Ramp / helix / arc feed (0 = cutting feed)");
    num_feed(ui, u, &mut a.ramp_feed);
    field(ui, "Entry plunge feed (0 = plunge feed)");
    num_feed(ui, u, &mut a.plunge_feed);
    field(ui, "Lead-in feed (% of cutting feed)");
    num_plain(ui, &mut a.lead_feed_pct);
    field(ui, "Entry engagement limit (% of D, 0 = off)");
    let mut pct = a.max_engagement * 100.0;
    let before = pct;
    num_plain(ui, &mut pct);
    if pct != before {
        a.max_engagement = pct / 100.0;
    }
    ui.label("a descent cuts the full diameter: over the limit its feeds scale down to it, named in the status");
}

fn lead_idx(l: Option<Lead>) -> usize {
    match l {
        None => 0,
        Some(Lead::None) => 1,
        Some(Lead::Linear { .. }) => 2,
        Some(Lead::Arc { .. }) => 3,
        Some(Lead::Tangential { .. }) => 4,
        Some(Lead::Perpendicular { .. }) => 5,
    }
}

/// The contour's lead geometry beyond the lead-in's shape.
pub(super) fn contour_leads_editor(ui: &mut Ui, u: Unit, a: &mut Approach) {
    field(ui, "Lead out");
    let mut i = lead_idx(a.lead_out);
    let before = i;
    combo(ui, "c_lead_out", &mut i, &LEAD_OUTS);
    if i != before {
        a.lead_out = match i {
            1 => Some(Lead::None),
            2 => Some(Lead::Linear { length: 8.0 }),
            3 => Some(Lead::Arc { radius: 5.0 }),
            4 => Some(Lead::Tangential { radius: 5.0 }),
            5 => Some(Lead::Perpendicular { length: 5.0 }),
            _ => None,
        };
    }
    match &mut a.lead_out {
        Some(Lead::Linear { length } | Lead::Perpendicular { length }) => {
            field(ui, "Lead-out length");
            num(ui, u, length);
        }
        Some(Lead::Arc { radius } | Lead::Tangential { radius }) => {
            field(ui, "Lead-out arc radius");
            num(ui, u, radius);
        }
        Some(Lead::None) | None => {}
    }
    field(ui, "Extension before lead-in");
    num(ui, u, &mut a.ext_in);
    field(ui, "Extension after lead-out");
    num(ui, u, &mut a.ext_out);
    field(ui, "Vertical lead-in height");
    num(ui, u, &mut a.lead_height);
    field(ui, "Overlap past the start");
    num(ui, u, &mut a.overlap);
    ui.checkbox(&mut a.axis_start, "Axis-aligned lead start");
    if a.axis_start {
        field(ui, "Axis tolerance (deg)");
        num_plain(ui, &mut a.axis_tol);
        ui.label("starts each ring on an X- or Y-aligned run, so an Arc entry posts as one G18/G19 arc");
    }
    ui.label("every lead and extension stays in the legal room - shrunk, then refused by name");
}

/// The entry fallbacks of a generated path, as lines for the op page's
/// status block (M11.3, the user's "say it on the op page"): every entry
/// substitution, second dive, centre-cutting fallback and lifted link the
/// generator named, each with the fix a user can make.
#[must_use]
pub(crate) fn entry_status_lines(notes: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for n in notes {
        let fix = if n.contains("not centre-cutting") {
            "fix: a centre-cutting tool, or a Helix / Ramp entry that fits"
        } else if n.contains("second dive") {
            "fix: a helix radius that fits at the first cut's start, or Keep down links"
        } else if n.contains("axial dive") || n.contains("fits nowhere") || n.contains("stands down to a ramp") {
            "fix: a smaller helix radius (or a minimum radius), a pre-drilled start hole, or a smaller tool"
        } else if n.starts_with("link: the tool lifted") {
            "fix: Retract plane links, or a smaller stepover so the cuts it crosses are already cleared"
        } else {
            continue;
        };
        let body = n.strip_prefix("entry: ").unwrap_or(n);
        out.push(format!("Entry: {body} - {fix}"));
    }
    out
}
