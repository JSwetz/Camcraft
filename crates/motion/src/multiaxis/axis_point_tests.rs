//! Tests of the point-concurrency guard (`axis_point.rs`, issue #208):
//! prismatic bodies whose two-triangle faces put every normal line
//! through one point, and single-band shafts whose walls do, must not
//! lend an off-axis feature's bearings to an arbitrary line through
//! that point. Provenance: our own fixtures; no vendor material.

use crate::geometry::{Point3, Triangle};
use crate::multiaxis::axis::{fit, fit_report};

const SEGS: usize = 72;

/// An r x h wall about Z, z0..z0+h, at (cx, cy), in `bands` bands,
/// skipping the bearings `skip` says (a flat's span), with flat caps.
fn turned(r: f64, h: f64, c: (f64, f64, f64), bands: usize, skip: &dyn Fn(f64) -> bool) -> Vec<Triangle> {
    let q = |rad: f64, th: f64, z: f64| Point3::new(c.0 + rad * th.cos(), c.1 + rad * th.sin(), c.2 + z);
    let zs: Vec<f64> = (0..=bands).map(|k| h * k as f64 / bands as f64).collect();
    let mut f = Vec::new();
    for j in 0..SEGS {
        let (t0, t1) = (std::f64::consts::TAU * j as f64 / SEGS as f64, std::f64::consts::TAU * (j + 1) as f64 / SEGS as f64);
        if !skip(0.5 * (t0 + t1)) {
            for w in zs.windows(2) {
                let (a1, a2, b1, b2) = (q(r, t0, w[0]), q(r, t1, w[0]), q(r, t0, w[1]), q(r, t1, w[1]));
                f.push(Triangle::new(a1, a2, b2));
                f.push(Triangle::new(a1, b2, b1));
            }
        }
        f.push(Triangle::new(q(0.0, 0.0, 0.0), q(r, t1, 0.0), q(r, t0, 0.0)));
        f.push(Triangle::new(q(0.0, 0.0, h), q(r, t0, h), q(r, t1, h)));
    }
    f
}

/// A box, two triangles a face (the usual STL export).
fn block(lo: Point3, hi: Point3) -> Vec<Triangle> {
    let q = Point3::new;
    let v = [
        q(lo.x, lo.y, lo.z), q(hi.x, lo.y, lo.z), q(hi.x, hi.y, lo.z), q(lo.x, hi.y, lo.z),
        q(lo.x, lo.y, hi.z), q(hi.x, lo.y, hi.z), q(hi.x, hi.y, hi.z), q(lo.x, hi.y, hi.z),
    ];
    let quads = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]];
    let mut f = Vec::new();
    for [i, j, k, l] in quads {
        f.push(Triangle::new(v[i], v[j], v[k]));
        f.push(Triangle::new(v[i], v[k], v[l]));
    }
    f
}

#[test]
fn a_plate_with_a_boss_off_its_centre_line_is_refused_by_name() {
    // Issue #208: a 100 x 100 x 10 plate with a turned r5 h10 boss at
    // (10, 0). The plate's face lines all meet at its centre, so any
    // line through it "meets" them; main trusted a 45-degree axis.
    for bands in [1, 5] {
        let mut f = block(Point3::new(-50.0, -50.0, 0.0), Point3::new(50.0, 50.0, 10.0));
        f.extend(turned(5.0, 10.0, (10.0, 0.0, 10.0), bands, &|_| false));
        let e = fit_report(&f).expect_err("a plate is not turned about a point of its faces");
        assert!(e.to_string().contains("share no rotation axis"), "{bands} bands: {e}");
    }
}

#[test]
fn a_single_band_shaft_with_a_flat_and_a_cross_hole_is_not_read_across() {
    // Issue #208: r10 h150, one band along its length (the walls' lines
    // meet at the centre), a D-flat facing +X and a cross hole along X.
    // Main read X, trusted. It must read Z or be refused by name.
    let flat = 8.0f64;
    let half = (flat / 10.0).acos();
    let mut f = turned(10.0, 150.0, (0.0, 0.0, -75.0), 1, &|t| {
        let t = if t > std::f64::consts::PI { t - std::f64::consts::TAU } else { t };
        t.abs() < half
    });
    let (yf, q) = (10.0 * half.sin(), Point3::new);
    f.push(Triangle::new(q(flat, -yf, -75.0), q(flat, yf, -75.0), q(flat, yf, 75.0)));
    f.push(Triangle::new(q(flat, -yf, -75.0), q(flat, yf, 75.0), q(flat, -yf, 75.0)));
    // The cross hole: r3 along X through the centre, x -10..flat.
    let hole = |th: f64, x: f64| q(x, 3.0 * th.cos(), 3.0 * th.sin());
    let xs = [-10.0, -5.0, 0.0, 4.0, flat];
    for j in 0..36 {
        let (t0, t1) = (std::f64::consts::TAU * j as f64 / 36.0, std::f64::consts::TAU * (j + 1) as f64 / 36.0);
        for w in xs.windows(2) {
            f.push(Triangle::new(hole(t0, w[0]), hole(t1, w[1]), hole(t1, w[0])));
            f.push(Triangle::new(hole(t0, w[0]), hole(t0, w[1]), hole(t1, w[1])));
        }
    }
    let got = fit_report(&f);
    println!("single-band D-flat shaft: {got:?}");
    match got {
        Ok(r) => {
            let d = r.axis.dir;
            assert!(d.x.abs() < 1e-6 && d.y.abs() < 1e-6, "read off Z: {r:?}");
            assert!((r.axis.origin.x.powi(2) + r.axis.origin.y.powi(2)).sqrt() < 1e-3, "{r:?}");
        }
        Err(e) => assert!(e.to_string().contains("rotation axis") || e.to_string().contains("equally"), "{e}"),
    }
}

#[test]
fn a_cube_with_an_off_centre_boss_is_refused_by_name() {
    // Re-review of PR #204: a 60 mm cube with an r5 h10 boss at (10, 10)
    // read a trusted axis through the cube's centre, tilted toward the
    // boss. Its faces meet at the centre; no axis is earned.
    for bands in [1, 5] {
        let mut f = block(Point3::new(-30.0, -30.0, -30.0), Point3::new(30.0, 30.0, 30.0));
        f.extend(turned(5.0, 10.0, (10.0, 10.0, 30.0), bands, &|_| false));
        let e = fit(&f).expect_err("a cube with a boss has no one axis");
        let s = e.to_string();
        assert!(s.contains("equally") || s.contains("share no rotation axis"), "{bands} bands: {s}");
    }
}

#[test]
fn a_single_band_bar_still_fits_its_true_axis() {
    // The common CAD export: one band along the wall. Its wall lines
    // meet at a point, but they bear every azimuth about Z, so the
    // point-concurrent set itself bears the axis out.
    for h in [20.0, 150.0] {
        let ax = fit(&turned(10.0, h, (3.0, -2.0, 5.0), 1, &|_| false)).expect("a single-band bar is turned");
        assert!(ax.dir.x.abs() < 1e-9 && ax.dir.y.abs() < 1e-9, "h {h}: {ax:?}");
        assert!((ax.origin.x - 3.0).abs() < 1e-6 && (ax.origin.y + 2.0).abs() < 1e-6, "h {h}: {ax:?}");
    }
}
