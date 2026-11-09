//! #255: the structural twins of the three pixel snapshots. The PNG
//! goldens (`tests/snapshots/egui_app*.png`, `egui_tool_editor.png`) ride
//! the release lane now; what each one guarded is asserted here as text -
//! the AccessKit tree egui_kittest exposes (widgets by label, role,
//! enabled/disabled, window presence) plus the text the painted strips
//! lay out (the ribbon, the browser rows and the dock's tab titles are
//! painted, not widgets, so their galleys are the readable surface).
//! Text-only and coarse-geometry-only, so they hold on every OS and merge
//! like code.
//! Provenance: our own tests, specced from the egui_kittest/kittest and
//! AccessKit public APIs; no HSMWorks material involved.

use egui_kittest::kittest::NodeT;

use super::*;

/// Every painted text of the last frame with its top-left, in paint order.
fn painted(h: &Harness<'static, ShellUi>) -> Vec<(String, egui::Pos2)> {
    fn walk(shape: &egui::Shape, out: &mut Vec<(String, egui::Pos2)>) {
        match shape {
            egui::Shape::Text(t) => out.push((t.galley.text().to_owned(), t.pos)),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for s in &h.output().shapes {
        walk(&s.shape, &mut out);
    }
    out
}

/// Where a painted text sits; fails with the whole list so a miss reads.
fn at(texts: &[(String, egui::Pos2)], want: &str) -> egui::Pos2 {
    at_where(texts, want, |_| true)
}

/// As `at`, for a text painted more than once (the ribbon's "Setups" cell
/// and the browser's "Setups" row): the first copy `inside` accepts.
fn at_where(
    texts: &[(String, egui::Pos2)],
    want: &str,
    inside: impl Fn(egui::Pos2) -> bool,
) -> egui::Pos2 {
    match texts.iter().find(|(t, p)| t == want && inside(*p)) {
        Some((_, p)) => *p,
        None => panic!(
            "no painted text {want:?}; painted: {:?}",
            texts.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>()
        ),
    }
}

fn disabled(h: &Harness<'static, ShellUi>, label: &str) -> bool {
    h.get_by_label(label).accesskit_node().is_disabled()
}

/// The seeded shell's skeleton - what `egui_app.png` pinned, theme-free:
/// ribbon on top with its groups and cells, the three dock leaves in
/// browser | viewport | properties order, the status strip at the bottom,
/// the seed's rows and page, and every floating window closed.
fn assert_shell_skeleton(h: &Harness<'static, ShellUi>) {
    let texts = painted(h);
    // Ribbon: the group headers and a cell per group, all above the dock.
    let groups = [
        "File", "Setup", "Milling 2D", "Milling 3D", "Drilling", "Multi-axis", "Modify",
        "Inspect", "View",
    ];
    let cells = ["New", "Open", "Save", "Post", "Stock", "Origin", "Undo", "Redo", "Simulate"];
    let tab_y = at(&texts, "browser").y;
    for t in groups.iter().chain(cells.iter()) {
        assert!(at(&texts, t).y < tab_y, "ribbon text {t:?} must sit above the dock tabs");
    }
    // The build line rides the ribbon's corner (#352's fixed test identity).
    let ver = format!("v{} · test", env!("CARGO_PKG_VERSION"));
    assert!(at(&texts, &ver).y < tab_y, "the build identity sits on the ribbon");
    // Dock: three leaves, left to right, titles on one row.
    let (b, v, p) = (at(&texts, "browser"), at(&texts, "viewport"), at(&texts, "properties"));
    assert!(b.x < v.x && v.x < p.x, "dock order must be browser | viewport | properties");
    assert!((b.y - v.y).abs() < 1.0 && (v.y - p.y).abs() < 1.0, "the tab titles share a row");
    // Browser rows of the seed, under its tab.
    let in_browser = |q: egui::Pos2| q.y > b.y && q.x < v.x;
    // #343: each setup owns its WCS, Stock, Machine, Post, Fixtures and
    // Tool list rows; a row paints its name and value as one text.
    for row in ["Setups", "Setup", "Stock", "Fixtures", "Operations", "Models", "Block", "Tools"] {
        at_where(&texts, row, in_browser);
    }
    for (name, value) in [("WCS", "G54"), ("Machine", ""), ("Post", ""), ("Tool list", "")] {
        assert!(
            texts.iter().any(|(t, q)| t.starts_with(name) && t.contains(value) && in_browser(*q)),
            "the setup's {name} row ({value:?}) must show in the browser"
        );
    }
    assert!(
        texts.iter().any(|(t, q)| t.starts_with("Contour1") && t.contains("2D Contour") && q.x < v.x),
        "the seed's operation row must show in the browser"
    );
    // Viewport: its toolbar widgets and the view cube's faces.
    for t in ["TOP", "RIGHT", "BACK"] {
        let q = at(&texts, t);
        assert!(q.x > v.x && q.x < p.x, "view cube face {t:?} belongs to the viewport");
    }
    for label in ["Show world origin axes", "Show all toolpaths", "Shaded", "Wireframe"] {
        assert!(!disabled(h, label), "viewport control {label:?} must be live");
    }
    // Properties: the seed's page, its sections, and the draft law's
    // disabled Apply (nothing staged on a fresh frame).
    assert!(h.get_by_label("Operation: Contour1").rect().min.x > v.x, "the page is the right leaf");
    for label in ["Type: 2D Contour", "Geometry", "Heights", "Passes", "Lead in / Lead out", "Feeds"] {
        assert!(h.query_by_label(label).is_some(), "property page must show {label:?}");
    }
    assert!(!disabled(h, "Generate"), "Generate is live on a valid op");
    assert!(disabled(h, "Apply"), "Apply waits for a staged edit");
    assert!(disabled(h, "Align WCS (3-2-1)"), "3-2-1 waits for three picks");
    // Status strip: the status line and the Debug info door at the bottom.
    let status = h.state().state.status.clone();
    assert!(status.starts_with("generated Contour1"), "seed status: {status}");
    let s = at(&texts, &status);
    assert!(s.y > p.y && s.y > 600.0, "the status line sits on the bottom strip");
    assert!(h.get_by_label("Debug info").rect().min.y > 600.0, "Debug info is on the status strip");
    // Every floating surface is closed by default.
    let titled: Vec<_> = h
        .query_all_by_role(egui::accesskit::Role::Window)
        .filter_map(|n| n.accesskit_node().label())
        .collect();
    assert!(titled.is_empty(), "no floating window may be open by default: {titled:?}");
    assert!(h.query_by_label("Shank diameter").is_none(), "the tool editor stays closed");
}

/// `egui_app.png`'s intent in the dark house look.
#[test]
fn shell_structure_dark() {
    let _g = snapshot_guard();
    let mut harness = shell_harness();
    harness.run_steps(3);
    assert_eq!(theme::current(), theme::Tokens::DARK, "the house look is dark");
    let fill = harness.ctx.global_style().visuals.panel_fill;
    assert_eq!(fill, theme::Tokens::DARK.ink, "egui's visuals follow the dark table");
    assert_shell_skeleton(&harness);
}

/// `egui_app_light.png`'s intent: the light column lands through the
/// switcher's door (the world's `ThemeRef`, synced on the frame edge),
/// reads (contrast law), and moves no widget out of the skeleton.
#[test]
fn shell_structure_light() {
    let _g = snapshot_guard();
    let mut harness = shell_harness();
    theme_set(&harness.state_mut().state.world, &ThemeRef::Named("light".to_owned()));
    harness.run_steps(3);
    let light = theme::current();
    let fill = harness.ctx.global_style().visuals.panel_fill;
    // Restore before asserting, so a failure cannot leave the shared
    // table light for the next test that takes the lock.
    let skeleton = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_shell_skeleton(&harness);
    }));
    theme_set(&harness.state_mut().state.world, &ThemeRef::Dark);
    harness.run_steps(1);
    assert_eq!(light, theme::Tokens::LIGHT, "the frame-edge sync rode the world's reference");
    assert_eq!(contrast::readable(&light), None, "the light table reads");
    assert_eq!(fill, theme::Tokens::LIGHT.ink, "egui's visuals follow the light table");
    if let Err(e) = skeleton {
        std::panic::resume_unwind(e);
    }
}

