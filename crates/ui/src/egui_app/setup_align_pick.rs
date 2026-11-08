//! The Setups page's part-alignment control (#326): how the part sits on
//! the table in one setup - flipped, turned, or as modelled - by a quick
//! choice or by typed angles. Every change queues the undoable
//! `AlignPartToSetup` (through `CommandKind::AlignPart`); nothing here
//! writes the world. The work offset is a separate row (its own picker).
//!
//! Provenance: our own UI over plain shop practice (re-clamping a part
//! face-down or turned); no vendor material consulted.

use egui::{ComboBox, DragValue, Ui};

use camcraft_app::setup_align::{self, AlignPreset};

use crate::state::{CommandKind, State};

/// The words and angles for a stored alignment live in the app (the
/// setup sheet prints the same words); the page reads them from there.
pub(crate) use camcraft_app::setup_align::{angles_of, describe as align_text};

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
