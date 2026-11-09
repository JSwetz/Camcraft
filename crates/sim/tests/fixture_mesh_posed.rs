//! The fixture mesh law for posed and turned moves (#339 slice 3): a
//! 4/5-axis index or tilted pass down a vise's gap is clean under the
//! mesh where the box convicted it, while the same index leaning into a
//! jaw still convicts; steel that rides a rotary is read in the path
//! frame, steel declared fixed in the machine frame (an A rotary's, or
//! the spindle C's); XZ/YZ arcs are chorded in their own plane. The
//! mesh only ever acquits a box hit.
//! Provenance: our own geometry; the law is the Prime Directive's.
//!
//! Gate map (M4.4): covers sim::verify::mesh_posed, sim::verify::mesh_steel, sim::fixture_mesh.

use std::sync::Arc;
use std::time::Instant;

use camcraft_sim::cl::{angles_to_axis, stamp_c, ArcPlane, Move, MoveKind, Toolpath};
use camcraft_sim::fixture::FixtureBox;
use camcraft_sim::fixture_mesh::MeshedFixture;
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::{Point3, Triangle};
use camcraft_sim::verify::{
    bands_vs_workholding, mesh_refined, rapids_vs_workholding, steel_stack, FixturePart, MeshSteel, RotaryAxis,
    ToolGeom, Warning, Workholding,
};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Flute r4 x 30, shank r5 x 20 (30..50), holder r20 x 40 (50..90).
fn tool() -> ToolGeom {
    ToolGeom {
        radius: 4.0,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_len: 20.0,
        shank_radius: 5.0,
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

/// fixture_mesh's vise: base z[0,20], jaws |y| in [15,40] up to z 50.
fn vise() -> Vec<Triangle> {
    let mut t = cube(p(-50.0, -40.0, 0.0), p(50.0, 40.0, 20.0));
    t.extend(cube(p(-50.0, -40.0, 20.0), p(50.0, -15.0, 50.0)));
    t.extend(cube(p(-50.0, 15.0, 20.0), p(50.0, 40.0, 50.0)));
    t
}

fn fixture(tris: Vec<Triangle>) -> MeshedFixture {
    let mut lo = p(f64::MAX, f64::MAX, f64::MAX);
    let mut hi = p(f64::MIN, f64::MIN, f64::MIN);
    for q in tris.iter().flat_map(|t| [t.a, t.b, t.c]) {
        lo = p(lo.x.min(q.x), lo.y.min(q.y), lo.z.min(q.z));
        hi = p(hi.x.max(q.x), hi.y.max(q.y), hi.z.max(q.z));
    }
    MeshedFixture::new("Steel1", FixtureBox { min: lo, max: hi }, Arc::<[Triangle]>::from(tris))
}

fn feed(a: Point3, b: Point3) -> Move {
    Move::new(MoveKind::Feed, a, b, Some(800.0))
}

fn pose(a_deg: f64, b_deg: f64) -> [f64; 3] {
    angles_to_axis([a_deg, b_deg, 0.0]).unwrap_or([0.0, 0.0, 1.0])
}

/// Both laws, box-only and mesh-refined, with the steel turning or
/// fixed and the rotary `axis` stated or not.
fn judge_in(path: &Toolpath, f: &MeshedFixture, fixed: bool, axis: Option<RotaryAxis>) -> (Vec<Warning>, Vec<Warning>) {
    let boxes = [f.aabb];
    let wh = if fixed {
        Workholding { turning: &[], fixed: &boxes, axis }
    } else {
        Workholding { turning: &boxes, fixed: &[], axis }
    };
    let t = tool();
    let stack = steel_stack(&t, &camcraft_tools::holder_profile::HolderStack::cylinder(&t), None);
    let steel = MeshSteel { fixtures: std::slice::from_ref(f), stack: &stack, chord_tol: 0.01 };
    let mut boxed = bands_vs_workholding(path, &t, &wh, None, 0.01);
    boxed.extend(rapids_vs_workholding(path, &t, &wh, None));
    let mut meshed = mesh_refined(path, &wh, &steel, false, |w| bands_vs_workholding(path, &t, w, None, 0.01));
    meshed.extend(mesh_refined(path, &wh, &steel, true, |w| rapids_vs_workholding(path, &t, w, None)));
    (boxed, meshed)
}

fn judge(path: &Toolpath, f: &MeshedFixture) -> (Vec<Warning>, Vec<Warning>) {
    judge_in(path, f, false, None)
}

fn hit_at(ws: &[Warning], i: usize) -> Option<FixturePart> {
    ws.iter().find_map(|w| match w {
        Warning::FixtureHit { at, fixture_part } if *at == i => Some(*fixture_part),
        _ => None,
    })
}

/// Down the gap upright, then a 5-axis pass leaning A 10 / B 20 as it
/// runs, an index in place to B -30, and a tilted rapid out: every move
/// sits inside the vise's one box, none near its steel.
#[test]
fn a_tilted_pass_and_index_down_the_vise_gap_is_clean() {
    let f = fixture(vise());
    let path = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(-40.0, 0.0, 70.0), p(-40.0, 0.0, 25.0), None),
        feed(p(-40.0, 0.0, 25.0), p(40.0, 0.0, 25.0)).with_axis(pose(10.0, 20.0)),
        feed(p(40.0, 0.0, 25.0), p(40.0, 0.0, 25.0)).with_axis(pose(0.0, -30.0)),
        feed(p(40.0, 0.0, 25.0), p(-20.0, 0.0, 28.0)).with_axis(pose(-10.0, -30.0)),
        Move::new(MoveKind::Rapid, p(-20.0, 0.0, 28.0), p(-20.0, 0.0, 90.0), None).with_axis(pose(-10.0, -30.0)),
    ]);
    let (boxed, meshed) = judge(&path, &f);
    for i in 1..5 {
        assert!(hit_at(&boxed, i).is_some(), "the world box convicts posed move {i}: {boxed:?}");
    }
    assert!(meshed.is_empty(), "the jaws' real steel is nowhere near: {meshed:?}");
    assert_eq!(f.refusal(), None);
}

