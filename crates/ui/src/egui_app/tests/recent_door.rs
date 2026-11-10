//! #662: recent projects and models on the ribbon's Open and Model
//! buttons. A save / open / import fills the lists; with entries the
//! button opens a list (AccessKit-labelled rows: the files by name,
//! then "Clear recent" and "Browse…"); a click on an entry opens it
//! through Open's own path (the unsaved prompt included); a missing
//! file is greyed and refused by name; Clear recent empties the list;
//! with no entries the button is the dialog, as before. The lists
//! survive a restart through the per-user file.
//! Provenance: our own tests over our own shell; no HSMWorks material.
//!
//! Gate map (M4.4): covers ui::egui_app::recent_drop, ui::state (state_recent), ui::unsaved.

use std::path::PathBuf;

use camcraft_app::recent::{RecentFiles, RecentKind};

use super::super::recent_drop::{self, BROWSE, CLEAR, RecentRow};
use super::*;
use crate::unsaved::UnsavedChoice;

fn click(h: &mut Harness<'static, ShellUi>, at: egui::Pos2) {
    set_pos(h, at);
    button_at(h, egui::PointerButton::Primary, at, true);
    button_at(h, egui::PointerButton::Primary, at, false);
    h.run_steps(2);
}

/// A fresh folder for one test, canonical (macOS temp is a symlink).
fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft-recent-ui-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the test folder");
    std::fs::canonicalize(&d).expect("canonical folder")
}

fn cube_path() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/cube.stl")
}

/// A saved copy of the seeded document at `path`, written by another
/// State so the harness's own document stays as it is.
fn saved_project(path: &std::path::Path) {
    let mut other = State::new();
    world::seed(&mut other);
    other.save_project_path(path.to_path_buf());
    assert!(path.is_file(), "fixture project saved: {}", other.status);
}

fn button(cmd: Cmd) -> egui::Rect {
    let (cells, _) = mill_plan_at(1280.0);
    cells.iter().find(|c| c.cmd == cmd).expect("the File cell stands on the bar").rect
}

fn rows(state: &State, kind: RecentKind) -> Vec<(RecentRow, egui::Rect)> {
    let cmd = if kind == RecentKind::Project { Cmd::Project } else { Cmd::Model };
    let strip =
        egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280.0, super::super::shell::RIBBON_H));
    recent_drop::rows_of(state, kind, button(cmd), strip, &theme::current())
}

fn row_rect(state: &State, kind: RecentKind, label: &str) -> egui::Rect {
    rows(state, kind)
        .into_iter()
        .find(|(r, _)| r.label() == label)
        .unwrap_or_else(|| panic!("the list has a '{label}' row"))
        .1
}

#[test]
fn the_door_lists_entries_then_clear_and_browse_with_accesskit_labels() {
    let d = temp_dir("structural");
    let (a, b) = (d.join("a.camproj"), d.join("b.camproj"));
    saved_project(&a);
    saved_project(&b);
    let mut h = shell_harness();
    {
        let st = &mut h.state_mut().state;
        st.recent.add(RecentKind::Project, &a);
        st.recent.add(RecentKind::Project, &b);
        st.recent.add(RecentKind::Project, &d.join("gone.camproj"));
    }
    h.run();
    assert!(h.query_by_label(CLEAR).is_none(), "the list is closed until pressed");
    click(&mut h, button(Cmd::Project).center());
    assert_eq!(h.state().state.ribbon_recent_open, Some(RecentKind::Project));
    let labels: Vec<String> = rows(&h.state().state, RecentKind::Project).iter().map(|(r, _)| r.label()).collect();
    assert_eq!(labels, ["gone.camproj (missing)", "b.camproj", "a.camproj", CLEAR, BROWSE], "newest first");
    for label in &labels {
        h.get_by_label(label);
    }
    // The Model list is its own, and empty.
    assert!(h.state().state.recent.list(RecentKind::Model).is_empty());
}

#[test]
fn a_click_on_an_entry_opens_it_through_open() {
    let d = temp_dir("open");
    let path = d.join("job.camproj");
    let mut h = shell_harness();
    // Saving fills the list (Save / Save As are noted) and leaves the
    // document clean, so the open needs no prompt.
    h.state_mut().state.pending.push_back(CommandKind::SaveProjectFile(path.clone()));
    h.run_steps(2);
    assert_eq!(h.state().state.recent.list(RecentKind::Project), std::slice::from_ref(&path));
    h.state_mut().state.pending.push_back(CommandKind::NewDocument);
    h.run_steps(2);
    assert_eq!(h.state().state.project_path, None);
    click(&mut h, button(Cmd::Project).center());
    let at = row_rect(&h.state().state, RecentKind::Project, "job.camproj").center();
    click(&mut h, at);
    h.run_steps(2);
    let st = &h.state().state;
    assert_eq!(st.ribbon_recent_open, None, "a choice closes the list");
    assert_eq!(st.project_path.as_deref(), Some(path.as_path()), "status: {}", st.status);
    assert!(st.status.starts_with("opened"), "{}", st.status);
    assert!(model::find_by_name(&st.world, "Block").is_some(), "the saved document came back");
}

