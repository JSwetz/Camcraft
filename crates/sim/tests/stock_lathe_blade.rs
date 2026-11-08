//! The blade-profile agreement suite (#21, M13.1 owed): a grooving
//! plunge, a part-off past centre, a thread pass and a form sink cut
//! on the field with their true meridian footprint
//! (`Stock::simulate_lathe_profile`), against a brute-force reference
//! written here independently of the engine - the blade's profile
//! STAMPED at fine steps along each move, a cell centre removed where
//! some stamp holds it (in its meridian, both signs of its radius).
//!
//! Two readings at every cell centre off the bar's skin:
//! - **never under-cut**: a centre inside a stamp reads air (the
//!   stamps are a subset of the continuous sweep, so this is exact);
//! - **agreement within the resolution**: a centre the field reads as
//!   air stands within one stamp step of a stamp, and a centre farther
//!   than that from every stamp reads steel.
//!
//! Provenance: our own engines; the blade footprints of the machine-
//! tool handbook turning chapters (`field_blade`'s header); the
//! reference signed distances are the published box and polygon
//! distance functions. Nothing from vendor binaries.
//!
//! Gate map (M4.4): covers stock_sim::field_blade (LatheProfile),
//! stock_sim::field_lathe (LatheSweep's blade hull, simulate_lathe_profile).
//! Lane shape (M4.4): seconds in debug.

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::Point3;
use camcraft_sim::stock::StockShape;
use camcraft_sim::stock_sim::{Backend, LatheAxis, LatheProfile, Stock, CROWN_SAG_TOL, FIELD_BAND};

const CELL: f64 = 0.5;
const BAR_R: f64 = 10.0;
const BAR_H: f64 = 30.0;
/// The stamp step along a move: a twentieth of a cell.
const STEP: f64 = CELL / 20.0;

fn feed(from: Point3, to: Point3) -> Move {
    Move { kind: MoveKind::Feed, from, to, feed: Some(60.0), arc: None, axis: None, rotary: None }
}

fn bar() -> Stock {
    Stock::from_shape(Backend::Field { band: FIELD_BAND }, &StockShape::Cylinder { diameter: 2.0 * BAR_R, height: BAR_H }, CELL).expect("the bar")
}

/// The signed distance to a closed polygon (even-odd winding, the
/// published polygon distance function), any orientation.
fn sd_polygon(p: &[(f64, f64)], x: f64, y: f64) -> f64 {
    let mut d = f64::INFINITY;
    let mut inside = false;
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        let (ex, ey) = (b.0 - a.0, b.1 - a.1);
        let (wx, wy) = (x - a.0, y - a.1);
        let t = ((wx * ex + wy * ey) / (ex * ex + ey * ey)).clamp(0.0, 1.0);
        d = d.min((wx - t * ex).hypot(wy - t * ey));
        if (a.1 > y) != (b.1 > y) && x < a.0 + (y - a.1) * ex / ey {
            inside = !inside;
        }
    }
    if inside {
        -d
    } else {
        d
    }
}

/// The reference footprint's signed distance at `(u, v)`: `u` outward
/// from the tip, `v` along the axis from the band's centre.
fn sd_profile(p: &LatheProfile, u: f64, v: f64) -> f64 {
    match *p {
        LatheProfile::Nose { radius } => u.hypot(v) - radius,
        LatheProfile::Blade { width, corner, height } => {
            // The rounded box: centre (H/2, 0), half extents (H/2, w/2).
            let (bx, by) = (0.5 * height - corner, 0.5 * width - corner);
            let (qx, qy) = ((u - 0.5 * height).abs() - bx, v.abs() - by);
            qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - corner
        }
        LatheProfile::Thread { width, angle, height } => {
            let h = 0.5 * width;
            let crest = h / (0.5 * angle.to_radians()).tan();
            let top = height.max(crest);
            sd_polygon(&[(0.0, 0.0), (crest, h), (top, h), (top, -h), (crest, -h)], u, v)
        }
        LatheProfile::Form { width, crown, height } => {
            // Band slab, cap, and the crown disc continued straight up
            // as a ray capsule (inside the band, `u >= sag(v)` is
            // exactly that capsule): the max of the three is the exact
            // sign and a LOWER bound of the distance outside, so an
            // over-cut it flags is real.
            let h = 0.5 * width;
            let top = height.max(crown - (crown * crown - h * h).max(0.0).sqrt());
            let capsule = if u >= crown { v.abs() - crown } else { (u - crown).hypot(v) - crown };
            (v.abs() - h).max(u - top).max(capsule)
        }
    }
}

