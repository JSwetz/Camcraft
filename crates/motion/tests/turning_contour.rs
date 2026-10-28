//! M5.2 stage E turning law tests: the promised-face rides and the
//! multi-pass ladder. What "right" means here: the chamfer and the
//! taper ride the MODEL (the verdict measures against it, so the
//! pass must chase the skin, not the authored numbers), and the
//! authored numbers spend themselves at the door - a face that is
//! not there, an angle that lies, diameters that cannot both be
//! the axis, all refuse by name; the multi-pass ladder stands
//! strictly inward block by block, and one finishing ride is the
//! rough-plus-finish pair move for move.
//!
//! Gate map (M4.4): covers camcraft_motion::turning (contour
//! kernels, prof::setup refusals through them, the spring-pass
//! ride law) with this crate's own revolve fixtures.
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::MoveKind;
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::turning::profile::Frame;
use camcraft_motion::turning::{
    chamfer, multi_pass, profile_finish, profile_rough, taper, ChamferParams, MultiPassParams,
    ProfileFinishParams, ProfileRoughParams, TaperParams,
};

const TAU: f64 = std::f64::consts::TAU;
const NOSE: f64 = 0.8;
const TOL: f64 = 0.05;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The closed-solid revolve the turning suites share.
fn revolve(n_circ: usize, meridian: &[(f64, f64)]) -> Vec<Triangle> {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / n_circ as f64;
        p3(r * th.cos(), r * th.sin(), z)
    };
    let mut t = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..n_circ {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                t.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                t.push(Triangle::new(a1, a2, b2));
            } else {
                t.push(Triangle::new(a1, a2, b2));
                t.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    t
}

/// r 10 wall, chamfered at the bottom end: the face runs from
/// (r 10, z -14) to (r 8, z -16) - a true 45-degree, width-2 cone.
fn chamfered() -> Vec<Triangle> {
    revolve(16, &[(0.0, -16.0), (8.0, -16.0), (10.0, -14.0), (10.0, 40.0), (0.0, 40.0)])
}

/// The same bar with no chamfer at all: a square corner at z -16.
fn wall() -> Vec<Triangle> {
    revolve(16, &[(0.0, -16.0), (10.0, -16.0), (10.0, 40.0), (0.0, 40.0)])
}

/// A full taper: r 10 at z -16 to r 8 at z 40 (dia 20 to 16).
fn tapered() -> Vec<Triangle> {
    revolve(16, &[(0.0, -16.0), (10.0, -16.0), (8.0, 40.0), (0.0, 40.0)])
}

/// The stepped bar of the profile suite: wall r 6 to z 2, neck r 5
/// to z 16. Its 1.0 of standing shell gives the multi-pass ladder
/// a small but real rung count at the seed stepdown.
fn stepped() -> Vec<Triangle> {
    revolve(16, &[(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)])
}

fn sr(f: &Frame, q: Point3) -> (f64, f64) {
    f.sr(q)
}

fn ch(angle_deg: f64, bottom_z: f64, top_z: f64) -> ChamferParams {
    ChamferParams {
        clearance_z: 60.0,
        top_z,
        bottom_z,
        angle_deg,
        feed: 200.0,
        plunge_feed: 50.0,
        nose_radius: NOSE,
        tolerance: TOL,
    }
}

fn tp(bottom_dia: f64, top_dia: f64) -> TaperParams {
    TaperParams {
        clearance_z: 60.0,
        top_z: 40.0,
        bottom_z: -16.0,
        bottom_dia,
        top_dia,
        feed: 200.0,
        plunge_feed: 50.0,
        nose_radius: NOSE,
        tolerance: TOL,
    }
}

fn mp(passes: u32, stepdown: f64, stock: f64) -> MultiPassParams {
    MultiPassParams {
        clearance_z: 30.0,
        top_z: 16.0,
        bottom_z: -16.0,
        stepdown,
        stock_to_leave: stock,
        passes,
        feed: 200.0,
        plunge_feed: 50.0,
        nose_radius: NOSE,
        tolerance: TOL,
    }
}

/// The feeds of a path: every non-rapid move.
fn feeds(t: &camcraft_motion::cl::Toolpath) -> Vec<&camcraft_motion::cl::Move> {
    t.moves.iter().filter(|m| m.kind != MoveKind::Rapid).collect()
}

// ------------------------------------------------------------ chamfer

