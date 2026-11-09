//! The Machines pane's file editor through the real shell (issue #383):
//! a machine made from a template has its travels and coolant and
//! live-tool codes typed in and is saved to the user machines folder;
//! a refusal sits on its field and blocks the save; a shipped machine's
//! Save becomes Save As; a hand-written file keeps its unknown keys.
//! None of it touches the command queue or the history - editing a
//! machine FILE is a save with its own confirmation, not a document edit.
//! Provenance: our own tests, specced from the issue's "Done means" and
//! the egui_kittest vendored sources; no HSMWorks material involved.
//! Structural only: no golden PNG.
//!
//! Gate map (M4.4): covers ui::egui_app::machines_pane, ui::egui_app::machine_file_editor, app::machine::draft, app::machine::fields, app::machine::store.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;

use super::machines_pane::{assert_no_commands, harness_with_user_dir, open_machines};
use super::*;
use crate::egui_app::machine_file_editor::Prompt;

/// A fresh, empty user machines folder for one test.
fn user_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft_ui_machines_edit_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the test folder");
    d
}

fn select_row(h: &mut Harness<'static, ShellUi>, name: &str) {
    let label = h
        .state()
        .state
        .machines_pane
        .list
        .as_ref()
        .and_then(|l| l.iter().find(|e| e.name == name))
        .map(super::super::machines_pane::row_label)
        .unwrap_or_else(|| panic!("the catalogue lists {name}"));
    h.get_by_label(&label).scroll_to_me();
    h.run_steps(10);
    h.get_by_label(&label).click();
    h.run_steps(2);
}

fn press(h: &mut Harness<'static, ShellUi>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(4);
    h.get_by_label(label).click();
    h.run_steps(3);
}

/// Replace a grid field's text the way a user does: click it, select
/// all, type.
fn type_into(h: &mut Harness<'static, ShellUi>, field: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, field).scroll_to_me();
    h.run_steps(6);
    h.get_by_role_and_label(Role::TextInput, field).click();
    h.run();
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::COMMAND));
    h.input_mut().events.push(Event::Key {
        key: Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    h.run();
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::NONE));
    h.get_by_role_and_label(Role::TextInput, field).type_text(text);
    h.run_steps(3);
}

fn draft_at(h: &Harness<'static, ShellUi>, pointer: &str) -> Option<serde_json::Value> {
    h.state().state.machines_pane.editor.as_ref().and_then(|e| e.draft.value.pointer(pointer).cloned())
}

fn prompt(h: &Harness<'static, ShellUi>) -> Option<Prompt> {
    h.state().state.machines_pane.editor.as_ref().and_then(|e| e.prompt.clone())
}

fn read_json(p: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(p).expect("read back")).expect("json")
}