/// `egui_tool_editor.png`'s intent: Edit opens one floating window over
/// the dock, titled by the tool, fields left of the to-scale profile,
/// every field on the world's numbers, Apply/Close at its foot.
#[test]
fn tool_editor_structure() {
    let _g = snapshot_guard();
    let mut harness = shell_harness();
    harness.state_mut().state.ui_target = UiTarget::Tool("EM-0600-4FL".to_owned());
    harness.run();
    harness.run();
    harness.get_by_label("Edit").click();
    harness.run_steps(3);
    let win = harness
        .query_all_by_role(egui::accesskit::Role::Window)
        .find(|n| n.accesskit_node().label().as_deref() == Some("Tool: EM-0600-4FL"))
        .expect("Edit must open the editor window titled by the tool");
    let wr = win.rect();
    assert!(
        wr.min.x > 0.0 && wr.max.x < 1280.0 && wr.min.y > 0.0 && wr.max.y <= 720.0,
        "the editor floats inside the shell: {wr:?}"
    );
    // Fields in their order, each on the seed tool's number.
    let fields = [
        ("Diameter", "6.0 mm"),
        ("Corner radius", "0.0 mm"),
        ("Flute length", "15.0 mm"),
        ("Shank diameter", "8.0 mm"),
        ("Shank length", "40.0 mm"),
        ("Holder diameter", "44.0 mm"),
        ("Holder length", "35.0 mm"),
        ("Spindle rpm", "9500.0"),
        ("Feed (mm/min)", "1800.0 mm/min"),
    ];
    let spins: Vec<_> = win.get_all_by_role(egui::accesskit::Role::SpinButton).collect();
    let mut last_y = f32::MIN;
    for (label, value) in fields {
        let y = win.get_by_label(label).rect().center().y;
        assert!(y > last_y, "{label:?} must follow the field above it");
        last_y = y;
        assert!(
            spins.iter().any(|s| s.value().as_deref() == Some(value)),
            "the editor must show {label:?} = {value}"
        );
    }
    // Fields on the left half; the right half is the painted profile.
    assert!(
        win.get_by_label("Shank diameter").rect().max.x < wr.center().x,
        "fields sit left of the profile"
    );
    assert!(win.query_by_label("Centre cutting (may plunge into solid)").is_some());
    assert!(win.query_by_label("Apply").is_some() && win.query_by_label("Close").is_some());
    assert!(
        win.get_by_label("Apply").rect().min.y >= last_y,
        "Apply sits at the window's foot"
    );
}