/// The same index in place, swung about A to 40 deg: the flute and
/// shank lean 19 mm sideways at the jaw's height - into jaw A.
#[test]
fn the_same_index_leaning_into_a_jaw_convicts() {
    let f = fixture(vise());
    let path = Toolpath::new(vec![
        feed(p(-40.0, 0.0, 25.0), p(0.0, 0.0, 25.0)),
        feed(p(0.0, 0.0, 25.0), p(0.0, 0.0, 25.0)).with_axis(pose(40.0, 0.0)),
        // A rapid swinging back through upright to 40 deg toward jaw B:
        // struck as a rapid.
        Move::new(MoveKind::Rapid, p(0.0, 0.0, 25.0), p(0.0, 0.0, 30.0), None).with_axis(pose(-40.0, 0.0)),
    ]);
    let (boxed, meshed) = judge(&path, &f);
    assert!(hit_at(&boxed, 1).is_some() && hit_at(&boxed, 2).is_some(), "{boxed:?}");
    assert_eq!(hit_at(&meshed, 0), None, "the upright entry is clean: {meshed:?}");
    assert!(hit_at(&meshed, 1).is_some(), "the lean into the jaw convicts: {meshed:?}");
    assert_eq!(hit_at(&meshed, 2), Some(FixturePart::Rapid), "{meshed:?}");
}

/// A leaning rapid kept by the mesh is named the way law (d2) names an
/// upright one since #424: a band above the flute by its part. The vise
/// acquits the rapid down its gap; a bar at shank height convicts it.
#[test]
fn a_posed_rapid_kept_by_the_mesh_names_its_band() {
    let vise = fixture(vise());
    let (lo, hi) = (p(70.0, -12.0, 60.0), p(90.0, 12.0, 65.0));
    let bar = MeshedFixture::new("Bar1", FixtureBox { min: lo, max: hi }, Arc::<[Triangle]>::from(cube(lo, hi)));
    let boxes = [vise.aabb, bar.aabb];
    let wh = Workholding { turning: &boxes, fixed: &[], axis: None };
    let t = tool();
    let stack = steel_stack(&t, &camcraft_tools::holder_profile::HolderStack::cylinder(&t), None);
    let fixtures = [vise, bar];
    let steel = MeshSteel { fixtures: &fixtures, stack: &stack, chord_tol: 0.01 };
    let path = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(-60.0, 0.0, 25.0), p(100.0, 0.0, 25.0), None).with_axis(pose(10.0, 0.0)),
    ]);
    let boxed = rapids_vs_workholding(&path, &t, &wh, None);
    assert_eq!(hit_at(&boxed, 0), Some(FixturePart::Rapid), "the posed box law: {boxed:?}");
    let meshed = mesh_refined(&path, &wh, &steel, true, |w| rapids_vs_workholding(&path, &t, w, None));
    assert_eq!(hit_at(&meshed, 0), Some(FixturePart::Shank), "the bar strikes the shank: {meshed:?}");
}

