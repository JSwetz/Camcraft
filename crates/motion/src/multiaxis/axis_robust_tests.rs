//! Tests of the outlier-resistant axis fit (`axis_robust.rs`): keyed,
//! stripped and noisy shafts, and a prism that is not revolved.
//! Provenance: our own fixtures; no vendor material.

use crate::geometry::{Point3, Triangle};
use crate::multiaxis::axis::{fit, fit_report};

const SEGS: i64 = 72;

fn ring(j: i64, z: f64) -> Point3 {
    let th = std::f64::consts::TAU * j as f64 / SEGS as f64;
    Point3::new(10.0 * th.cos(), 10.0 * th.sin(), z)
}

/// A D20 shaft on world Z, z -30..30; `half > 0` cuts a keyway on +x
/// over z 0..10 whose walls meet the ring at +-`half` segments.
fn shaft(half: i64) -> Vec<Triangle> {
    let mut f = Vec::new();
    let zs = [-30.0, 0.0, 10.0, 30.0];
    for j in -SEGS / 2..SEGS / 2 {
        for (k, w) in zs.windows(2).enumerate() {
            if k == 1 && j >= -half && j < half {
                continue;
            }
            let (a1, a2, b1, b2) = (ring(j, w[0]), ring(j + 1, w[0]), ring(j, w[1]), ring(j + 1, w[1]));
            f.push(Triangle::new(a1, a2, b2));
            f.push(Triangle::new(a1, b2, b1));
        }
        let (c0, c1) = (Point3::new(0.0, 0.0, -30.0), Point3::new(0.0, 0.0, 30.0));
        f.push(Triangle::new(c0, ring(j + 1, -30.0), ring(j, -30.0)));
        f.push(Triangle::new(c1, ring(j, 30.0), ring(j + 1, 30.0)));
    }
    if half > 0 {
        let (xt, ys, xf) = (ring(half, 0.0).x, ring(half, 0.0).y, 8.0);
        let q = Point3::new;
        f.push(Triangle::new(q(xf, -ys, 0.0), q(xf, ys, 0.0), q(xf, ys, 10.0)));
        f.push(Triangle::new(q(xf, -ys, 0.0), q(xf, ys, 10.0), q(xf, -ys, 10.0)));
        f.push(Triangle::new(q(xf, ys, 0.0), q(xt, ys, 0.0), q(xt, ys, 10.0)));
        f.push(Triangle::new(q(xf, ys, 0.0), q(xt, ys, 10.0), q(xf, ys, 10.0)));
        f.push(Triangle::new(q(xt, -ys, 0.0), q(xf, -ys, 0.0), q(xf, -ys, 10.0)));
        f.push(Triangle::new(q(xt, -ys, 0.0), q(xf, -ys, 10.0), q(xt, -ys, 10.0)));
    }
    f
}

#[test]
fn a_plain_revolve_keeps_its_plain_fit_bit_for_bit() {
    let r = fit_report(&shaft(0)).expect("a revolve fits");
    assert_eq!(r.axis, r.plain, "nothing set aside, nothing moved");
    assert!(r.trusted && r.outlier_share == 0.0, "{r:?}");
    assert_eq!(r.axis.origin, Point3::new(0.0, 0.0, 0.0), "{r:?}");
    assert_eq!(r.axis.dir, Point3::new(0.0, 0.0, 1.0), "{r:?}");
    assert_eq!(fit(&shaft(0)).ok(), Some(r.axis));
}

#[test]
fn a_keyway_no_longer_drags_the_axis_and_keeps_the_plain_direction() {
    for half in [3, 6] {
        let r = fit_report(&shaft(half)).expect("a keyed shaft fits");
        let off = |p: Point3| (p.x * p.x + p.y * p.y).sqrt();
        println!("half {half}: plain off {:.4e} mm, robust off {:.4e} mm, set aside {:.3}", off(r.plain.origin), off(r.axis.origin), r.outlier_share);
        assert!(off(r.plain.origin) > 0.01, "the plain fit is dragged (the case under test): {r:?}");
        assert!(off(r.axis.origin) < 1e-9, "half {half}: the robust fit sits on the true axis: {r:?}");
        assert!(r.trusted && r.outlier_share > 0.0 && r.outlier_share < 0.1, "{r:?}");
        // The refine guard on a non-revolved remnant: the keyway
        // cannot tilt a straight shaft, so the direction is the
        // plain fit's, bit for bit.
        assert_eq!(r.axis.dir, r.plain.dir, "half {half}: {r:?}");
        assert_eq!(r.axis.dir, Point3::new(0.0, 0.0, 1.0), "half {half}: {r:?}");
    }
}

/// The revolved shaft plus a radial flat strip on +x standing just
/// off the meridian plane (circumcenters 0.125 mm off it): inside
/// the sagitta band, so before #116 it stayed an inlier and leaned
/// the line by microns - a cross hole's mouth ring and floor strip
/// in miniature.
fn shaft_with_near_meridian_strip() -> Vec<Triangle> {
    let mut f = shaft(0);
    let q = Point3::new;
    let (x, y1) = (9.9, 0.25);
    f.push(Triangle::new(q(x, 0.0, 0.0), q(x, y1, 0.0), q(x, y1, 20.0)));
    f.push(Triangle::new(q(x, 0.0, 0.0), q(x, y1, 20.0), q(x, 0.0, 20.0)));
    f
}

