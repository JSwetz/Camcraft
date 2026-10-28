//! M5.2 stage E2 form-turning law tests: the crown floor of the
//! revolve verdict. What "right" means here: the plunge depth is
//! READ, never authored - the tip is the largest of the claims the
//! crown can wear, so the floor stands at or above the model at
//! every event the scan judges (gouge-free BY construction, and the
//! assertions below hammer that: a mismatched gauge may rest, but
//! never once measures a gouge millimetre). A matched crown is
//! silent, a mismatched one speaks with millimetres to say so, and
//! the doors refuse nonsense by name before any steel moves.
//!
//! Gate map (M4.4): covers camcraft_motion::turning::form (the
//! kernel's doors, the one-plunge-per-valley path, the crown floor
//! through form_warnings) with this crate's own revolve fixtures.
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::turning::profile::Frame;
use camcraft_motion::turning::{form, form_warnings, FormParams};
use camcraft_motion::verify::Warning;

const TAU: f64 = std::f64::consts::TAU;
const TOL: f64 = 0.05;
/// The valley this suite forms: floor radius 8, crown radius 6,
/// so the modeled arc and a crown-6 gauge are the same curve.
const FLOOR: f64 = 8.0;
const CROWN: f64 = 6.0;

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

/// The crown-6 arc's rise at distance d off center.
fn s6(d: f64) -> f64 {
    CROWN - (CROWN * CROWN - d * d).sqrt()
}

/// r 10 bar with one crown-6 valley centered at station c: the
/// formed surface is in the model, which is what a form op promises.
fn valley(c: f64) -> Vec<Triangle> {
    revolve(
        16,
        &[
            (0.0, -50.0),
            (10.0, -50.0),
            (10.0, c - 2.0),
            (FLOOR + s6(2.0), c - 2.0),
            (FLOOR + s6(1.0), c - 1.0),
            (FLOOR, c),
            (FLOOR + s6(1.0), c + 1.0),
            (FLOOR + s6(2.0), c + 2.0),
            (10.0, c + 2.0),
            (10.0, 50.0),
            (0.0, 50.0),
        ],
    )
}

/// The plain bar: a smooth skin and nothing formed anywhere.
fn bar() -> Vec<Triangle> {
    revolve(16, &[(0.0, -50.0), (10.0, -50.0), (10.0, 50.0), (0.0, 50.0)])
}

/// A valley with a FLAT floor - a crown gauge cannot match it: the
/// crown stands 0.343 off the floor's edges, and the verdict says so.
fn flat_valley(c: f64) -> Vec<Triangle> {
    revolve(
        16,
        &[
            (0.0, -50.0),
            (10.0, -50.0),
            (10.0, c - 2.0),
            (FLOOR, c - 2.0),
            (FLOOR, c + 2.0),
            (10.0, c + 2.0),
            (10.0, 50.0),
            (0.0, 50.0),
        ],
    )
}

/// Two crown-6 valleys, at stations 0 and 26.
fn twin() -> Vec<Triangle> {
    revolve(
        16,
        &[
            (0.0, -50.0),
            (10.0, -50.0),
            (10.0, -2.0),
            (FLOOR + s6(2.0), -2.0),
            (FLOOR + s6(1.0), -1.0),
            (FLOOR, 0.0),
            (FLOOR + s6(1.0), 1.0),
            (FLOOR + s6(2.0), 2.0),
            (10.0, 2.0),
            (10.0, 24.0),
            (FLOOR + s6(2.0), 24.0),
            (FLOOR + s6(1.0), 25.0),
            (FLOOR, 26.0),
            (FLOOR + s6(1.0), 27.0),
            (FLOOR + s6(2.0), 28.0),
            (10.0, 28.0),
            (10.0, 50.0),
            (0.0, 50.0),
        ],
    )
}

/// A "valley" whose floor is the axis itself across the whole
/// band: the deepest legal plunge stands at r 0 - a hole.
fn split(c: f64) -> Vec<Triangle> {
    revolve(
        16,
        &[
            (0.0, -50.0),
            (10.0, -50.0),
            (10.0, c - 2.0),
            (0.0, c - 2.0),
            (0.0, c + 2.0),
            (10.0, c + 2.0),
            (10.0, 50.0),
            (0.0, 50.0),
        ],
    )
}

fn fp(band_width: f64, crown_radius: f64) -> FormParams {
    FormParams {
        tolerance: TOL,
        clearance_z: 60.0,
        top_z: 50.0,
        bottom_z: -50.0,
        feed: 40.0,
        plunge_feed: 20.0,
        band_width,
        crown_radius,
    }
}

/// The feed moves' end points in (station, radius) - the plunges.
fn feeds(facets: &[Triangle], t: &Toolpath) -> Vec<(f64, f64)> {
    let f = Frame::about(facets).unwrap();
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed)
        .map(|m| f.sr(m.to))
        .collect()
}

