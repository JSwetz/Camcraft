//! #509: the Fixtures page fits a default-width properties pane - each
//! fixture row's name, position, Place / Unmark / Delete, the Fixed box
//! (its caption whole, wrapped not truncated) and the place fields stay
//! inside the pane, and the place fields say which axis they move.
//! Provenance: our own tests over our own shell; no HSMWorks material.
//!
//! Gate map (M4.4): covers ui::egui_app::pages (fixtures_section), ui::state.

use super::*;
use crate::state::SetupPage;

const FIXED: &str = "Fixed (does not turn with the part)";
const PLACE_FIELDS: [&str; 4] = ["Place X", "Place Y", "Place Z", "Place yaw"];

fn cube_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/cube.stl")
}

/// A blank window with one setup and a cube marked as its fixture, the
/// Fixtures page open in the properties pane at the default dock split.
fn fixtures_page() -> Harness<'static, ShellUi> {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::blank());
    {
        let st = &mut h.state_mut().state;
        st.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        st.drain_actions();
        st.pending.push_back(CommandKind::ImportFile(cube_path()));
        st.settle();
        st.pending.push_back(CommandKind::TagFixture { name: "cube".into(), setup: Some("Setup1".into()) });
        st.drain_actions();
        crate::actions::target(st, UiTarget::SetupNode("Setup1".into(), SetupPage::Fixtures));
    }
    h.run_steps(3);
    h
}

fn pane(h: &Harness<'static, ShellUi>) -> egui::Rect {
    let dock = &h.state().dock;
    let path = dock.find_tab(&Tab::Properties).expect("properties open");
    dock[path.node_path()].rect().expect("properties laid out")
}

/// Every node labelled `label` that the properties pane drew (the
/// browser and viewport sit left of it) lies wholly inside the pane.
fn assert_inside(h: &Harness<'static, ShellUi>, pane: egui::Rect, label: &str) {
    let nodes: Vec<egui::Rect> = h
        .query_all_by_label_contains(label)
        .map(|n| n.rect())
        .filter(|r| r.min.x >= pane.min.x - 1.0)
        .collect();
    assert!(!nodes.is_empty(), "the pane draws {label:?}");
    for r in nodes {
        assert!(
            r.min.x >= pane.min.x - 1.0 && r.max.x <= pane.max.x + 1.0,
            "{label:?} at {r:?} spills out of the pane {pane:?}"
        );
    }
}

#[test]
fn fixture_rows_fit_the_default_pane_and_label_the_place_fields() {
    let mut h = fixtures_page();
    let pane = pane(&h);
    assert!(h.query_by_label("Fixtures - Setup1").is_some(), "the page is open");
    // The place fields are one click away on every row, not hidden
    // behind clicking the name.
    h.get_by_label("Place…").click();
    h.run_steps(2);
    assert_eq!(h.state().state.fixture_ui.name, "cube", "Place… opens the draft");
    for label in ["cube", "yaw", "Place…", "Unmark", "Delete", FIXED, "Apply"] {
        assert_inside(&h, pane, label);
    }
    for label in PLACE_FIELDS {
        assert!(h.query_by_label(label).is_some(), "the place row names {label:?}");
        assert_inside(&h, pane, label);
    }
    // The Fixed caption is whole: as wide as its unwrapped text, or
    // wrapped onto a second line - never cut short with an ellipsis.
    let r = h
        .query_all_by_label(FIXED)
        .map(|n| n.rect())
        .find(|r| r.min.x >= pane.min.x - 1.0)
        .expect("the Fixed caption");
    let (full, row_h) = h.ctx.fonts_mut(|f| {
        let g = f.layout_no_wrap(FIXED.into(), egui::FontId::proportional(13.0), egui::Color32::WHITE);
        (g.size().x, g.size().y)
    });
    assert!(
        r.width() >= full * 0.9 || r.height() > row_h * 1.5,
        "the Fixed caption is truncated: {r:?} vs text width {full}"
    );
    // Opening the draft is view state: nothing reached the queue.
    assert!(h.state().state.pending.is_empty(), "{:?}", h.state().state.pending);
}