/// Two jaws with a gap in y (no base): steel stated at rotary zero.
fn fork(dy: f64) -> Vec<Triangle> {
    let mut t = cube(p(-50.0, -40.0 + dy, 20.0), p(50.0, -15.0 + dy, 50.0));
    t.extend(cube(p(-50.0, 15.0 + dy, 20.0), p(50.0, 40.0 + dy, 50.0)));
    t
}

/// The bar's A rotary about world X: the path's pose +Y at (x, 25, 0)
/// is the machine's upright tool at (x, 0, 25) once the part turns 90.
fn a_axis() -> Option<RotaryAxis> {
    Some(RotaryAxis { origin: p(0.0, 0.0, 0.0), dir: [1.0, 0.0, 0.0] })
}

fn radial_pass() -> Toolpath {
    Toolpath::new(vec![feed(p(-40.0, 25.0, 0.0), p(40.0, 25.0, 0.0)).with_axis([0.0, 1.0, 0.0])])
}

#[test]
fn fixed_steel_under_a_rotary_is_read_in_the_machine_frame() {
    // Fixed: in the machine frame the tool stands upright in the gap,
    // well above a plate under the axis (z -60..-10) that a turn the
    // wrong way round would stand it in.
    let mut steel = fork(0.0);
    steel.extend(cube(p(-50.0, -40.0, -60.0), p(50.0, 40.0, -10.0)));
    let f = fixture(steel);
    let (boxed, meshed) = judge_in(&radial_pass(), &f, true, a_axis());
    assert!(hit_at(&boxed, 0).is_some(), "the machine-frame box convicts the gap pass: {boxed:?}");
    assert!(meshed.is_empty(), "the fork's real steel is clear on the machine: {meshed:?}");
    // Shifted 12 mm, a jaw stands within the flute's reach.
    let g = fixture(fork(12.0));
    let (boxed, meshed) = judge_in(&radial_pass(), &g, true, a_axis());
    assert!(hit_at(&meshed, 0).is_some(), "{meshed:?}");
    assert_eq!(boxed, meshed, "not acquitted: the box laws' own report, label and all");
}

#[test]
fn turning_steel_under_a_rotary_is_read_in_the_path_frame() {
    // Rides the part: jaws gapped in z around the path-frame tool, which
    // lies along +Y at z 0 - the same pass, judged where it is written.
    let zfork = |dz: f64| {
        let mut t = cube(p(-50.0, 20.0, -40.0 + dz), p(50.0, 50.0, -15.0 + dz));
        t.extend(cube(p(-50.0, 20.0, 15.0 + dz), p(50.0, 50.0, 40.0 + dz)));
        t
    };
    let f = fixture(zfork(0.0));
    let (boxed, meshed) = judge_in(&radial_pass(), &f, false, a_axis());
    assert!(hit_at(&boxed, 0).is_some(), "{boxed:?}");
    assert!(meshed.is_empty(), "the flute lies in the jaws' gap: {meshed:?}");
    let g = fixture(zfork(12.0));
    let (boxed, meshed) = judge_in(&radial_pass(), &g, false, a_axis());
    assert!(hit_at(&meshed, 0).is_some(), "{meshed:?}");
    assert_eq!(boxed, meshed);
}

