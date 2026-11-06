//! The clipped finish's ramped re-entry (M13.10, the M11.5 owed
//! clipped-entry law): a non-centre-cutting ball may not plunge into
//! the stock standing over a clipped piece, so the re-entry slides in
//! ALONG the kept chain instead - the same certified-diagonal law the
//! ring ladder's `entry_ramp` uses, read with the clip's own currency:
//! every point of the ramp stands vertically over a point of the kept
//! chain, which the whole-model kernel certified as a tangent centre, so
//! the ramp is in air over the model by the 3-axis tangent law and
//! inside the centre area because the chain is. Called by `finish_clip`
//! under the centre-cutting guard's `entry_ramp` retry.
//!
//! Provenance: the ramp entry of machining handbooks (a non-centre-
//! cutting end mill may ramp into material but not plunge); the lifted-
//! over-the-certified-chain certification is this project's own design
//! note. No vendor material consulted.

use super::finish_clip::lerp;
use crate::cl::{Move, MoveKind};
use crate::geometry::Point3;

/// Two lengths closer than this (mm) are one.
const LEN_EPS: f64 = 1e-12;

/// The ramp's vertices, first to last, for the run starting at
/// `run[0].from`: it starts on the solid's top plane (`top`), at the
/// chain point `drop / tan(angle_deg)` along from the run's start, and
/// descends at that slope back to the start, vertex for vertex over the
/// chain. `None` says the plunge stands: no drop (air), a chain that
/// ends before the ramp's length (the piece is too small - the audit
/// then refuses by name), or a chain that rises above the ramp's line
/// (a bump the ramp would bite).
pub(super) fn ramp_into(run: &[Move], top: f64, angle_deg: f64, tol: f64) -> Option<Vec<Point3>> {
    let p = run.first()?.from;
    let drop = top - p.z;
    let slope = angle_deg.to_radians().tan();
    if !(drop.is_finite() && drop > LEN_EPS && slope.is_finite() && slope > 0.0) {
        return None;
    }
    let len = drop / slope;
    // The chain's vertices from `p` out, each with its arc length.
    let mut chain = vec![(p, 0.0)];
    let mut acc = 0.0;
    let mut prev = p;
    'walk: for m in run {
        if m.kind == MoveKind::Rapid || m.from != prev {
            break;
        }
        for (a, b) in m.segments3(tol) {
            let d = (b.x - a.x).hypot(b.y - a.y);
            if acc + d >= len {
                let t = if d > 0.0 { (len - acc) / d } else { 0.0 };
                chain.push((lerp(a, b, t), len));
                acc = len;
                break 'walk;
            }
            acc += d;
            chain.push((b, acc));
        }
        prev = m.to;
    }
    if acc < len - LEN_EPS || chain.len() < 2 {
        return None;
    }
    // The ramp's line over each chain vertex, and the bite check: the
    // ramp and the chain are both linear between vertices, so holding
    // at the vertices holds throughout.
    let z_at = |s: f64| p.z + drop * s / len;
    if chain.iter().any(|(c, s)| c.z > z_at(*s) + 1e-9) {
        return None;
    }
    let mut legs: Vec<Point3> = chain.iter().rev().map(|(c, s)| Point3::new(c.x, c.y, z_at(*s))).collect();
    legs.dedup();
    (legs.len() >= 2).then_some(legs)
}
