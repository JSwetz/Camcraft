//! The fixture laws against a fixture's real mesh (#339): a vise is two
//! jaws with a gap, and its world box is one block. A path down the gap
//! was a false crash under the box; under the mesh it is clean, while a
//! flute clipping a jaw corner, a rapid dipping into a jaw, and a tool
//! standing wholly inside steel still convict. Open and non-finite
//! meshes keep the box verdict.
//! Provenance: our own geometry; the law is the Prime Directive's.

use std::sync::Arc;
use std::time::Instant;

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::fixture::FixtureBox;
use camcraft_sim::fixture_mesh::{FixtureMesh, MeshError, MeshedFixture};
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::{Point3, Triangle};
use camcraft_sim::verify::{
    bands_vs_workholding, mesh_refined, rapids_vs_workholding, steel_stack, FixturePart, MeshSteel, ToolGeom,
    Warning, Workholding,
};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Flute r4 x 30, shank r5 x 20, holder r20 x 40: in the vise's gap
/// (|y| < 15) with the tip at z 25, the flute and shank stand between
/// the jaws and the holder (75 mm up) clears their tops at z 50.
fn tool() -> ToolGeom {
    ToolGeom {
        radius: 4.0,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: 5.0,
        shank_len: 20.0,
        holder_radius: 20.0,
        holder_len: 40.0,
    }
}

/// A closed, outward box: 12 facets sharing 8 exact corners.
fn cube(lo: Point3, hi: Point3) -> Vec<Triangle> {
    let c = |i: usize| {
        p(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z })
    };
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    quads
        .iter()
        .flat_map(|q| [Triangle::new(c(q[0]), c(q[1]), c(q[2])), Triangle::new(c(q[0]), c(q[2]), c(q[3]))])
        .collect()
}

/// Base x[-50,50] y[-40,40] z[0,20]; jaws y[-40,-15] and y[15,40],
/// z[20,50]. Three closed shells (they touch, they do not share
/// edges): the gap is |y| < 15 above z 20.
fn vise() -> Vec<Triangle> {
    let mut t = cube(p(-50.0, -40.0, 0.0), p(50.0, 40.0, 20.0));
    t.extend(cube(p(-50.0, -40.0, 20.0), p(50.0, -15.0, 50.0)));
    t.extend(cube(p(-50.0, 15.0, 20.0), p(50.0, 40.0, 50.0)));
    t
}

fn aabb(tris: &[Triangle]) -> FixtureBox {
    let mut lo = p(f64::MAX, f64::MAX, f64::MAX);
    let mut hi = p(f64::MIN, f64::MIN, f64::MIN);
    for q in tris.iter().flat_map(|t| [t.a, t.b, t.c]) {
        lo = p(lo.x.min(q.x), lo.y.min(q.y), lo.z.min(q.z));
        hi = p(hi.x.max(q.x), hi.y.max(q.y), hi.z.max(q.z));
    }
    FixtureBox { min: lo, max: hi }
}

fn fixture(tris: Vec<Triangle>) -> MeshedFixture {
    MeshedFixture::new("Vise1", aabb(&tris), Arc::<[Triangle]>::from(tris))
}

fn feed(a: Point3, b: Point3) -> Move {
    Move::new(MoveKind::Feed, a, b, Some(800.0))
}

/// Both laws (c2)/(d2), box-only and mesh-refined, for one fixture.
fn judge(path: &Toolpath, f: &MeshedFixture) -> (Vec<Warning>, Vec<Warning>) {
    let boxes = [f.aabb];
    let wh = Workholding { turning: &boxes, fixed: &[], axis: None };
    let t = tool();
    let stack = steel_stack(&t, &camcraft_tools::holder_profile::HolderStack::cylinder(&t), None);
    let fixtures = std::slice::from_ref(f);
    let steel = MeshSteel { fixtures, stack: &stack, chord_tol: 0.01 };
    let mut boxed = bands_vs_workholding(path, &t, &wh, None, 0.01);
    boxed.extend(rapids_vs_workholding(path, &t, &wh, None));
    let mut meshed = mesh_refined(path, &wh, &steel, false, |w| bands_vs_workholding(path, &t, w, None, 0.01));
    meshed.extend(mesh_refined(path, &wh, &steel, true, |w| rapids_vs_workholding(path, &t, w, None)));
    (boxed, meshed)
}

fn hit_at(ws: &[Warning], i: usize) -> Option<FixturePart> {
    ws.iter().find_map(|w| match w {
        Warning::FixtureHit { at, fixture_part } if *at == i => Some(*fixture_part),
        _ => None,
    })
}

