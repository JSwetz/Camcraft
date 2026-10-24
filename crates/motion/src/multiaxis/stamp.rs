//! The STAMP: the pass that reads a finished ball-center path back
//! into an oriented one (M5.1 stage C). A sphere's envelope is
//! rotation-invariant, so the CL ladder a 5-axis finish cuts is
//! exactly the waterline/raster path of stage A/B - and what makes it
//! 5-axis is the pose each move holds: at every move that ends in
//! contact, the nearest surface point gives the normal
//! (`d = |c - q| = r` is tangency), the move's own travel gives the
//! tangent, and [`super::tilt::command_axis`] folds lead, lag and the
//! clamp into the direction the spindle must point. Moves that end in
//! air - the hops, the clearances - hold no angle, and a pose change
//! where the tip stands still is a position intent the post commands
//! on its own axis words (M5.1 A's law).
//!
//! Provenance: the ball-center/axis-pose split is standard multi-axis
//! CL practice (tool-axis vectors ride the location words,
//! RS-274/ISO 10791 tradition); written from those published
//! descriptions and our own design notes only.

use crate::cl::{Move, MoveKind, PathBuilder, Toolpath};
use crate::finishing3d::closest_pt_tri;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::tilt::{command_axis, AxisControl};

/// Read a ball-center path back as an oriented one: same positions,
/// same order, same arc intents - only the pose each move holds is
/// new. `r` is the ball radius the path was generated with; contact
/// is decided by the tangent law `|c - q| = r`, with a band that
/// forgives the project's micron lattice and the arc fit, never a
/// whole radius of slack. Facets are scanned with an AABB shell
/// reject; these paths have thousands of moves and models have tens
/// of thousands of facets, so the cheap box answer must gate the
/// Voronoi solve.
pub(super) fn stamp_axes(
    path: &Toolpath,
    facets: &[Triangle],
    r: f64,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    if !path.cycles.is_empty() {
        return Err(GenError::Invalid(
            "a 5-axis stamp cannot re-pose drill cycles".into(),
        ));
    }
    let band = 0.01 + 1e-6 * r; // micron lattice + float dust, not a radius
    let boxes: Vec<[f64; 6]> = facets
        .iter()
        .map(|f| {
            [
                f.a.x.min(f.b.x.min(f.c.x)),
                f.a.y.min(f.b.y.min(f.c.y)),
                f.a.z.min(f.b.z.min(f.c.z)),
                f.a.x.max(f.b.x.max(f.c.x)),
                f.a.y.max(f.b.y.max(f.c.y)),
                f.a.z.max(f.b.z.max(f.c.z)),
            ]
        })
        .collect();
    let mut b = PathBuilder::new(path.start().unwrap_or(Point3::ZERO));
    for (i, mv) in path.moves.iter().enumerate() {
        let axis = match mv.kind {
            MoveKind::Rapid => None,
            _ => cut_axis(mv, facets, &boxes, r, band, &path.moves[i + 1..], c),
        };
        b.set_axis(axis);
        match &mv.arc {
            Some(a) => b.push_arc(mv.kind, mv.to, mv.feed, a.center, a.ccw),
            None => b.push(mv.kind, mv.to, mv.feed),
        }
    }
    let stamped = b.finish();
    if !stamped.all_finite() {
        return Err(GenError::Invalid("a 5-axis stamp left the finite horizon".into()));
    }
    Ok(stamped)
}

/// The pose one cutting move holds: None in air, the surface normal
/// with lead/lag/clamp folded in wherever the move's endpoint rests
/// on a facet at the ball's tangent distance.
fn cut_axis(
    mv: &Move,
    facets: &[Triangle],
    boxes: &[[f64; 6]],
    r: f64,
    band: f64,
    rest: &[Move],
    c: &AxisControl,
) -> Option<[f64; 3]> {
    let p = mv.to;
    let mut best: Option<(f64, Point3)> = None;
    for (f, bx) in facets.iter().zip(boxes.iter()) {
        // Chebyshev-ish box distance: if the nearest corner-axis gap
        // already exceeds r + band, no point of the box is in contact.
        let gx = (bx[0] - p.x).max(0.0).max(p.x - bx[3]);
        let gy = (bx[1] - p.y).max(0.0).max(p.y - bx[4]);
        let gz = (bx[2] - p.z).max(0.0).max(p.z - bx[5]);
        if (gx * gx + gy * gy + gz * gz).sqrt() > r + band {
            continue;
        }
        let q = closest_pt_tri(p, &[f.a, f.b, f.c]);
        let d2 = {
            let (dx, dy, dz) = (p.x - q.x, p.y - q.y, p.z - q.z);
            dx * dx + dy * dy + dz * dz
        };
        if best.is_none_or(|(bd, _)| d2 < bd) {
            best = Some((d2, q));
        }
    }
    let (d2, q) = best?;
    let d = d2.sqrt();
    // Contact (or a bite - verify is the law that judges a bite, the
    // normal here is still the honest direction out of the surface).
    if d > r + band || d <= 1e-9 {
        return None;
    }
    let n = Point3::new((p.x - q.x) / d, (p.y - q.y) / d, (p.z - q.z) / d);
    // The feed direction: this move's travel; a vertical entry has
    // none of its own, so the next move's states where the cut goes.
    let mut t = p - mv.from;
    if tl(t) <= 1e-9 {
        t = rest
            .iter()
            .map(|m| m.to - p)
            .find(|&tt| tl(tt) > 1e-9)
            .unwrap_or(t);
    }
    if tl(t) <= 1e-9 {
        // Nowhere to lead: the path ends here; hold the normal.
        return Some([n.x, n.y, n.z]);
    }
    match command_axis(n, t, c) {
        Some(a) => Some([a.x, a.y, a.z]),
        None => {
            // A control that states no direction (a NaN the sane
            // door somehow missed): hold vertical, say so - never
            // invent an angle the law refused to compute.
            eprintln!("multiaxis: the axis law stated no direction at {p:?}; the move holds vertical");
            None
        }
    }
}

/// The length of a difference vector, spelled once for `cut_axis`.
fn tl(v: Point3) -> f64 {
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multiaxis::tilt::angle_between;

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

    // - stamp-level law, over one clean pitched facet -
    //
    // The generator tests (tests/multiaxis.rs) prove the choreography;
    // these prove the STAMP's exact angle identities, which a dome
    // mesh's band angle can only bound. The world is a plane pitched
    // 30 degrees about Y, tiled twice, and a straight ladder of ball
    // centers lying on it at the tangent distance |c - q| = r:
    // there, contact is face-interior, n is the same exact vector at
    // every step, and the feed chord lies IN the plane (n . t = 0),
    // so the lead and lag rotations hold their angles exactly.

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
        let e = stamp_axes(&path, &pitched_plane(), r, &AxisControl::default()).err();
        assert!(matches!(e, Some(GenError::Invalid(_))), "got {e:?}");
    }
}
