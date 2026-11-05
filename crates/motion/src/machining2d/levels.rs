//! Split from `machining2d.rs` by M5.0.1; structure only.

use crate::cl::{MoveKind, PathBuilder};
use crate::entry::{Entry3D, chord_hits_plan, dive_room, helix_down};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;

use super::arcs::{Seg, fit_arcs, push_segs};
use super::slots::chord_clears;

/// The certified legal-center area a descent is proved against (M8.2
/// C). Usually a real [`Region`]; for the slot the closed-form
/// cutter-center stadium. The tessellated form of a stadium cannot
/// certify a slot: the row law stands legitimate cutter centers
/// EXACTLY on its boundary, and an inscribed tessellation sits
/// ~1.9e-5·h inside the true arcs - a ramp landing on its own row's
/// endpoint would be refused by the polygon test, over and over. A
/// stadium is convex, so for it the endpoint test IS the chord test:
/// endpoints within h of the axis put the whole chord inside the true
/// stadium, no tessellation error in sight. The polygon rides along
/// only for `dive_room`, which reads triangle centroids - strictly
/// interior points, where an inscribed region can refuse an orbit but
/// never bless a gouge.
pub(super) enum Cert<'a> {
    Region(&'a Region),
    Stadium { a: Point2, b: Point2, h: f64, tess: &'a Region },
}

impl Cert<'_> {
    /// The region `dive_room` triangulates.
    fn room(&self) -> &Region {
        match self {
            Cert::Region(r) => r,
            Cert::Stadium { tess, .. } => tess,
        }
    }

    /// A descent's chord, certified. `1e-7` absorbs float noise in
    /// endpoints the generators construct on the boundary; it is a
    /// tenth of a micrometer, below any cut tolerance and any gouge
    /// that could matter.
    fn chord_ok(&self, p: Point2, q: Point2) -> bool {
        match self {
            Cert::Region(r) => chord_clears(r, p, q),
            Cert::Stadium { a, b, h, .. } => {
                dist_to_seg(p, *a, *b) <= h + 1e-7 && dist_to_seg(q, *a, *b) <= h + 1e-7
            }
        }
    }
}

/// Distance from `p` to the segment `a -> b`: the stadium membership
/// test (a point is in the stadium of half-width h iff it is within
/// h of the axis segment).
fn dist_to_seg(p: Point2, a: Point2, b: Point2) -> f64 {
    let d = Point2::new(b.x - a.x, b.y - a.y);
    let dd = d.x * d.x + d.y * d.y;
    if dd == 0.0 || !dd.is_finite() {
        return (p.x - a.x).hypot(p.y - a.y);
    }
    let t = (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / dd).clamp(0.0, 1.0);
    (p.x - a.x - t * d.x).hypot(p.y - a.y - t * d.y)
}
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
    /// family starts at `Plunge`: the pocket (M8.1), the contour
    /// (M8.2 A) and the slot (M8.2 C) opt into [`with_entry`]; trace
    /// never does - its zero-width kerf admits no arm (the verdict
    /// comment in `slots.rs`).
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
/// at each plane. `Some` lets the entry family prove its descents
/// against it, at the first level and at every crossing (M8.1); `None`
/// keeps the climb-and-dive slot and trace have always posted, which
/// no arm can change without new proofs.
pub(super) fn run_levels(
    b: &mut PathBuilder,
    passes: &[Vec<Point2>],
    zs: &[f64],
    m: &Motion,
    fit: Option<f64>,
    center: Option<&Cert<'_>>,
) {
    let runs: Option<Vec<Vec<Seg>>> =
        fit.map(|tol| passes.iter().map(|p| fit_arcs(p, tol)).collect());
    run_level_walks(b, passes, zs, m, runs.as_deref(), center);
}