/// The stamped reference: the nearest stamp's signed distance over
/// every move (the moves lie in the y = 0 meridian, x signed).
fn stamped(p: &LatheProfile, moves: &[Move], reach: f64, rho: f64, s: f64) -> f64 {
    let mut best = f64::INFINITY;
    for m in moves {
        let (r0, z0, r1, z1) = (m.from.x, m.from.z, m.to.x, m.to.z);
        let n = ((r1 - r0).hypot(z1 - z0) / STEP).ceil().max(1.0) as usize;
        // Only the stamps whose station can reach `s` (a speed-up, not
        // a law: a skipped stamp stands farther than its own reach).
        let dz = z1 - z0;
        let (k0, k1) = if dz.abs() > 1e-12 {
            let (ta, tb) = ((s - reach - CELL - z0) / dz, (s + reach + CELL - z0) / dz);
            let (lo, hi) = (ta.min(tb).clamp(0.0, 1.0), ta.max(tb).clamp(0.0, 1.0));
            ((lo * n as f64).floor() as usize, (hi * n as f64).ceil() as usize)
        } else if (s - z0).abs() > reach + CELL {
            continue;
        } else {
            (0, n)
        };
        for k in k0..=k1.min(n) {
            let t = k as f64 / n as f64;
            let (r, z) = (r0 + t * (r1 - r0), z0 + t * (z1 - z0));
            if (s - z).abs() > reach + CELL {
                continue;
            }
            best = best.min(sd_profile(p, rho - r, s - z)).min(sd_profile(p, -rho - r, s - z));
        }
    }
    best
}

/// Cut `moves` with `p` and read both laws at every cell centre of
/// the bar off its skin within `zs`; returns (checked, air).
fn agree(p: LatheProfile, moves: &[Move], zs: (f64, f64), reach: f64, slack: f64) -> (usize, usize) {
    let mut f = bar();
    f.simulate_lathe_profile(moves, &p, &LatheAxis::Z).expect("the blade cuts");
    let (mut checked, mut air, mut under, mut over) = (0usize, 0usize, 0usize, 0usize);
    let n = (BAR_R / CELL) as i32 + 1;
    for k in 0..((BAR_H / CELL) as i32) {
        let z = (k as f64 + 0.5) * CELL;
        if z < zs.0 || z > zs.1 {
            continue;
        }
        for j in -n..n {
            for i in -n..n {
                let (x, y) = ((i as f64 + 0.5) * CELL, (j as f64 + 0.5) * CELL);
                let rho = x.hypot(y);
                if rho >= BAR_R - CELL {
                    continue; // the tessellated skin is the mesh's business
                }
                let law = stamped(&p, moves, reach, rho, z);
                let is_air = f.distance(Point3::new(x, y, z)) > 0.0;
                checked += 1;
                air += usize::from(is_air);
                if law < -1e-9 && !is_air {
                    under += 1;
                    println!("UNDER-CUT ({x}, {y}, {z}) rho {rho:.4}: inside a stamp by {:.3e}, the field kept steel", -law);
                }
                if is_air && law > STEP + slack {
                    over += 1;
                    println!("OVER-CUT ({x}, {y}, {z}) rho {rho:.4}: {law:.4} from every stamp, the field reads air");
                }
            }
        }
    }
    println!("{p:?}: {checked} centres ({air} air), {under} under-cut, {over} beyond the resolution");
    assert_eq!(under, 0, "the blade profile is never under-cut");
    assert_eq!(over, 0, "the field agrees with the stamps within one step");
    (checked, air)
}

/// The cells only the blade removes: inside a stamp of `p`, outside
/// every nose disc of radius `nose` on the same moves.
fn beyond_nose(p: LatheProfile, moves: &[Move], reach: f64, nose: f64, rho: f64, z: f64) -> bool {
    stamped(&p, moves, reach, rho, z) < 0.0 && stamped(&LatheProfile::Nose { radius: nose }, moves, nose, rho, z) > 0.0
}