#[test]
fn a_template_machine_takes_new_travels_and_codes_and_saves_to_the_user_folder() {
    let dir = user_dir("template");
    let mut h = harness_with_user_dir(Some(dir.clone()));
    h.run();
    open_machines(&mut h);
    select_row(&mut h, "Lathe");
    press(&mut h, "New from template");
    let model = h.state().state.machines_pane.editor.as_ref().map(|e| e.draft.model());
    assert_eq!(model.as_deref(), Some("TURNING CENTER 2-AXIS copy"));
    // Travels: the lathe's X turret is the first part under the bed.
    type_into(&mut h, "X travel max", "350");
    let x = "/kinematics/default/parts/0/parts/0";
    assert_eq!(draft_at(&h, &format!("{x}/travel/max")).and_then(|v| v.as_f64()), Some(350.0));
    type_into(&mut h, "Z travel min", "-700");
    assert_eq!(draft_at(&h, &format!("{x}/parts/0/travel/min")).and_then(|v| v.as_f64()), Some(-700.0));
    // Coolant code.
    type_into(&mut h, "Coolant Flood code", "M7");
    // Live-tool codes: one alone is refused on its field and blocks the save.
    type_into(&mut h, "Live tool C on", "M154");
    h.get_by_label("Validation: 1 refusal - save blocked");
    assert!(h.query_all_by_label_contains("live_tool: states 1 of the 4 words").count() >= 1);
    press(&mut h, "Save machine");
    assert_eq!(prompt(&h), None, "a blocked save asks nothing");
    for (f, c) in [("Live tool C off", "M155"), ("Live tool spindle on", "M133"), ("Live tool spindle off", "M135")] {
        type_into(&mut h, f, c);
    }
    h.get_by_label("Validation: passes");
    // Save on a never-saved copy is Save As, named from the model.
    press(&mut h, "Save machine");
    assert_eq!(prompt(&h), Some(Prompt::SaveAs));
    press(&mut h, "Write file");
    let path = dir.join("turning-center-2-axis-copy.mch");
    assert!(path.is_file(), "the machine was written to the user folder");
    let v = read_json(&path);
    assert_eq!(v.pointer(&format!("{x}/travel/max")).and_then(|v| v.as_f64()), Some(350.0));
    assert_eq!(v.pointer(&format!("{x}/parts/0/travel/min")).and_then(|v| v.as_f64()), Some(-700.0));
    let ts = "/kinematics/default/parts/0/parts/1/tool_station";
    assert_eq!(v.pointer(&format!("{ts}/coolants/0/mCode")).and_then(|v| v.as_str()), Some("M7"));
    assert_eq!(v.pointer(&format!("{ts}/live_tool/spindle_off")).and_then(|v| v.as_str()), Some("M135"));
    // The catalogue re-read lists it, selected; the editor says so.
    let st = &h.state().state.machines_pane;
    let sel = st.selected.and_then(|i| st.list.as_ref().and_then(|l| l.get(i)));
    assert_eq!(sel.map(|e| e.name.as_str()), Some("TURNING CENTER 2-AXIS copy"));
    assert!(sel.is_some_and(|e| e.is_valid()));
    h.get_by_label(&format!("Saved {}", path.display()));
    assert_no_commands(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_backwards_travel_is_refused_on_its_field_and_nothing_is_written() {
    let dir = user_dir("refused");
    let mut h = harness_with_user_dir(Some(dir.clone()));
    h.run();
    open_machines(&mut h);
    press(&mut h, "New blank");
    type_into(&mut h, "X travel min", "100");
    type_into(&mut h, "X travel max", "50");
    h.get_by_label("Validation: 1 refusal - save blocked");
    // Shown under both ends of the stroke.
    assert_eq!(h.query_all_by_label("X travel: min 100 must be below max 50").count(), 2);
    press(&mut h, "Save machine");
    press(&mut h, "Save machine as...");
    assert_eq!(prompt(&h), None, "both saves are blocked");
    assert_eq!(std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0), 0, "nothing written");
    // Fixing the field unblocks it.
    type_into(&mut h, "X travel max", "150");
    h.get_by_label("Validation: passes");
    assert_no_commands(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

/// GUI-test finding on PR #478: a bare `M` in an M-code field passed
/// validation. It is refused under its field and blocks both saves.
#[test]
fn a_bare_m_code_is_refused_on_its_field_and_blocks_the_save() {
    let dir = user_dir("bare_m");
    let mut h = harness_with_user_dir(Some(dir.clone()));
    h.run();
    open_machines(&mut h);
    select_row(&mut h, "Lathe");
    press(&mut h, "New from template");
    for (f, c) in [("Live tool C on", "M154"), ("Live tool C off", "M155"), ("Live tool spindle on", "M133"), ("Live tool spindle off", "M135")] {
        type_into(&mut h, f, c);
    }
    h.get_by_label("Validation: passes");
    type_into(&mut h, "Live tool C on", "M");
    h.get_by_label("Validation: 1 refusal - save blocked");
    h.get_by_label(r#"live_tool.c_axis_on: code "M" is not an M word like M8 (no number after M)"#);
    type_into(&mut h, "Coolant Flood code", "M");
    h.get_by_label("Validation: 2 refusals - save blocked");
    h.get_by_label(r#"coolant Flood: code "M" is not an M word like M8 (no number after M)"#);
    press(&mut h, "Save machine");
    press(&mut h, "Save machine as...");
    assert_eq!(prompt(&h), None, "both saves are blocked");
    assert_eq!(std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0), 0, "nothing written");
    type_into(&mut h, "Live tool C on", "M08");
    type_into(&mut h, "Coolant Flood code", "M7");
    h.get_by_label("Validation: passes");
    assert_no_commands(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn editing_a_shipped_machine_saves_as_a_copy() {
    let dir = user_dir("shipped");
    let mut h = harness_with_user_dir(Some(dir.clone()));
    h.run();
    open_machines(&mut h);
    select_row(&mut h, "VMC 3-axis");
    press(&mut h, "Edit machine");
    assert!(h.state().state.machines_pane.editor.as_ref().is_some_and(|e| e.draft.read_only));
    h.get_by_label("Shipped machine - read-only; Save As writes your own copy");
    press(&mut h, "Save machine");
    assert_eq!(prompt(&h), Some(Prompt::SaveAs), "Save on a shipped machine is Save As");
    assert_no_commands(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_hand_written_user_file_keeps_its_unknown_keys_through_an_edit_and_save() {
    let dir = user_dir("unknown");
    let path = dir.join("shop.mch");
    let text = r#"{
      "general": { "model": "SHOP MILL", "kind": "mill" },
      "kinematics": { "default": { "units": { "length": "mm" }, "parts": [
        { "name": "X", "type": "linear", "travel": { "min": -100.0, "max": 100.0 } },
        { "name": "Y", "type": "linear" }, { "name": "Z", "type": "linear" }
      ] } },
      "x_shop_notes": { "probe": "OMP40" }
    }"#;
    std::fs::write(&path, text).expect("write");
    let mut h = harness_with_user_dir(Some(dir.clone()));
    h.run();
    open_machines(&mut h);
    select_row(&mut h, "SHOP MILL");
    press(&mut h, "Edit machine");
    type_into(&mut h, "X travel max", "120");
    press(&mut h, "Save machine");
    assert_eq!(prompt(&h), Some(Prompt::Save(path.clone())), "Save over its own file asks first");
    assert_eq!(read_json(&path).pointer("/kinematics/default/parts/0/travel/max").and_then(|v| v.as_f64()), Some(100.0));
    press(&mut h, "Write file");
    let v = read_json(&path);
    assert_eq!(v.pointer("/kinematics/default/parts/0/travel/max").and_then(|v| v.as_f64()), Some(120.0));
    assert_eq!(v.pointer("/x_shop_notes/probe").and_then(|v| v.as_str()), Some("OMP40"), "unknown key kept");
    assert_no_commands(&h);
    let _ = std::fs::remove_dir_all(&dir);
}
