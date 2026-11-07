//! The post's user options (issue #34): today the Fanuc Lathe's thread
//! output - "Threading: G76 cycle / G32 passes" - shown under the post
//! picker while that post is the document's. A pick queues the
//! undoable `SetPostOptions`; the widget only reads the singleton.
//! Provenance: our own panel; the choice's meaning is the post's
//! (assets/posts/fanuc-lathe.cps).

use camcraft_app::posting;
use camcraft_app::schema::{PostOptions, PostRef, ThreadingOutput};
use egui::Ui;

use crate::state::{CommandKind, State};

/// The threading row, when the document posts through Fanuc Lathe.
pub(super) fn threading_picker(ui: &mut Ui, state: &mut State) {
    if posting::post_of(&state.world) != PostRef::Named("Fanuc Lathe".into()) {
        return;
    }
    let now = posting::post_options(&state.world);
    let label = |t: ThreadingOutput| match t {
        ThreadingOutput::G32Passes => "G32 passes",
        ThreadingOutput::G76Cycle => "G76 cycle",
    };
    let mut pick: Option<ThreadingOutput> = None;
    ui.horizontal(|ui| {
        ui.label("Threading");
        egui::ComboBox::from_id_salt("post_threading")
            .selected_text(label(now.threading))
            .show_ui(ui, |ui| {
                for t in [ThreadingOutput::G76Cycle, ThreadingOutput::G32Passes] {
                    if ui.selectable_label(now.threading == t, label(t)).clicked() && now.threading != t {
                        pick = Some(t);
                    }
                }
            })
            .response
            .on_hover_text("How a thread posts: one G76 multi-pass cycle the control runs, or every pass as G32 moves");
    });
    if let Some(t) = pick {
        state.pending.push_back(CommandKind::SetPostOptions(PostOptions { threading: t }));
    }
}
