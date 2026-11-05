//! The arc entry (M11.1): a circular ramp in the vertical plane of the
//! first cut, tangent to it at the bottom, flattened to a chord fan.
//!
//! Provenance: the tangential arc lead-in of general machining practice
//! (circular interpolation in a vertical plane, RS-274 / ISO 6983 G18 /
//! G19 semantics, as public G-code manuals describe it) and the sagitta
//! law already used by `cl::Move::chords`; the legality argument is the
//! ramp's own (Clean-Room Protocol: no vendor material consulted).
//!
//! Why a fan and not one arc move: the CL vocabulary's `ArcMove`
//! carries an XY center only - an arc turns in XY, a Z change along it
//! is a helix - and the post states no plane word (no G17/G18/G19 is
//! ever emitted). A vertical-plane arc therefore has no honest single
//! move to ride; it is posted as feed chords whose sagitta stays inside
//! the caller's tolerance, which every reader of the CL (verify, sim,
//! post, golden dump) already understands exactly.
//!
//! The geometry, in the plane spanned by the cut direction `u` and Z,
//! with `s` measured along `u` from the pass entry `e`: the circle of
//! radius `R` centered at `(s, z) = (0, z_cut + R)` touches the cut
//! plane at `e` with a horizontal tangent along `+u`. The drop
//! `h = z_top - z_cut` is covered by the sweep `theta0 = acos(1 - h/R)`
//! (at most a quarter turn, which is why `R >= h`), starting `L = R sin
//! theta0` behind `e`. The cutter's XY footprint over the whole move is
//! the straight segment `start -> e`, and its Z stays in
//! `[z_cut, z_top]` - exactly the ramp's diagonal footprint, so the
//! ramp's chord law (one chord test at the destination level, the
//! islands superset covers every plane above) certifies the arc too.

use crate::cl::{lattice, MoveKind, PathBuilder};
use crate::geo2d::Point2;
use crate::geometry::Point3;

/// The most chords one arc entry may post: bounds the fan on a
/// nonsense tolerance instead of letting it grow without end.
pub const ARC_ENTRY_MAX_CHORDS: usize = 4096;

/// How many radii the lead room is asked about, halving from the
/// nominal down to the drop itself (the quarter turn). Bounds the
/// search on any input.
const RADIUS_TRIES: usize = 24;

/// A certified circular ramp: where it starts (on the entry plane,
/// `z_top`), the radius it was proved at, and the fan of feed points
/// after the start - the last one is exactly the pass entry at the cut
/// plane.
#[derive(Clone, Debug, PartialEq)]
pub struct ArcLead {
    pub start: Point2,
    pub radius: f64,
    pub fan: Vec<Point3>,
}

/// Plan the arc entry onto `e` at `z_cut` from `z_top`, tangent at the
/// bottom to the first cut's direction `dir` (any non-zero length).
/// `nominal_r` is the radius asked first; the search halves toward the
/// drop `h` (the quarter turn, the smallest arc that covers the drop
/// without a vertical leg) and takes the first radius whose lead chord
/// `clears` proves. `tol` bounds every chord's sagitta.
///
/// `Err` names why nothing was proved - a drop that is not positive or
/// not finite, a cut with no direction, a nonsense tolerance, no lead
/// the room allows, a fan past [`ARC_ENTRY_MAX_CHORDS`] - and the
/// caller substitutes exactly as the ramp does; this never panics and
/// never loops past its bounds.
pub fn arc_lead(
    e: Point2,
    dir: Point2,
    z_top: f64,
    z_cut: f64,
    nominal_r: f64,
    tol: f64,
    clears: impl Fn(Point2, Point2) -> bool,
) -> Result<ArcLead, &'static str> {
    let h = z_top - z_cut;
    if !(h.is_finite() && h > 0.0) || !e.x.is_finite() || !e.y.is_finite() {
        return Err("the arc has no drop to make");
    }
    let len = dir.x.hypot(dir.y);
    if !(len.is_finite() && len > 0.0) {
        return Err("the first cut has no direction to be tangent to");
    }
    if !(tol.is_finite() && tol > 0.0) {
        return Err("the arc's chord tolerance is not a length");
    }
    let u = Point2::new(dir.x / len, dir.y / len);
    let mut r = if nominal_r.is_finite() { nominal_r.max(h) } else { h };
    let mut short = false;
    for _ in 0..RADIUS_TRIES {
        match lead_at(e, u, h, z_cut, r, tol) {
            Ok(lead) if clears(lead.start, e) => return Ok(lead),
            Ok(_) => {}
            Err(why) if why == TOO_SHORT => short = true,
            Err(why) => return Err(why),
        }
        if r <= h {
            break;
        }
        // Halve toward the quarter turn; land on it exactly last.
        r = (r * 0.5).max(h);
    }
    if short {
        return Err(TOO_SHORT);
    }
    Err("the arc's lead room is blocked")
}

