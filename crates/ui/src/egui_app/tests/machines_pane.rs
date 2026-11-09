//! The Machines pane's frame tests (issue #287): the View door opens it
//! beside the properties pane, it lists the shipped library, selecting a
//! machine shows its properties and its validation - a clean pass, or
//! each refusal by name - and none of it touches the command queue (the
//! pane is view state; it edits and assigns nothing).
//! Provenance: our own tests, specced from the issue and the egui_dock /
//! egui_kittest vendored sources; no HSMWorks material involved.
//!
//! Gate map (M4.4): covers ui::egui_app::machines_pane, ui::egui_app::layout, ui::state, app::machine::library, app::machine::validate.

use super::super::layout::{self, ViewRow};
use super::super::machines_pane::{row_label, verdict};
use super::*;
use crate::state::Panel;

fn blank_harness() -> Harness<'static, ShellUi> {
    harness_with_user_dir(None)
}

/// A blank shell whose Machines pane lists `dir` as the user machines
/// folder (`None`: no user folder) - never the tester's real one.
pub(super) fn harness_with_user_dir(dir: Option<std::path::PathBuf>) -> Harness<'static, ShellUi> {
    let mut sh = ShellUi::blank();
    sh.state.machines_pane.user_dir = Some(dir);
    Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, sh)
}

pub(super) fn click(h: &mut Harness<'static, ShellUi>, at: egui::Pos2) {
    set_pos(h, at);
    button_at(h, egui::PointerButton::Primary, at, true);
    button_at(h, egui::PointerButton::Primary, at, false);
    h.run_steps(2);
}

/// Open the Machines pane the way a user does: the ribbon's View door,
/// then its "Machines" row.
pub(super) fn open_machines(h: &mut Harness<'static, ShellUi>) {
    let strip =
        egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280.0, super::super::shell::RIBBON_H));
    let (cells, _) = mill_plan_at(1280.0);
    let button = cells
        .iter()
        .find(|c| c.cmd == Cmd::View)
        .expect("the View door stands on the bar")
        .rect;
    let rows = panes::view_rows_of(button, strip, &theme::current());
    let at = rows
        .iter()
        .find(|(r, _)| *r == ViewRow::Panel(Panel::Machines))
        .expect("the View door lists Machines")
        .1
        .center();
    click(h, button.center());
    click(h, at);
}

pub(super) fn assert_no_commands(h: &Harness<'static, ShellUi>) {
    let st = &h.state().state;
    assert!(st.pending.is_empty(), "the pane queued a command: {:?}", st.pending);
    assert!(st.history.is_empty(), "the pane landed in the history");
}

#[test]
fn the_view_door_opens_the_machines_pane_beside_properties() {
    let mut h = blank_harness();
    h.run();
    assert!(!layout::is_open(&h.state().dock, &Tab::Machines), "closed by default");
    open_machines(&mut h);
    let dock = &h.state().dock;
    let m = dock.find_tab(&Tab::Machines).expect("the pane opened");
    let p = dock.find_tab(&Tab::Properties).expect("properties still open");
    assert_eq!(m.node_path(), p.node_path(), "a tab in the properties pane's leaf");
    assert!(h.state().state.panels_open.machines);
    // The pane read the catalogue: the whole shipped library, in order.
    let names: Vec<String> = h
        .state()
        .state
        .machines_pane
        .list
        .as_ref()
        .expect("the catalogue was read")
        .iter()
        .map(|e| e.name.clone())
        .collect();
    let want: Vec<String> = camcraft_app::posting::MACHINES.iter().map(|(n, _)| (*n).to_owned()).collect();
    assert_eq!(names, want);
    // The same door closes it.
    open_machines(&mut h);
    assert!(!layout::is_open(&h.state().dock, &Tab::Machines));
    assert_no_commands(&h);
}

#[test]
fn selecting_a_shipped_machine_shows_its_sheet_and_a_clean_validation() {
    let mut h = blank_harness();
    h.run();
    open_machines(&mut h);
    let lathe = h
        .state()
        .state
        .machines_pane
        .list
        .as_ref()
        .and_then(|l| l.iter().find(|e| e.name == "Lathe").cloned())
        .expect("the library lists the lathe");
    h.get_by_label(&row_label(&lathe)).click();
    h.run_steps(2);
    let sel = h.state().state.machines_pane.selected;
    let picked = h.state().state.machines_pane.list.as_ref().and_then(|l| sel.and_then(|i| l.get(i)));
    assert_eq!(picked.map(|e| e.name.as_str()), Some("Lathe"));
    h.get_by_label("Validation: passes");
    h.get_by_label("Axes");
    h.get_by_label("Spindle");
    assert_no_commands(&h);
}

#[test]
fn a_refused_machine_file_shows_each_refusal_by_name() {
    let text = r#"{
      "general": { "model": "BACKWARDS MILL", "kind": "mill" },
      "kinematics": { "default": { "units": { "length": "mm" }, "parts": [
        { "name": "X", "type": "linear", "travel": { "min": 10.0, "max": 5.0 } },
        { "name": "Y", "type": "linear" }, { "name": "Z", "type": "linear" }
      ] } }
    }"#;
    let path = std::env::temp_dir().join(format!("camcraft_ui_machines_pane_{}.mch", std::process::id()));
    std::fs::write(&path, text).expect("write the fixture file");
    let mut h = blank_harness();
    let m = camcraft_app::machine::parse_mch(text).expect("the reader takes it (it drops the travel)");
    camcraft_app::machine::set(&h.state().state.world, &path.display().to_string(), Some(&m));
    h.run();
    open_machines(&mut h);
    let bad = h
        .state()
        .state
        .machines_pane
        .list
        .as_ref()
        .and_then(|l| l.last().cloned())
        .expect("the document's file is listed last");
    assert_eq!(bad.name, "BACKWARDS MILL");
    let label = row_label(&bad);
    // The document's row is last, below the list's fold: scroll it in,
    // let the scroll settle, then press it with the real pointer.
    h.get_by_label(&label).scroll_to_me();
    h.run_steps(10);
    let at = h.get_by_label(&label).rect().center();
    click(&mut h, at);
    assert_eq!(verdict(&bad), "Validation: 1 refusal");
    h.get_by_label("Validation: 1 refusal");
    h.get_by_label("X travel: min 10 must be below max 5");
    assert_no_commands(&h);
    let _ = std::fs::remove_file(&path);
}
