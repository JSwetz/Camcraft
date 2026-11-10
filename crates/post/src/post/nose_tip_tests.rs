//! The nose-centre to control-point law (#630), pinned per orientation:
//! diameters, faces, tapers and arcs. Provenance: our own tests of the
//! LinuxCNC control-point chart (`camcraft_tools::orientation`, mirrored
//! here as offsets so the post crate stays tool-free).

use super::*;
use crate::cl::{ArcMove, Move, MoveKind};

/// LinuxCNC's chart, restated as test data (the tools crate owns it).
const TIP: [(u8, (f64, f64)); 9] = [
    (1, (-1.0, 1.0)),
    (2, (-1.0, -1.0)),
    (3, (1.0, -1.0)),
    (4, (1.0, 1.0)),
    (5, (0.0, 1.0)),
    (6, (-1.0, 0.0)),
    (7, (0.0, -1.0)),
    (8, (1.0, 0.0)),
    (9, (0.0, 0.0)),
];

fn feed(from: Point3, to: Point3) -> Move {
    Move { kind: MoveKind::Feed, from, to, feed: Some(100.0), arc: None, axis: None, rotary: None }
}

/// A point on the kernel's meridian (+Y), radius `rho`, height `z`.
fn ym(rho: f64, z: f64) -> Point3 {
    Point3::new(0.0, rho, z)
}

const R: f64 = 0.8;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn an_od_finish_posts_the_finished_diameter_and_a_face_posts_z0() {
    // Ø19.05 finish: nose centre at radius 19.05/2 + r; a face at Z0:
    // nose centre at Z = r. Orientation 2 (OD toward the chuck).
    let d = 19.05;
    let t = Toolpath::new(vec![
        feed(ym(d / 2.0 + R, 10.0), ym(d / 2.0 + R, R)),
        feed(ym(d / 2.0 + R, R), ym(0.0, R)),
    ]);
    let out = control_point_path(&t, (-R, -R), [0.0; 3]).expect("one meridian");
    let m = &out.moves[0];
    assert!(close(2.0 * m.to.y, d), "posted diameter {} for Ø{d}", 2.0 * m.to.y);
    assert!(close(m.to.z, 0.0), "the face posts Z0, got {}", m.to.z);
    // Facing to the centre: the control point stands r past the axis.
    assert!(close(out.moves[1].to.y, -R));
    assert!(close(out.moves[1].to.x, 0.0));
}

#[test]
fn every_orientation_moves_the_centre_by_its_own_tip() {
    let c = ym(10.0, 5.0);
    let t = Toolpath::new(vec![feed(c, ym(10.0, -5.0))]);
    for (q, (ox, oz)) in TIP {
        let out = control_point_path(&t, (R * ox, R * oz), [0.0; 3]).expect("one meridian");
        let p = out.moves[0].from;
        assert!(close(p.y, 10.0 + R * ox) && close(p.z, 5.0 + R * oz), "orientation {q}: {p:?}");
        assert!(close(p.x, 0.0), "orientation {q} left the meridian");
    }
}

#[test]
fn a_taper_is_exact_where_a_tip_on_the_profile_would_not_be() {
    // A 30-degree taper (profile line through (rho 5, z 0) toward
    // (rho 5 + 10 tan30, z -10)); the nose centre rides it offset by r
    // along the outward normal - the kernel's construction.
    let a = 30f64.to_radians();
    let (dir_r, dir_z) = (a.sin(), -a.cos()); // down the taper, toward the chuck
    let (nr, nz) = (-dir_z, dir_r); // outward normal: +radius side
    assert!(nr > 0.0);
    let p0 = (5.0, 0.0);
    let p1 = (5.0 + 10.0 * dir_r / -dir_z, -10.0);
    let c0 = ym(p0.0 + R * nr, p0.1 + R * nz);
    let c1 = ym(p1.0 + R * nr, p1.1 + R * nz);
    let t = Toolpath::new(vec![feed(c0, c1)]);
    let out = control_point_path(&t, (-R, -R), [0.0; 3]).expect("one meridian");
    let (q0, q1) = (out.moves[0].from, out.moves[0].to);
    // The nose centre the control lands is the CL, exactly: the cut
    // stays tangent to the taper at distance r - the compensated path.
    let back = |p: Point3| (p.y + R, p.z + R);
    for (q, p) in [(q0, p0), (q1, p1)] {
        let (cy, cz) = back(q);
        let dist = (cy - p.0) * nr + (cz - p.1) * nz;
        assert!(close(dist, R), "the nose stands {dist} off the taper, not r");
    }
    // ... while the control point itself stands OFF the profile by
    // r(1 - sin a - cos a)... read along the normal: the error a plain
    // part-profile-as-tip program would carry, here accounted for.
    let off = (q0.y - p0.0) * nr + (q0.z - p0.1) * nz;
    assert!(close(off, R * (1.0 - nr - nz)), "control point offset {off}");
    assert!(off.abs() > 1e-3, "a taper's control point is not on the profile");
}

