//! Issue #237: a finely meshed ball (UV, <= 3 degree facets) with a flat
//! rod or bar through its centre. The ball's facets all meet at its
//! centre, so they share a pencil with the rod's faces; their latitudes
//! run continuously about any cross axis, one "wide" class the ring check
//! skipped - and the rod's faces hid inside it. The fit must refuse or
//! read the rod's own axis, never a cross axis (turning about X would
//! sweep a 100 mm rod into a disc). Provenance: our own fixtures.

use crate::geometry::{Point3, Triangle};
use crate::multiaxis::axis::fit_report;

/// A UV sphere of radius `r` about the origin, `nlon` x `nlat` quads.
fn ball(r: f64, nlon: usize, nlat: usize) -> Vec<Triangle> {
    let pt = |i: usize, j: usize| {
        let ph = std::f64::consts::PI * (i as f64 / nlat as f64 - 0.5);
        let th = std::f64::consts::TAU * j as f64 / nlon as f64;
        Point3::new(r * ph.cos() * th.cos(), r * ph.cos() * th.sin(), r * ph.sin())
    };
    let mut s = Vec::new();
    for i in 0..nlat {
        for j in 0..nlon {
            if i > 0 {
                s.push(Triangle::new(pt(i, j), pt(i + 1, j), pt(i + 1, j + 1)));
            }
            if i + 1 < nlat {
                s.push(Triangle::new(pt(i, j), pt(i + 1, j + 1), pt(i, j + 1)));
            }
        }
    }
    s
}

/// A box centred on the origin, `w` x `d` x `h`, two triangles a face.
fn rod(w: f64, d: f64, h: f64) -> Vec<Triangle> {
    let (lo, hi) = (Point3::new(-w / 2.0, -d / 2.0, -h / 2.0), Point3::new(w / 2.0, d / 2.0, h / 2.0));
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

/// Rotate about X by `ang` radians.
fn tilt(t: &Triangle, ang: f64) -> Triangle {
    let (s, c) = ang.sin_cos();
    let r = |p: Point3| Point3::new(p.x, p.y * c - p.z * s, p.y * s + p.z * c);
    Triangle::new(r(t.a), r(t.b), r(t.c))
}

/// Refused, or the rod's own axis `along` - never a cross axis.
fn never_across(f: &[Triangle], along: Point3, what: &str) {
    if let Ok(r) = fit_report(f) {
        let d = r.axis.dir;
        let par = (d.x * along.x + d.y * along.y + d.z * along.z).abs();
        assert!(par > 1.0 - 1e-6, "{what}: read a cross axis {r:?}");
    }
}

#[test]
fn a_fine_ball_with_a_flat_rod_through_it_never_reads_a_cross_axis() {
    for (nlon, nlat) in [(120, 60), (144, 72), (180, 90)] {
        for (w, d, h) in [(10.0, 10.0, 100.0), (6.0, 6.0, 80.0), (40.0, 4.0, 100.0)] {
            let mut f = ball(30.0, nlon, nlat);
            f.extend(rod(w, d, h));
            never_across(&f, Point3::new(0.0, 0.0, 1.0), &format!("ball {nlon}x{nlat} + rod {w}x{d}x{h}"));
        }
    }
}

#[test]
fn the_same_ball_and_rod_rotated_never_reads_the_rotated_cross_axis() {
    let ang = 0.4_f64;
    let mut f = ball(30.0, 120, 60);
    f.extend(rod(10.0, 10.0, 100.0));
    let turned: Vec<Triangle> = f.iter().map(|t| tilt(t, ang)).collect();
    let along = Point3::new(0.0, -ang.sin(), ang.cos());
    never_across(&turned, along, "ball 120x60 + rod, tilted 0.4 rad about X");
}

#[test]
fn a_fine_ball_alone_is_still_refused() {
    assert!(fit_report(&ball(30.0, 120, 60)).is_err(), "a sphere has no one axis");
}
