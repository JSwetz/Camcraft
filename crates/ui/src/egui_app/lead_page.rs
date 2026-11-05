//! The operation page's "Lead in / Lead out" section for every level-
//! ladder kind (M11.2 C/D, user review 2026-11-05: entry, transition and
//! link settings in their own section, "as is common in most CAM
//! packages"): the first entry, the descent between levels, and how the
//! tool travels between levels and between passes - all the user's,
//! nothing premade. Draws into the draft params; the page's Apply rides
//! the command history like every other field.
//!
//! Provenance: our own UI over standard CAM link strategies (the motion
//! crate's `links.rs` names the sources); no vendor material or text.

use egui::Ui;

use camcraft_app::links::{Link, Links};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::units::Unit;

use super::pages::{combo, field, num, num_plain};

const ENTRY_KINDS: [&str; 4] = ["Plunge", "Ramp", "Helix", "Arc"];
const TRANSITIONS: [&str; 5] = ["Same as entry", "Plunge", "Ramp", "Helix", "Arc"];
const LINKS: [&str; 3] = ["Clearance plane", "Retract plane", "Keep down"];

fn entry_idx(e: Entry3D) -> usize {
    match e {
        Entry3D::Plunge => 0,
        Entry3D::Ramp => 1,
        Entry3D::Helix => 2,
        Entry3D::Arc => 3,
    }
}

fn entry_of(i: usize) -> Entry3D {
    match i {
        1 => Entry3D::Ramp,
        2 => Entry3D::Helix,
        3 => Entry3D::Arc,
        _ => Entry3D::Plunge,
    }
}

fn link_idx(l: Link) -> usize {
    match l {
        Link::Clearance => 0,
        Link::Retract => 1,
        Link::KeepDown => 2,
    }
}

fn link_of(i: usize) -> Link {
    match i {
        1 => Link::Retract,
        2 => Link::KeepDown,
        _ => Link::Clearance,
    }
}

/// The first entry: the family combo and its numbers. `entry` is `None`
/// for a kind whose first engagement is not a choice (the bore's centre
/// dive).
pub(super) fn entry_editor(ui: &mut Ui, u: Unit, tag: &str, entry: &mut Entry3D, helix_radius: &mut f64) {
    field(ui, "Entry");
    let mut e_idx = entry_idx(*entry);
    let before = e_idx;
    combo(ui, &format!("{tag}_entry"), &mut e_idx, &ENTRY_KINDS);
    if e_idx != before {
        *entry = entry_of(e_idx);
    }
    if matches!(entry, Entry3D::Helix) {
        field(ui, "Helix lead radius");
        num(ui, u, helix_radius);
        ui.label("a plunge stands in wherever a helix of this radius cannot fit");
    }
    if matches!(entry, Entry3D::Arc) {
        ui.label("a tangent arc onto the first cut; a plunge stands in wherever its lead cannot fit");
    }
}

/// The level transition and the two links. `helix_radius` is shown for a
/// Helix transition when the entry is not already showing it (`None`
/// where the kind has no radius of its own to edit).
pub(super) fn links_editor(
    ui: &mut Ui,
    u: Unit,
    tag: &str,
    entry: Option<Entry3D>,
    helix_radius: Option<&mut f64>,
    links: &mut Links,
) {
    field(ui, "Level to level descent");
    let mut t_idx = links.transition.map_or(0, |e| entry_idx(e) + 1);
    combo(ui, &format!("{tag}_transition"), &mut t_idx, &TRANSITIONS);
    links.transition = t_idx.checked_sub(1).map(entry_of);
    match links.transition {
        None => {
            ui.label("each level is entered the way the first one is");
        }
        Some(Entry3D::Ramp) => {}
        Some(Entry3D::Helix) => {
            if entry != Some(Entry3D::Helix) {
                match helix_radius {
                    Some(r) => {
                        field(ui, "Descent helix radius");
                        num(ui, u, r);
                    }
                    None => {
                        ui.label("the helix descends on a 1 mm radius");
                    }
                }
            }
            field(ui, "Helix pitch (0 = one turn per tool diameter)");
            num(ui, u, &mut links.helix_pitch);
        }
        Some(Entry3D::Arc) => {
            ui.label("a vertical-plane arc onto the first cut; a plunge stands in where it cannot fit");
        }
        Some(Entry3D::Plunge) => {
            ui.label("an axial dive where the floor has room");
        }
    }
    if links.reads_ramp_angle() {
        field(ui, "Max ramp angle (deg)");
        num_plain(ui, &mut links.ramp_angle);
    }
    field(ui, "Between levels");
    let mut l_idx = link_idx(links.level);
    combo(ui, &format!("{tag}_level_link"), &mut l_idx, &LINKS);
    links.level = link_of(l_idx);
    field(ui, "Between passes");
    let mut p_idx = link_idx(links.pass);
    combo(ui, &format!("{tag}_pass_link"), &mut p_idx, &LINKS);
    links.pass = link_of(p_idx);
    if links.level == Link::KeepDown || links.pass == Link::KeepDown {
        field(ui, "Max stay-down distance (0 = no limit)");
        num(ui, u, &mut links.max_stay_down);
        ui.label(
            "the tool stays at depth only where the link is proved clear of the part; \
             every other link lifts to the retract plane, named in the status line",
        );
    }
}
