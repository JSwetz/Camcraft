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
//! ephemeral like the dock (never a command, never saved). The one
//! document change the pane makes is "Use for this setup" (#384): it
//! only queues `CommandKind::UseCatalogueMachine`, which lands as the
//! undoable `LoadMachine`; a refused or already-mounted machine leaves
//! the button disabled with the reason beside it.

use egui::{Label, RichText, ScrollArea, Ui};

use camcraft_app::machine::library::{self, MachineEntry};

use super::colorvision::text_inks;
use super::layout::{group_open, prop_grid};
use super::theme::Tokens;
use crate::state::{CommandKind, State};

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
}

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
    if p.list.is_none() || p.listed_for != path {
        let list = library::enumerate(&state.world);
        if p.selected.is_some_and(|i| i >= list.len()) {
            p.selected = None;
        }
        p.list = Some(list);
        p.listed_for = path;
    }
}

/// The pane's assign button label (a test finds it by these words).
pub(crate) const USE_FOR_SETUP: &str = "Use for this setup";

pub(crate) fn draw(ui: &mut Ui, state: &mut State, pal: &Tokens) {
    refresh(state);
    let inks = text_inks(ui, &state.world);
    // Why the selected machine can't be assigned (read before the pane
    // borrow below takes `state`).
    let block = state
        .machines_pane
        .selected
        .and_then(|i| state.machines_pane.list.as_ref().and_then(|l| l.get(i)))
        .map(|e| e.assign_block(&state.world));
    let mut assign = None;
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
        let why = block.clone().flatten();
        let r = ui.add_enabled(why.is_none(), egui::Button::new(USE_FOR_SETUP));
        match &why {
            None => {
                if r.on_hover_text("Make this the setup's machine (Ctrl+Z undoes it)").clicked() {
                    assign = Some(e.source.clone());
                }
            }
            Some(why) => {
                r.on_disabled_hover_text(format!("Cannot assign: {why}"));
                ui.add(Label::new(RichText::new(format!("Cannot assign: {why}")).color(pal.text_dim)).wrap());
            }
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
    if let Some(source) = assign {
        state.pending.push_back(CommandKind::UseCatalogueMachine(source));
    }
}
