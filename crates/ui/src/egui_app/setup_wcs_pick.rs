//! The Setups page's work-offset picker (#196): which `G54`..`G59` word
//! a setup's program runs in. "Auto" is the per-operation law (the first
//! offset an operation's heights reference, else G54) - the default, and
//! what every older document reads as. A pick queues the undoable
//! `SetSetupWorkOffset`; nothing here writes the world.
//!
//! Provenance: our own UI over RS-274 / ISO 6983 work-offset selection;
//! no vendor material consulted.

use egui::{ComboBox, Ui};

use camcraft_app::schema::WCS_SLOTS;

use crate::state::{CommandKind, State};

/// The combo text for a stored pick.
#[must_use]
pub(super) fn pick_text(slot: Option<usize>) -> &'static str {
    match slot.and_then(|i| WCS_SLOTS.get(i)) {
        Some(w) => w,
        None => "Auto (from heights, else G54)",
    }
}

/// One setup row's picker: label, combo, and the queued command when the
/// user picks a different entry.
pub(super) fn picker(ui: &mut Ui, state: &mut State, setup: &str) {
    let current = camcraft_app::setup::work_offset(&state.world, setup);
    let mut chosen = current;
    ui.horizontal(|ui| {
        ui.label("    Work offset");
        ComboBox::from_id_salt(("setup_wcs", setup))
            .selected_text(pick_text(current))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut chosen, None, pick_text(None));
                for (i, w) in WCS_SLOTS.iter().enumerate() {
                    ui.selectable_value(&mut chosen, Some(i), *w);
                }
            })
            .response
            .on_hover_text(
                "The G-word this setup's program runs in; every operation of the setup \
                 posts at it, measured from that offset's placement here.",
            );
    });
    if chosen != current {
        state.pending.push_back(CommandKind::SetSetupWorkOffset {
            setup: setup.to_owned(),
            slot: chosen,
        });
    }
}
