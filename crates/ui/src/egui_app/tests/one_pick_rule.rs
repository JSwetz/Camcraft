//! #692's frame tests: one pick rule on every page. Real viewport clicks
//! on the seed's 60 x 60 x 30 block, on the operation page (Edge mode,
//! Contour1's geometry) and on the Setups page (an axis row armed):
//! a click on a flat face's triangulation diagonal never picks an edge;
//! a click over an edge hidden behind the block takes the near face or
//! nothing, never that edge; a box corner picks as a point where points
//! are taken, and as an edge where only edges and faces are. Plus the
//! smooth-wall case at the State level: a cylinder's facet seam is no
//! edge.
//! Provenance: our own pick law test over our own shell; no vendor
//! material consulted.

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cylinder, transform_point};
use camcraft_app::operation::GeoSel;
use camcraft_app::pick::{EdgeUnit, PickKinds, PickTarget};

use super::*;

type P = [f64; 3];

/// Where world point `q` draws, if well inside the pane.
fn px_of(sh: &ShellUi, q: P) -> Option<egui::Pos2> {
    let ([x0, y0], [x1, y1]) = sh.state.viewport_rect;
    let (w, hgt) = (x1 - x0, y1 - y0);
    let uv = interact::project_uv(&sh.state.cam.mvp(f64::from(w / hgt)), q)?;
    (uv[0] > 0.05 && uv[0] < 0.95 && uv[1] > 0.05 && uv[1] < 0.95)
        .then(|| egui::pos2(x0 + uv[0] * w, y0 + uv[1] * hgt))
}

fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn mid(a: P, b: P) -> P {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (a[2] + b[2]) * 0.5]
}

/// The block's twelve edges: (a, b, the outward normals of its two faces).
fn block_edges() -> Vec<(P, P, P, P)> {
    let (hi, mut out) = ([60.0, 60.0, 30.0], Vec::new());
    for axis in 0..3 {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        for su in [false, true] {
            for sv in [false, true] {
                let mut a = [0.0; 3];
                a[u] = if su { hi[u] } else { 0.0 };
                a[v] = if sv { hi[v] } else { 0.0 };
                let mut b = a;
                b[axis] = hi[axis];
                let (mut nu, mut nv) = ([0.0; 3], [0.0; 3]);
                nu[u] = if su { 1.0 } else { -1.0 };
                nv[v] = if sv { 1.0 } else { -1.0 };
                out.push((a, b, nu, nv));
            }
        }
    }
    out
}

/// Faces toward the eye at `p` (on the face).
fn facing(sh: &ShellUi, p: P, n: P) -> bool {
    let e = sh.state.cam.eye();
    dot([e[0] - p[0], e[1] - p[1], e[2] - p[2]], n) > 0.0
}

fn seg_dist(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let t = if ab.length_sq() > 0.0 { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) } else { 0.0 };
    (a + ab * t - p).length()
}

/// The pixel over the middle of a block edge hidden behind the block
/// (both its faces turned away), the one drawn farthest from every
/// visible edge - and that distance, px.
fn over_a_hidden_edge(sh: &ShellUi) -> (egui::Pos2, f32) {
    let edges = block_edges();
    let shown: Vec<(egui::Pos2, egui::Pos2)> = edges
        .iter()
        .filter(|(a, b, nu, nv)| facing(sh, mid(*a, *b), *nu) || facing(sh, mid(*a, *b), *nv))
        .filter_map(|(a, b, _, _)| Some((px_of(sh, *a)?, px_of(sh, *b)?)))
        .collect();
    edges
        .iter()
        .filter(|(a, b, nu, nv)| !facing(sh, mid(*a, *b), *nu) && !facing(sh, mid(*a, *b), *nv))
        .filter_map(|(a, b, _, _)| px_of(sh, mid(*a, *b)))
        .map(|p| (p, shown.iter().map(|(a, b)| seg_dist(p, *a, *b)).fold(f32::MAX, f32::min)))
        .max_by(|x, y| x.1.total_cmp(&y.1))
        .expect("the default camera hides an edge of the block")
}

