//! #709 hand check: on the user's own part (`Milling-test.step`, the
//! D-shaped plate), the Setups page's "+Y by Two points" keeps BOTH
//! picked points marked after the second real click. The 3D cross of a
//! picked point is ~1 mm, depth-tested and half sunk in its corner (~3 px
//! across at a framed part), so a picked corner could vanish; each pick
//! now also carries a screen-space violet diamond that depth cannot hide.
//! Provenance: our own test over our own shell; no vendor material
//! consulted.
//!
//! Gate map (M4.4): covers ui::interact (shown_picks,
//! selection_feature_lines), ui::vertex_pick (picked_point_marks),
//! ui::axis_pick, ui::feature_click.

use camcraft_app::geometry::{Point3, transform_point};
use camcraft_app::pick;

use super::*;

fn part_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step")
}

/// A blank window holding the user's part and Setup1, framed, with the
/// Setups page showing.
fn part_on_setups_page() -> Harness<'static, ShellUi> {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::blank());
    {
        let st = &mut h.state_mut().state;
        st.pending.push_back(CommandKind::ImportFile(part_path()));
        st.settle();
        st.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        st.drain_actions();
        crate::actions::frame_all(st);
        st.drain_actions();
        crate::actions::target(st, UiTarget::SetupNode("Setup1".into(), crate::state::SetupPage::Settings));
    }
    h.run_steps(3);
    h
}

/// The part's feature points on the top face (z max), world frame, with
/// where each draws in the pane (px).
fn top_points(st: &State) -> Vec<([f64; 3], egui::Pos2)> {
    let e = model::find_by_name(&st.world, "Milling-test").expect("the part imports as Milling-test");
    let tris = model::facets_of(&st.world, &e).expect("facets");
    let g = pick::feature_graph_shared(&tris);
    let loc = camcraft_app::setup_align::view_pose(&st.world, &e).unwrap_or_default();
    let ([x0, y0], [x1, y1]) = st.viewport_rect;
    let mvp = st.cam.mvp(f64::from((x1 - x0) / (y1 - y0)));
    let world: Vec<[f64; 3]> = g
        .points
        .iter()
        .map(|p| transform_point(Point3::new(p.at[0], p.at[1], p.at[2]), loc.pos, loc.rot))
        .map(|w| [w.x, w.y, w.z])
        .collect();
    let top = world.iter().map(|w| w[2]).fold(f64::MIN, f64::max);
    world
        .into_iter()
        .filter(|w| (w[2] - top).abs() < 1e-6)
        .filter_map(|w| {
            let uv = interact::project_uv(&mvp, w)?;
            Some((w, egui::pos2(x0 + uv[0] * (x1 - x0), y0 + uv[1] * (y1 - y0))))
        })
        .collect()
}

/// The centres (px) of the picked-point diamonds, four segments each.
fn mark_centres(st: &State) -> Vec<egui::Pos2> {
    let ([x0, y0], [x1, y1]) = st.viewport_rect;
    let (w, h) = (x1 - x0, y1 - y0);
    let segs = crate::vertex_pick::picked_point_marks(st, f64::from(w / h));
    assert_eq!(segs.len() % 4, 0, "whole diamonds: {segs:?}");
    segs.chunks(4)
        .map(|d| {
            let (sx, sy) = d.iter().fold((0.0, 0.0), |(sx, sy), (a, _)| (sx + a[0], sy + a[1]));
            egui::pos2(x0 + sx / 4.0 * w, y0 + sy / 4.0 * h)
        })
        .collect()
}

#[test]
fn both_points_of_a_two_point_axis_stay_marked_on_the_users_part() {
    let mut h = part_on_setups_page();
    h.get_by_label("+Y by Two points").click();
    h.run();
    assert!(crate::axis_pick::collecting(&h.state().state), "+Y by Two points arms the pick");
    let tops = top_points(&h.state().state);
    assert_eq!(tops.len(), 2, "the D's flat meets its round at two top corners: {tops:?}");

    for (k, (_, px)) in tops.iter().enumerate() {
        click_viewport(&mut h, *px);
        let st = &h.state().state;
        let picks = st.axis_pick.as_ref().and_then(|d| d.axes.y.clone()).map(|a| a.picks).unwrap_or_default();
        assert_eq!(picks.len(), k + 1, "{}", st.status);
        assert!(picks.iter().all(|g| matches!(g.feature, FeatureSel::Vertex(_))), "{picks:?}");
        // Both resolve into the 3D overlay: one three-arm cross each.
        assert_eq!(interact::selection_feature_lines(st).len(), 6 * (k + 1), "a cross per point");
        // And each has its screen mark, centred where the point draws.
        let marks = mark_centres(st);
        assert_eq!(marks.len(), k + 1, "a screen mark per picked point after click {}: {marks:?}", k + 1);
        for (_, want) in &tops[..=k] {
            assert!(
                marks.iter().any(|m| m.distance(*want) < 1.0),
                "a mark sits on the point at {want:?}: {marks:?}"
            );
        }
    }
    // The page still reads both points.
    assert!(h.state().state.status.contains("2 points for +Y"), "{}", h.state().state.status);
}
