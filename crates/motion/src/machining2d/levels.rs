//! Split from `machining2d.rs` by M5.0.1; structure only.

use crate::cl::{MoveKind, PathBuilder};
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::arcs::{Seg, fit_arcs, push_segs};
/// Cut levels from `top` down to `bottom`, snapped so the last level is
/// `bottom` exactly. Shared by depth-stepping operations.
pub(crate) fn cut_levels(top: f64, bottom: f64, stepdown: f64) -> Vec<f64> {
    let span = top - bottom;
    if span <= 0.0 {
        return vec![bottom];
    }
    let tol = span * 1e-9;
    let mut zs = Vec::new();
    let mut z = top;
    while z > bottom + tol {
        zs.push(z);
        z -= stepdown;
    }
    zs.push(bottom);
    zs
}

/// Run XY passes at each of the cut levels: every pass starts with a lift
/// (except the very first entry) and a rapid transfer, then plunges at
/// the pass start and feeds along it. The builder drops zero-length
/// moves, so the first entry's lift collapses when the path already
/// starts at clearance above the entry point. A level change lifts to
/// `clearance_z` - the new level engages material the previous one never
/// touched, and clearance is the honest plane for that. A between-pass
/// lift *within* a level travels `hop_z`: floor already swept, air over
/// everything still standing, and a full clearance climb between passes
/// of one level pays a climb nobody asked for. `hop_z` is validated at
/// or above `top_z`, so a hop is never a slide through solid. Shared by
/// the depth-stepping area operations (pocket, slot, trace).
/// `fit = Some(tol)` runs each pass through [`fit_arcs`] once (the walk
/// shape is level-independent): waterline rings of a fine tessellation
/// travel as arcs. `None` keeps the per-point feed walk every straight
/// generator has always had.
/// The motion envelope a depth-stepping generator shares with
/// [`run_levels`]: the two lift planes (clearance for a level change,
/// hop between passes of one level) and the two feeds.
pub(super) struct Motion {
    clearance_z: f64,
    hop_z: f64,
    feed: f64,
    plunge_feed: f64,
}

impl Motion {
    /// `retract_z: None` hops at clearance - the legacy shape, every
    /// lift a full climb. A set plane is clamped at clearance, so a
    /// pre-validation file whose retract sits above clearance still
    /// crosses at the plane the machine already obeyed.
    pub(super) fn new(clearance_z: f64, retract_z: Option<f64>, feed: f64, plunge_feed: f64) -> Self {
        Self {
            clearance_z,
            hop_z: retract_z.unwrap_or(clearance_z).min(clearance_z),
            feed,
            plunge_feed,
        }
    }
}

pub(super) fn run_levels(
    b: &mut PathBuilder,
    passes: &[Vec<Point2>],
    zs: &[f64],
    m: &Motion,
    fit: Option<f64>,
) {
    let runs: Vec<Vec<Seg>> = match fit {
        Some(tol) => passes.iter().map(|p| fit_arcs(p, tol)).collect(),
        None => Vec::new(),
    };
    let mut first = true;
    // The z the tool last ENGAGED (cut at), not the z the loop last
    // visited: a hop is earned only by having floor already swept at
    // this very level. A level change - or a first entry, or a level
    // whose earlier passes came out degenerate - crosses in over new
    // material and keeps the full clearance climb.
    let mut last_cut_z: Option<f64> = None;
    for &z in zs {
        for (pi, pass) in passes.iter().enumerate() {
            let Some((entry, rest)) = pass.split_first() else {
                continue;
            };
            if rest.is_empty() {
                continue;
            }
            let lift_z = if last_cut_z == Some(z) { m.hop_z } else { m.clearance_z };
            if !first {
                let cur = b.at();
                b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, lift_z), None);
            }
            b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, lift_z), None);
            b.push(MoveKind::Plunge, Point3::new(entry.x, entry.y, z), Some(m.plunge_feed));
            first = false;
            match fit {
                Some(_) => push_segs(b, &runs[pi], z, m.feed),
                None => {
                    for q in rest {
                        b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), Some(m.feed));
                    }
                }
            }
            last_cut_z = Some(z);
        }
    }
}