/// [`run_levels`] with the walks already built: `runs[i]` is pass `i`
/// past its first point as [`Seg`]s (fitted arcs, M11.1's corner
/// fillets); `None` walks the points. Entries, lifts and links read
/// `passes` exactly as before - a walk only replaces the feed along it.
pub(super) fn run_level_walks(
    b: &mut PathBuilder,
    passes: &[Vec<Point2>],
    zs: &[f64],
    m: &Motion,
    runs: Option<&[Vec<Seg>]>,
    center: Option<&Cert<'_>>,
) {
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
            // A move onto fresh material: a plane crossing (the tool
            // last cut at another z) or the path's very first entry -
            // the entry family's arms replace the climb-and-dive on
            // BOTH (M8.1's law reads the family from level 0, not
            // only at crossings; the `!first` gate cc5d89d left in
            // place made a chosen Helix open with a straight wall
            // plunge, the founding defect wearing a different hat).
            // Every arm ends standing at (e, z), so the walk below is
            // shared. Between-pass moves WITHIN a level never take an
            // arm - their floors are already swept; that is the
            // ladder.rs split too.
            let entering = first || last_cut_z != Some(z);
            match (entering, m.entry, center) {
                (true, Entry3D::Ramp, Some(cen)) => {
                    // The diagonal's legality is the single center the
                    // whole ladder shares: it is the deepest cut at the
                    // crossing's bottom, every plane above crosses the
                    // same material columns already swept by the level
                    // above, and one chord test covers the move. A
                    // blocked chord is substituted, not skipped: M8.1
                    // sends the fallback to the floor's own dive room
                    // (the wall dive it used to post is what the entry
                    // law refuses), and says so.
                    let cur = b.at();
                    let leg = Point2::new(cur.x, cur.y);
                    let prev_z = last_cut_z.unwrap_or(m.clearance_z);
                    if cen.chord_ok(leg, e) {
                        b.push(MoveKind::Rapid, Point3::new(leg.x, leg.y, prev_z), None);
                        b.push(MoveKind::Feed, Point3::new(e.x, e.y, z), Some(m.feed));
                    } else {
                        substituted_dive(b, cen, passes, e, z, m, "the ramp's chord is blocked");
                    }
                }
                (true, Entry3D::Helix, Some(cen)) => {
                    // The orbit's landing spot is the floor's own room
                    // (M8.1): the destination center eroded by the
                    // widened disc, taken at its widest open triangle.
                    // The old chord-midpoint scan resolved None on every
                    // all-arcs ring plan - the tessellated waterline's
                    // midpoints sit on the ring lines, not in the eroded
                    // leg - and the helix died silently into a wall
                    // plunge. Room at the destination is room across the
                    // whole crossed depth: the pocket's plan is one
                    // region at every plane.
                    let cur = b.at();
                    let leg = Point2::new(cur.x, cur.y);
                    let prev_z = last_cut_z.unwrap_or(m.clearance_z);
                    // The found point is the orbit's CENTER; the dive
                    // starts at its near side, `rh` west - one radius
                    // of center-room certifies the whole swept tube
                    // exactly (see `entry::dive_room`, M2.5 F's dome;
                    // the 2-radius ask over-refused the channel,
                    // M8.2's slot).
                    let dive = dive_room(cen.room(), m.helix_radius + 1e-3);
                    if let Some(c) = dive {
                        let d = Point2::new(c.x - m.helix_radius, c.y);
                        if leg.x != d.x || leg.y != d.y {
                            b.push(MoveKind::Rapid, Point3::new(d.x, d.y, m.clearance_z), None);
                        }
                        b.push(MoveKind::Rapid, Point3::new(d.x, d.y, prev_z), None);
                        helix_down(b, d, prev_z, z, m.helix_radius, m.feed);
                        dive_link(b, cen, passes, d, e, z, m);
                    } else {
                        substituted_dive(
                            b,
                            cen,
                            passes,
                            e,
                            z,
                            m,
                            "the widened disc fits nowhere at this level",
                        );
                    }
                }
                (true, Entry3D::Plunge, Some(cen)) => {
                    // The family's axial arm dives where the disc has
                    // room at this level's floor, never at `passes[0]`
                    // on the wall (M8.1). The link to the pass start
                    // then runs over the level's own fresh plane.
                    match dive_room(cen.room(), 0.0) {
                        Some(d) => {
                            let cur = b.at();
                            if cur.x != d.x || cur.y != d.y {
                                b.push(MoveKind::Rapid, Point3::new(d.x, d.y, m.clearance_z), None);
                            }
                            b.push(MoveKind::Plunge, Point3::new(d.x, d.y, z), Some(m.plunge_feed));
                            dive_link(b, cen, passes, d, e, z, m);
                        }
                        None => {
                            substituted_dive(b, cen, passes, e, z, m, "the disc fits nowhere here")
                        }
                    }
                }
                // An in-level join, or a ladder that passed no
                // center: the climb-and-dive this function has
                // always posted - the floor its neighbours already
                // cleared, and the slot/trace shape no arm can
                // change without proofs.
                _ => {
                    b.push(MoveKind::Rapid, Point3::new(e.x, e.y, lift_z), None);
                    b.push(MoveKind::Plunge, Point3::new(e.x, e.y, z), Some(m.plunge_feed));
                }
            }
            first = false;
            match runs.and_then(|r| r.get(pi)) {
                Some(walk) => push_segs(b, walk, z, m.feed),
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

/// The link from a just-completed dive at `d` up to plane `z` to the
/// pass start `e`: feed it when the chord is proven (inside the
/// center, crossing none of the level's planned paths), fly it over -
/// hop, traverse, dive - when it is not. Shared by every arm's entry
/// so the law reads once: an unproven chord never drags the tool
/// across a wall face it has no right to cross. `pub(super)` since
/// M8.2 A: the contour ladder runs the same law with its rings as
/// the plan.
pub(super) fn dive_link(
    b: &mut PathBuilder,
    cen: &Cert,
    passes: &[Vec<Point2>],
    d: Point2,
    e: Point2,
    z: f64,
    m: &Motion,
) {
    if cen.chord_ok(d, e) && !chord_hits_plan(d, e, passes) {
        b.push(MoveKind::Feed, Point3::new(e.x, e.y, z), Some(m.feed));
    } else {
        b.push(MoveKind::Rapid, Point3::new(d.x, d.y, m.hop_z), None);
        b.push(MoveKind::Rapid, Point3::new(e.x, e.y, m.hop_z), None);
        b.push(MoveKind::Plunge, Point3::new(e.x, e.y, z), Some(m.plunge_feed));
    }
}

/// The entry family's honorable failure (M8.1): the stated motion
/// could not be honored at this level for `why`, so the ladder
/// substitutes the axial dive at the floor's own room - and says so on
/// the status line. A floor with no room either (a region too dense to
/// triangulate, an erosion that vanished) falls back to the wall dive
/// the pocket used to post, still loud. `pub(super)` since M8.2 A:
/// the contour ladder's substitutions need the same words spoken the
/// same way.
pub(super) fn substituted_dive(
    b: &mut PathBuilder,
    cen: &Cert,
    passes: &[Vec<Point2>],
    e: Point2,
    z: f64,
    m: &Motion,
    why: &str,
) {
    match dive_room(cen.room(), 0.0) {
        Some(d) => {
            b.note(format!("entry: {why} - axial dive on the floor instead"));
            let cur = b.at();
            if cur.x != d.x || cur.y != d.y {
                b.push(MoveKind::Rapid, Point3::new(d.x, d.y, m.clearance_z), None);
            }
            b.push(MoveKind::Plunge, Point3::new(d.x, d.y, z), Some(m.plunge_feed));
            dive_link(b, cen, passes, d, e, z, m);
        }
        None => {
            b.note(format!(
                "entry: {why} - the floor has no dive room either; dived at the pass start"
            ));
            b.push(MoveKind::Rapid, Point3::new(e.x, e.y, m.clearance_z), None);
            b.push(MoveKind::Plunge, Point3::new(e.x, e.y, z), Some(m.plunge_feed));
        }
    }
}