#[test]
fn a_near_meridian_strip_is_set_aside_by_the_tightened_band() {
    let r = fit_report(&shaft_with_near_meridian_strip()).expect("the shaft fits");
    let off = |p: Point3| (p.x * p.x + p.y * p.y).sqrt();
    println!("strip: plain off {:.4e} mm, robust off {:.4e} mm, set aside {:.4}", off(r.plain.origin), off(r.axis.origin), r.outlier_share);
    assert!(r.trusted, "{r:?}");
    assert!(off(r.axis.origin) < 1e-9, "the robust fit sits on the true axis: {r:?}");
    assert!(r.axis.dir.x.abs() < 1e-12 && r.axis.dir.y.abs() < 1e-12, "no lean: {r:?}");
}

#[test]
fn a_rectangular_prism_is_still_refused_as_not_revolved() {
    let q = Point3::new;
    let (a, b, h) = (5.0, 10.0, 30.0);
    let v = [q(-a, -b, -h), q(a, -b, -h), q(a, b, -h), q(-a, b, -h), q(-a, -b, h), q(a, -b, h), q(a, b, h), q(-a, b, h)];
    let quads = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]];
    let mut f = Vec::new();
    for [i, j, k, l] in quads {
        f.push(Triangle::new(v[i], v[j], v[k]));
        f.push(Triangle::new(v[i], v[k], v[l]));
    }
    assert!(fit_report(&f).is_err(), "a prism is not a surface of revolution");
}

/// `f` with every vertex off the axis moved radially by a
/// deterministic jitter in [-amp, amp], keyed on the vertex itself
/// (shared vertices move together, the mesh stays closed).
fn jittered(f: &[Triangle], amp: f64) -> Vec<Triangle> {
    let j = |p: Point3| {
        let r = (p.x * p.x + p.y * p.y).sqrt();
        if r < 1.0 {
            return p;
        }
        let mut h = p.x.to_bits() ^ p.y.to_bits().rotate_left(21) ^ p.z.to_bits().rotate_left(42);
        h ^= h >> 33;
        h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
        h ^= h >> 33;
        let t = (h >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0;
        let s = 1.0 + amp * t / r;
        Point3::new(p.x * s, p.y * s, p.z)
    };
    f.iter().map(|t| Triangle::new(j(t.a), j(t.b), j(t.c))).collect()
}

/// The axis' worst displacement from world Z over the shaft's 30 mm
/// half-length: offset plus lean times reach.
fn worst(ax: crate::multiaxis::axis::Axis) -> f64 {
    let off = (ax.origin.x * ax.origin.x + ax.origin.y * ax.origin.y).sqrt();
    off + 30.0 * (ax.dir.x * ax.dir.x + ax.dir.y * ax.dir.y).sqrt()
}

fn strip(f: &mut Vec<Triangle>, width: f64) {
    let q = Point3::new;
    let x = 9.9;
    f.push(Triangle::new(q(x, 0.0, 0.0), q(x, width, 0.0), q(x, width, 20.0)));
    f.push(Triangle::new(q(x, 0.0, 0.0), q(x, width, 20.0), q(x, 0.0, 20.0)));
}

/// Review of PR #168: on a noisy mesh the tightened fit must never
/// land further from the axis than the plain one (a band picking
/// facets by the sign of their noise once did, still trusted).
#[test]
fn a_noisy_shaft_with_a_strip_never_fits_worse_than_plain() {
    for amp in [2e-4, 5e-4, 1e-3, 2e-3, 5e-3, 1e-2, 1.48e-2, 1.7e-2] {
        for width in [0.1, 0.25, 0.3, 0.5] {
            let mut f = jittered(&shaft(0), amp);
            strip(&mut f, width);
            let r = fit_report(&f).expect("the noisy shaft fits");
            let (w, p) = (worst(r.axis), worst(r.plain));
            assert!(r.trusted, "amp {amp} width {width}: {r:?}");
            assert!(w <= p + 1e-12, "amp {amp} width {width}: robust {w:.3e} worse than plain {p:.3e}");
        }
    }
    let mut f = jittered(&shaft(0), 1e-3);
    strip(&mut f, 0.25);
    let r = fit_report(&f).expect("the noisy shaft fits");
    println!("1 um noise + strip: robust {:.3e}, plain {:.3e}", worst(r.axis), worst(r.plain));
}

/// Review of PR #168: noise is not a feature - a noisy plain bar
/// sets nothing aside, so no "features not turned" note is raised.
#[test]
fn a_noisy_plain_shaft_sets_nothing_aside() {
    for amp in [1e-5, 1e-4, 1e-3, 1e-2] {
        let r = fit_report(&jittered(&shaft(0), amp)).expect("the noisy shaft fits");
        assert!(r.trusted && r.outlier_share == 0.0, "amp {amp}: {r:?}");
    }
    // An f32-stored import far off the origin (a mill-turn's bar at
    // X1234.5): its storage rounding is the noise.
    let f32ed = |p: Point3| Point3::new((p.x + 1234.5) as f32 as f64, p.y as f32 as f64, p.z as f32 as f64);
    let far: Vec<Triangle> = shaft(0).iter().map(|t| Triangle::new(f32ed(t.a), f32ed(t.b), f32ed(t.c))).collect();
    let r = fit_report(&far).expect("the f32 shaft fits");
    assert!(r.trusted && r.outlier_share == 0.0, "f32 at X1234.5: {r:?}");
}