#[test]
fn a_path_down_the_vise_gap_is_clean_under_the_mesh() {
    let f = fixture(vise());
    let path = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(-60.0, 0.0, 60.0), p(-60.0, 0.0, 25.0), None),
        feed(p(-60.0, 0.0, 25.0), p(60.0, 0.0, 25.0)),
        // A plunge straight down the gap, and an XY arc inside it.
        Move::new(MoveKind::Rapid, p(60.0, 0.0, 25.0), p(0.0, 0.0, 70.0), None),
        Move::new(MoveKind::Rapid, p(0.0, 0.0, 70.0), p(6.0, 0.0, 25.0), None),
        feed(p(6.0, 0.0, 25.0), p(-6.0, 0.0, 25.0)).with_arc(Point2::new(0.0, 0.0), true),
    ]);
    let (boxed, meshed) = judge(&path, &f);
    assert!(hit_at(&boxed, 1).is_some(), "the world box convicts the gap pass: {boxed:?}");
    assert!(hit_at(&boxed, 4).is_some(), "and the arc in the gap: {boxed:?}");
    assert!(meshed.is_empty(), "the jaws' real steel is nowhere near: {meshed:?}");
    assert_eq!(f.refusal(), None, "the closed vise judged");
}

#[test]
fn a_flute_clipping_a_jaw_corner_still_blocks() {
    let f = fixture(vise());
    // The diagonal y = x - 40 passes 3.54 mm from jaw B's corner edge
    // (x 50, y 15) - inside the flute's 4 mm. Shifted to y = x - 42 it
    // passes 4.95 mm off: clear.
    let path = Toolpath::new(vec![
        feed(p(40.0, 0.0, 25.0), p(60.0, 20.0, 25.0)),
        feed(p(40.0, -2.0, 25.0), p(60.0, 18.0, 25.0)),
        // A pass 12 mm off centre: the flute reaches y 16, into jaw B.
        feed(p(-60.0, 12.0, 25.0), p(60.0, 12.0, 25.0)),
    ]);
    let (boxed, meshed) = judge(&path, &f);
    assert_eq!(hit_at(&boxed, 1), Some(FixturePart::Flute), "the box flags the near miss too: {boxed:?}");
    assert_eq!(hit_at(&meshed, 0), Some(FixturePart::Flute), "the corner clip convicts: {meshed:?}");
    assert_eq!(hit_at(&meshed, 1), None, "the near miss is acquitted: {meshed:?}");
    assert_eq!(hit_at(&meshed, 2), Some(FixturePart::Flute), "the jaw-face clip convicts: {meshed:?}");
}

#[test]
fn a_shank_or_holder_through_a_jaw_names_the_band() {
    let f = fixture(vise());
    // A pass beside the vise, 15 mm off jaw B's outer face (y 40), tip
    // at z -5: the flute (r4) and the shank (r5, z 25..45) stay clear,
    // the holder (r20, z 45..85) reaches 5 mm into the jaw's top.
    let path = Toolpath::new(vec![feed(p(-30.0, 55.0, -5.0), p(30.0, 55.0, -5.0))]);
    let (boxed, meshed) = judge(&path, &f);
    assert_eq!(hit_at(&boxed, 0), Some(FixturePart::Holder), "{boxed:?}");
    assert_eq!(hit_at(&meshed, 0), Some(FixturePart::Holder), "{meshed:?}");
}

#[test]
fn a_rapid_dipping_into_a_jaw_top_still_blocks() {
    let f = fixture(vise());
    let path = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(0.0, 30.0, 70.0), p(0.0, 30.0, 45.0), None),
        Move::new(MoveKind::Rapid, p(0.0, 0.0, 70.0), p(0.0, 0.0, 45.0), None),
    ]);
    let (boxed, meshed) = judge(&path, &f);
    assert!(hit_at(&boxed, 1).is_some(), "the box flags the gap retract: {boxed:?}");
    assert_eq!(hit_at(&meshed, 0), Some(FixturePart::Rapid), "into jaw B: {meshed:?}");
    assert_eq!(hit_at(&meshed, 1), None, "down the gap: {meshed:?}");
}

