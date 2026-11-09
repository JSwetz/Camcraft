//! The swarf exit rise (#48): a raster line's climb off its last
//! contact to the clearance plane, posed as a RAPID under the pose
//! the line cut with, wherever that is proved to be air.
//!
//! The base raster leaves every line with a straight feed up Z to the
//! clearance plane. Its end stands in air, where the stamp states no
//! lean (a lean is a claim of contact: the verifier's posed-contact
//! law reads a posed feed in air as a cut standing off its surface),
//! so a swarf program that lay along the blade turned upright on the
//! rise - a turn past the link threshold on every raster end, taken
//! through a retract / swing / re-enter air link, and back again on
//! the next line's plunge. The ball's path up the rise does not
//! depend on the pose (a sphere about its own centre); only the flank
//! does. So where
//!
//! - the station is a feed straight up Z (no XY travel),
//! - its end ball stands in air (past the contact band),
//! - the ball never comes nearer the part on the way up than it stood
//!   at the rise's start (the link law's own floor: `min(d0, r)`),
//! - and the held pose is a lean, not under level, whose flank is
//!   proved in air ([`flank_clear`]) at every sample up the rise,
//!
//! the rise is a rapid under the held pose: no turn at all, so no link
//! (the lying pose then rides the clearance rapid that follows, as
//! every rapid inherits its pose, and the next line's plunge states
//! its own). A rapid is not a cut and claims no contact; the
//! verifier's posed-rapid solid law judges its swept stack as it does
//! every link retract's. Anything not proved keeps the upright feed,
//! and the link law takes its turn through air as before.
//!
//! Provenance: retract-in-air under the cutting pose, with no tool
//! axis change off the surface, is general multi-axis practice; the
//! flank proof is the stamp's own. Our own design notes; no vendor
//! material consulted.

use crate::cl::{is_vertical, MoveKind};
use crate::finishing3d::closest_pt_tri;
use crate::geometry::Point3;

use super::retip::along;
use super::stamp::Station;
use super::stamp_link::flank_clear;
use super::stamp_pose::{Mesh, Ruled};

/// Whether station `s` is an exit rise the swarf stamp may run as a
/// rapid under `held` (module header). `band` is the stamp's contact
/// band, `r` the ball radius.
pub(super) fn rapid_rise(s: &Station, mesh: &Mesh, r: f64, band: f64, rl: Ruled, held: [f64; 3]) -> bool {
    if s.kind != MoveKind::Feed || is_vertical(&held) || held[2] < -1e-9 {
        return false;
    }
    let (dx, dy, dz) = (s.to.x - s.from.x, s.to.y - s.from.y, s.to.z - s.from.z);
    if !(dz > 1e-9 && dx.hypot(dy) <= 1e-9 && dz.is_finite()) {
        return false;
    }
    let d0 = nearest(s.from, mesh);
    if !(d0.is_finite() && nearest(s.to, mesh) > r + band) {
        return false;
    }
    let floor = d0.min(r) - 1e-6;
    let step = (r * 0.5).max(1e-3);
    let k = ((dz / step).ceil() as usize).clamp(1, 4096);
    (0..=k).all(|j| {
        let c = along(s.from, [0.0, 0.0, 1.0], dz * j as f64 / k as f64);
        (j == 0 || nearest(c, mesh) >= floor)
            && flank_clear(c, held, r, rl.tol, mesh.facets, mesh.boxes, &mesh.bounds)
    })
}

/// The distance from `p` to the nearest facet (infinite with none).
fn nearest(p: Point3, mesh: &Mesh) -> f64 {
    let mut best = f64::INFINITY;
    for (f, bx) in mesh.facets.iter().zip(mesh.boxes) {
        let gx = (bx[0] - p.x).max(0.0).max(p.x - bx[3]);
        let gy = (bx[1] - p.y).max(0.0).max(p.y - bx[4]);
        let gz = (bx[2] - p.z).max(0.0).max(p.z - bx[5]);
        if (gx * gx + gy * gy + gz * gz).sqrt() >= best {
            continue;
        }
        let q = closest_pt_tri(p, &[f.a, f.b, f.c]);
        let (ex, ey, ez) = (p.x - q.x, p.y - q.y, p.z - q.z);
        best = best.min((ex * ex + ey * ey + ez * ez).sqrt());
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Triangle;

    const R: f64 = 3.0;
    const LYING: [f64; 3] = [1.0, 0.0, 0.0];

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    /// A 40 mm floor at Z 0, and (with `wall`) a wall standing across
    /// +X at X 10, up to Z 40.
    fn part(wall: bool) -> Vec<Triangle> {
        let mut f = vec![
            Triangle::new(p(-20.0, -20.0, 0.0), p(20.0, -20.0, 0.0), p(20.0, 20.0, 0.0)),
            Triangle::new(p(-20.0, -20.0, 0.0), p(20.0, 20.0, 0.0), p(-20.0, 20.0, 0.0)),
        ];
        if wall {
            f.push(Triangle::new(p(10.0, -20.0, 0.0), p(10.0, 20.0, 0.0), p(10.0, 20.0, 40.0)));
            f.push(Triangle::new(p(10.0, -20.0, 0.0), p(10.0, 20.0, 40.0), p(10.0, -20.0, 40.0)));
        }
        f
    }

    fn rise(facets: &[Triangle], kind: MoveKind, to: Point3, held: [f64; 3]) -> bool {
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
        let mesh = Mesh::new(facets, &boxes);
        let ruled = vec![true; facets.len()];
        let rl = Ruled { dir: p(1.0, 0.0, 0.0), ruled: &ruled, tol: 0.1 };
        let s = Station { kind, feed: Some(500.0), from: p(0.0, 0.0, R), to, mv: 0 };
        rapid_rise(&s, &mesh, R, 0.1, rl, held)
    }

    #[test]
    fn a_rise_into_air_under_a_clear_lean_is_a_rapid() {
        assert!(rise(&part(false), MoveKind::Feed, p(0.0, 0.0, 28.0), LYING));
    }

    #[test]
    fn a_rise_whose_flank_meets_a_wall_keeps_its_feed() {
        // The lying flank points +X into the wall 10 mm off: the link
        // law takes the turn through air instead.
        assert!(!rise(&part(true), MoveKind::Feed, p(0.0, 0.0, 28.0), LYING));
    }

    #[test]
    fn only_a_straight_feed_up_from_a_lean_into_air_qualifies() {
        let f = part(false);
        assert!(!rise(&f, MoveKind::Feed, p(1.0, 0.0, 28.0), LYING), "XY travel");
        assert!(!rise(&f, MoveKind::Plunge, p(0.0, 0.0, 28.0), LYING), "a plunge");
        assert!(!rise(&f, MoveKind::Feed, p(0.0, 0.0, 28.0), [0.0, 0.0, 1.0]), "upright held");
        assert!(!rise(&f, MoveKind::Feed, p(0.0, 0.0, 28.0), [0.6, 0.0, -0.8]), "under level");
        assert!(!rise(&f, MoveKind::Feed, p(0.0, 0.0, R + 0.05), LYING), "ends in contact");
        assert!(!rise(&[], MoveKind::Feed, p(0.0, 0.0, 28.0), LYING), "no part to prove against");
    }
}
