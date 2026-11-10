//! A setup's own post options and tool-list export rows (#389), drawn on
//! the setup's Post and Tool list pages. The options follow the
//! document's until the setup takes its own (`SetupOwnPostOptions`, one
//! undoable command); the export queues the dialog arm and writes a file,
//! nothing enters the history (the setup-sheet law).
//! Provenance: our own page law (issue #389); no vendor material consulted.

use egui::{RichText, Ui};

use camcraft_app::setup_post_options;

use crate::state::{CommandKind, State};

/// The own / follow-the-document choice of a setup's post options.
pub(super) fn options_ownership(ui: &mut Ui, state: &mut State, ctx: Option<&str>) {
    let Some(name) = ctx else {
        ui.label(RichText::new("A document without setups posts with the document's post options.").weak());
        return;
    };
    ui.label(RichText::new("Post options").strong());
    let owns = setup_post_options::own(&state.world, Some(name)).is_some();
    let mut pick = owns;
    ui.radio_value(&mut pick, false, "Use the document's post options");
    ui.radio_value(&mut pick, true, "This setup has its own post options");
    if pick != owns {
        state.pending.push_back(CommandKind::SetupOwnPostOptions { setup: name.to_owned(), own: pick });
    }
    let note = if owns {
        "Skip pauses and homes, Output and Threading above are this setup's own."
    } else {
        "Skip pauses and homes, Output and Threading above are the document's, shared by every setup that follows it."
    };
    ui.label(RichText::new(note).weak());
}

/// The setup's own tool-list export row.
pub(super) fn tool_list_export(ui: &mut Ui, state: &mut State, ctx: Option<&str>) {
    ui.horizontal(|ui| {
        if ui.button("Export tool list…").clicked() {
            state.pending.push_back(CommandKind::ToolListExportDialog(ctx.map(str::to_owned)));
        }
        ui.label(RichText::new("(this setup's tools: CSV or text)").weak());
    });
}