#[test]
fn an_arc_keeps_its_radius_and_moves_its_centre() {
    // A YZ quarter arc of the nose centre about (rho 8, z 2), radius 3.
    let m = Move {
        kind: MoveKind::Feed,
        from: ym(11.0, 2.0),
        to: ym(8.0, 5.0),
        feed: Some(100.0),
        arc: Some(ArcMove { center: Point2::new(8.0, 2.0), ccw: true, plane: ArcPlane::Yz }),
        axis: None,
        rotary: None,
    };
    let t = Toolpath::new(vec![m]);
    for (q, (ox, oz)) in TIP {
        let out = control_point_path(&t, (R * ox, R * oz), [0.0; 3]).expect("one meridian");
        let o = &out.moves[0];
        let a = o.arc.expect("still an arc");
        assert!(a.ccw && a.plane == ArcPlane::Yz);
        assert!(close(a.center.x, 8.0 + R * ox) && close(a.center.y, 2.0 + R * oz), "orientation {q}");
        for p in [o.from, o.to] {
            let rr = ((p.y - a.center.x).powi(2) + (p.z - a.center.y).powi(2)).sqrt();
            assert!(close(rr, 3.0), "orientation {q}: arc radius {rr}");
        }
    }
}

#[test]
fn an_x_meridian_path_shifts_in_x_and_reads_radii_from_the_origin() {
    let o = [100.0, 50.0, 0.0];
    let t = Toolpath::new(vec![feed(Point3::new(110.0, 50.0, 3.0), Point3::new(100.0, 50.0, 3.0))]);
    let out = control_point_path(&t, (-R, -R), o).expect("one meridian");
    assert!(close(out.moves[0].from.x, 110.0 - R) && close(out.moves[0].from.y, 50.0));
    assert!(close(out.moves[0].to.x, 100.0 - R));
}

#[test]
fn what_is_not_one_meridian_turning_cut_is_refused_by_name() {
    let off = Toolpath::new(vec![feed(Point3::new(3.0, 4.0, 0.0), ym(5.0, 0.0))]);
    let e = control_point_path(&off, (-R, -R), [0.0; 3]).expect_err("off every meridian");
    assert!(e.contains("milling work"), "{e}");
    let two = Toolpath::new(vec![feed(Point3::new(5.0, 0.0, 0.0), ym(5.0, 0.0))]);
    let e = control_point_path(&two, (-R, -R), [0.0; 3]).expect_err("two meridians");
    assert!(e.contains("two meridians"), "{e}");
    let mut c = feed(ym(5.0, 0.0), ym(5.0, -1.0));
    c.rotary = Some(10.0);
    let e = control_point_path(&Toolpath::new(vec![c]), (-R, -R), [0.0; 3]).expect_err("C angle");
    assert!(e.contains("C angle"), "{e}");
    let xy = Move {
        arc: Some(ArcMove { center: Point2::new(0.0, 0.0), ccw: true, plane: ArcPlane::Xy }),
        ..feed(ym(5.0, 0.0), ym(-5.0, -0.5))
    };
    let e = control_point_path(&Toolpath::new(vec![xy]), (-R, -R), [0.0; 3]).expect_err("XY arc");
    assert!(e.contains("arc off"), "{e}");
    assert!(control_point_path(&off, (f64::NAN, 0.0), [0.0; 3]).is_err());
}

#[test]
fn the_centre_code_posts_the_path_unchanged() {
    let t = Toolpath::new(vec![feed(ym(10.0, 1.0), ym(10.0, -1.0))]);
    let out = control_point_path(&t, (0.0, 0.0), [0.0; 3]).expect("one meridian");
    assert_eq!(out.moves, t.moves);
}