#[test]
fn a_plunge_groove_cuts_its_corner_rounded_rectangle() {
    let p = LatheProfile::Blade { width: 3.0, corner: 0.4, height: 8.0 };
    let m = [feed(Point3::new(12.0, 0.0, 15.0), Point3::new(6.0, 0.0, 15.0)), feed(Point3::new(6.0, 0.0, 15.0), Point3::new(12.0, 0.0, 15.0))];
    let (checked, air) = agree(p, &m, (11.0, 19.0), 8.5, 0.0);
    assert!(checked > 5_000 && air > 1_000, "fixture: the scan must cover the kerf ({checked}, {air})");
    // The flat floor's corners are the blade's, not a nose disc's.
    assert!(beyond_nose(p, &m, 8.5, 0.4, 6.25, 16.25));
}

#[test]
fn a_part_off_past_centre_cuts_the_whole_kerf_disc() {
    let p = LatheProfile::Blade { width: 2.0, corner: 0.0, height: 14.0 };
    let m = [feed(Point3::new(12.0, 0.0, 8.0), Point3::new(-0.5, 0.0, 8.0))];
    let (_, air) = agree(p, &m, (5.0, 11.0), 14.5, 0.0);
    assert!(air > 1_000, "fixture: the kerf disc must be cut ({air})");
    // On the axis itself, within the band: air at every azimuth.
    let mut f = bar();
    f.simulate_lathe_profile(&m, &p, &LatheAxis::Z).expect("the part-off runs");
    for (x, y) in [(0.25, 0.25), (-0.25, 0.25), (-0.25, -0.25), (0.25, -0.25)] {
        assert!(f.distance(Point3::new(x, y, 8.75)) > 0.0, "the kerf is parted through at ({x}, {y})");
    }
}

#[test]
fn a_thread_pass_cuts_its_truncated_v() {
    // A metric 60-degree V, 1.3 wide at the crest, two passes in.
    let p = LatheProfile::Thread { width: 1.3, angle: 60.0, height: 6.0 };
    let m = [
        feed(Point3::new(8.6, 0.0, 26.0), Point3::new(8.6, 0.0, 6.0)),
        feed(Point3::new(8.3, 0.0, 26.0), Point3::new(8.3, 0.0, 6.0)),
    ];
    let (_, air) = agree(p, &m, (2.0, 12.0), 6.2, 0.0);
    assert!(air > 1_000, "fixture: the thread pass must cut ({air})");
}

#[test]
fn a_form_sink_cuts_its_crown_within_the_chord_tolerance() {
    let p = LatheProfile::Form { width: 4.0, crown: 3.0, height: 6.0 };
    let m = [feed(Point3::new(12.0, 0.0, 20.0), Point3::new(7.0, 0.0, 20.0))];
    let (_, air) = agree(p, &m, (16.0, 24.0), 6.5, CROWN_SAG_TOL);
    assert!(air > 500, "fixture: the crown must cut ({air})");
}

#[test]
fn a_nonsense_blade_is_refused_by_name() {
    let m = [feed(Point3::new(12.0, 0.0, 15.0), Point3::new(6.0, 0.0, 15.0))];
    for p in [
        LatheProfile::Blade { width: 0.0, corner: 0.0, height: 8.0 },
        LatheProfile::Blade { width: 2.0, corner: 1.5, height: 8.0 },
        LatheProfile::Blade { width: f64::NAN, corner: 0.0, height: 8.0 },
        LatheProfile::Thread { width: 1.0, angle: 180.0, height: 5.0 },
        LatheProfile::Thread { width: 1.0, angle: 0.0, height: 5.0 },
        LatheProfile::Form { width: 7.0, crown: 3.0, height: 5.0 },
        LatheProfile::Form { width: 2.0, crown: -1.0, height: 5.0 },
        LatheProfile::Nose { radius: 0.0 },
    ] {
        let mut f = bar();
        let e = f.simulate_lathe_profile(&m, &p, &LatheAxis::Z).expect_err("nonsense refuses");
        assert!(e.to_string().contains("lathe profile"), "{p:?}: {e}");
    }
    let mut c = Stock::from_shape(Backend::Column, &StockShape::Cylinder { diameter: 20.0, height: 30.0 }, CELL).expect("columns");
    assert!(c.simulate_lathe_profile(&m, &LatheProfile::Blade { width: 2.0, corner: 0.0, height: 8.0 }, &LatheAxis::Z).is_err());
}
