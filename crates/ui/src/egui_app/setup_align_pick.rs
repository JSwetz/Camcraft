//! The Setups page's part-alignment control (#326): how the part sits on
//! the table in one setup - flipped, turned, or as modelled - by a quick
//! choice or by typed angles. Every change queues the undoable
//! `AlignPartToSetup` (through `CommandKind::AlignPart`); nothing here
//! writes the world. The work offset is a separate row (its own picker).
//!
//! Provenance: our own UI over plain shop practice (re-clamping a part
//! face-down or turned); no vendor material consulted.

use egui::{ComboBox, DragValue, Ui};

use camcraft_app::geometry::{quat_mul, quat_to_euler};
use camcraft_app::schema::Location;
use camcraft_app::setup_align::{self, AlignPreset};

use crate::state::{CommandKind, State};

/// The preset a stored rotation is, if any (a quaternion and its negation
/// are one rotation).
fn preset_of(rot: [f64; 4]) -> Option<AlignPreset> {
    AlignPreset::ALL.into_iter().find(|p| {
        setup_align::rotation_from_angles(p.angles()).is_some_and(|q| {
            // q^-1 * rot is the identity when they are the same turn.
            let d = quat_mul([-q[0], -q[1], -q[2], q[3]], rot);
            setup_align::is_identity_rot(d)
        })
    })
}

/// The stored alignment as (about X, about Y, about Z) degrees, rounded
/// to a millionth so a quarter turn reads 90, not 89.99999999.
#[must_use]
pub(crate) fn angles_of(a: Option<Location>) -> [f64; 3] {
    let Some(a) = a else { return [0.0; 3] };
    let (r, p, y) = quat_to_euler(a.rot);
    [r, p, y].map(|v| {
        let d = (v.to_degrees() * 1e6).round() / 1e6;
        if d == 0.0 { 0.0 } else { d } // no "-0"
    })
}

/// The plain words for a setup's alignment: a preset's name, or the
/// three angles.
#[must_use]
pub(crate) fn align_text(a: Option<Location>) -> String {
    let Some(loc) = a else { return AlignPreset::AsModelled.label().to_owned() };
    if let Some(p) = preset_of(loc.rot) {
        return p.label().to_owned();
    }
    let [x, y, z] = angles_of(a);
    format!("turned X {x}°, Y {y}°, Z {z}°")
}

/// One setup row's control: the quick-choice combo, then the typed angles
/// with an Apply button.
pub(super) fn picker(ui: &mut Ui, state: &mut State, setup: &str) {
    let current = setup_align::stored(&state.world, setup);
    let mut queued: Option<[f64; 3]> = None;
    ui.horizontal(|ui| {
        ui.label("    Part alignment");
        ComboBox::from_id_salt(("setup_align", setup))
            .selected_text(align_text(current))
            .show_ui(ui, |ui| {
                for p in AlignPreset::ALL {
                    let held = align_text(current) == p.label();
                    if ui.selectable_label(held, p.label()).clicked() && !held {
                        queued = Some(p.angles());
                    }
                }
            })
            .response
            .on_hover_text(
                "How the part sits on the table in this setup. The stock follows and is \
                 rebuilt square to the table; the work offset is set separately below.",
            );
    });
    let id = ui.id().with(("setup_align_draft", setup));
    let mut draft: [f64; 3] = ui.data_mut(|d| *d.get_temp_mut_or_insert_with(id, || angles_of(current)));
    ui.horizontal(|ui| {
        ui.label("      Turn");
        for (axis, v) in ["X", "Y", "Z"].into_iter().zip(draft.iter_mut()) {
            ui.label(axis);
            ui.add(DragValue::new(v).speed(1.0).range(-360.0..=360.0).suffix("°"));
        }
        if ui
            .button("Apply")
            .on_hover_text("Turn the part about X, then Y, then Z, about the centre of its bounds.")
            .clicked()
        {
            queued = Some(draft);
        }
    });
    if let Some(angles) = queued {
        draft = angles;
        state.pending.push_back(CommandKind::AlignPart { setup: setup.to_owned(), angles });
    }
    ui.data_mut(|d| d.insert_temp(id, draft));
}