#[test]
fn a_rapid_kept_by_the_mesh_names_its_band_like_law_d2() {
    // Two meshed fixtures: the vise acquits the rapid down its gap, a
    // bar at shank height (z 60..65, 35 mm over the tip) convicts it.
    // Law (d2) reads the whole upright stack since #424 and names a
    // band above the flute by its part - the mesh's verdict must name
    // it the same way, not as the flute's Rapid.
    let vise = fixture(vise());
    let bar_tris = cube(p(70.0, -3.0, 60.0), p(90.0, 3.0, 65.0));
    let bar = MeshedFixture::new("Bar1", aabb(&bar_tris), Arc::<[Triangle]>::from(bar_tris));
    let boxes = [vise.aabb, bar.aabb];
    let wh = Workholding { turning: &boxes, fixed: &[], axis: None };
    let t = tool();
    let stack = steel_stack(&t, &camcraft_tools::holder_profile::HolderStack::cylinder(&t), None);
    let fixtures = [vise, bar];
    let steel = MeshSteel { fixtures: &fixtures, stack: &stack, chord_tol: 0.01 };
    let path = Toolpath::new(vec![Move::new(MoveKind::Rapid, p(-60.0, 0.0, 25.0), p(100.0, 0.0, 25.0), None)]);
    let boxed = rapids_vs_workholding(&path, &t, &wh, None);
    assert_eq!(hit_at(&boxed, 0), Some(FixturePart::Rapid), "the vise box's flute disc: {boxed:?}");
    let meshed = mesh_refined(&path, &wh, &steel, true, |w| rapids_vs_workholding(&path, &t, w, None));
    assert_eq!(hit_at(&meshed, 0), Some(FixturePart::Shank), "the bar strikes the shank: {meshed:?}");
}

#[test]
fn a_tool_standing_wholly_inside_steel_is_a_hit() {
    // A tall block: the whole stack (175 mm tall from z 60) sits inside
    // it, far from every facet - only the inside test can see it.
    let f = fixture(cube(p(-50.0, -50.0, 0.0), p(50.0, 50.0, 300.0)));
    let path = Toolpath::new(vec![feed(p(-10.0, 0.0, 60.0), p(10.0, 0.0, 60.0))]);
    let (_, meshed) = judge(&path, &f);
    assert!(hit_at(&meshed, 0).is_some(), "buried in steel is a crash: {meshed:?}");
    let mesh = f.mesh().expect("closed");
    assert!(!mesh.band_meets(p(-10.0, 0.0, 60.0), p(10.0, 0.0, 60.0), 0.0, 115.0, 20.0, 1e-9), "no facet is near");
    assert!(mesh.contains(p(0.0, 0.0, 100.0)), "so the inside test decides");
    let v = fixture(vise());
    let vm = v.mesh().expect("closed");
    assert!(vm.contains(p(0.0, 30.0, 30.0)), "in jaw B");
    assert!(vm.contains(p(0.0, 0.0, 10.0)), "in the base");
    assert!(!vm.contains(p(0.0, 0.0, 30.0)), "in the gap");
    assert!(!vm.contains(p(0.0, 0.0, 80.0)), "above it all");
}

#[test]
fn an_open_or_broken_mesh_keeps_the_box_verdict_by_name() {
    let path = Toolpath::new(vec![feed(p(-60.0, 0.0, 25.0), p(60.0, 0.0, 25.0))]);
    let mut open = vise();
    open.truncate(open.len() - 2); // one jaw face gone
    let f = fixture(open);
    let (boxed, meshed) = judge(&path, &f);
    assert_eq!(boxed, meshed, "an open mesh judges nothing");
    assert_eq!(f.refusal(), Some(MeshError::Open));

    let mut nan = vise();
    nan.push(Triangle::new(p(f64::NAN, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)));
    let g = MeshedFixture::new("Vise2", aabb(&vise()), Arc::<[Triangle]>::from(nan));
    let (boxed, meshed) = judge(&path, &g);
    assert_eq!(boxed, meshed);
    assert_eq!(g.refusal(), Some(MeshError::NonFinite));
    assert_eq!(FixtureMesh::build(&[]).err(), Some(MeshError::Empty));
    let flat = vec![Triangle::new(p(0.0, 0.0, 0.0), p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0))];
    assert_eq!(FixtureMesh::build(&flat).err(), Some(MeshError::Degenerate));
}

#[test]
fn unpaired_boxes_keep_the_box_verdict_and_posed_moves_are_read() {
    let f = fixture(vise());
    // Slice 3: a posed pass down the gap is the mesh's to read too
    // (tests/fixture_mesh_posed.rs holds the posed laws).
    let path = Toolpath::new(vec![feed(p(-60.0, 0.0, 25.0), p(60.0, 0.0, 25.0)).with_axis([0.0, 0.1, 0.995])]);
    let (boxed, meshed) = judge(&path, &f);
    assert!(!boxed.is_empty());
    assert!(meshed.is_empty(), "leaning 6 deg in the gap, the stack clears both jaws: {meshed:?}");

    // A box no fixture owns (the machine table) stays a box.
    let upright = Toolpath::new(vec![feed(p(-60.0, 0.0, 25.0), p(60.0, 0.0, 25.0))]);
    let t = tool();
    let stack = steel_stack(&t, &camcraft_tools::holder_profile::HolderStack::cylinder(&t), None);
    let table = [FixtureBox { min: p(-100.0, -100.0, -1000.0), max: p(100.0, 100.0, 30.0) }];
    let wh = Workholding { turning: &[], fixed: &table, axis: None };
    let steel = MeshSteel { fixtures: std::slice::from_ref(&f), stack: &stack, chord_tol: 0.01 };
    let law = |w: &Workholding<'_>| bands_vs_workholding(&upright, &t, w, None, 0.01);
    assert_eq!(mesh_refined(&upright, &wh, &steel, false, law), law(&wh));
}

