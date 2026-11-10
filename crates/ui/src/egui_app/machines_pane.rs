//! The Machines pane (issue #287, first slice): a dockable, read-only
//! browser of every machine definition the app can name - the shipped
//! library and the document's own `.mch` file - with each machine's
//! grouped properties and its validation, refusal by refusal.
//!
//! Provenance: the workflow is specced from the issue (list, inspect,
//! validate machine configurations); every word and widget here is
//! ours. The catalogue, the sheet and the rules live in the app layer
//! (`camcraft_app::machine::library` / `validate`): this pane only draws
//! what they answer - it reads no file itself.
//!
//! View state only: which row is selected and the cached catalogue are
//! ephemeral like the dock (never a command, never saved). #383 adds
//! the file editor (`machine_file_editor`): New blank, New from
//! template and Edit open a machine FILE for editing, saved with its
//! own confirmation - still no command, no history, and no setup's
//! machine changes (assigning one is `LoadMachine`'s job).

use std::path::PathBuf;

use egui::{Label, RichText, ScrollArea, Ui};

use camcraft_app::machine::draft::MachineDraft;
use camcraft_app::machine::library::{self, MachineEntry, MachineSource};
use camcraft_app::machine::{store, Kind};

use super::machine_file_editor::{self as file_editor, MachineFileEditor};

use super::colorvision::text_inks;
use super::layout::{group_open, prop_grid};
use super::theme::Tokens;
use crate::state::State;

/// The pane's view state.
#[derive(Default)]
pub struct MachinesPane {
    /// The catalogue as last read; `None` = read it on the next draw.
    pub list: Option<Vec<MachineEntry>>,
    /// The document's machine path the catalogue was read for - a new
    /// mount re-reads it (the document's file is one of the entries).
    pub listed_for: String,
    /// The selected row, an index into `list`.
    pub selected: Option<usize>,
    /// The user machines folder: `None` = not resolved yet (resolved
    /// once, `store::user_dir`); `Some(None)` = there is none. Tests
    /// point it at a temp folder.
    pub user_dir: Option<Option<PathBuf>>,
    /// The machine file open in the editor, if any.
    pub editor: Option<MachineFileEditor>,
    /// The kind New blank starts.
    pub new_kind: usize,
}

/// The kinds New blank offers, in the combo's order.
const KINDS: [Kind; 3] = [Kind::Mill, Kind::Lathe, Kind::MillTurn];

/// A list row's words: name, then kind, axes and units - unique per
/// entry in practice, and what a test finds the row by.
pub(crate) fn row_label(e: &MachineEntry) -> String {
    let kind = e.kind.map_or("?", |k| k.word());
    let mut axes: String = e.axes.concat();
    axes.push_str(&e.rotaries.concat());
    if axes.is_empty() {
        axes.push('?');
    }
    let mark = if e.is_valid() { "" } else { "  [refused]" };
    format!("{}  ({kind}, {axes}, {}){mark}", e.name, e.units)
}

/// The validation heading for an entry.
pub(crate) fn verdict(e: &MachineEntry) -> String {
    match e.refusals.len() {
        0 => "Validation: passes".to_owned(),
        1 => "Validation: 1 refusal".to_owned(),
        n => format!("Validation: {n} refusals"),
    }
}

/// Make sure the catalogue is current: read once, and again when the
/// document mounts a different machine.
fn refresh(state: &mut State) {
    let (path, _) = camcraft_app::machine::of(&state.world);
    let p = &mut state.machines_pane;
    let dir = p.user_dir.get_or_insert_with(store::user_dir).clone();
    if p.list.is_none() || p.listed_for != path {
        let list = library::enumerate_with(&state.world, dir.as_deref());
        if p.selected.is_some_and(|i| i >= list.len()) {
            p.selected = None;
        }
        p.list = Some(list);
        p.listed_for = path;
    }
}

