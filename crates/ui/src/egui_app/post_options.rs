//! The post's user options (issue #34): the Fanuc Lathe's thread
//! output - "Threading: G76 cycle / G32 passes" - shown under the post
//! picker while that post is the document's, and per-operation
//! smoothing (#616) for every mill post. A pick queues the
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
        state.pending.push_back(CommandKind::SetPostOptions(PostOptions { threading: t, ..now }));
    }
}

/// Per-operation smoothing (#616): off by default (no smoothing words);
/// on, each operation states its finish or rough tolerance and the post
/// writes its control's mode. Not offered for the lathe post, which
/// writes none.
pub(super) fn smoothing_switch(ui: &mut Ui, state: &mut State) {
    if posting::post_of(&state.world) == PostRef::Named("Fanuc Lathe".into()) {
        return;
    }
    let now = posting::post_options(&state.world);
    let mut want = now.smoothing;
    ui.checkbox(&mut want, "Smoothing per operation").on_hover_text(
        "write the control's smoothing mode per operation: finishing ops ask their tolerance, roughing ops a looser one from the stock they leave (G64 P, G5.1, G187, CYCLE832, G62) - needs the control's smoothing option on some machines",
    );
    if want != now.smoothing {
        state.pending.push_back(CommandKind::SetPostOptions(PostOptions { smoothing: want, ..now }));
    }
}
