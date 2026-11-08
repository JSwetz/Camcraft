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

/// A box whose four side faces are each split into `n` rectangles
/// along Z (two triangles each): its side lines meet the centre line
/// at `n` points, no one of them holding half the area.
fn split_block(lo: Point3, hi: Point3, n: usize) -> Vec<Triangle> {
    let q = Point3::new;
    let mut f: Vec<Triangle> = block(lo, hi).into_iter().filter(|t| t.normal().z.abs() > 0.5).collect();
    let corners = [q(lo.x, lo.y, 0.0), q(hi.x, lo.y, 0.0), q(hi.x, hi.y, 0.0), q(lo.x, hi.y, 0.0)];
    for k in 0..n {
        let (z0, z1) = (lo.z + (hi.z - lo.z) * k as f64 / n as f64, lo.z + (hi.z - lo.z) * (k + 1) as f64 / n as f64);
        for s in 0..4 {
            let (a, b) = (corners[s], corners[(s + 1) % 4]);
            let (a0, b0, a1, b1) = (q(a.x, a.y, z0), q(b.x, b.y, z0), q(a.x, a.y, z1), q(b.x, b.y, z1));
            f.push(Triangle::new(a0, b0, b1));
            f.push(Triangle::new(a0, b1, a1));
        }
    }
    f
}

fn refused(f: &[Triangle], what: &str) {
    match fit_report(f) {
        Ok(r) => panic!("{what}: read {r:?}"),
        Err(e) => {
            let s = e.to_string();
            assert!(s.contains("share no rotation axis") || s.contains("equally"), "{what}: {s}");
        }
    }
}

#[test]
fn a_square_bar_split_along_its_length_with_a_boss_is_refused_by_name() {
    // Review of PR #223, B1: no single point holds half the side area,
    // and the boss (an outlier) once lent its bearings to the gate.
    for (n, len) in [(4, 160.0), (3, 120.0)] {
        for (bx, by) in [(5.0, 0.0), (10.0, 10.0)] {
            let mut f = split_block(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, len), n);
            f.extend(turned(5.0, 10.0, (bx, by, len), 1, &|_| false));
            refused(&f, &format!("bar split {n}, boss at ({bx}, {by})"));
        }
    }
}

#[test]
fn stepped_and_stacked_blocks_with_a_boss_are_refused_by_name() {
    // Review of PR #223, B1: a three-step block and four stacked cubes.
    let p = Point3::new;
    let mut steps = Vec::new();
    for (k, w) in [100.0, 90.0, 80.0].iter().enumerate() {
        let z = 10.0 * k as f64;
        steps.extend(block(p(-w / 2.0, -w / 2.0, z), p(w / 2.0, w / 2.0, z + 10.0)));
    }
    let mut stack = Vec::new();
    for k in 0..4 {
        let z = 40.0 * k as f64;
        stack.extend(block(p(-20.0, -20.0, z), p(20.0, 20.0, z + 40.0)));
    }
    for (body, top) in [(steps, 30.0), (stack, 160.0)] {
        for (bx, by) in [(5.0, 0.0), (10.0, 0.0), (20.0, 20.0)] {
            let mut f = body.clone();
            f.extend(turned(5.0, 10.0, (bx, by, top), 1, &|_| false));
            refused(&f, &format!("top {top}, boss at ({bx}, {by})"));
        }
    }
}

#[test]
fn crossed_cylinders_never_read_the_cross_axis() {
    // Review of PR #223, B2: a Z rod and a Y rod crossing at the origin.
    // The Z rod's single-band lines all pass through the origin and
    // "meet" Y; turning about Y would cut the rod to a disc. Refused, or
    // Z - never Y.
    let about_y = |t: &Triangle| {
        let r = |p: Point3| Point3::new(p.x, -p.z, p.y);
        Triangle::new(r(t.a), r(t.b), r(t.c))
    };
    for (hz, hy, ybands) in [(100.0, 60.0, 1), (60.0, 60.0, 1), (60.0, 100.0, 1), (100.0, 60.0, 5)] {
        let mut f = turned(10.0, hz, (0.0, 0.0, -hz / 2.0), 1, &|_| false);
        f.extend(turned(10.0, hy, (0.0, 0.0, -hy / 2.0), ybands, &|_| false).iter().map(about_y));
        if let Ok(r) = fit_report(&f) {
            let d = r.axis.dir;
            assert!(d.x.abs() < 1e-6 && d.y.abs() < 1e-6, "Z h{hz} x Y h{hy} ({ybands} bands): {r:?}");
        }
    }
}

#[test]
fn a_single_band_shaft_with_a_shallow_flat_still_reads_its_axis() {
    // A D-flat 1 deep on r10: the flat's line meets the axis and lumps
    // the cut arc's area on one bearing - inside the bias budget.
    let flat = 9.0f64;
    let half = (flat / 10.0).acos();
    let mut f = turned(10.0, 100.0, (0.0, 0.0, -50.0), 1, &|t| {
        let t = if t > std::f64::consts::PI { t - std::f64::consts::TAU } else { t };
        t.abs() < half
    });
    let (yf, q) = (10.0 * half.sin(), Point3::new);
    f.push(Triangle::new(q(flat, -yf, -50.0), q(flat, yf, -50.0), q(flat, yf, 50.0)));
    f.push(Triangle::new(q(flat, -yf, -50.0), q(flat, yf, 50.0), q(flat, -yf, 50.0)));
    let ax = fit(&f).expect("a shallow D-flat shaft is turned");
    assert!(ax.dir.x.abs() < 1e-9 && ax.dir.y.abs() < 1e-9, "{ax:?}");
    assert!((ax.origin.x.powi(2) + ax.origin.y.powi(2)).sqrt() < 1e-6, "{ax:?}");
}
