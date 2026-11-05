//! The dock layout law's frame tests: the viewport is permanent, a
//! closed pane comes back through the ribbon's View door at its default
//! side, Reset layout restores all three, and none of it touches the
//! command queue or the history (layout is view state).
//! Provenance: our own tests, specced from the egui_dock and
//! egui_kittest vendored sources; no HSMWorks material involved.

use super::super::layout::{self, ViewRow};
use super::*;
use crate::state::{LayoutRequest, Panel};

fn blank_harness() -> Harness<'static, ShellUi> {
    Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::blank())
}

fn click(h: &mut Harness<'static, ShellUi>, at: egui::Pos2) {
    set_pos(h, at);
    button_at(h, egui::PointerButton::Primary, at, true);
    button_at(h, egui::PointerButton::Primary, at, false);
    h.run_steps(2);
}

/// The View door's button and its rows, from the same geometry the
/// ribbon draws (strip at the harness's width).
fn view_door() -> (egui::Rect, Vec<(ViewRow, egui::Rect)>) {
    let strip =
        egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280.0, super::super::shell::RIBBON_H));
    let (cells, _) = mill_plan_at(1280.0);
    let button = cells
        .iter()
        .find(|c| c.cmd == Cmd::View)
        .expect("the View door stands on the bar")
        .rect;
    let rows = panes::view_rows_of(button, strip, &theme::current());
    (button, rows)
}

fn row(rows: &[(ViewRow, egui::Rect)], want: ViewRow) -> egui::Pos2 {
    rows.iter()
        .find(|(r, _)| *r == want)
        .expect("the View door lists the row")
        .1
        .center()
}

/// The laid-out rect of the leaf holding `tab` (None when closed).
fn leaf_rect(dock: &DockState<Tab>, tab: &Tab) -> Option<egui::Rect> {
    let path = dock.find_tab(tab)?;
    dock[path.node_path()].rect()
}

/// Nothing the layout law does may ride the command lane.
fn assert_no_commands(h: &Harness<'static, ShellUi>) {
    let st = &h.state().state;
    assert!(st.pending.is_empty(), "layout queued a command: {:?}", st.pending);
    assert!(st.history.is_empty(), "layout landed in the history");
}

#[test]
fn the_viewport_tab_is_permanent() {
    assert!(!layout::closeable(&Tab::Viewport), "the viewport must not close or float");
    assert!(layout::closeable(&Tab::Browser));
    assert!(layout::closeable(&Tab::Properties));
}

#[test]
fn a_closed_browser_comes_back_through_the_view_door() {
    let mut h = blank_harness();
    h.run();
    let vp = leaf_rect(&h.state().dock, &Tab::Viewport).expect("viewport laid out");
    // The tab's X removes the tab from the dock - the same removal.
    layout::apply(&mut h.state_mut().dock, LayoutRequest::Toggle(Panel::Browser));
    h.run_steps(2);
    assert!(!layout::is_open(&h.state().dock, &Tab::Browser));
    assert!(!h.state().state.panels_open.browser, "the mirror follows the dock");
    let (button, rows) = view_door();
    click(&mut h, button.center());
    assert!(h.state().state.ribbon_view_open, "pressing the door opens its list");
    click(&mut h, row(&rows, ViewRow::Panel(Panel::Browser)));
    assert!(!h.state().state.ribbon_view_open, "a choice closes the list");
    let dock = &h.state().dock;
    assert!(layout::is_open(dock, &Tab::Browser), "the toggle reopens the browser");
    assert!(h.state().state.panels_open.browser);
    let br = leaf_rect(dock, &Tab::Browser).expect("browser laid out");
    let vp_now = leaf_rect(dock, &Tab::Viewport).expect("viewport laid out");
    assert!(br.right() <= vp_now.left() + 1.0, "the browser returns at the left: {br:?} vs {vp_now:?}");
    assert!((vp_now.width() - vp.width()).abs() < 2.0, "the viewport gets its width back");
    // And the same toggle closes it again.
    click(&mut h, button.center());
    click(&mut h, row(&rows, ViewRow::Panel(Panel::Browser)));
    assert!(!layout::is_open(&h.state().dock, &Tab::Browser));
    assert_no_commands(&h);
}

#[test]
fn a_closed_properties_pane_returns_at_the_right() {
    let mut h = blank_harness();
    h.run();
    layout::apply(&mut h.state_mut().dock, LayoutRequest::Toggle(Panel::Properties));
    h.run_steps(2);
    let (button, rows) = view_door();
    click(&mut h, button.center());
    click(&mut h, row(&rows, ViewRow::Panel(Panel::Properties)));
    let dock = &h.state().dock;
    let pr = leaf_rect(dock, &Tab::Properties).expect("properties reopened");
    let vp = leaf_rect(dock, &Tab::Viewport).expect("viewport laid out");
    assert!(pr.left() + 1.0 >= vp.right(), "properties return at the right: {pr:?} vs {vp:?}");
    assert_no_commands(&h);
}

#[test]
fn reset_layout_restores_all_three_panes() {
    let mut h = blank_harness();
    h.run();
    let want: Vec<_> = [Tab::Browser, Tab::Viewport, Tab::Properties]
        .iter()
        .map(|t| leaf_rect(&h.state().dock, t).expect("default layout"))
        .collect();
    layout::apply(&mut h.state_mut().dock, LayoutRequest::Toggle(Panel::Browser));
    layout::apply(&mut h.state_mut().dock, LayoutRequest::Toggle(Panel::Properties));
    h.run_steps(2);
    let (button, rows) = view_door();
    click(&mut h, button.center());
    click(&mut h, row(&rows, ViewRow::Reset));
    let dock = &h.state().dock;
    for (t, w) in [Tab::Browser, Tab::Viewport, Tab::Properties].iter().zip(&want) {
        let got = leaf_rect(dock, t).expect("reset reopens every pane");
        assert!(
            (got.min - w.min).length() < 1.0 && (got.max - w.max).length() < 1.0,
            "{t:?} back at its default rect: {got:?} vs {w:?}"
        );
    }
    assert_no_commands(&h);
}

#[test]
fn the_viewport_row_is_inert_and_the_door_dismisses_outside() {
    let mut h = blank_harness();
    h.run();
    let (button, rows) = view_door();
    click(&mut h, button.center());
    click(&mut h, row(&rows, ViewRow::Viewport));
    assert!(h.state().state.ribbon_view_open, "the always-open row keeps the list up");
    assert!(layout::is_open(&h.state().dock, &Tab::Viewport));
    // A press on the status strip, away from the door, dismisses it
    // (the viewport would pick, and a pick is a command).
    click(&mut h, egui::pos2(640.0, 712.0));
    assert!(!h.state().state.ribbon_view_open, "an outside press closes the list");
    assert_no_commands(&h);
}
