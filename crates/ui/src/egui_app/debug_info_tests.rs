//! Issue #157: the status strip's Debug info door opens a window with a
//! plain-text report and a Copy button; the door closes it again.
//! Provenance: our own tests over our own window.

use egui::Vec2;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use egui::accesskit::Role;

use super::{debug_info, panes, theme};
use crate::state::State;

fn frame(ui: &mut egui::Ui, st: &mut State) {
    let pal = theme::current();
    panes::status(ui, st, &pal, None);
    debug_info::window(ui.ctx(), st);
}

fn harness() -> Harness<'static, State> {
    Harness::builder()
        .with_size(Vec2::new(900.0, 500.0))
        .build_ui_state(frame, State::new())
}

#[test]
fn the_report_names_build_platform_gpu_and_document() {
    let state = State::new();
    let text = debug_info::report(&state);
    assert!(text.starts_with(&format!("CamCraft {}", env!("CARGO_PKG_VERSION"))), "{text}");
    for want in [
        format!("Build time: {}", crate::build_info::BUILT_UTC),
        format!("Source commit: {}", crate::build_info::SOURCE_SHA),
        format!("Platform: {} {}", std::env::consts::OS, std::env::consts::ARCH),
        "GPU: none (headless)".to_owned(),
        "Project: unsaved".to_owned(),
        "Units: ".to_owned(),
        "Machine: ".to_owned(),
        "Document: ".to_owned(),
        "Status: ".to_owned(),
    ] {
        assert!(text.contains(&want), "missing {want:?} in:\n{text}");
    }
}

#[test]
fn the_door_opens_the_window_copy_copies_and_the_door_closes_it() {
    let mut h = harness();
    h.run();
    assert!(h.query_by_label("Copy").is_none(), "closed on a fresh state");
    h.get_by_label("Debug info").click();
    h.run();
    assert!(h.state().debug_info_open);
    h.get_by_label("Copy").click();
    h.step(); // the click lands this frame; its output is this frame's
    let copied = h.output().platform_output.commands.iter().any(|c| {
        matches!(c, egui::OutputCommand::CopyText(t) if t.starts_with("CamCraft "))
    });
    assert!(copied, "Copy put the report on the clipboard");
    h.run();
    assert_eq!(h.state().status, "debug info copied to the clipboard");
    // The window's title reads "Debug info" too: click the door, the button.
    h.get_by(|n| n.role() == Role::Button && n.label().as_deref() == Some("Debug info")).click();
    h.run();
    assert!(!h.state().debug_info_open);
    assert!(h.query_by_label("Copy").is_none(), "the door closed it");
}
