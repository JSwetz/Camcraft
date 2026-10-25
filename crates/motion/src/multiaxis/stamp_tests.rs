//! The STAMP's exact angle identities (moved out of `stamp.rs` at the
//! AGENTS 400-line seam; the law itself is documented there). The
//! generator tests (tests/multiaxis.rs) prove the choreography; these
//! prove the identities a dome mesh's band angle can only bound. The
//! world is a plane pitched 30 degrees about Y, tiled twice, and a
//! straight ladder of ball centers lying on it at the tangent distance
//! `|c - q| = r`: there, contact is face-interior, n is the same exact
//! vector at every step, and the feed chord lies IN the plane
//! (n . t = 0), so the lead and lag rotations hold their angles
//! exactly.

use crate::cl::{Move, MoveKind, Toolpath};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::stamp::stamp_axes;
use super::tilt::{angle_between, AxisControl};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn len(v: Point3) -> f64 {
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

fn unit(v: Point3) -> Point3 {
    let l = len(v);
    p(v.x / l, v.y / l, v.z / l)
}

const PITCH: f64 = 30f64.to_radians();

fn pitched_plane() -> Vec<Triangle> {
    let v = |x: f64, y: f64| p(x, y, 5.0 + PITCH.tan() * x);
    let (a, b, c, d) = (v(-60.0, -60.0), v(60.0, -60.0), v(60.0, 60.0), v(-60.0, 60.0));
    vec![Triangle::new(a, b, c), Triangle::new(a, c, d)]
}

fn plane_normal() -> Point3 {
    unit(p(-PITCH.tan(), 0.0, 1.0))
}

/// The tangent centers of the line y = 0 on the offset surface,
/// five steps ten millimeters apart.
fn ladder_centers(r: f64) -> Vec<Point3> {
    let n = plane_normal();
    (0..5)
        .map(|i| {
            let q = p(10.0 * i as f64 - 20.0, 0.0, 5.0 + PITCH.tan() * (10.0 * i as f64 - 20.0));
            p(q.x + n.x * r, q.y + n.y * r, q.z + n.z * r)
        })
        .collect()
}

fn ladder(centers: &[Point3], kinds: impl Fn(usize) -> MoveKind) -> Toolpath {
    Toolpath::new(
        centers
            .windows(2)
            .enumerate()
            .map(|(i, w)| Move::new(kinds(i), w[0], w[1], Some(800.0)))
            .collect(),
    )
}

fn axis_of(a: [f64; 3]) -> Point3 {
    p(a[0], a[1], a[2])
}

#[test]
fn the_stamp_poses_every_cut_move_at_the_exact_lead() {
    let r = 3.0;
    let centers = ladder_centers(r);
    let n = plane_normal();
    let t_hat = unit(centers[1] - centers[0]);
    let out = stamp_axes(
        &ladder(&centers, |_| MoveKind::Feed),
        &pitched_plane(),
        r,
        &AxisControl {
            lead_deg: 15.0,
            lag_deg: 0.0,
            max_tilt_deg: None,
        },
        None,
    )
    .unwrap();
    assert_eq!(out.moves.len(), 4);
    for mv in &out.moves {
        let a = axis_of(mv.axis.expect("a tangent ladder must pose every cut move"));
        assert!(
            (angle_between(n, a) - 15f64.to_radians()).abs() < 1e-9,
            "the lead identity broke on the pitched plane: {a:?}"
        );
        assert!(
            (a.dot(t_hat) - 15f64.to_radians().sin()).abs() < 1e-9,
            "the lean must go WITH the feed: {a:?}"
        );
    }
}

#[test]
fn the_stamp_folds_an_overlean_onto_the_cone_exactly() {
    let r = 3.0;
    let n = plane_normal();
    let out = stamp_axes(
        &ladder(&ladder_centers(r), |_| MoveKind::Feed),
        &pitched_plane(),
        r,
        &AxisControl {
            lead_deg: 75.0,
            lag_deg: 0.0,
            max_tilt_deg: Some(40.0),
        },
        None,
    )
    .unwrap();
    for mv in &out.moves {
        let a = axis_of(mv.axis.expect("a posed cut move"));
        assert!(
            (angle_between(n, a) - 40f64.to_radians()).abs() < 1e-9,
            "the clamp must land the pose ON the cone: {a:?}"
        );
    }
}

#[test]
fn the_stamp_leans_side_with_the_lag_exactly() {
    let r = 3.0;
    let n = plane_normal();
    let out = stamp_axes(
        &ladder(&ladder_centers(r), |_| MoveKind::Feed),
        &pitched_plane(),
        r,
        &AxisControl {
            lead_deg: 0.0,
            lag_deg: 25.0,
            max_tilt_deg: None,
        },
        None,
    )
    .unwrap();
    for mv in &out.moves {
        let a = axis_of(mv.axis.expect("a posed cut move"));
        assert!(
            (angle_between(n, a) - 25f64.to_radians()).abs() < 1e-9,
            "the lag identity broke on the pitched plane: {a:?}"
        );
    }
}

#[test]
fn the_stamp_poses_only_moves_that_touch() {
    // Rapid over the plane, a feed that never reaches it, then
    // the tangent ladder: only the touching feeds carry a pose,
    // and a rapid never does even when it lands on contact.
    let r = 3.0;
    let centers = ladder_centers(r);
    let air = p(30.0, 0.0, 45.0);
    let ms = vec![
        Move::new(MoveKind::Rapid, p(-40.0, 0.0, 45.0), centers[0], Some(3000.0)),
        Move::new(MoveKind::Feed, centers[0], air, Some(800.0)),
        Move::new(MoveKind::Feed, air, centers[1], Some(800.0)),
        Move::new(MoveKind::Feed, centers[1], centers[2], Some(800.0)),
    ];
    let out = stamp_axes(
        &Toolpath::new(ms),
        &pitched_plane(),
        r,
        &AxisControl {
            lead_deg: 15.0,
            lag_deg: 0.0,
            max_tilt_deg: None,
        },
        None,
    )
    .unwrap();
    assert_eq!(out.moves.len(), 4);
    assert_eq!(out.moves[0].axis, None, "the rapid must hold no angle");
    assert_eq!(out.moves[1].axis, None, "a move ending in air is vertical");
    assert!(out.moves[2].axis.is_some(), "the plunge onto the surface poses");
    assert!(out.moves[3].axis.is_some(), "the cut along the surface poses");
}

#[test]
fn drill_cycles_refuse_the_stamp_by_name() {
    let r = 3.0;
    let mut path = ladder(&ladder_centers(r), |_| MoveKind::Feed);
    path.cycles.push(crate::cl::Cycle {
        kind: crate::cl::CycleKind::Drill,
        xy: crate::geo2d::Point2::new(0.0, 0.0),
        r_z: 10.0,
        bottom_z: -5.0,
        entry_z: 5.0,
        peck: 0.0,
        dwell: 0.0,
        first_move: 0,
    });
    let e = stamp_axes(&path, &pitched_plane(), r, &AxisControl::default(), None).err();
    assert!(matches!(e, Some(GenError::Invalid(_))), "got {e:?}");
}
