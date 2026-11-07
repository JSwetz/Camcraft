//! Tests of the pole candidates (`axis_poles.rs`, issue #152): short
//! turned bars whose normal moment is isotropic read about their true
//! axis; a sphere and a cube are still refused by name.
//! Provenance: our own fixtures; no vendor material.

use crate::geometry::{Point3, Triangle};
use crate::multiaxis::axis::fit;

const SEGS: usize = 72;
const BANDS: usize = 5;

/// A plain r x h bar, faced flat at both ends, about the axis `dir`
/// (`Z` or `X`), centred on the origin.
fn bar(r: f64, h: f64, along_x: bool) -> Vec<Triangle> {
    let q = |rad: f64, th: f64, z: f64| {
        let (a, b) = (rad * th.cos(), rad * th.sin());
        if along_x {
            Point3::new(z, a, b)
        } else {
            Point3::new(a, b, z)
        }
    };
    let (z0, z1) = (-h / 2.0, h / 2.0);
    let mut f = Vec::new();
    // The wall in BANDS bands, so the facets' circumcenters stand at
    // several heights: one band would put them all in the mid plane,
    // concurrent at a point, and any tilt of the axis would meet them.
    let zs: Vec<f64> = (0..=BANDS).map(|k| z0 + h * k as f64 / BANDS as f64).collect();
    for j in 0..SEGS {
        let (t0, t1) = (std::f64::consts::TAU * j as f64 / SEGS as f64, std::f64::consts::TAU * (j + 1) as f64 / SEGS as f64);
        for w in zs.windows(2) {
            let (a1, a2, b1, b2) = (q(r, t0, w[0]), q(r, t1, w[0]), q(r, t0, w[1]), q(r, t1, w[1]));
            f.push(Triangle::new(a1, a2, b2));
            f.push(Triangle::new(a1, b2, b1));
        }
        f.push(Triangle::new(q(0.0, 0.0, z0), q(r, t1, z0), q(r, t0, z0)));
        f.push(Triangle::new(q(0.0, 0.0, z1), q(r, t0, z1), q(r, t1, z1)));
    }
    f
}

fn axial(dir: Point3, want: Point3) -> bool {
    dir.cross(want).dot(dir.cross(want)).sqrt() < 1e-9
}

#[test]
fn an_l_d_2_bar_fits_its_true_axis() {
    // r10 x 40: the caps' axial moment 2*pi*R^2 equals half the walls'
    // pi*R*h - the isotropy knife-edge #152 was refused on.
    let ax = fit(&bar(10.0, 40.0, false)).expect("an L/D 2 bar is turned");
    assert!(axial(ax.dir, Point3::new(0.0, 0.0, 1.0)), "{ax:?}");
    assert!(ax.origin.dot(ax.origin).sqrt() < 1e-9, "{ax:?}");
}

#[test]
fn an_l_d_1_5_bar_fits_its_true_axis_lying_down() {
    let ax = fit(&bar(10.0, 30.0, true)).expect("an L/D 1.5 bar is turned");
    assert!(axial(ax.dir, Point3::new(1.0, 0.0, 0.0)), "{ax:?}");
    assert!(ax.origin.dot(ax.origin).sqrt() < 1e-9, "{ax:?}");
}

#[test]
fn a_cube_is_still_refused_by_name() {
    let q = Point3::new;
    let a = 10.0;
    let v = [q(-a, -a, -a), q(a, -a, -a), q(a, a, -a), q(-a, a, -a), q(-a, -a, a), q(a, -a, a), q(a, a, a), q(-a, a, a)];
    let quads = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]];
    let mut f = Vec::new();
    for [i, j, k, l] in quads {
        f.push(Triangle::new(v[i], v[j], v[k]));
        f.push(Triangle::new(v[i], v[k], v[l]));
    }
    let e = fit(&f).expect_err("a cube has no one axis");
    assert!(e.to_string().contains("equally"), "{e}");
}

#[test]
fn a_sphere_is_still_refused_by_name() {
    let (r, nlat, nlon) = (10.0, 24, 48);
    let pt = |i: usize, j: usize| {
        let ph = std::f64::consts::PI * (i as f64 / nlat as f64 - 0.5);
        let th = std::f64::consts::TAU * j as f64 / nlon as f64;
        Point3::new(r * ph.cos() * th.cos(), r * ph.cos() * th.sin(), r * ph.sin())
    };
    let mut s = Vec::new();
    for i in 0..nlat {
        for j in 0..nlon {
            s.push(Triangle::new(pt(i, j), pt(i + 1, j), pt(i + 1, j + 1)));
            s.push(Triangle::new(pt(i, j), pt(i + 1, j + 1), pt(i, j + 1)));
        }
    }
    let e = fit(&s).expect_err("a sphere has no one axis");
    assert!(e.to_string().contains("equally"), "{e}");
}