#[test]
fn a_modeled_face_rides_plain() {
    // The 45-degree cone IS in the model: the pass traces it, from
    // the corner end up, and every feed point stands a nose off
    // the skin it is riding.
    let t = chamfer(&chamfered(), &ch(45.0, -16.0, -14.0)).expect("face is there");
    let f = Frame::about(&chamfered()).expect("plumb");
    let fs = feeds(&t);
    assert!(fs.len() >= 2, "a cone face chords to more than a point");
    for m in &fs {
        let (s, r) = sr(&f, m.to);
        assert!(
            // On a 45-degree face the nose center stands d*sqrt(2)
            // out radially - the ride's honest reach past the skin.
            (-16.0 - NOSE - TOL..=-14.0 + NOSE + TOL).contains(&s)
                && (8.0 - TOL..=10.0 + NOSE * 2.0f64.sqrt() + TOL).contains(&r),
            "the ride stands off the face it was told to ride: {r} at {s}"
        );
    }
    assert!(t.is_chained(), "the chain holds");
}

#[test]
fn a_square_corner_is_refused_by_name() {
    // The window promises a chamfer; the model is a cylinder to
    // the corner. The pass would chase a cone that is not there,
    // so the door says so instead.
    let e = chamfer(&wall(), &ch(45.0, -16.0, -14.0)).expect_err("no chamfer in a wall");
    let m = format!("{e:?}");
    assert!(
        m.contains("not a 45.0-degree chamfer") && m.contains("MODELED chamfer"),
        "{m}"
    );
}

#[test]
fn a_lying_angle_is_refused() {
    // The face IS there, but 45 authored against a 45 model would
    // pass; a 30 reading of the same face wanders off the cone by
    // most of a millimetre and cannot.
    let e = chamfer(&chamfered(), &ch(30.0, -16.0, -14.0)).expect_err("30 is not 45");
    let m = format!("{e:?}");
    assert!(m.contains("not a 30.0-degree chamfer"), "{m}");
}

#[test]
fn zero_and_right_angles_are_not_chamfers() {
    for (a, word) in [(0.0, "cylinder"), (90.0, "facing is its own kind")] {
        let e = chamfer(&chamfered(), &ch(a, -16.0, -14.0)).expect_err("not a cone");
        let m = format!("{e:?}");
        assert!(m.contains(word), "{m}");
    }
}

#[test]
fn an_inboard_corner_rides_the_other_way() {
    // Wall r 6 with a relief cone up to the neck's shoulder: the
    // face rises toward the window top, and the sign comes from
    // the model, not the parameters. (6, 2) to (5, 1) is 45 deg.
    let relief = revolve(
        16,
        &[
            (0.0, -16.0),
            (6.0, -16.0),
            (6.0, 1.0),
            (5.0, 2.0),
            (5.0, 16.0),
            (0.0, 16.0),
        ],
    );
    let t = chamfer(&relief, &ch(45.0, 1.0, 2.0)).expect("rising face");
    assert!(feeds(&t).len() >= 2);
}

// -------------------------------------------------------------- taper

#[test]
fn the_authored_diameters_ride_their_cone() {
    let t = taper(&tapered(), &tp(20.0, 16.0)).expect("the cone is the model");
    let f = Frame::about(&tapered()).expect("plumb");
    let fs = feeds(&t);
    let (s0, r0) = sr(&f, fs[0].to);
    let (s1, r1) = sr(&f, fs[fs.len() - 1].to);
    assert!(s0 < s1, "the ride walks bottom-up: {s0} then {s1}");
    assert!((r0 - (10.0 + NOSE)).abs() <= 2.0 * TOL + 1e-9, "start: {r0}");
    assert!((r1 - (8.0 + NOSE)).abs() <= 2.0 * TOL + 1e-9, "end: {r1}");
}

#[test]
fn a_straight_taper_is_allowed_and_rides() {
    // Equal diameters state a cylinder: a legal promise over a
    // cylindrical model, and the ride is a skim of the skin.
    let t = taper(&wall(), &tp(20.0, 20.0)).expect("straight is turnable");
    assert!(feeds(&t).len() >= 2);
}

#[test]
fn a_cone_the_model_does_not_have_refuses() {
    let e = taper(&wall(), &tp(20.0, 16.0)).expect_err("the wall has no cone");
    let m = format!("{e:?}");
    assert!(m.contains("not the authored cone") && m.contains("MODELED taper"), "{m}");
}

#[test]
fn nonsense_diameters_refuse() {
    let e = taper(&tapered(), &tp(-20.0, 16.0)).expect_err("no negative sizes");
    assert!(format!("{e:?}").contains("not a size"), "{e:?}");
    let e = taper(&tapered(), &tp(0.0, 0.0)).expect_err("no axis-only part");
    assert!(format!("{e:?}").contains("a line, not a part"), "{e:?}");
}

// --------------------------------------------------------- multi-pass

