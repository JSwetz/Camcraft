//! Split from `machining2d.rs` by M5.0.1; structure only.

use crate::cl::{MoveKind, PathBuilder};
use crate::entry::{Entry3D, chord_hits_plan, dive_point, helix_down};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;

use super::arcs::{Seg, fit_arcs, push_segs};
use super::slots::chord_clears;
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
/// hop between passes of one level), the two feeds, and the entry
/// family the ladder crosses a level plane with (M5.1 H).
pub(super) struct Motion {
    clearance_z: f64,
    hop_z: f64,
    feed: f64,
    plunge_feed: f64,
    /// How a level change engages material. `Plunge` is the legacy
    /// climb-and-dive every depth-stepping operation has always
    /// posted; the pocket's validation already proved the other arms'
    /// radii against the tool, the way `machining3d`'s does.
    entry: Entry3D,
    helix_radius: f64,
}

impl Motion {
    /// `retract_z: None` hops at clearance - the legacy shape, every
    /// lift a full climb. A set plane is clamped at clearance, so a
    /// pre-validation file whose retract sits above clearance still
    /// crosses at the plane the machine already obeyed. The entry
    /// family starts at `Plunge`: slot and trace cross levels the way
    /// they always have, and only the pocket opts into [`with_entry`].
    pub(super) fn new(clearance_z: f64, retract_z: Option<f64>, feed: f64, plunge_feed: f64) -> Self {
        Self {
            clearance_z,
            hop_z: retract_z.unwrap_or(clearance_z).min(clearance_z),
            feed,
            plunge_feed,
            entry: Entry3D::default(),
            helix_radius: 0.0,
        }
    }

    /// Opt this ladder into the entry family (the pocket's `entry`
    /// parameter); `helix_radius` is the dive circle, ignored by the
    /// other arms.
    pub(super) fn with_entry(mut self, entry: Entry3D, helix_radius: f64) -> Self {
        self.entry = entry;
        self.helix_radius = helix_radius;
        self
    }
}

/// `center` is the level plan's cutter-center area - the same region at
/// every level, because a planar pocket's passes are one tiling walked
/// at each plane. `Some` lets a non-`Plunge` entry family prove its
/// level-crossing chords against it (the pocket); `None` keeps the
/// climb-and-dive every crossing used before M5.1 H (slot, trace),
/// which no arm can change without new proofs.
pub(super) fn run_levels(
    b: &mut PathBuilder,
    passes: &[Vec<Point2>],
    zs: &[f64],
    m: &Motion,
    fit: Option<f64>,
    center: Option<&Region>,
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
            let e = Point2::new(entry.x, entry.y);
            // A plane crossing (the tool last cut at another z, and
            // this is not the path's very first entry). The entry
            // family's arms replace the climb-and-dive here; every
            // arm ends standing at (e, z), so the walk below is
            // shared. Between-pass moves WITHIN a level never take an
            // arm - their floors are already swept; that is the
            // ladder.rs split too.
            let crossing = !first && last_cut_z != Some(z);
            match (crossing, m.entry, center) {
                (true, Entry3D::Ramp, Some(cen)) => {
                    // The diagonal's legality is the single center the
                    // whole ladder shares: it is the deepest cut at the
                    // crossing's bottom, every plane above crosses the
                    // same material columns already swept by the level
                    // above, and one chord test covers the move.
                    let cur = b.at();
                    let leg = Point2::new(cur.x, cur.y);
                    let prev_z = last_cut_z.unwrap_or(z);
                    if chord_clears(cen, leg, e) {
                        b.push(MoveKind::Rapid, Point3::new(leg.x, leg.y, prev_z), None);
                        b.push(MoveKind::Feed, Point3::new(e.x, e.y, z), Some(m.feed));
                    } else {
                        b.push(MoveKind::Rapid, Point3::new(e.x, e.y, m.clearance_z), None);
                        b.push(MoveKind::Plunge, Point3::new(e.x, e.y, z), Some(m.plunge_feed));
                    }
                }
                (true, Entry3D::Helix, Some(cen)) => {
                    // Pass ends sit on the walls by construction, so
                    // the orbit is never legal about one: the dive
                    // lives at the midpoint of the plan's longest run,
                    // where the widened disc (tool radius + helix
                    // radius) has room at both planes it crosses - the
                    // same region twice, because the planes see the
                    // same tiling. An erosion that fails (a plan too
                    // narrow for the disc, a clipper edge) leaves the
                    // dive None: the honest fallback is the climb and
                    // dive the pocket has always posted.
                    let cur = b.at();
                    let leg = Point2::new(cur.x, cur.y);
                    let prev_z = last_cut_z.unwrap_or(z);
                    let dive = cen
                        .offset(-(m.helix_radius + 1e-3))
                        .ok()
                        .and_then(|leg_r| dive_point(passes, &leg_r, &leg_r));
                    if let Some(d) = dive {
                        if leg.x != d.x || leg.y != d.y {
                            b.push(MoveKind::Rapid, Point3::new(d.x, d.y, m.clearance_z), None);
                        }
                        b.push(MoveKind::Rapid, Point3::new(d.x, d.y, prev_z), None);
                        helix_down(b, d, prev_z, z, m.helix_radius, m.feed);
                        if chord_clears(cen, d, e) && !chord_hits_plan(d, e, passes) {
                            b.push(MoveKind::Feed, Point3::new(e.x, e.y, z), Some(m.feed));
                        } else {
                            b.push(MoveKind::Rapid, Point3::new(d.x, d.y, m.hop_z), None);
                            b.push(MoveKind::Rapid, Point3::new(e.x, e.y, m.hop_z), None);
                            b.push(
                                MoveKind::Plunge,
                                Point3::new(e.x, e.y, z),
                                Some(m.plunge_feed),
                            );
                        }
                    } else {
                        b.push(MoveKind::Rapid, Point3::new(e.x, e.y, m.clearance_z), None);
                        b.push(MoveKind::Plunge, Point3::new(e.x, e.y, z), Some(m.plunge_feed));
                    }
                }
                // Plunge, a first entry, an in-level join, or a
                // ladder that passed no center: the climb-and-dive
                // this function has always posted.
                _ => {
                    b.push(MoveKind::Rapid, Point3::new(e.x, e.y, lift_z), None);
                    b.push(MoveKind::Plunge, Point3::new(e.x, e.y, z), Some(m.plunge_feed));
                }
            }
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