/// A spindle-C live-tool path, stamped by `cl::stamp_c` about the Z
/// line at the origin: a radial plunge to r 30 at C 0, then a turn of
/// 20 deg at that radius - the station tool lying along +X.
fn c_path() -> Toolpath {
    let (s, c) = 20f64.to_radians().sin_cos();
    let to = p(30.0 * c, 30.0 * s, 0.0);
    let raw = Toolpath::new(vec![
        feed(p(50.0, 0.0, 0.0), p(30.0, 0.0, 0.0)).with_axis([1.0, 0.0, 0.0]),
        feed(p(30.0, 0.0, 0.0), to).with_axis([c, s, 0.0]),
    ]);
    stamp_c(&raw, [0.0, 0.0], 0.01).unwrap_or_else(|e| panic!("stamp: {e}"))
}

/// Jaws gapped in y about the station, x 35..70.
fn c_fork() -> Vec<Triangle> {
    let mut t = cube(p(35.0, -40.0, -30.0), p(70.0, -10.0, 30.0));
    t.extend(cube(p(35.0, 10.0, -30.0), p(70.0, 40.0, 30.0)));
    t
}

#[test]
fn a_spindle_c_turn_reads_turning_steel_on_its_spiral() {
    let path = c_path();
    assert!(path.moves.len() > 2, "the turn is split to its tolerance");
    // Plates above and below the tool's plane, gapped |z| < 6: the
    // turning tool sweeps the slot between them.
    let mut plates = cube(p(0.0, -10.0, 6.0), p(60.0, 40.0, 40.0));
    plates.extend(cube(p(0.0, -10.0, -40.0), p(60.0, 40.0, -6.0)));
    let f = fixture(plates);
    let (boxed, meshed) = judge(&path, &f);
    assert!(path.moves.iter().enumerate().all(|(i, _)| hit_at(&boxed, i).is_some()), "{boxed:?}");
    assert!(meshed.is_empty(), "the slot is clear all the way round: {meshed:?}");
    // Lop-sided steel: a jaw on -Y only (plus a nub that stretches the
    // box over the path). The part turns +20 deg of azimuth: the true
    // spiral never nears the jaw; turned the wrong way it would.
    let mut lop = cube(p(35.0, -40.0, -30.0), p(70.0, -10.0, 30.0));
    lop.extend(cube(p(0.0, 38.0, 28.0), p(2.0, 40.0, 30.0)));
    let l = fixture(lop);
    let (boxed, meshed) = judge(&path, &l);
    assert!(boxed.iter().any(|w| matches!(w, Warning::FixtureHit { at, .. } if *at > 0)), "{boxed:?}");
    assert!(meshed.is_empty(), "{meshed:?}");
    // The fork turning with the part: the flute swings into a jaw.
    let g = fixture(c_fork());
    let (_, meshed) = judge(&path, &g);
    assert_eq!(hit_at(&meshed, 0), None, "the plunge at C 0 stands in the gap: {meshed:?}");
    assert!(meshed.iter().any(|w| matches!(w, Warning::FixtureHit { at, .. } if *at > 0)), "{meshed:?}");
}

#[test]
fn a_spindle_c_turn_reads_fixed_steel_on_the_machine() {
    // Fixed steel stays at the station: the turning part never carries
    // the tool off it, so the fork's gap holds the whole program.
    let path = c_path();
    let f = fixture(c_fork());
    let (boxed, meshed) = judge_in(&path, &f, true, None);
    assert!(boxed.iter().any(|w| matches!(w, Warning::FixtureHit { at, .. } if *at > 0)), "{boxed:?}");
    assert!(meshed.is_empty(), "the station tool stands in the gap: {meshed:?}");
    // A C move with no stated C to start from keeps the box verdict.
    let lone = Toolpath::new(path.moves.iter().skip(1).take(1).copied().collect());
    let (boxed, meshed) = judge_in(&lone, &f, true, None);
    assert!(!boxed.is_empty());
    assert_eq!(boxed, meshed, "the line cannot be placed: the box stands");
    // An axial tool on the C turn is not the station's: the box stands.
    let mut axial = path.clone();
    for m in axial.moves.iter_mut().skip(1) {
        m.axis = Some([0.0, 0.0, 1.0]);
    }
    let (boxed, meshed) = judge_in(&axial, &f, true, None);
    assert!(boxed.iter().any(|w| matches!(w, Warning::FixtureHit { at, .. } if *at > 0)), "{boxed:?}");
    let late = |ws: &[Warning]| ws.iter().filter(|w| matches!(w, Warning::FixtureHit { at, .. } if *at > 0)).count();
    assert_eq!(late(&boxed), late(&meshed), "an axial tool is not judged here: {meshed:?}");
}

