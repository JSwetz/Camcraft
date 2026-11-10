//! #441: the Stock page shows stock only - Machine and Post are edited on
//! their own setup nodes, and the document's theme moved to a Theme door
//! on the ribbon, right of the View door, whose picks ride the history.
//! Provenance: our own tests over our own shell; no HSMWorks material.

use super::super::theme_drop::{self, ThemeRow};
use super::*;
use crate::state::SetupPage;
use camcraft_app::commands::{color_vision_of, theme_of};
use camcraft_app::schema::ColorVision;

fn click(h: &mut Harness<'static, ShellUi>, at: egui::Pos2) {
    set_pos(h, at);
    button_at(h, egui::PointerButton::Primary, at, true);
    button_at(h, egui::PointerButton::Primary, at, false);
    h.run_steps(2);
}

/// The Theme door's button, from the geometry the ribbon draws.
fn theme_button() -> egui::Rect {
    let (cells, _) = mill_plan_at(1280.0);
    cells.iter().find(|c| c.cmd == Cmd::Theme).expect("the Theme door stands on the bar").rect
}

fn theme_rows(state: &State) -> Vec<(ThemeRow, egui::Rect)> {
    let strip =
        egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280.0, super::super::shell::RIBBON_H));
    theme_drop::rows_of(state, theme_button(), strip, &theme::current())
}

#[test]
fn the_stock_page_has_no_machine_post_or_theme_controls() {
    let mut harness = shell_harness();
    harness.state_mut().state.ui_target = UiTarget::Stock;
    harness.run();
    assert!(harness.query_by_label("Fit to Models").is_some(), "the stock page is on screen");
    let post = camcraft_app::posting::post_label(&harness.state().state.world);
    for gone in ["Library", "Colour vision", "Theme", post.as_str()] {
        assert!(harness.query_by_label(gone).is_none(), "the stock page still shows '{gone}'");
    }
}

#[test]
fn the_theme_door_sits_right_of_panels() {
    let plan = panes::ribbon_plan(camcraft_app::machine::Kind::Mill, &[]);
    let view = plan.iter().find(|g| g.title == "View").expect("the View group");
    assert_eq!(view.cells.as_slice(), &[Cmd::View, Cmd::Theme]);
    let (cells, _) = mill_plan_at(1280.0);
    let panels = cells.iter().find(|c| c.cmd == Cmd::View).expect("Panels door").rect;
    assert!(theme_button().left() >= panels.right(), "Theme stands right of Panels");
}

#[test]
fn the_theme_door_switches_the_theme_and_undo_restores_it() {
    // #605: the pick lands through the frame-edge sync, which writes the
    // ONE process-wide live table (`theme::current()`); the structural
    // and snapshot tests assert on that table, so this test shares their
    // lock or a parallel run flips their look mid-frame.
    let _g = snapshot_guard();
    let mut harness = shell_harness();
    harness.run();
    assert_eq!(theme_of(&harness.state().state.world), ThemeRef::Dark);
    click(&mut harness, theme_button().center());
    assert!(harness.state().state.ribbon_theme_open, "pressing the door opens its list");
    let rows = theme_rows(&harness.state().state);
    let light = rows
        .iter()
        .find(|(r, _)| matches!(r, ThemeRow::Theme(l, _) if l == "Light"))
        .expect("the list offers the light theme")
        .1;
    assert!(
        rows.iter().any(|(r, _)| matches!(r, ThemeRow::Theme(_, None))),
        "the list offers Browse for any theme file"
    );
    click(&mut harness, light.center());
    harness.run_steps(2);
    assert!(!harness.state().state.ribbon_theme_open, "a choice closes the list");
    assert_eq!(theme_of(&harness.state().state.world), ThemeRef::Named("light".into()));
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run_steps(3);
    assert_eq!(theme_of(&harness.state().state.world), ThemeRef::Dark, "undo restores the theme");
    harness.state_mut().state.pending.push_back(CommandKind::Redo);
    harness.run_steps(3);
    assert_eq!(theme_of(&harness.state().state.world), ThemeRef::Named("light".into()), "redo re-applies it");
    // Leave the house look for whatever frame follows.
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run_steps(3);
    assert_eq!(theme_of(&harness.state().state.world), ThemeRef::Dark);
}

#[test]
fn the_theme_door_carries_the_colour_vision_modes() {
    let mut harness = shell_harness();
    harness.run();
    click(&mut harness, theme_button().center());
    let rows = theme_rows(&harness.state().state);
    let deutan = rows
        .iter()
        .find(|(r, _)| *r == ThemeRow::Vision(ColorVision::Deutan))
        .expect("the list offers the colour-vision modes")
        .1;
    click(&mut harness, deutan.center());
    harness.run_steps(2);
    assert_eq!(color_vision_of(&harness.state().state.world), ColorVision::Deutan);
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run_steps(3);
    assert_eq!(color_vision_of(&harness.state().state.world), ColorVision::Off, "undo restores the mode");
}

#[test]
fn machine_and_post_stay_editable_on_their_setup_nodes() {
    let mut harness = shell_harness();
    harness.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    harness.run_steps(2);
    crate::actions::target(&mut harness.state_mut().state, UiTarget::SetupNode("Setup1".into(), SetupPage::Machine));
    harness.run_steps(2);
    assert!(harness.query_by_label("Library").is_some(), "the machine node carries the machine library");
    crate::actions::target(&mut harness.state_mut().state, UiTarget::SetupNode("Setup1".into(), SetupPage::Post));
    harness.run_steps(2);
    let post = camcraft_app::posting::post_label(&harness.state().state.world);
    assert!(harness.query_by_label(&post).is_some(), "the post node carries the post choice");
}
