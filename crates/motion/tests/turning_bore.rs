//! M5.2 stage F boring law tests: the profile ride read through the
//! mirror. What "right" means here: the nose-center curve stands BELOW
//! every piece of the hole's inner wall - min-of-claims, the twin of
//! stage B's theorem - so the swept ball never enters the steel, and
//! the doors ask the model itself whether a hole stands open before
//! any path exists to be wrong. A plain bar is refused as EMPTY (no
//! hole to bore), a window standing in steel is refused as INVALID,
//! a floor the nose cannot pass without biting is refused by name, and
//! a bar too big for its own hole is refused with both numbers stated.
//!
//! Gate map (M4.4): covers camcraft_motion::turning::bore (the kernel
//! doors, the axis-parity vote, the min-fold wall) and
//! camcraft_motion::turning::offset::profile_offset_in with this
//! crate's own revolve fixtures. Lane shape (M4.4): all tests here run
//! in seconds.

use camcraft_motion::cl::MoveKind;
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::turning::{bore, BoreParams};

const TAU: f64 = std::f64::consts::TAU;
/// The bar this suite bores: OD 10, a wall at radius 6.
const OD: f64 = 10.0;
const WALL: f64 = 6.0;
const D: f64 = 0.4;

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

/// The plain bar: a smooth skin and no hole anywhere.
fn bar() -> Vec<Triangle> {
    revolve(16, &[(0.0, -50.0), (OD, -50.0), (OD, 50.0), (0.0, 50.0)])
}

/// The blind hole: air on the axis from z 50 down to `floor_z`, steel
/// below it out to the OD.
fn blind(floor_z: f64) -> Vec<Triangle> {
    revolve(
        16,
        &[
            (0.0, -50.0),
            (OD, -50.0),
            (OD, 50.0),
            (WALL, 50.0),
            (WALL, floor_z),
            (0.0, floor_z),
        ],
    )
}

/// The through hole: a tube, the wall open at both ends. The meridian
/// closes the annulus by repeating the first point - `revolve`'s
/// windows(2) never sweeps an implied closing edge, and the parity
/// vote needs a closed surface.
fn through() -> Vec<Triangle> {
    revolve(
        16,
        &[
            (OD, -50.0),
            (OD, 50.0),
            (WALL, 50.0),
            (WALL, -50.0),
            (OD, -50.0),
        ],
    )
}

/// Two coaxial tubes with an air gap between them: no wall, and no
/// continuous turned body either.
fn split_tube() -> Vec<Triangle> {
    let mut t = revolve(
        16,
        &[(OD, -50.0), (OD, -10.0), (WALL, -10.0), (WALL, -50.0), (OD, -50.0)],
    );
    t.extend(revolve(
        16,
        &[(OD, 10.0), (OD, 50.0), (WALL, 50.0), (WALL, 10.0), (OD, 10.0)],
    ));
    t
}

fn params(bottom_z: f64, top_z: f64) -> BoreParams {
    BoreParams {
        tolerance: 0.05,
        clearance_z: 70.0,
        top_z,
        bottom_z,
        feed: 120.0,
        plunge_feed: 40.0,
        nose_radius: D,
        head_d: 0.0,
        shank_d: 0.0,
    }
}

/// Every feed point's radius, in path order.
fn centers(path: &camcraft_motion::cl::Toolpath) -> Vec<(f64, f64)> {
    path.moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .map(|m| (m.to.z, (m.to.x * m.to.x + m.to.y * m.to.y).sqrt()))
        .collect()
}

/// The tessellated wall's own innermost radius mid-band: the chords
/// between the 16-gon rings, 11.25 degrees off either.
fn chord_floor() -> f64 {
    WALL * (std::f64::consts::FRAC_PI_8).cos()
}

#[test]
fn the_bar_rides_the_wall() {
    let path = bore(&blind(-20.0), &params(-10.0, 40.0)).expect("an open hole in the window bores");
    let cs = centers(&path);
    assert!(cs.len() > 4, "the ride is chord-settled, not two endpoints");
    // Below the wall everywhere: gouge-free by construction - even the
    // tessellation chord leaves the nose's ball out of every triangle.
    for (s, r) in &cs {
        assert!(*r <= WALL - D + 1e-9, "center at station {s:.3} stands {r:.4} - inside the wall");
        assert!(*r >= chord_floor() - D - 0.05, "center at {s:.3} gave the hole away at {r:.4}");
    }
    // The window is spent: the ride runs bottom Z to top Z.
    assert!(cs.first().unwrap().0 <= -10.0 + 1e-9);
    assert!(cs.last().unwrap().0 >= 40.0 - 1e-9);
    // The kinematics: rapid down the axis, one radial lead-out at the
    // plunge feed, the ride at the wall feed, then rapids off.
    assert_eq!(path.moves[0].kind, MoveKind::Rapid);
    let lead = &path.moves[1];
    assert_eq!(lead.kind, MoveKind::Feed);
    assert_eq!(lead.feed, Some(40.0));
    assert!((lead.from.x * lead.from.x + lead.from.y * lead.from.y).sqrt() < 0.1, "the lead starts on the axis");
    let ride = &path.moves[2];
    assert_eq!(ride.kind, MoveKind::Feed);
    assert_eq!(ride.feed, Some(120.0));
    assert_eq!(path.moves.last().unwrap().kind, MoveKind::Rapid);
    // The retract stands back ON the axis - the bar leaves through
    // the hole's own air.
    let off = &path.moves[path.moves.len() - 2];
    assert_eq!(off.kind, MoveKind::Rapid);
    assert!((off.to.x * off.to.x + off.to.y * off.to.y).sqrt() < 1e-9);
}

