//! #555's frame test: with the Setups page's three-points row armed,
//! real viewport clicks on three top corners of the seed's 60 x 60 x 30
//! block land as points 1, 2, 3 of the draft (Vertex mode, never the
//! open operation), and the app accepts them as a plane - flipped, they
//! rest the top on the table.
//! Provenance: our own pick law test over our own shell; no vendor
//! material consulted.

use camcraft_app::cl::ArcPlane;
use camcraft_app::commands::AlignPartToSetup;
use camcraft_app::pick::{PlaneLay, PointsLay};

use super::*;

/// The screen pixel of a world point in the viewport.
fn pixel(sh: &ShellUi, p: [f64; 3]) -> egui::Pos2 {
    let ([x0, y0], [x1, y1]) = sh.state.viewport_rect;
    let (w, hgt) = (x1 - x0, y1 - y0);
    let mvp = sh.state.cam.mvp(f64::from(w / hgt));
    let uv = interact::project_uv(&mvp, p).expect("the corner is in front of the camera");
    egui::pos2(x0 + uv[0] * w, y0 + uv[1] * hgt)
}

#[test]
fn three_clicked_top_corners_are_points_1_2_3_of_the_plane() {
    let mut harness = shell_harness();
    harness.run();
    {
        let st = &mut harness.state_mut().state;
        st.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        st.drain_actions();
    }
    let setup = camcraft_app::setup::active(&harness.state().state.world).expect("Setup1");
    // The draft lives while its Setups page row is on screen (#664).
    crate::actions::target(
        &mut harness.state_mut().state,
        UiTarget::SetupNode(setup.clone(), crate::state::SetupPage::Settings),
    );
    harness.run();
    crate::points_pick::arm(&mut harness.state_mut().state, &setup);
    assert_eq!(harness.state().state.geo_pick, GeoPick::Vertex, "the points row arms vertices");
    let geo_before = harness.state().state.op_ui.geo.clone();
    for p in [[0.0, 0.0, 30.0], [60.0, 0.0, 30.0], [60.0, 60.0, 30.0]] {
        let px = pixel(harness.state(), p);
        click_viewport(&mut harness, px);
    }
    let st = &harness.state().state;
    let picks = st.points_pick.as_ref().map(|d| d.picks.clone()).unwrap_or_default();
    assert_eq!(picks.len(), 3, "three points drafted: {picks:?} ({})", st.status);
    assert!(picks.iter().all(|g| g.entity == "Block" && matches!(g.feature, FeatureSel::Vertex(_))), "{picks:?}");
    assert_eq!(st.op_ui.geo, geo_before, "nothing joins the open operation");
    let pl = PointsLay { lay: PlaneLay { plane: ArcPlane::Xy, flip: true, place: true }, frame: false };
    let check = AlignPartToSetup::check_points(&st.world, &setup, &picks, pl);
    assert!(check.is_ok(), "the clicked corners make a plane: {check:?}");
}