/// The pixel over the middle of the top face's triangulation diagonal:
/// the one facet edge shared by two of the top face's triangles.
fn over_the_top_diagonal(sh: &ShellUi) -> egui::Pos2 {
    let w = &sh.state.world;
    let e = model::find_by_name(w, "Block").expect("seed has Block");
    let tris = model::facets_of(w, &e).expect("Block has facets");
    let loc = model::transform_of(w, &e).unwrap_or_default();
    let top: Vec<_> = tris.iter().filter(|t| [t.a, t.b, t.c].iter().all(|p| (p.z - 30.0).abs() < 1e-9)).collect();
    let key = |p: Point3| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()];
    let mut seen: Vec<([u64; 3], [u64; 3], Point3, Point3)> = Vec::new();
    for t in &top {
        for (a, b) in [(t.a, t.b), (t.b, t.c), (t.c, t.a)] {
            let (ka, kb) = (key(a), key(b));
            if let Some(&(_, _, a, b)) = seen.iter().find(|s| (s.0 == kb && s.1 == ka) || (s.0 == ka && s.1 == kb)) {
                let m = transform_point(Point3::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5, 30.0), loc.pos, loc.rot);
                return px_of(sh, [m.x, m.y, m.z]).expect("the top face's middle is on screen");
            }
            seen.push((ka, kb, a, b));
        }
    }
    panic!("the block's top face is not two triangles: {top:?}");
}

/// The block's corner nearest the eye, on screen.
fn a_corner(sh: &ShellUi) -> egui::Pos2 {
    let e = sh.state.cam.eye();
    let mut best: Option<(f64, egui::Pos2)> = None;
    for c in block_edges().iter().map(|x| x.0) {
        let d = (c[0] - e[0]).powi(2) + (c[1] - e[1]).powi(2) + (c[2] - e[2]).powi(2);
        if let Some(p) = px_of(sh, c)
            && best.is_none_or(|(bd, _)| d < bd)
        {
            best = Some((d, p));
        }
    }
    best.expect("the block shows a corner").1
}

/// Click `p` on Contour1's page in Edge mode; Contour1's picks after.
fn op_click(harness: &mut Harness<'static, ShellUi>, p: egui::Pos2) -> Vec<GeoSel> {
    crate::actions::target(&mut harness.state_mut().state, UiTarget::Operation("Contour1".into()));
    harness.run();
    harness.state_mut().state.geo_pick = GeoPick::Edge;
    click_viewport(harness, p);
    let st = &harness.state().state;
    assert_eq!(st.status, "geometry selection updated for Contour1", "the operation takes the click");
    let op = model::find_by_name(&st.world, "Contour1").expect("seed has Contour1");
    match model::operation_of(&st.world, &op).map(|o| o.src) {
        Some(SrcRef::Selection(items)) => items,
        other => panic!("Contour1's source became {other:?}"),
    }
}

/// Make Setup1, show its page and choose `mode` for +Y (which arms it).
fn arm_axis(harness: &mut Harness<'static, ShellUi>, mode: &str) {
    if camcraft_app::setup::active(&harness.state().state.world).is_none() {
        harness.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        harness.run();
    }
    crate::actions::target(
        &mut harness.state_mut().state,
        UiTarget::SetupNode("Setup1".into(), crate::state::SetupPage::Settings),
    );
    harness.run();
    harness.get_by_label(mode).click();
    harness.run();
    assert!(crate::axis_pick::collecting(&harness.state().state), "{mode} arms +Y");
}

fn axis_y(harness: &Harness<'static, ShellUi>) -> Vec<GeoSel> {
    let d = harness.state().state.axis_pick.clone().expect("the axis draft");
    d.axes.y.map(|a| a.picks).unwrap_or_default()
}

#[test]
fn a_click_on_a_flat_faces_diagonal_picks_the_face_never_an_edge() {
    let mut harness = shell_harness();
    let p = over_the_top_diagonal(harness.state());
    let items = op_click(&mut harness, p);
    assert_eq!(items.len(), 1, "{items:?}");
    let FeatureSel::Face(fid) = items[0].feature else {
        panic!("an Edge-mode click on the diagonal takes the face, got {:?}", items[0].feature);
    };
    let st = &harness.state().state;
    let block = model::find_by_name(&st.world, "Block").expect("Block");
    let faces = model::features_of(&st.world, &block).expect("features");
    let face = faces.faces.iter().find(|f| f.id == fid).expect("picked face exists");
    assert!(face.normal[2].abs() > 0.9, "the top face, under the diagonal");

    // The Setups page's axis takes edges only: on the diagonal, nothing.
    arm_axis(&mut harness, "+Y by Edge");
    click_viewport(&mut harness, p);
    assert!(axis_y(&harness).is_empty(), "the diagonal is no edge: {:?}", axis_y(&harness));
    assert!(harness.state().state.status.contains("no edge under the cursor"), "{}", harness.state().state.status);
}

