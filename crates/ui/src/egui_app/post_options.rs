//! The post's user options (issue #34): the Fanuc Lathe's thread
//! output - "Threading: G76 cycle / G32 passes" - and the Tormach
//! PathPilot's end units (#596), each shown under the post picker while
//! its post is the document's. A pick queues the
//! undoable `SetPostOptions`; the widget only reads the singleton.
//! Provenance: our own panel; the choice's meaning is the post's
//! (assets/posts/fanuc-lathe.cps).

use camcraft_app::posting;
use camcraft_app::schema::{EndUnits, PostOptions, PostRef, ThreadingOutput};
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

/// The end-units row (#596), when the document posts through Tormach
/// PathPilot: the unit the control is left in after M30 (LinuxCNC's
/// program end does not reset G20 / G21). "Leave" is the default and
/// posts the program unchanged.
pub(super) fn end_units_picker(ui: &mut Ui, state: &mut State) {
    if posting::post_of(&state.world) != PostRef::Named("Tormach PathPilot".into()) {
        return;
    }
    let now = posting::post_options(&state.world);
    let label = |u: EndUnits| match u {
        EndUnits::Leave => "Program's unit",
        EndUnits::Inch => "Inch (G20)",
        EndUnits::Millimeters => "mm (G21)",
    };
    let mut pick: Option<EndUnits> = None;
    ui.horizontal(|ui| {
        ui.label("End in");
        egui::ComboBox::from_id_salt("post_end_units")
            .selected_text(label(now.end_units))
            .show_ui(ui, |ui| {
                for u in [EndUnits::Leave, EndUnits::Inch, EndUnits::Millimeters] {
                    if ui.selectable_label(now.end_units == u, label(u)).clicked() && now.end_units != u {
                        pick = Some(u);
                    }
                }
            })
            .response
            .on_hover_text("the unit the control is left in after the program: M30 does not reset G20/G21, so a mm program on an inch-run control leaves the DRO and MDI in mm unless the unit word is written before M30");
    });
    if let Some(u) = pick {
        state.pending.push_back(CommandKind::SetPostOptions(PostOptions { end_units: u, ..now }));
    }
}