/// The verdict's two millimetre readings, summed into one pair;
/// any non-Envelope warning fails the caller's expectation instead.
fn measures(facets: &[Triangle], path: &Toolpath, band: f64, crown: f64) -> (f64, f64) {
    let w = form_warnings(facets, path, band, crown, TOL);
    let mut g: f64 = 0.0;
    let mut r: f64 = 0.0;
    for warn in &w {
        match warn {
            Warning::Envelope {
                gouge_mm,
                rest_mm,
                ..
            } => {
                g = g.max(*gouge_mm);
                r = r.max(*rest_mm);
            }
            other => panic!("form verdict spoke {other:?}, not the envelope"),
        }
    }
    (g, r)
}

#[test]
fn a_matched_crown_sinks_flush_and_says_nothing() {
    let facets = valley(10.0);
    let path = form(&facets, &fp(4.0, CROWN)).expect("a modeled valley is form work");
    let plunges = feeds(&facets, &path);
    assert_eq!(plunges.len(), 1, "one formed surface, one plunge");
    let (s, r) = plunges[0];
    assert!((s - 10.0).abs() < 0.1, "the crown centers on its own valley, not {s}");
    assert!(
        (r - FLOOR).abs() < TOL,
        "the tip reads the floor the model states, not {r}"
    );
    let (g, rest) = measures(&facets, &path, 4.0, CROWN);
    assert_eq!(g, 0.0, "flush steel never gouges: {g}");
    assert_eq!(rest, 0.0, "a matched crown is silent, rested {rest}");
}

#[test]
fn the_plunge_is_straight_in_and_out() {
    let facets = valley(10.0);
    let path = form(&facets, &fp(4.0, CROWN)).unwrap();
    let f = Frame::about(&facets).unwrap();
    let feed = path.moves.iter().find(|m| m.kind == MoveKind::Feed).unwrap();
    let (sa, ra) = f.sr(feed.from);
    let (sb, rb) = f.sr(feed.to);
    assert_eq!(sa, sb, "a crown gauge goes in along a radius, not a ride");
    assert!(ra > rb + 1.0, "in from the clearance stand, tip last");
    assert_eq!(path.moves.first().unwrap().kind, MoveKind::Rapid);
    assert_eq!(path.moves.last().unwrap().kind, MoveKind::Rapid);
}

#[test]
fn mismatched_gauges_rest_never_gouge() {
    // Every mismatch must stand SHALLOW (rest) and never UNDER
    // (gouge) the model - the tip law is max(claims - crown), and
    // this test is its proof against four wrong gauges.
    let facets = valley(10.0);
    let flat_floor = flat_valley(10.0);
    for (case, model, band, crown, want_rest) in [
        ("too flat", &facets, 4.0, 12.0, 0.1),
        ("too steep", &facets, 4.0, 3.0, 0.4),
        ("too wide", &facets, 6.0, CROWN, 1.5),
        ("flat floor", &flat_floor, 4.0, CROWN, 0.25),
    ] {
        let path = form(model, &fp(band, crown)).unwrap_or_else(|e| panic!("{case}: {e:?}"));
        let (g, r) = measures(model, &path, band, crown);
        assert_eq!(g, 0.0, "{case} gouged by {g} - the tip law broke");
        assert!(
            r > want_rest,
            "{case} rested {r}, under the {want_rest} the geometry promises"
        );
        assert!(r < 2.0, "{case} rested {r} - off by a whole wall");
    }
}

#[test]
fn every_valley_in_the_window_gets_its_own_plunge() {
    let facets = twin();
    let path = form(&facets, &fp(4.0, CROWN)).unwrap();
    let plunges = feeds(&facets, &path);
    assert_eq!(plunges.len(), 2, "two modeled crowns, two plunges");
    for (want, (s, r)) in [(0.0, plunges[0]), (26.0, plunges[1])] {
        assert!((s - want).abs() < 0.1, "a plunge drifted to station {s}");
        assert!((r - FLOOR).abs() < TOL, "a plunge sunk to {r}");
    }
    let (g, rest) = measures(&facets, &path, 4.0, CROWN);
    assert_eq!(g, 0.0);
    assert_eq!(rest, 0.0, "both valleys matched, so the verdict is silent");
}

#[test]
fn doors_refuse_nonsense_by_name() {
    // A smooth bar: no formed surface to promise.
    let e = form(&bar(), &fp(4.0, CROWN)).unwrap_err();
    assert!(format!("{e:?}").contains("nothing to form"), "{e:?}");
    // A crown of no radius is a wide blade, and of infinite radius
    // is a cylinder - both are other kinds' steel.
    for crown in [0.0, -1.0, f64::INFINITY] {
        let e = form(&valley(10.0), &fp(4.0, crown)).unwrap_err();
        assert!(format!("{e:?}").contains("is no arc at all"), "crown {crown}: {e:?}");
    }
    // A band ground past the crown's own reach has no steel at its corners.
    let e = form(&valley(10.0), &fp(14.0, CROWN)).unwrap_err();
    assert!(
        format!("{e:?}").contains("past the reach of its own"),
        "{e:?}"
    );
    // A floor on the axis is a hole, and holes are boring work.
    let e = form(&split(10.0), &fp(4.0, CROWN)).unwrap_err();
    assert!(format!("{e:?}").contains("that is a hole"), "{e:?}");
}