#[test]
fn a_click_over_a_hidden_edge_takes_the_near_face_or_nothing_never_the_edge() {
    let mut harness = shell_harness();
    let (p, clear) = over_a_hidden_edge(harness.state());
    assert!(clear > 2.0 * crate::feature_click::PICK_EDGE_PX, "no visible edge within snap reach ({clear} px)");
    let items = op_click(&mut harness, p);
    assert_eq!(items.len(), 1, "{items:?}");
    assert!(matches!(items[0].feature, FeatureSel::Face(_)), "the near face, not the hidden edge: {items:?}");

    arm_axis(&mut harness, "+Y by Edge");
    click_viewport(&mut harness, p);
    assert!(axis_y(&harness).is_empty(), "a hidden edge is never picked: {:?}", axis_y(&harness));
}

#[test]
fn a_box_corner_picks_as_a_point_where_points_are_taken() {
    let mut harness = shell_harness();
    let p = a_corner(harness.state());
    // The operation's Edge mode takes edges and faces, not points: the
    // corner's click lands on an edge through it.
    let items = op_click(&mut harness, p);
    assert_eq!(items.len(), 1, "{items:?}");
    assert!(matches!(items[0].feature, FeatureSel::Chain(_)), "Edge mode never snaps to a point: {items:?}");

    arm_axis(&mut harness, "+Y by Two points");
    click_viewport(&mut harness, p);
    let y = axis_y(&harness);
    assert_eq!(y.len(), 1, "one point for +Y: {y:?} ({})", harness.state().state.status);
    assert!(matches!(y[0].feature, FeatureSel::Vertex(_)), "the corner, as a point: {y:?}");
}

#[test]
fn a_smooth_walls_facet_seam_is_no_edge() {
    // A 40 mm bore-like boss, 60 tall: its 24 wall strips meet at facet
    // seams the old screen-space edge search offered as edges.
    let mut state = State::new();
    state.history.execute(&state.world, ImportModel::from_mesh("boss", cylinder(40.0, 60.0, 24)));
    state.viewport_rect = ([0.0, 0.0], [400.0, 400.0]);
    state.cam = crate::viewport::Camera { target: [0.0, 0.0, 30.0], yaw: 0.3, pitch: 0.2, dist: 160.0, ..Default::default() };
    let e = state.cam.eye();
    let mvp = state.cam.mvp(1.0);
    // The vertical seam nearest the eye, at mid-height.
    let seam = (0..24)
        .map(|i| {
            let a = std::f64::consts::TAU * f64::from(i) / 24.0;
            [20.0 * a.cos(), 20.0 * a.sin(), 30.0]
        })
        .min_by(|x, y| {
            let d = |q: &P| (q[0] - e[0]).powi(2) + (q[1] - e[1]).powi(2) + (q[2] - e[2]).powi(2);
            d(x).total_cmp(&d(y))
        })
        .expect("seams");
    let uv = interact::project_uv(&mvp, seam).expect("on screen");
    let p = [uv[0] * 400.0, uv[1] * 400.0];
    let edges = crate::feature_click::Ask { kinds: PickKinds::EDGES, unit: EdgeUnit::Chain };
    assert!(crate::feature_click::feature_at(&state, p, edges).is_none(), "a facet seam is no edge");
    let both = crate::feature_click::Ask { kinds: PickKinds { faces: true, ..PickKinds::EDGES }, unit: EdgeUnit::Chain };
    let (pick, sel) = crate::feature_click::feature_at(&state, p, both).expect("the wall");
    assert!(matches!(pick.target, PickTarget::Face { .. }), "{pick:?}");
    assert!(matches!(sel, Some(FeatureSel::Patch(_))), "the whole welded wall: {sel:?}");
}
