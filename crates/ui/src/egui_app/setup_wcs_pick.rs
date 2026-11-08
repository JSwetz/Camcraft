//! The Setups page's work-offset picker (#196): which `G54`..`G59` word
//! a setup's program runs in and every work-offset height of its
//! operations measures from (#329 - the Heights page only shows it).
//! An unset setup IS `G54` - one `G54` entry, stored unset, so the file
//! format is unchanged (#329). A pick queues the undoable
//! `SetSetupWorkOffset`; nothing here writes the world.
//!
//! Provenance: our own UI over RS-274 / ISO 6983 work-offset selection;
//! no vendor material consulted.

use egui::{ComboBox, Ui};

use camcraft_app::schema::WCS_SLOTS;

use crate::state::{CommandKind, State};

/// The combo text for a stored pick: the setup's word, plain - unset
/// reads `G54`, exactly like an explicit `G54`.
#[must_use]
pub(super) fn pick_text(slot: Option<usize>) -> &'static str {
    WCS_SLOTS.get(slot.unwrap_or(0)).copied().unwrap_or("G54")
}

/// A stored pick in the combo's terms: `G54` (slot 0) and unset are one
/// entry, so both read `None` and choosing `G54` on either queues nothing.
fn normalized(slot: Option<usize>) -> Option<usize> {
    slot.filter(|i| *i != 0)
}

/// One setup row's picker: label, combo, and the queued command when the
/// user picks a different entry.
pub(super) fn picker(ui: &mut Ui, state: &mut State, setup: &str) {
    let current = normalized(camcraft_app::setup::work_offset(&state.world, setup));
    let mut chosen = current;
    ui.horizontal(|ui| {
        ui.label("    Work offset");
        ComboBox::from_id_salt(("setup_wcs", setup))
            .selected_text(pick_text(current))
            .show_ui(ui, |ui| {
                for (i, w) in WCS_SLOTS.iter().enumerate() {
                    ui.selectable_value(&mut chosen, normalized(Some(i)), *w);
                }
            })
            .response
            .on_hover_text(
                "The G-word this setup's program runs in; every operation of the setup \
                 posts at it and measures its work-offset heights from that offset's \
                 placement here.",
            );
    });
    if chosen != current {
        state.pending.push_back(CommandKind::SetSetupWorkOffset {
            setup: setup.to_owned(),
            slot: chosen,
        });
    }
}
