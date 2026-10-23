//! The machine editor window (M3.3 stage C): "edit a machine" from
//! the Setup pane's machine group.
//!
//! Provenance: the workflow is specced from published CAM user
//! documentation behavior (a machine definition is picked or edited,
//! its name shows in the setup); every field, word and widget here is
//! ours, keyed to the grammar in `hsmworks_app::machine`.
//!
//! The editor's law is the draft law of the tool editor: every field
//! edits a COPY of the mounted machine, and nothing is a command until
//! Apply pushes one - [`CommandKind::EditMachineApply`], which mounts
//! the edited definition through `LoadMachine` at the SAME path, so
//! undo puts the machine's previous numbers back exactly. Every
//! unstated limit keeps its "unstated" face: a checkbox decides
//! whether a number participates, mirroring the checker's law that
//! `None` checks nothing and a zero is not a ceiling.

use egui::{ComboBox, DragValue, TextEdit, Ui, Window};
use hsmworks_app::machine::{self, Coolant, Kind, Machine};
use hsmworks_app::units::Unit;

use super::pages;
use crate::state::{CommandKind, State};

/// The open window's whole state: the draft and where it was mounted
/// from. Ephemeral like `tool_editor` - never saved, never undoable;
/// the Apply it drives is.
pub struct MachineEditor {
    pub path: String,
    pub machine: Machine,
}

const KIND_WORDS: [&str; 4] = ["auto (axis inventory)", "mill", "lathe", "mill-turn"];

pub(crate) fn window(ctx: &egui::Context, state: &mut State) {
    // Take the draft out for the frame: the draw wants `&mut` on it
    // while the buttons want `&mut` on the queue, and egui windows
    // span both. Re-attached at the end unless Apply or ✖ retires it.
    let Some(mut ed) = state.machine_editor.take() else { return };
    let u = state.units();
    let mut apply = false;
    let mut revert = false;
    let mut open = true;
    let title = if ed.machine.model.trim().is_empty() {
        String::from("Machine")
    } else {
        format!("Machine: {}", ed.machine.model)
    };
    Window::new(title)
        .id(egui::Id::new("machine-editor"))
        .open(&mut open)
        .default_size([480.0, 560.0])
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Model");
                ui.add(TextEdit::singleline(&mut ed.machine.model).desired_width(150.0));
                ui.label("Vendor");
                ui.add(TextEdit::singleline(&mut ed.machine.vendor).desired_width(110.0));
            });
            draw_kind(ui, &mut ed.machine);
            draw_spindle(ui, u, &mut ed.machine);
            draw_axes(ui, u, &mut ed.machine);
            draw_tooling(ui, u, &mut ed.machine);
            draw_coolants(ui, &mut ed.machine);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() {
                    apply = true;
                }
                if ui.button("Revert to mounted").clicked() {
                    revert = true;
                }
                ui.label(machine::kind(Some(&ed.machine)).label());
                ui.label(ed.machine.summary());
            });
        });
    if apply {
        state.pending.push_back(CommandKind::EditMachineApply {
            path: ed.path.clone(),
            machine: Box::new(ed.machine.clone()),
        });
        // The draft waits for the command's verdict: Apply's arm
        // closes the window on success and keeps it open on refusal
        // (an invalid edit is the user's to fix, not ours to discard).
        state.machine_editor = Some(ed);
    } else {
        if revert {
            ed.machine = machine::of(&state.world).1.unwrap_or_default();
        }
        if open {
            state.machine_editor = Some(ed);
        }
    }
}

fn draw_kind(ui: &mut Ui, m: &mut Machine) {
    pages::field(ui, "Kind");
    // The `_ => 0` is load-bearing honesty: an unreadable word was
    // refused at load and cannot exist on a mounted machine, but if a
    // draft somehow carries one, the combo says "auto" and the
    // summary below still reports what the axes say - never a lie
    // about a declaration we cannot read.
    let mut idx = match m.kind.as_deref().and_then(Kind::from_word) {
        Some(Kind::Mill) => 1,
        Some(Kind::Lathe) => 2,
        Some(Kind::MillTurn) => 3,
        _ => 0,
    };
    let before = idx;
    ComboBox::from_id_salt("machine_kind")
        .selected_text(KIND_WORDS[idx.min(3)])
        .show_ui(ui, |ui| {
            for (i, w) in KIND_WORDS.iter().enumerate() {
                ui.selectable_value(&mut idx, i, *w);
            }
        });
    if idx != before {
        m.kind = match idx {
            0 => None,
            i => Kind::from_word(KIND_WORDS[i.min(3)])
                .map(|k| k.word().to_owned())
                .or_else(|| Some(String::new())),
        };
    }
    if idx == 0 && m.kind.is_some() {
        m.kind = None;
    }
    ui.label("auto follows the axes below: X/Y/Z is a mill, X/Z is a lathe, Y plus a rotary is a mill-turn");
}

