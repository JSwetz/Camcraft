//! M5.2 stage B turning law tests: the profile silhouette, the nose
//! offset ride, and the two kernels built from them. What "right"
//! means here: the silhouette says the envelope EXACTLY at every
//! station it quotes (the edge sweep, not window-filtered vertices -
//! the shoulder's vertical pair descends, never sorts); every ride
//! point stands at distance exactly d from the profile to within a
//! hair (no gouge by construction, no float by the chord law); the
//! narrow groove bridges itself because the tall claim wins; the
//! rough's rung ladder leaves a constant stock on the whole bar and
//! starts outermost; and every refusal names itself - a point nose,
//! an infeed past twice the nose, negative leave, a lying axis, an
//! empty height window, a model that is not one bar.
//!
//! Gate map (M4.4): covers camcraft_motion::turning (profile
//! silhouette, offset ride, prof kernels, setup refusals) with this
//! crate's own revolve fixtures.
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::GenError;
use camcraft_motion::turning::profile::Frame;
use camcraft_motion::turning::{
    profile_finish, profile_offset, profile_rough, silhouette, ProfileFinishParams,
    ProfileRoughParams,
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

/// The stepped bar: wall r 6 from z -16 to 2, neck r 5 to the top,
/// capped both ends. Its shoulder at z 2 is the case a global sort
/// would break - the step face is a DESCENDING vertical pair there.
fn bar() -> Vec<Triangle> {
    revolve(16, &[(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)])
}

/// A flange over a long neck: r 6 to z 2, then r 3 to z 20. The
/// neck's wall keeps the normal tensor's ratio far from the axis
/// fit's isotropy gate, and the 3.0 of standing shell below the
/// flange forces a real multi-rung roughing ladder.
fn flange() -> Vec<Triangle> {
    revolve(16, &[(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (3.0, 2.0), (3.0, 20.0), (0.0, 20.0)])
}

/// A narrow groove in a long wall: 1.0 wide, 2.0 deep, stations
/// -4..-3 - narrower than twice the nose, so no ride of this nose
/// may ever touch its floor.
fn grooved() -> Vec<Triangle> {
    revolve(
        16,
        &[
            (0.0, -16.0),
            (6.0, -16.0),
            (6.0, -4.0),
            (4.0, -4.0),
            (4.0, -3.0),
            (6.0, -3.0),
            (6.0, 40.0),
            (0.0, 40.0),
        ],
    )
}

/// The (station, radius) a CL point rides, read in the model's own
/// frame - world Z would carry the fit's last-bit tilt and make two
/// stations a float apart look like a plunge.
fn sr(f: &Frame, q: Point3) -> (f64, f64) {
    f.sr(q)
}

/// The distance from a (station, radius) point to the profile
/// polyline: what "the nose touches, never past" measures against.
/// These fixtures' silhouettes are the envelope EXACTLY (flat bands
/// chord themselves), so this is distance to the model's skin.
fn dist_to(s: f64, r: f64, pts: &[(f64, f64)]) -> f64 {
    let mut d = f64::INFINITY;
    for w in pts.windows(2) {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        let dx = x1 - x0;
        let dy = y1 - y0;
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 {
            (((s - x0) * dx + (r - y0) * dy) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (px, py) = (x0 + dx * t, y0 + dy * t);
        d = d.min(((s - px) * (s - px) + (r - py) * (r - py)).sqrt());
    }
    d
}

/// The largest feed-move radius of a path: the ride's own skyline.
/// A radial LEAD starts above it (at the clearance radius); every
/// other same-station radial move is the ride itself stepping over
/// a shoulder's shadow, which is lawful.
fn skyline(t: &Toolpath, f: &Frame) -> f64 {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed)
        .fold(0.0f64, |acc, m| acc.max(sr(f, m.to).1))
}

#[test]
fn the_silhouette_says_the_envelope_exactly() {
    // Every quoted station carries the envelope's true radius (wall
    // 6, neck 5), the stations ascend, and the shoulder at z 2
    // appears as a DESCENDING vertical pair (2, 6) then (2, 5) -
    // the step face, in walk order, which a global sort would flip
    // into a bogus up-facing step.
    let f = Frame::about(&bar()).expect("an upright bar");
    let pts = silhouette(&bar(), &f, TOL).expect("a bar has a profile");
    for &(s, r) in &pts {
        // Away from the shoulder the envelope is single-valued; AT
        // the shoulder station it is the vertical pair, so either
        // radius is the truth there.
        let ok = if (s - 2.0).abs() < 1e-9 {
            (r - 6.0).abs() < 1e-6 || (r - 5.0).abs() < 1e-6
        } else if s < 2.0 {
            (r - 6.0).abs() < 1e-6
        } else {
            (r - 5.0).abs() < 1e-6
        };
        assert!(ok, "silhouette lies at station {s}: r {r}");
    }
    for w in pts.windows(2) {
        assert!(w[0].0 <= w[1].0, "stations out of order: {pts:?}");
    }
    let i = pts
        .iter()
        .position(|q| (q.0 - 2.0).abs() < 1e-6)
        .expect("the shoulder station");
    assert!(
        (pts[i].1 - 6.0).abs() < 1e-6,
        "the shoulder must ENTER at its wide radius: {:?}",
        pts[i]
    );
    assert!(
        (pts[i + 1].1 - 5.0).abs() < 1e-6,
        "and step DOWN through the face, in order: {:?}",
        pts[i + 1]
    );
}

#[test]
fn the_ride_stands_on_the_nose_everywhere() {
    // The no-gouge theorem, checked point by point: every ride
    // sample is at distance exactly d from the profile - never
    // inside (that would be a gouge), never floating above by more
    // than the chord settles (that would be an under-cut).
    let f = Frame::about(&bar()).expect("upright");
    let pts = silhouette(&bar(), &f, TOL).expect("profile");
    let cl = profile_offset(&pts, NOSE, TOL, -16.0, 16.0).expect("a ride");
    for &(s, r) in &cl {
        let d = dist_to(s, r, &pts);
        assert!((d - NOSE).abs() < 1e-6, "ride at station {s} floats or bites: dist {d}");
    }
    // On the wall it rides at 6 + nose, on the neck at 5 + nose:
    // the offsets a finish pass must leave as its own signature.
    assert!(cl.iter().any(|q| (q.1 - 6.8).abs() < 1e-6 && q.0 < 1.9));
    assert!(cl.iter().any(|q| (q.1 - 5.8).abs() < 1e-6 && q.0 > 2.9));
}

#[test]
fn a_narrow_groove_bridges_itself() {
    // The fold that defeats naive line-offset-and-join: a groove
    // narrower than the nose's diameter. The ride must never dip
    // into the shadow of the lips - the tall claims win and the
    // ball rolls over, exactly as a real nose rolls.
    let f = Frame::about(&grooved()).expect("upright");
    let pts = silhouette(&grooved(), &f, TOL).expect("profile");
    let cl = profile_offset(&pts, NOSE, TOL, -16.0, 16.0).expect("a ride");
    for &(s, r) in &cl {
        if s > -4.2 && s < -2.8 {
            // Inside the groove's reach band the ball stands on the
            // LIPS' circles, never on the floor's claim.
            assert!(r > 6.5, "the nose dove into a 1.0-wide groove at {s} (r {r})");
        }
    }
    for &(s, r) in &cl {
        assert!((dist_to(s, r, &pts) - NOSE).abs() < 1e-6, "grooved ride strayed at {s}");
    }
}

#[test]
fn the_finish_rides_once_and_leads_radially() {
    // One ride of the nose on the offset curve: the program enters
    // with a radial plunge at the window's bottom station (a feed
    // at plunge_feed from above the ride's skyline, no station
    // change) and traverses ascending at feed; rapids stay outside
    // the skyline and the program ends home above the rapids plane.
    let f = Frame::about(&bar()).expect("upright");
    let t = profile_finish(
        &bar(),
        &ProfileFinishParams {
            clearance_z: 24.0,
            top_z: 16.0,
            bottom_z: -16.0,
            feed: 120.0,
            plunge_feed: 60.0,
            nose_radius: NOSE,
            tolerance: TOL,
        },
    )
    .expect("a bar finishes");
    let sky = skyline(&t, &f);
    let mut plunges = 0;
    let mut feeds = 0;
    for m in &t.moves {
        if m.kind != MoveKind::Feed {
            continue;
        }
        feeds += 1;
        let (a, b) = (sr(&f, m.from), sr(&f, m.to));
        if (a.0 - b.0).abs() < 1e-9 && a.1 > sky + 1e-3 {
            assert_eq!(m.feed, Some(60.0), "the radial lead-in is a plunge, not a traverse");
            plunges += 1;
        } else {
            assert_eq!(m.feed, Some(120.0), "in-cut motion at the traverse feed");
            assert!(b.0 >= a.0 - 1e-9, "the ride walks ascending: {a:?} -> {b:?}");
        }
    }
    assert_eq!(plunges, 1, "one ride: one radial lead-in");
    assert!(feeds > 20, "a whole bar in one ride is a lot of traverse");
    let last = t.moves.last().expect("a path ends");
    assert_eq!(last.kind, MoveKind::Rapid);
    assert!(last.to.z > 23.0, "the program goes home above the rapids plane");
}

#[test]
fn the_rough_ladder_leaves_a_constant_stock() {
    // Concentric rungs outermost first, each at distance
    // nose + stock + j*stepdown from the profile: so every point
    // the nose rode on any rung stands at least nose + stock off
    // the model, the deepest rung rides at exactly that, and the
    // rungs' skylines descend rung after rung.
    let f = Frame::about(&flange()).expect("upright");
    let pts = silhouette(&flange(), &f, TOL).expect("profile");
    let stock = 0.5;
    let step = 0.8;
    let t = profile_rough(
        &flange(),
        &ProfileRoughParams {
            clearance_z: 28.0,
            top_z: 20.0,
            bottom_z: -16.0,
            stepdown: step,
            stock_to_leave: stock,
            feed: 200.0,
            plunge_feed: 60.0,
            nose_radius: NOSE,
            tolerance: TOL,
        },
    )
    .expect("the flange roughs");
    let sky = skyline(&t, &f);
    let mut rung_top: Vec<f64> = Vec::new();
    let mut leave_touched = false;
    for m in &t.moves {
        if m.kind != MoveKind::Feed {
            continue;
        }
        let (a, b) = (sr(&f, m.from), sr(&f, m.to));
        if (a.0 - b.0).abs() < 1e-9 && a.1 > sky + 1e-3 {
            rung_top.push(0.0f64); // a plunge: the next rung begins
        }
        let tail = rung_top.last_mut().expect("a rung begins before its first cut");
        *tail = tail.max(b.1);
        let d = dist_to(b.0, b.1, &pts);
        assert!(
            d >= NOSE + stock - 1e-6,
            "a roughing CL point bit to within {d:.4} of the model: less than nose plus leave"
        );
        leave_touched |= (d - (NOSE + stock)).abs() < 1e-6;
    }
    let e_max = 6.0 - 3.0; // the deepest stock standing off the profile
    let want = ((e_max - (2.0 * NOSE + stock)) / step).ceil() as usize + 1;
    assert_eq!(rung_top.len(), want, "one rung per offset in the ladder");
    assert!(
        rung_top.windows(2).all(|w| w[0] >= w[1] - 1e-9),
        "ladder order broke (must start outermost): {rung_top:?}"
    );
    assert!(leave_touched, "the ladder's deepest rung must ride at exactly nose + leave");
}

#[test]
fn tolerance_is_load_bearing() {
    // The chord law the whole "within tolerance" promise rides on:
    // a looser tolerance really does draw fewer samples off the
    // arcs at the shoulder's nose circles.
    let f = Frame::about(&bar()).expect("upright");
    let pts = silhouette(&bar(), &f, TOL).expect("profile");
    let tight = profile_offset(&pts, NOSE, 1e-4, -16.0, 16.0).expect("tight ride");
    let loose = profile_offset(&pts, NOSE, 0.5, -16.0, 16.0).expect("loose ride");
    assert!(
        tight.len() > loose.len(),
        "tolerance changed nothing: {} vs {}",
        tight.len(),
        loose.len()
    );
}

#[test]
fn every_refusal_names_itself() {
    // Degenerate or unlawful input errors, loudly and by word -
    // never a path, never a crash, never a hang.
    let fin = ProfileFinishParams {
        clearance_z: 24.0,
        top_z: 16.0,
        bottom_z: -16.0,
        feed: 120.0,
        plunge_feed: 60.0,
        nose_radius: NOSE,
        tolerance: TOL,
    };

    let mut p = fin;
    p.nose_radius = 0.0;
    let err = profile_finish(&bar(), &p).expect_err("a point cannot ride");
    assert!(format!("{err:?}").contains("arc"), "{err:?}");

    // A height window above the bar's top holds no length of bar.
    let mut p = fin;
    p.bottom_z = 18.0;
    p.top_z = 20.0;
    let err = profile_finish(&bar(), &p).expect_err("an empty window is Empty");
    assert!(matches!(err, GenError::Empty(_)), "{err:?}");
    assert!(format!("{err:?}").contains("nothing to turn"), "{err:?}");

    // A nan number, and no facets at all.
    let mut p = fin;
    p.top_z = f64::NAN;
    assert!(profile_finish(&bar(), &p).is_err(), "a nan height is refused");
    assert!(profile_finish(&[], &fin).is_err(), "no facets to turn");

    // The lying axis: profile::Frame::about's own refusal, reached
    // through the setup both kernels share.
    let down: Vec<Triangle> = bar()
        .into_iter()
        .map(|tr| {
            let s = |q: Point3| p3(q.z, q.x, q.y);
            Triangle::new(s(tr.a), s(tr.b), s(tr.c))
        })
        .collect();
    let err = profile_finish(&down, &fin).expect_err("a lying axis must refuse");
    assert!(format!("{err:?}").contains("off vertical"), "{err:?}");

    // The rough's own doors: the radial ridge law and the leave.
    let mut r = ProfileRoughParams {
        clearance_z: 28.0,
        top_z: 20.0,
        bottom_z: -16.0,
        stepdown: 0.8,
        stock_to_leave: 0.5,
        feed: 200.0,
        plunge_feed: 60.0,
        nose_radius: NOSE,
        tolerance: TOL,
    };
    r.stepdown = 1.7;
    let err = profile_rough(&flange(), &r).expect_err("1.7 > 2*0.8");
    assert!(format!("{err:?}").contains("stepdown"), "{err:?}");
    r.stepdown = 1.6;
    assert!(profile_rough(&flange(), &r).is_ok(), "2r is the last lawful infeed");
    r.stepdown = 0.8;
    r.stock_to_leave = -0.1;
    let err = profile_rough(&flange(), &r).expect_err("negative leave");
    assert!(format!("{err:?}").contains("stock"), "{err:?}");
    r.stock_to_leave = 0.5;
    r.stepdown = 1e-8;
    let err = profile_rough(&flange(), &r).expect_err("the rung cap must fire first");
    assert!(format!("{err:?}").contains("ladder"), "{err:?}");
}

#[test]
fn two_bars_are_not_one_profile() {
    // A station interval no facet crosses is a GAP along the axis;
    // profile turning machines ONE bar, and says so by name rather
    // than ride two silhouettes as if they joined.
    let mut t = bar();
    t.extend(revolve(
        16,
        &[(0.0, 20.0), (4.0, 20.0), (4.0, 30.0), (0.0, 30.0)],
    ));
    let err = profile_finish(
        &t,
        &ProfileFinishParams {
            clearance_z: 40.0,
            top_z: 30.0,
            bottom_z: -16.0,
            feed: 120.0,
            plunge_feed: 60.0,
            nose_radius: NOSE,
            tolerance: TOL,
        },
    )
    .expect_err("a gapped model is not a bar");
    assert!(format!("{err:?}").contains("not one continuous turned body"), "{err:?}");
}
