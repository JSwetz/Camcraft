//! The Heights tab's height editor (M1.5.4's From / Offset law, regrouped
//! for the user's 2026-11-06 review: "a flat stack ... no grouping and
//! ragged alignment"): each height is its own titled group - "Clearance
//! Z", "Top Z", ... - holding the From reference, the Work-offset slot
//! when the reference is one, and the Offset with the resolved Z as a
//! dimmed suffix, all on the shared property grid. Plus the two height
//! relationship notes (retract vs clearance, the corner), which stay
//! visible: they are consequences, not help.
//!
//! Provenance: our own port of the M-c height rows (the wording is
//! `crate::operations`' law); no vendor material consulted.

use egui::{Label, RichText, Ui};

use camcraft_app::operation::{Height, HeightReference, SrcRef};
use camcraft_app::schema::WCS_SLOTS;
use camcraft_app::units;

use crate::operations::{height_echo, offset_label};

use super::fields::{combo, num};
use super::layout::{consequence, group, prop_row};

/// The frame-temp key `params_fields` stores the edited op's source under,
/// for the height editors' From: Selection echo (M11.3 A).
pub(super) const HEIGHT_SRC_ID: &str = "camcraft.height_src";

fn op_src(ui: &Ui) -> Option<SrcRef> {
    ui.data(|d| d.get_temp(egui::Id::new(HEIGHT_SRC_ID)))
}

/// One height as a titled group. `tag` scopes the widgets' egui ids: the
/// combos salt through `make_persistent_id`, which ignores position, so
/// five height editors in one panel would otherwise share one id per
/// combo (the duplicate-id badges the first S2 dump caught). Each call
/// site passes its section name.
pub(super) fn height_editor(
    ui: &mut Ui,
    title: &str,
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    h: &mut Height,
    tag: &str,
) {
    ui.push_id(tag, |ui| {
        group(ui, title, None, |ui| height_rows(ui, world, setup, h, height_tag_is_bottom(tag)));
    });
}

/// The retract-vs-clearance note under the Heights rows, resolved through
/// the edited op's own source (the same frame-temp the echo reads), so a
/// Selection-referenced height speaks too (M11.3 A owed item).
pub(super) fn retract_note(
    ui: &mut Ui,
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    retract: &Height,
    clearance: &Height,
) {
    let src = op_src(ui);
    if let Some(note) = crate::operations::retract_note(world, setup, src.as_ref(), retract, clearance) {
        consequence(ui, &note);
    }
}

pub(super) fn corner_note(ui: &mut Ui, world: &flecs_ecs::prelude::World, setup: Option<&str>, top: &Height, drop: f64) {
    let src = op_src(ui);
    consequence(ui, &crate::operations::corner_note(world, setup, src.as_ref(), top, drop));
}

/// Whether a height editor's tag names a floor - the last level, the cut
/// plane, the facing or parting plane - so "Selection" there measures from
/// the selection's LOWEST Z; every other height from its highest.
fn height_tag_is_bottom(tag: &str) -> bool {
    [".bottom", ".cut", ".face", ".plane"].iter().any(|s| tag.ends_with(s))
}

fn height_rows(ui: &mut Ui, world: &flecs_ecs::prelude::World, setup: Option<&str>, h: &mut Height, bottom: bool) {
    let u = units::of(world);
    let src = op_src(ui);
    let mut idx = h.reference.kind_index().min(HeightReference::KIND_LABELS.len() - 1);
    let before = idx;
    prop_row(ui, "From", |ui| combo(ui, "h_kind", &mut idx, &HeightReference::KIND_LABELS));
    if idx != before {
        h.reference = match idx {
            0 => HeightReference::Fixed,
            1 => HeightReference::TopOfStock,
            2 => HeightReference::BottomOfStock,
            3 => HeightReference::TopOfModel,
            4 => HeightReference::BottomOfModel,
            5 => HeightReference::WorkOffset(h.reference.wcs_slot().min(WCS_SLOTS.len() - 1)),
            _ => HeightReference::selection_for(bottom),
        };
    }
    if matches!(h.reference, HeightReference::WorkOffset(_)) {
        let mut slot = h.reference.wcs_slot().min(WCS_SLOTS.len() - 1);
        prop_row(ui, "WCS", |ui| combo(ui, "h_wcs", &mut slot, &WCS_SLOTS));
        h.reference = HeightReference::WorkOffset(slot.min(WCS_SLOTS.len() - 1));
    }
    // Both strings are operations.rs's law (the retired imgui height row
    // said the same words); the page only lays them out - the resolved Z
    // as the row's dimmed suffix (right-to-left: the echo goes first).
    let echo = height_echo(world, setup, src.as_ref(), h);
    prop_row(ui, offset_label(h.reference), |ui| {
        ui.add(Label::new(RichText::new(echo).weak()).selectable(false));
        num(ui, u, &mut h.offset);
    });
}