const TOO_SHORT: &str = "the arc's lead is shorter than the lattice";

/// The fan for one radius (`r >= h`, both positive and finite).
fn lead_at(e: Point2, u: Point2, h: f64, z_cut: f64, r: f64, tol: f64) -> Result<ArcLead, &'static str> {
    // cos(theta0) = 1 - h/r, clamped against float noise at r == h.
    let theta0 = (1.0 - h / r).clamp(-1.0, 1.0).acos();
    let lead = r * theta0.sin();
    let start = Point2::new(lattice(e.x - u.x * lead), lattice(e.y - u.y * lead));
    if start.x == e.x && start.y == e.y {
        return Err(TOO_SHORT);
    }
    // Chord angle whose sagitta r(1 - cos(d/2)) is the tolerance; a
    // tolerance past the radius allows one chord per half turn.
    let half = if tol >= r { std::f64::consts::FRAC_PI_2 } else { (1.0 - tol / r).acos() };
    // A tolerance under the radius's float resolution rounds the chord
    // angle to zero: no finite fan meets it, and the budget says so.
    let n =(theta0 / (2.0 * half)).ceil().max(1.0);
    if !n.is_finite() || n > ARC_ENTRY_MAX_CHORDS as f64 {
        return Err("the arc's chord fan would pass its segment budget");
    }
    let n = n as usize;
    let mut fan = Vec::with_capacity(n);
    for k in 1..n {
        let t = theta0 * (1.0 - k as f64 / n as f64);
        let s = -r * t.sin();
        let z = z_cut + r * (1.0 - t.cos());
        fan.push(Point3::new(lattice(e.x + u.x * s), lattice(e.y + u.y * s), lattice(z)));
    }
    fan.push(Point3::new(e.x, e.y, z_cut));
    Ok(ArcLead { start, radius: r, fan })
}

/// The cut direction at the start of a fitted walk's first segment: a
/// line's first leg, an arc's tangent (perpendicular to the radius, on
/// the side the arc turns). `None` when the walk is empty or degenerate.
pub(crate) fn seg_dir(e: Point2, first: &crate::machining2d::Seg) -> Option<Point2> {
    use crate::machining2d::Seg;
    let d = match first {
        Seg::Line(pts) => {
            let q = pts.iter().find(|q| q.x != e.x || q.y != e.y)?;
            Point2::new(q.x - e.x, q.y - e.y)
        }
        Seg::Arc { center, ccw, .. } => {
            let (rx, ry) = (e.x - center.x, e.y - center.y);
            if *ccw { Point2::new(-ry, rx) } else { Point2::new(ry, -rx) }
        }
    };
    (d.x != 0.0 || d.y != 0.0).then_some(d)
}

/// The cut direction of a plain point walk: the first vertex past `e`
/// that is not `e` itself.
pub(crate) fn pts_dir(e: Point2, rest: &[Point2]) -> Option<Point2> {
    let q = rest.iter().find(|q| q.x != e.x || q.y != e.y)?;
    Some(Point2::new(q.x - e.x, q.y - e.y))
}

/// Post a certified lead: traverse at `clearance_z` to its start when
/// the tool stands elsewhere (the helix arm's own traverse), drop at
/// rapid to `z_top` - air by the superset law, the start is a center
/// point of the destination level - then feed the fan.
pub(crate) fn push_arc_lead(b: &mut PathBuilder, lead: &ArcLead, clearance_z: f64, z_top: f64, feed: f64) {
    let s = lead.start;
    let cur = b.at();
    if cur.x != s.x || cur.y != s.y {
        b.push(MoveKind::Rapid, Point3::new(s.x, s.y, clearance_z), None);
    }
    b.push(MoveKind::Rapid, Point3::new(s.x, s.y, z_top), None);
    push_vertical_arc(b, lead, feed);
}

/// THE one place a vertical-plane arc is emitted (M11.2 C): every arc
/// entry, wall arc and arc level transition posts through here, so the
/// day the CL carries the arc's plane (M11.2 B) this body becomes one
/// G18/G19 arc move and nothing else changes. Today: the certified
/// chord fan, each chord's sagitta inside the tolerance it was planned
/// at. The builder must stand at `lead.start` on the arc's top plane.
pub(crate) fn push_vertical_arc(b: &mut PathBuilder, lead: &ArcLead, feed: f64) {
    for q in &lead.fan {
        b.push(MoveKind::Feed, *q, Some(feed));
    }
}
