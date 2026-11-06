//! Unit tests of `stock_sim` (split from it by the 400-line law, M13.1):
//! the engine atoms the document sim stands on, checked in the app.
//! Provenance: the dexel atoms' own published laws; nothing vendor.

use super::*;
use crate::geo2d::Point2;
use crate::geometry::Point3;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

#[test]
fn static_ball_is_the_sphere_section() {
    // A motionless ball is the exact circle of its section: bottom
    // at `z_c - sqrt(r^2 - d^2)`, nothing beyond `r`, no sampling
    // error anywhere - the identity the swept law generalizes.
    let mut dex = Dexel::block(-4.5, -4.5, 4.5, 4.5, 100.0, 0.5).unwrap();
    dex.cut_ball(p3(0.0, 0.0, 5.0), p3(0.0, 0.0, 5.0), 2.0);
    let r = 2.0;
    for iy in 0..dex.ny {
        for ix in 0..dex.nx {
            let q = dex.center(ix, iy);
            let d = q.x.hypot(q.y);
            let want = if d <= r {
                5.0 - (r * r - d * d).sqrt()
            } else {
                100.0
            };
            assert!(
                (dex.top_at(ix + iy * dex.nx) - want).abs() <= 1e-12,
                "cell {q:?} d={d}: got {}, want {want}",
                dex.top_at(ix + iy * dex.nx)
            );
        }
    }
}

#[test]
fn oblique_sweep_finds_the_interior_stationary_point() {
    // Endpoints and the XY-nearest instant are NOT enough on an
    // oblique move: the convex bottom's minimum sits before the
    // midpoint when the center climbs. f(t) = (5 + 7t) -
    // sqrt(9 - (10 - 20t)^2) bottoms at u = 21/sqrt(449) off the
    // midpoint, 5.3204 - strictly under f(t_c) = 5.5.
    let mut dex = Dexel::block(-13.8, -0.2, 13.8, 0.2, 100.0, 0.4).unwrap();
    dex.cut_ball(p3(-10.0, 0.0, 5.0), p3(10.0, 0.0, 12.0), 3.0);
    let q = p3(0.0, 0.0, 0.0);
    let k = dex.col(Point2::new(q.x, q.y)).unwrap();
    let u = 21.0 / 449.0f64.sqrt();
    let want = 5.0 + 7.0 * (10.0 - u) / 20.0 - 20.0 * u / 7.0;
    assert!(
        (dex.top_at(k) - want).abs() <= 1e-9,
        "got {}, want {want} (t_c-only would say 5.5)",
        dex.top_at(k)
    );
    assert!(dex.top_at(k) < 5.5 - 0.05, "the quadratic root never ran");
}

#[test]
fn planar_sweep_is_a_constant_offset_profile() {
    // k = 0 (flat feed): the bottom at any column is the section at
    // the perpendicular distance - the double root collapses onto
    // t_c and the clamp evaluation owns it.
    let mut dex = Dexel::block(-0.25, 0.5, 0.25, 1.0, 100.0, 0.5).unwrap();
    dex.cut_ball(p3(-10.0, 0.0, 5.0), p3(10.0, 0.0, 5.0), 3.0);
    let k = dex.col(Point2::new(0.0, 0.75)).unwrap();
    let want = 5.0 - (9.0f64 - 0.75 * 0.75).sqrt();
    assert!(
        (dex.top_at(k) - want).abs() <= 1e-12,
        "got {}, want {want}",
        dex.top_at(k)
    );
}

#[test]
fn out_of_reach_cells_are_never_touched() {
    // The phantom-window law: a sweep whose disc misses a column
    // leaves its stock top exactly as it stood - the reach test,
    // not a margin, decides.
    let mut dex = Dexel::block(-20.0, -1.0, 20.0, 1.0, 7.0, 0.5).unwrap();
    dex.cut_ball(p3(-10.0, 0.0, 5.0), p3(10.0, 0.0, 5.0), 3.0);
    for iy in 0..dex.ny {
        for ix in 0..dex.nx {
            let q = dex.center(ix, iy);
            let touched = q.x.abs() <= 10.0 + 3.0 && q.y.abs() <= 3.0;
            if !touched {
                assert_eq!(dex.top_at(ix + iy * dex.nx), 7.0, "phantom cut at {q:?}");
            }
        }
    }
    // And the reach law itself, directly: a perpendicular offset
    // just under r grazes; just over r gives nothing at all.
    assert!(reach(0.0, 2.999, 20.0, 0.0, 3.0).is_some());
    assert!(reach(0.0, 3.001, 20.0, 0.0, 3.0).is_none());
    // w is an offset from the chord's START, so the middle of the
    // window is at w = (10, 0): the disc covers the middle 6 of a
    // 20-long chord, t 0.35..0.65. (At w = (0,0) the point is the
    // start endpoint itself and the clamped half-window is 0.15.)
    let (t0, t1) = reach(10.0, 0.0, 20.0, 0.0, 3.0).unwrap();
    assert!((t1 - t0 - 0.3).abs() <= 1e-12, "window {t0}..{t1}");
}

#[test]
fn non_finite_inputs_cut_nothing_and_never_panic() {
    let mut dex = Dexel::block(-5.0, -5.0, 5.0, 5.0, 9.0, 1.0).unwrap();
    dex.cut_ball(p3(0.0, 0.0, f64::NAN), p3(1.0, 0.0, 0.0), 2.0);
    dex.cut_ball(p3(0.0, 0.0, 1.0), p3(1.0, 0.0, 1.0), f64::INFINITY);
    dex.cut_chord(p3(0.0, 0.0, 1.0), p3(f64::NAN, 0.0, 1.0), 2.0);
    assert!(dex.tops().all(|v| v == 9.0), "garbage cut something");
}

#[test]
fn envelope_scan_states_its_grid_and_refuses_a_bad_ball() {
    let facets = vec![Triangle::new(
        p3(-5.0, -5.0, 0.0),
        p3(5.0, -5.0, 0.0),
        p3(5.0, 5.0, 0.0),
    )];
    assert!(envelope_scan(&facets, &[], 0.0, 0.01).is_err());
    assert!(envelope_scan(&facets, &[], -1.0, 0.01).is_err());
    assert!(envelope_scan(&facets, &[], 2.0, f64::NAN).is_err());
    // A non-finite vertex is an error, not an empty report.
    let mut bad = facets.clone();
    bad[0].c = p3(f64::NAN, 5.0, 0.0);
    assert!(envelope_scan(&bad, &[], 2.0, 0.01).is_err());
    // An empty model has no footprint to measure on: silence.
    let rep = envelope_scan(&[], &[], 2.0, 0.01).unwrap();
    assert_eq!(rep, EnvelopeReport::default());
}

#[test]
fn the_bull_corner_rides_only_a_bull() {
    // M13.1's plan law: Some(corner) for a bull nose with a corner, a
    // zero corner is the flat disc exactly, every other kind is None,
    // and a nonsense corner refuses by name.
    let cat = crate::tools::catalog();
    let get = |n: &str| cat.iter().find(|(k, _)| *k == n).map(|(_, t)| t.clone()).unwrap();
    assert_eq!(bull_corner(&get("BB-1000-R1")), Ok(Some(1.0)));
    assert_eq!(bull_corner(&get("EM-1000-4FL")), Ok(None));
    assert_eq!(bull_corner(&get("BN-1000")), Ok(None), "a ball is its own atom");
    let mut square = get("BB-1000-R1");
    square.corner_r = 0.0;
    assert_eq!(bull_corner(&square), Ok(None));
    square.corner_r = f64::NAN;
    assert!(bull_corner(&square).is_err());
}
