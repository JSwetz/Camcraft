//! The setup page's "relative to the previous setup" control (#316):
//! place this setup as a re-clamp of the setup before it - the same way
//! up, flipped, turned, or by typed angles - or unlink it. Every change
//! queues the undoable `SetSetupRelative` (through
//! `CommandKind::SetSetupRelative`); nothing here writes the world. The
//! turn is on the part; the work offset is its own row.
//!
//! Provenance: our own UI over plain shop practice (flipping a part for
//! its second operation); no vendor material consulted.

use egui::{ComboBox, DragValue, Ui};

use camcraft_app::setup;
use camcraft_app::setup_align::AlignPreset;
use camcraft_app::setup_relative;

use crate::state::{CommandKind, State};

/// The choice's words: "Same as Setup1" for no turn, else the preset's.
fn choice_label(p: AlignPreset, prev: &str) -> String {
    match p {
        AlignPreset::AsModelled => format!("Same as {prev}"),
        other => format!("{} from {prev}", other.label()),
    }
}

/// One setup's control. Shown only when there is a setup before it.
pub(super) fn picker(ui: &mut Ui, state: &mut State, name: &str) {
    let names = setup::names(&state.world);
    let Some(prev) = names.iter().position(|n| n == name).and_then(|i| i.checked_sub(1)).and_then(|i| names.get(i))
    else {
        return;
    };
    let prev = prev.clone();
    let linked = setup_relative::relative_to(&state.world, name);
    let shown = setup_relative::describe(&state.world, name).unwrap_or_else(|| "Not linked".to_owned());
    let mut queued: Option<CommandKind> = None;
    ui.horizontal(|ui| {
        ui.label("    Relative to previous setup");
        ComboBox::from_id_salt(("setup_relative", name))
            .selected_text(&shown)
            .show_ui(ui, |ui| {
                if ui.selectable_label(linked.is_none(), "Not linked").clicked() && linked.is_some() {
                    queued = Some(CommandKind::SetSetupRelative { setup: name.to_owned(), to: None, angles: [0.0; 3] });
                }
                for p in AlignPreset::ALL {
                    let words = choice_label(p, &prev);
                    if ui.selectable_label(shown == words, &words).clicked() && shown != words {
                        queued = Some(CommandKind::SetSetupRelative {
                            setup: name.to_owned(),
                            to: Some(prev.clone()),
                            angles: p.angles(),
                        });
                    }
                }
            })
            .response
            .on_hover_text(format!(
                "This setup is {prev} re-clamped: the part is turned from how {prev} holds it, about the \
                 centre of its bounds. While linked, changing how {prev} holds the part moves this setup's \
                 part (and its stock) with it. The work offset is not changed."
            ));
    });
    let id = ui.id().with(("setup_relative_draft", name));
    let held = setup_relative::angles(&state.world, name);
    let mut draft: [f64; 3] = ui.data_mut(|d| *d.get_temp_mut_or_insert_with(id, || held));
    ui.horizontal(|ui| {
        ui.label("      Turn from previous");
        for (axis, v) in ["X", "Y", "Z"].into_iter().zip(draft.iter_mut()) {
            ui.label(axis);
            ui.add(DragValue::new(v).speed(1.0).range(-360.0..=360.0).suffix("°"));
        }
        if ui
            .button("Apply")
            .on_hover_text(format!("Place this setup as {prev} turned about X, then Y, then Z."))
            .clicked()
        {
            queued = Some(CommandKind::SetSetupRelative { setup: name.to_owned(), to: Some(prev.clone()), angles: draft });
        }
    });
    if let Some(k) = queued {
        if let CommandKind::SetSetupRelative { angles, .. } = &k {
            draft = *angles;
        }
        state.pending.push_back(k);
    }
    ui.data_mut(|d| d.insert_temp(id, draft));
}