#[test]
fn a_recent_project_over_unsaved_changes_takes_opens_prompt() {
    let d = temp_dir("unsaved");
    let path = d.join("other.camproj");
    saved_project(&path);
    let mut h = shell_harness();
    assert!(h.state().state.history.is_dirty(), "the seed is unsaved work");
    h.state_mut().state.recent.add(RecentKind::Project, &path);
    click(&mut h, button(Cmd::Project).center());
    let at = row_rect(&h.state().state, RecentKind::Project, "other.camproj").center();
    click(&mut h, at);
    assert!(h.state().state.unsaved_prompt.is_some(), "the open is held for the prompt");
    h.get_by_label("Unsaved changes");
    crate::actions::resolve_unsaved(&mut h.state_mut().state, UnsavedChoice::DontSave);
    h.run_steps(2);
    assert_eq!(h.state().state.project_path.as_deref(), Some(path.as_path()));
}

#[test]
fn a_missing_entry_is_refused_by_name() {
    let d = temp_dir("missing");
    let gone = d.join("gone.camproj");
    let mut h = shell_harness();
    h.state_mut().state.recent.add(RecentKind::Project, &gone);
    let dirty = h.state().state.history.is_dirty();
    click(&mut h, button(Cmd::Project).center());
    h.get_by_label("gone.camproj (missing)");
    let at = row_rect(&h.state().state, RecentKind::Project, "gone.camproj (missing)").center();
    click(&mut h, at);
    h.run_steps(2);
    let st = &h.state().state;
    assert!(st.status.contains("gone.camproj is missing"), "refused by name: {}", st.status);
    assert!(st.unsaved_prompt.is_none(), "a missing file asks nothing");
    assert_eq!(st.project_path, None, "nothing opened");
    assert_eq!(st.history.is_dirty(), dirty, "the document is untouched");
    assert!(model::find_by_name(&st.world, "Block").is_some());
    // The same refusal holds for a queued command (the file went
    // between the frame that drew it and the click).
    h.state_mut().state.pending.push_back(CommandKind::OpenRecent(RecentKind::Model, d.join("gone.stl")));
    h.run_steps(2);
    assert!(h.state().state.status.contains("gone.stl is missing"), "{}", h.state().state.status);
}

#[test]
fn clear_recent_empties_the_list() {
    let mut h = shell_harness();
    h.state_mut().state.recent.add(RecentKind::Model, &cube_path());
    click(&mut h, button(Cmd::Model).center());
    assert_eq!(h.state().state.ribbon_recent_open, Some(RecentKind::Model));
    let at = row_rect(&h.state().state, RecentKind::Model, CLEAR).center();
    click(&mut h, at);
    h.run_steps(2);
    let st = &h.state().state;
    assert!(st.recent.list(RecentKind::Model).is_empty(), "Clear recent empties it");
    assert_eq!(st.ribbon_recent_open, None);
    assert!(h.query_by_label(CLEAR).is_none(), "the list is gone");
}

#[test]
fn a_recent_model_imports_through_the_worker_and_is_noted() {
    let d = temp_dir("model");
    let copy = d.join("bracket.stl");
    std::fs::copy(cube_path(), &copy).expect("copy the cube fixture");
    let mut h = shell_harness();
    h.state_mut().state.recent.add(RecentKind::Model, &copy);
    h.state_mut().state.recent.add(RecentKind::Model, &cube_path());
    click(&mut h, button(Cmd::Model).center());
    let at = row_rect(&h.state().state, RecentKind::Model, "bracket.stl").center();
    click(&mut h, at);
    h.state_mut().state.settle();
    h.run_steps(2);
    let st = &h.state().state;
    assert!(model::find_by_name(&st.world, "bracket").is_some(), "status: {}", st.status);
    assert_eq!(st.recent.list(RecentKind::Model).first(), Some(&copy), "the import moved it to the top");
}

#[test]
fn an_empty_list_and_browse_open_todays_dialog() {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame_undrained, ShellUi::seeded());
    h.run();
    click(&mut h, button(Cmd::Project).center());
    let queued = |h: &Harness<'static, ShellUi>| {
        h.state().state.pending.iter().any(|c| matches!(c, CommandKind::OpenProjectDialog))
    };
    assert!(queued(&h), "no recents: the button is the dialog, as before");
    assert_eq!(h.state().state.ribbon_recent_open, None);
    h.state_mut().state.pending.clear();
    h.state_mut().state.recent.add(RecentKind::Project, &cube_path());
    click(&mut h, button(Cmd::Project).center());
    assert!(!queued(&h), "with recents the button opens its list");
    let at = row_rect(&h.state().state, RecentKind::Project, BROWSE).center();
    click(&mut h, at);
    assert!(queued(&h), "Browse… opens the dialog");
    h.state_mut().state.pending.clear();
}

#[test]
fn the_lists_survive_a_restart_and_a_corrupt_file_starts_empty() {
    let d = temp_dir("restart");
    let file = d.join("recent.json");
    let project = d.join("kept.camproj");
    let mut first = State::new();
    world::seed(&mut first);
    first.load_recent_from(file.clone());
    first.save_project_path(project.clone());
    // A "restart": a new State reading the same file.
    let mut second = State::new();
    second.load_recent_from(file.clone());
    assert_eq!(second.recent.list(RecentKind::Project), &[project]);
    std::fs::write(&file, "{ not json").expect("corrupt the file");
    let mut third = State::new();
    third.load_recent_from(file);
    assert_eq!(third.recent, RecentFiles::default(), "a corrupt file starts empty");
    assert!(third.status.contains("starting empty"), "and says so: {}", third.status);
}