#[test]
fn a_stated_bar_enters_and_leaves_with_its_own_axis_on_the_spindle() {
    // #103: the insert sits at the body's edge - a Ø10 body with its
    // nose stands centred, the nose r 5 - D/2 off the spindle, both rapids.
    let path = bore(&blind(-20.0), &BoreParams { head_d: 10.0, shank_d: 10.0, ..params(-10.0, 40.0) }).expect("bores");
    let r = |q: Point3| (q.x * q.x + q.y * q.y).sqrt() - (5.0 - D * 0.5) + 5.0;
    let n = path.moves.len();
    for m in [&path.moves[0], &path.moves[n - 1]] {
        assert_eq!(m.kind, MoveKind::Rapid);
        assert!((r(m.from) - 5.0).abs() < 1e-9 && (r(m.to) - 5.0).abs() < 1e-9, "a rapid off the bar's centred line: {m:?}");
    }
    // Off the wall back to that line before the bar climbs out.
    assert_eq!(path.moves[n - 2].kind, MoveKind::Rapid);
    assert!((r(path.moves[n - 2].to) - 5.0).abs() < 1e-9);
}

#[test]
fn a_through_hole_rides_its_whole_length() {
    // The rims need no axial rule the offset does not already carry:
    // the vertex-circle claim holds the tangency exact at z = +-50.
    let path = bore(&through(), &params(-50.0, 50.0)).expect("a tube bores end to end");
    for (s, r) in centers(&path) {
        assert!(r <= WALL - D + 1e-9, "center at station {s:.3} stands {r:.4} - inside the wall");
    }
}

#[test]
fn a_bar_with_no_hole_is_refused_not_wrong() {
    // The killer law of this stage: a plain bar's min-envelope reads
    // its own skin, and only the axis parity knows the ball would
    // ride IN steel. Silent tangency is the failure this door closes.
    let e = bore(&bar(), &params(-10.0, 40.0)).expect_err("no hole, no bore");
    let msg = e.to_string();
    assert!(matches!(e, camcraft_motion::machining2d::GenError::Empty(_)), "{msg}");
    assert!(msg.contains("nothing to bore"), "{msg}");
    assert!(msg.contains("drilling is not this op's promise"), "{msg}");
}

#[test]
fn a_window_standing_in_steel_is_refused_by_name() {
    // The hole's floor at -20; the window reaches to -30 through it.
    let e = bore(&blind(-20.0), &params(-30.0, 40.0)).expect_err("the axis is steel below the floor");
    let msg = e.to_string();
    assert!(
        msg.contains("does not run open through the window"),
        "{msg}"
    );
}

#[test]
fn a_floor_within_the_nose_reach_is_refused_with_the_number_to_type() {
    // Air at s_lo, so the parity doors pass; the BALL, though, bites
    // the floor the nose cannot reach past.
    let e = bore(&blind(-20.0), &params(-19.8, 40.0)).expect_err("0.2 of air is not 0.4 of nose");
    let msg = e.to_string();
    assert!(msg.contains("floor"), "{msg}");
    assert!(msg.contains("-19.600"), "{msg}");
}

#[test]
fn a_tangent_ride_is_legal() {
    // bottom_z = floor + d EXACTLY is the tangency, not a bite: the
    // reach test stands its hair inside, and a legal touch passes.
    let path = bore(&blind(-20.0), &params(-19.6, 40.0)).expect("tangent is legal");
    assert!(!path.moves.is_empty());
}

#[test]
fn a_bar_bigger_than_its_own_hole_refuses_with_both_numbers() {
    let mut p = params(-10.0, 40.0);
    p.nose_radius = 7.0;
    let e = bore(&blind(-20.0), &p).expect_err("a nose of 7 cannot stand inside a hole of 6");
    let msg = e.to_string();
    assert!(msg.contains("bigger than its own work"), "{msg}");
    assert!(msg.contains("7.000"), "{msg}");
}

#[test]
fn a_model_that_is_not_one_body_refuses_the_gap() {
    let e = bore(&split_tube(), &params(-40.0, 40.0)).expect_err("the gap crosses no material");
    assert!(
        e.to_string().contains("not one continuous turned body"),
        "{e}"
    );
}

#[test]
fn degenerate_numbers_refuse_by_name() {
    let e = bore(&[], &params(-10.0, 40.0)).expect_err("no facets");
    assert!(e.to_string().contains("no facets"), "{e}");
    let mut p = params(-10.0, 40.0);
    p.nose_radius = 0.0;
    let e = bore(&blind(-20.0), &p).expect_err("a point cannot hold a hole round");
    assert!(e.to_string().contains("mathematical point"), "{e}");
    let mut p = params(-10.0, 40.0);
    p.tolerance = f64::NAN;
    let e = bore(&blind(-20.0), &p).expect_err("NaN is not a number to bore with");
    assert!(e.to_string().contains("is not one"), "{e}");
    let mut p = params(-10.0, 40.0);
    p.nose_radius = f64::INFINITY;
    let e = bore(&blind(-20.0), &p).expect_err("infinity is not a nose");
    assert!(e.to_string().contains("is not one"), "{e}");
}

#[test]
fn a_corrupt_model_refuses_instead_of_sorting_noise() {
    let mut t = blind(-20.0);
    t.push(Triangle::new(
        p3(f64::NAN, 0.0, 0.0),
        p3(WALL, 0.0, 40.0),
        p3(0.0, 0.0, 40.0),
    ));
    let e = bore(&t, &params(-10.0, 40.0)).expect_err("a NaN vertex never sorts its way in");
    assert!(e.to_string().contains("non-finite vertex"), "{e}");
}