#[test]
fn one_pass_is_the_rough_and_finish_pair() {
    // passes = 1 states the classic program: the rung ladder down
    // to the leave, one ride of the profile. Move for move, that
    // is profile_rough then profile_finish chained.
    let model = stepped();
    let m = multi_pass(&model, &mp(1, 0.5, 0.25)).expect("the ladder runs");
    let r = profile_rough(
        &model,
        &ProfileRoughParams {
            clearance_z: 30.0,
            top_z: 16.0,
            bottom_z: -16.0,
            stepdown: 0.5,
            stock_to_leave: 0.25,
            feed: 200.0,
            plunge_feed: 50.0,
            nose_radius: NOSE,
            tolerance: TOL,
        },
    )
    .expect("rough");
    let fi = profile_finish(
        &model,
        &ProfileFinishParams {
            clearance_z: 30.0,
            top_z: 16.0,
            bottom_z: -16.0,
            feed: 200.0,
            plunge_feed: 50.0,
            nose_radius: NOSE,
            tolerance: TOL,
        },
    )
    .expect("finish");
    // Chained, the pair shares one closing rapid.
    assert_eq!(m.moves.len(), r.moves.len() + fi.moves.len() - 1);
    assert_eq!(feeds(&m).len(), feeds(&r).len() + feeds(&fi).len());
}

#[test]
fn the_spring_rides_stand_strictly_inward() {
    // Each finishing ride's distance-to-part is the last block's
    // shell halved onward: the plunge of block k+1 must sit inside
    // the plunge of block k, and the final ride must land on the
    // nose alone.
    let model = wall();
    let t = multi_pass(&model, &mp(4, 1.5, 1.2)).expect("four rides");
    let f = Frame::about(&model).expect("plumb");
    // A block opens where a feed follows a rapid; its reach is the
    // largest radius its feeds stand at. On this cylinder the
    // blocks must read nose + stock, then the leave halved onward,
    // then the nose alone.
    let mut maxes: Vec<f64> = Vec::new();
    let mut prev_rapid = true;
    for m in &t.moves {
        let r = f.sr(m.to).1;
        if m.kind == MoveKind::Rapid {
            prev_rapid = true;
        } else {
            if prev_rapid {
                maxes.push(r);
            } else if let Some(last) = maxes.last_mut() {
                *last = last.max(r);
            }
            prev_rapid = false;
        }
    }
    // e_max 0 at this stock: the ladder is a single rung, so five
    // blocks - rung, three springs, finish.
    assert_eq!(maxes.len(), 5, "block count: {maxes:?}");
    for w in maxes.windows(2) {
        assert!(w[1] < w[0] - 0.1, "the spring law violated: {maxes:?}");
    }
    assert!(
        (maxes[4] - (10.0 + NOSE)).abs() <= 2.0 * TOL,
        "the last ride is the profile: {:?}",
        maxes[4]
    );
}

#[test]
fn the_ladder_doors_are_the_profile_roughs() {
    let e = multi_pass(&stepped(), &mp(0, 0.5, 0.25)).expect_err("no rides");
    assert!(format!("{e:?}").contains("no finishing ride"), "{e:?}");
    let e = multi_pass(&stepped(), &mp(65, 0.5, 0.25)).expect_err("a polishing sequence");
    assert!(format!("{e:?}").contains("polishing sequence"), "{e:?}");
    let e = multi_pass(&stepped(), &mp(2, 3.0, 0.25)).expect_err("past the nose reach");
    assert!(format!("{e:?}").contains("nose's reach"), "{e:?}");
    let e = multi_pass(&stepped(), &mp(2, 0.5, -0.1)).expect_err("negative leave");
    assert!(format!("{e:?}").contains("stock to remove"), "{e:?}");
    let e = multi_pass(&stepped(), &mp(2, -1.0, 0.25)).expect_err("no negative stepdown");
    assert!(format!("{e:?}").contains("positive stepdown"), "{e:?}");
}

#[test]
fn the_setup_doors_answer_through_all_three() {
    // A point nose, an empty window: prof::setup's law, one door
    // for every kind that stands on it.
    let e = chamfer(&wall(), &ChamferParams {
        nose_radius: 0.0,
        ..ch(45.0, -16.0, -14.0)
    })
    .expect_err("a point cannot ride");
    assert!(format!("{e:?}").contains("an arc, not a line"), "{e:?}");
    let e = taper(&wall(), &TaperParams {
        bottom_z: 40.0,
        top_z: -16.0,
        ..tp(20.0, 20.0)
    })
    .expect_err("an upside-down window");
    assert!(format!("{e:?}").contains("nothing to turn"), "{e:?}");
}