pub(crate) fn draw(ui: &mut Ui, state: &mut State, pal: &Tokens) {
    refresh(state);
    let inks = text_inks(ui, &state.world);
    let p = &mut state.machines_pane;
    let Some(list) = p.list.as_ref() else { return };
    let mut reread = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{} machines", list.len())).color(pal.text_dim));
        if ui
            .button("Refresh")
            .on_hover_text("Read the machine files again")
            .clicked()
        {
            reread = true;
        }
    });
    let picked = p.selected.and_then(|i| list.get(i)).map(|e| e.source.clone());
    let mut open: Option<Result<MachineDraft, String>> = None;
    if p.editor.is_none() {
        ui.horizontal_wrapped(|ui| {
            let kind = KINDS.get(p.new_kind).copied().unwrap_or(Kind::Mill);
            egui::ComboBox::from_id_salt("machines-new-kind")
                .selected_text(kind.word())
                .show_ui(ui, |ui| {
                    for (i, k) in KINDS.iter().enumerate() {
                        ui.selectable_value(&mut p.new_kind, i, k.word());
                    }
                });
            if ui.button("New blank").on_hover_text("A new machine file with the keys its kind requires").clicked() {
                open = Some(Ok(MachineDraft::blank(kind)));
            }
            let has = picked.is_some();
            if ui
                .add_enabled(has, egui::Button::new("New from template"))
                .on_hover_text("A new machine file copied from the selected machine")
                .on_disabled_hover_text("Select a machine to copy")
                .clicked()
                && let Some(src) = &picked
            {
                open = Some(MachineDraft::from_template(src).map_err(|r| r.to_string()));
            }
            if ui
                .add_enabled(has, egui::Button::new("Edit machine"))
                .on_hover_text("Edit the selected machine file (a shipped one saves as a copy)")
                .on_disabled_hover_text("Select a machine to edit")
                .clicked()
                && let Some(src) = &picked
            {
                open = Some(MachineDraft::open(src).map_err(|r| r.to_string()));
            }
        });
    }
    match open {
        Some(Ok(d)) => p.editor = Some(MachineFileEditor::new(d)),
        Some(Err(e)) => {
            ui.label(RichText::new(format!("Cannot open: {e}")).color(inks.error));
        }
        None => {}
    }
    ui.separator();
    let mut chose = None;
    ScrollArea::vertical()
        .id_salt("machines-list")
        .max_height((ui.available_height() * 0.4).max(80.0))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for (i, e) in list.iter().enumerate() {
                let r = ui
                    .selectable_label(p.selected == Some(i), row_label(e))
                    .on_hover_text(e.source.label());
                if r.clicked() {
                    chose = Some(i);
                }
            }
        });
    if let Some(i) = chose {
        p.selected = Some(i);
    }
    ui.separator();
    if let Some(ed) = p.editor.as_mut() {
        let dir = p.user_dir.clone().flatten();
        let out = ScrollArea::vertical()
            .id_salt("machines-editor")
            .show(ui, |ui| file_editor::draw(ui, ed, dir.as_deref(), pal, inks.error))
            .inner;
        if out.close {
            p.editor = None;
        }
        if let Some(saved) = out.saved {
            // Re-read the catalogue and select the file just written.
            let world_list = library::enumerate_with(&state.world, dir.as_deref());
            let p = &mut state.machines_pane;
            p.selected = world_list
                .iter()
                .position(|e| matches!(&e.source, MachineSource::File(f) if *f == saved));
            p.list = Some(world_list);
        } else if reread {
            p.list = None;
        }
        return;
    }
    let Some(e) = p.selected.and_then(|i| list.get(i)) else {
        ui.label(RichText::new("Select a machine to see its properties.").color(pal.text_dim));
        if reread {
            p.list = None;
        }
        return;
    };
    ScrollArea::vertical().id_salt("machines-detail").show(ui, |ui| {
        ui.add(Label::new(RichText::new(&e.name).strong()).wrap());
        ui.add(Label::new(RichText::new(e.source.label()).small().color(pal.text_dim)).wrap());
        let ink = if e.is_valid() { pal.accent } else { inks.error };
        ui.label(RichText::new(verdict(e)).color(ink).strong());
        for r in &e.refusals {
            ui.add(Label::new(RichText::new(r.to_string()).color(inks.error)).wrap());
        }
        ui.add_space(4.0);
        for g in &e.groups {
            group_open(ui, &g.title, None, true, |ui| {
                prop_grid(ui, &format!("machine-{}", g.title), |ui| {
                    for (label, value) in &g.rows {
                        ui.label(label);
                        ui.add(Label::new(value).wrap());
                        ui.end_row();
                    }
                });
            });
        }
    });
    if reread {
        p.list = None;
    }
}