/// A closed UV sphere: `lat` x `lon` quads on one shared vertex grid,
/// fans at the poles - 2 * lon * (lat - 1) facets.
fn sphere(r: f64, lat: usize, lon: usize) -> Vec<Triangle> {
    use std::f64::consts::PI;
    let ring = |i: usize| -> Vec<Point3> {
        let th = PI * i as f64 / lat as f64;
        (0..lon)
            .map(|j| {
                let ph = 2.0 * PI * j as f64 / lon as f64;
                p(r * th.sin() * ph.cos(), r * th.sin() * ph.sin(), 50.0 + r * th.cos())
            })
            .collect()
    };
    let (top, bot) = (p(0.0, 0.0, 50.0 + r), p(0.0, 0.0, 50.0 - r));
    let rings: Vec<Vec<Point3>> = (1..lat).map(ring).collect();
    let mut t = Vec::new();
    for j in 0..lon {
        let k = (j + 1) % lon;
        t.push(Triangle::new(top, rings[0][j], rings[0][k]));
        for w in rings.windows(2) {
            t.push(Triangle::new(w[0][j], w[1][j], w[1][k]));
            t.push(Triangle::new(w[0][j], w[1][k], w[0][k]));
        }
        let last = &rings[rings.len() - 1];
        t.push(Triangle::new(bot, last[k], last[j]));
    }
    t
}

fn sphere_run(lat: usize, lon: usize, moves: usize) -> (usize, f64, f64, usize) {
    let tris = sphere(50.0, lat, lon);
    let n = tris.len();
    let f = fixture(tris);
    let built = Instant::now();
    assert!(f.mesh().is_ok(), "the sphere is closed");
    let build_s = built.elapsed().as_secs_f64();
    // A zig-zag across the sphere's box at the equator: passes through
    // the ball convict, passes in the box's corners clear it.
    let mut v = Vec::new();
    for i in 0..moves {
        let y = -48.0 + 96.0 * (i as f64 / moves as f64);
        v.push(feed(p(-60.0, y, 95.0), p(60.0, y + 0.5, 95.0)));
    }
    let path = Toolpath::new(v);
    let t0 = Instant::now();
    let (_, meshed) = judge(&path, &f);
    (n, build_s, t0.elapsed().as_secs_f64(), meshed.len())
}

#[test]
fn a_fine_mesh_judges_in_bounded_time() {
    // 30k facets, 100 passes: the debug-lane guard. The 200k-facet
    // reading is the heavy lane's (below).
    let (n, build, judge_s, hits) = sphere_run(100, 150, 100);
    eprintln!("sphere {n} facets: build {build:.3}s, judge 100 moves {judge_s:.3}s, {hits} hits");
    assert!(hits > 0 && hits < 100, "some passes clip the ball, the corner ones clear: {hits}");
}

#[test]
#[ignore = "heavy lane (release): 200k-facet fixture timing"]
fn a_200k_facet_fixture_judges_in_bounded_time() {
    let (n, build, judge_s, hits) = sphere_run(300, 340, 2000);
    eprintln!("sphere {n} facets: build {build:.3}s, judge 2000 moves {judge_s:.3}s, {hits} hits");
    assert!(n > 200_000 && hits > 0 && hits < 2000);
    // The worst case for the narrow phase: 2000 passes grazing the ball
    // 0.05 mm off its equator, every one inside its box - each sweeps
    // past thousands of facets and must prove every gap.
    let f = fixture(sphere(50.0, 300, 340));
    let c = 54.05 * std::f64::consts::SQRT_2;
    let v: Vec<Move> = (0..2000)
        .map(|i| {
            let z = 50.0 + 0.002 * i as f64;
            feed(p(c - 16.0, 16.0, z), p(16.0, c - 16.0, z))
        })
        .collect();
    let path = Toolpath::new(v);
    let t0 = Instant::now();
    let (boxed, meshed) = judge(&path, &f);
    eprintln!("grazing 2000 moves: {:.3}s, box hits {}, mesh hits {}", t0.elapsed().as_secs_f64(), boxed.len(), meshed.len());
    assert_eq!(boxed.len(), 2000);
    assert!(meshed.is_empty(), "0.05 mm off the ball is clear: {:?}", meshed.first());
}