fn draw_spindle(ui: &mut Ui, u: Unit, m: &mut Machine) {
    pages::field(ui, "Spindle (rpm)");
    ui.horizontal(|ui| {
        ui.label("min");
        opt(ui, pages::num_plain, &mut m.spindle_min);
        ui.label("max");
        opt(ui, pages::num_plain, &mut m.spindle_max);
    });
    ui.horizontal(|ui| {
        ui.label("motor power");
        ui.label("kW");
        opt(ui, pages::num_plain, &mut m.spindle_power);
        ui.label("· body");
        opt(ui, |ui, v| pages::num(ui, u, v), &mut m.spindle_d);
        ui.label("mm");
        opt(ui, |ui, v| pages::num(ui, u, v), &mut m.spindle_len);
        ui.label("mm above holder");
    });
}

fn draw_axes(ui: &mut Ui, u: Unit, m: &mut Machine) {
    pages::field(ui, "Axes");
    let mut remove: Option<usize> = None;
    for i in 0..m.axes.len() {
        let a = &mut m.axes[i];
        ui.horizontal(|ui| {
            ui.add(TextEdit::singleline(&mut a.name).desired_width(16.0).char_limit(2));
            if a.rotary {
                ui.label("rotary");
            }
            ui.label("rapid");
            opt(ui, |ui, v| pages::num_feed(ui, u, v), &mut a.rapid);
            ui.label("feed");
            opt(ui, |ui, v| pages::num_feed(ui, u, v), &mut a.feed);
            ui.label("travel");
            let mut stated = a.travel.is_some();
            if ui.checkbox(&mut stated, "").changed() {
                a.travel = if stated { Some((0.0, 1.0)) } else { None };
            }
            if let Some((lo, hi)) = &mut a.travel {
                pages::num(ui, u, lo);
                ui.label("..");
                pages::num(ui, u, hi);
            }
            ui.label("res");
            opt(ui, |ui, v| pages::num(ui, u, v), &mut a.resolution);
            if ui.button("✖").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        m.axes.remove(i);
    }
    if ui.button("add axis").clicked() {
        m.axes.push(machine::AxisLimits {
            name: "U".to_owned(),
            ..Default::default()
        });
    }
}

fn draw_tooling(ui: &mut Ui, u: Unit, m: &mut Machine) {
    pages::field(ui, "Tooling");
    ui.horizontal(|ui| {
        ui.label("max tool length");
        opt(ui, |ui, v| pages::num(ui, u, v), &mut m.max_tool_length);
        ui.label("· max diameter");
        opt(ui, |ui, v| pages::num(ui, u, v), &mut m.max_tool_diameter);
    });
    ui.horizontal(|ui| {
        ui.label("changer");
        let mut stated = m.tool_count.is_some();
        if ui.checkbox(&mut stated, "").changed() {
            m.tool_count = if stated { Some(1) } else { None };
        }
        if let Some(n) = &mut m.tool_count {
            ui.add(DragValue::new(n).suffix(" slots"));
        } else {
            ui.label("unstated");
        }
    });
}

fn draw_coolants(ui: &mut Ui, m: &mut Machine) {
    pages::field(ui, "Coolants");
    let mut remove: Option<usize> = None;
    for (i, c) in m.coolants.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.add(TextEdit::singleline(&mut c.name).desired_width(90.0));
            ui.label("M-code");
            ui.add(TextEdit::singleline(&mut c.m_code).desired_width(50.0));
            if c.m_code.is_empty() {
                ui.label("(named only - posts nothing)");
            }
            if ui.button("✖").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        m.coolants.remove(i);
    }
    if ui
        .button("add coolant")
        .clicked()
    {
        m.coolants.push(Coolant {
            name: "Flood".to_owned(),
            m_code: String::new(),
        });
    }
}

/// The stated/unstated widget: the law that `None` checks nothing,
/// wearing a checkbox. Toggling on seeds a neutral 1.0 the user then
/// edits - never a 0 that reads back as unstated.
fn opt(ui: &mut Ui, show: impl FnOnce(&mut Ui, &mut f64), v: &mut Option<f64>) {
    let mut stated = v.is_some();
    if ui.checkbox(&mut stated, "").changed() {
        *v = if stated { Some(1.0) } else { None };
    }
    match v {
        Some(x) => show(ui, x),
        None => {
            ui.label("unstated");
        }
    }
}