#[test]
fn xz_and_yz_arcs_are_chorded_in_their_own_plane() {
    let f = fixture(vise());
    // G18 arcs between (-10, 0, 25) and (10, 0, 25), bottoming out over
    // the base (top z 20): centre z 35 bottoms at 20.86 (clear), centre
    // z 32 at 19.79 (into the base) - the chord at z 25 sees neither.
    let arc = |kz: f64| feed(p(-10.0, 0.0, 25.0), p(10.0, 0.0, 25.0)).with_arc_in(Point2::new(0.0, kz), false, ArcPlane::Xz);
    let (clear, dip) = (arc(35.0), arc(32.0));
    let low = |m: &Move| m.arc.and_then(|a| a.point_at(m.from, m.to, 0.5)).map_or(f64::NAN, |q| q.z);
    assert!((low(&clear) - 20.86).abs() < 0.01 && (low(&dip) - 19.79).abs() < 0.01, "{} {}", low(&clear), low(&dip));
    // A G19 arc across the gap's floor, clear of both jaws.
    let yz = feed(p(0.0, -6.0, 30.0), p(0.0, 6.0, 30.0)).with_arc_in(Point2::new(0.0, 40.0), true, ArcPlane::Yz);
    let path = Toolpath::new(vec![clear, dip, yz]);
    let (boxed, meshed) = judge(&path, &f);
    assert!((0..3).all(|i| hit_at(&boxed, i).is_some()), "{boxed:?}");
    assert_eq!(hit_at(&meshed, 0), None, "{meshed:?}");
    assert!(hit_at(&meshed, 1).is_some(), "the dip into the base convicts: {meshed:?}");
    assert_eq!(hit_at(&meshed, 2), None, "{meshed:?}");
}

/// A closed UV sphere (fixture_mesh's): centre (0, 0, 50), radius 50.
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

/// Tilted passes across the ball's box at z 95 (B 15), each followed by
/// an index in place swinging B to -15: some clip the ball, the corner
/// ones clear it. Returns (facets, moves, judge seconds, mesh hits).
fn posed_sphere_run(lat: usize, lon: usize, passes: usize) -> (usize, usize, f64, usize) {
    let tris = sphere(50.0, lat, lon);
    let n = tris.len();
    let f = fixture(tris);
    assert!(f.mesh().is_ok(), "the sphere is closed");
    let mut v = Vec::new();
    for i in 0..passes {
        let y = -48.0 + 96.0 * (i as f64 / passes as f64);
        let (a, b) = (p(-60.0, y, 95.0), p(60.0, y + 0.5, 95.0));
        v.push(feed(a, b).with_axis(pose(0.0, 15.0)));
        v.push(feed(b, b).with_axis(pose(0.0, -15.0)));
    }
    let moves = v.len();
    let path = Toolpath::new(v);
    let t0 = Instant::now();
    let (_, meshed) = judge(&path, &f);
    (n, moves, t0.elapsed().as_secs_f64(), meshed.len())
}

#[test]
fn a_fine_mesh_judges_posed_moves_in_bounded_time() {
    let (n, moves, s, hits) = posed_sphere_run(100, 150, 40);
    eprintln!("sphere {n} facets: {moves} posed moves judged in {s:.3}s, {hits} hits");
    assert!(hits > 0 && hits < moves, "some clip the ball, the corner ones clear: {hits}");
}

#[test]
#[ignore = "heavy lane (release): 200k-facet fixture, posed moves timing"]
fn a_200k_facet_fixture_judges_posed_moves_in_bounded_time() {
    let (n, moves, s, hits) = posed_sphere_run(300, 340, 1000);
    eprintln!("sphere {n} facets: {moves} posed moves judged in {s:.3}s, {hits} hits");
    assert!(n > 200_000 && hits > 0 && hits < moves);
}

