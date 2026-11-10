//! The tool editor's lathe orientation row (#630): the LinuxCNC 1-9
//! code that says which point of a turning insert's or boring bar's
//! nose the operator touches off - the point the lathe posts write.
//! The draft field only; Apply carries it through `ChangeTool` like
//! every other tool number, so it is undoable.
//!
//! Provenance: LinuxCNC Lathe User Information (tool orientation
//! chart, public docs); our own widget.

use egui::Ui;

use camcraft_app::orientation;

use crate::state::State;

use super::fields::field;

/// The row, shown for the kinds that ride a nose centre.
pub(super) fn orientation_row(ui: &mut Ui, state: &mut State) {
    let kind = state.tool_ui.tool_kind();
    let Some(default) = orientation::default_for(kind) else { return };
    field(ui, "Lathe orientation");
    let shown = match state.tool_ui.orientation {
        Some(q) => orientation::label(q).to_owned(),
        None => format!("default: {}", orientation::label(default)),
    };
    egui::ComboBox::from_id_salt("tool_lathe_orientation")
        .selected_text(shown)
        .width(200.0)
        .show_ui(ui, |ui| {
            let none = state.tool_ui.orientation.is_none();
            if ui.selectable_label(none, format!("default: {}", orientation::label(default))).clicked() {
                state.tool_ui.orientation = None;
            }
            for q in 1..=9u8 {
                let on = state.tool_ui.orientation == Some(q);
                if ui.selectable_label(on, orientation::label(q)).clicked() {
                    state.tool_ui.orientation = Some(q);
                }
            }
        });
    ui.label("where the touched-off tip sits on the nose (LinuxCNC tool table chart) - the lathe posts write that point, not the nose centre");
}

#[cfg(test)]
mod tests {
    use camcraft_app::tools::catalog;

    use crate::state::ToolUi;

    #[test]
    fn the_editor_draft_carries_the_orientation_through_apply() {
        let insert = catalog().into_iter().find(|(n, _)| *n == "TN-1600-0R8").map(|(_, t)| t);
        let Some(mut t) = insert else { panic!("catalog insert") };
        assert_eq!(ToolUi::from_tool("TN", &t, true).to_tool().orientation, None, "unstated stays unstated");
        t.orientation = Some(6);
        let mut draft = ToolUi::from_tool("TN", &t, true);
        assert_eq!(draft.to_tool().orientation, Some(6), "a stated code survives an Apply");
        draft.orientation = Some(1);
        assert_eq!(draft.to_tool().orientation, Some(1), "the row's pick is what Apply sends");
    }
}
