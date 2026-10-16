//! 2D machining generators: region + parameters in, CL path out.
//!
//! Provenance: serpentine facing, offset contouring, zigzag and waterline
//! pocketing, stadium slotting rows and engraved trace paths are textbook
//! CAM strategies as described in machine-tool handbooks and published CAM
//! literature; the arc fitter is the classic three-point circle with a
//! two-sided sagitta bound (geometry handbooks, e.g. Farin's curves and
//! surfaces primer). Every number (parameter model, entry/exit overruns,
//! snap rules) is ours. Pure module: no flecs, no
//! world access - the command layer resolves geometry into regions, this
//! module only cuts them.
//!
//! Prime Directive: parameters are validated before any geometry is
//! touched; impossible shapes return `GenError` (or a clean empty result
//! where the plan says collapse means empty), never a partially linked
//! path. Every returned path is chained, finite, and posts real motion.

use std::f64::consts::PI;

use thiserror::Error;

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{GeoError, Loop, Point2, Region};
use crate::geometry::Point3;

#[derive(Error, Debug, PartialEq)]
pub enum GenError {
    #[error("invalid machining parameters: {0}")]
    Invalid(String),
    #[error("nothing to machine: {0}")]
    Empty(String),
    #[error(transparent)]
    Geo(#[from] GeoError),
}

/// Facing: clear the top plane of a footprint in straight XY passes.
///
/// The path lives in world coordinates (regions arrive in the frame the
/// stock is drawn in). Rows band the footprint's Y extent at `stepover`,
/// centered a tool radius in from each edge so the full width is covered;
/// X runs overrun the bbox by a radius for clean entry and exit. Passes
/// snake back and forth at each cut depth - in the single-level default
/// rapids appear only at the two ends, matching how facers actually run;
/// staged facing adds the level-to-level lift through clearance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceParams {
    pub tool_diameter: f64,
    /// Pass spacing in Y; the overlap 2r - stepover is what leaves the
    /// scallop height, which facing simply accepts.
    pub stepover: f64,
    /// Rapid plane above the material.
    pub clearance_z: f64,
    /// The finished surface height.
    pub cut_z: f64,
    /// First cut level for staged facing. `None` - the default, and what
    /// every pre-stepdown file carries - faces in one level at `cut_z`.
    pub top_z: Option<f64>,
    /// Level spacing while staging; must be positive when `top_z` is
    /// set, ignored otherwise.
    pub stepdown: Option<f64>,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Climb milling: the first pass runs -X to +X. Conventional starts
    /// the other way.
    pub climb: bool,
}

impl FaceParams {
    fn validate(&self) -> Result<(), GenError> {
        let mut vals = vec![
            self.tool_diameter,
            self.stepover,
            self.clearance_z,
            self.cut_z,
            self.feed,
            self.plunge_feed,
        ];
        vals.extend([self.top_z, self.stepdown].into_iter().flatten());
        if !vals.into_iter().all(f64::is_finite) {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if let Some(t) = self.top_z {
            if t < self.cut_z {
                return Err(GenError::Invalid("staged top below the finished height".into()));
            }
            if self.clearance_z <= t {
                // Equality is refused too: a level sitting ON the rapid
                // plane would plunge zero-depth, and the builder drops
                // zero-length moves - a silently missing plunge.
                return Err(GenError::Invalid("clearance plane not above the first cut level".into()));
            }
            if !(self.stepdown.is_some_and(|s| s > 0.0)) && t > self.cut_z {
                return Err(GenError::Invalid("staged facing needs a positive stepdown".into()));
            }
        }
        if self.stepdown.is_some_and(|s| s <= 0.0) {
            return Err(GenError::Invalid("stepdown must be positive".into()));
        }
        if self.tool_diameter <= 0.0 {
            return Err(GenError::Invalid("tool diameter must be positive".into()));
        }
        if self.stepover <= 0.0 {
            return Err(GenError::Invalid("stepover must be positive".into()));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        Ok(())
    }
}

/// Serpentine face passes over `region`. One level at `p.cut_z` when
/// `p.top_z` is `None` - the legacy facing, unchanged to the byte - or a
/// staged descent through `cut_levels(top_z, cut_z, stepdown)` when it
/// is set: the rows are computed once and run at every level, each level
/// entered from the clearance plane the same way the single level was.
/// A stepover wider than the tool leaves an uncut band between rows -
/// visible in the path, never silently smoothed over, and never a crash:
/// facing simply cuts where it passes.
pub fn face(region: &Region, p: &FaceParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    let (lo, hi) = match region.bounds() {
        Some(b) => b,
        None => return Err(GenError::Empty("empty footprint".into())),
    };
    let r = p.tool_diameter * 0.5;
    let ys = band(lo.y, hi.y, r, p.stepover);
    let (x_in, x_out) = (lo.x - r, hi.x + r);
    let zs = match (p.top_z, p.stepdown) {
        (Some(t), Some(s)) => cut_levels(t, p.cut_z, s),
        _ => vec![p.cut_z],
    };

    let start_x = if p.climb { x_in } else { x_out };
    let mut b = PathBuilder::new(Point3::new(start_x, ys[0], p.clearance_z));
    for (k, &z) in zs.iter().enumerate() {
        if k > 0 {
            // New material is engaged from the air, never from under:
            // lift to clearance and re-enter above the row's start.
            let at = b.at();
            b.push(MoveKind::Rapid, Point3::new(at.x, at.y, p.clearance_z), None);
            b.push(MoveKind::Rapid, Point3::new(start_x, ys[0], p.clearance_z), None);
        }
        b.push(MoveKind::Plunge, Point3::new(start_x, ys[0], z), Some(p.plunge_feed));
        for (i, y) in ys.iter().enumerate() {
            // Serpentine: even rows run the climb direction, odd rows return.
            let forward = p.climb == (i % 2 == 0);
            b.push(
                MoveKind::Feed,
                Point3::new(if forward { x_out } else { x_in }, *y, z),
                Some(p.feed),
            );
            if i + 1 < ys.len() {
                let at = b.at();
                b.push(MoveKind::Feed, Point3::new(at.x, ys[i + 1], z), Some(p.feed));
            }
        }
    }
    let at = b.at();
    b.push(MoveKind::Rapid, Point3::new(at.x, at.y, p.clearance_z), None);

    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// Row centers banding [a + r, b - r]: start at the low edge's coverage
/// center, stride by `step`, and always end exactly at the high edge's
/// coverage center (a stride that would overshoot is replaced by that
/// snap; a band thinner than one tool diameter gets the single center
/// row). Never empty, never unsorted.
fn band(a: f64, b: f64, r: f64, step: f64) -> Vec<f64> {
    let (first, last) = (a + r, b - r);
    let span = last - first;
    if span <= 0.0 {
        return vec![(a + b) * 0.5];
    }
    let tol = span * 1e-9;
    let mut ys = vec![first];
    let mut y = first;
    while y + step < last - tol {
        y += step;
        ys.push(y);
    }
    if last - *ys.last().unwrap_or(&first) > tol {
        ys.push(last);
    }
    ys
}

/// Entry/exit motion appended to each contour subpath. Tangential leads
/// keep the tool from dwelling (and gouging) at a plunge point directly on
/// the finished wall; linear leads simply overrun the corner in air.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lead {
    /// Plunge straight onto the path.
    None,
    /// Start `length` behind the path along its tangent, run in, and
    /// overrun the same way on exit.
    Linear { length: f64 },
    /// Quarter arc of `radius`, tangent to the path at entry and exit,
    /// bulging to the air side. Posted as one arc move; chord error at
    /// `tolerance` only appears where a path gets flattened downstream
    /// (verification, display, the golden dump).
    Arc { radius: f64 },
}

/// 2D contouring: trim a material region's boundary at successive depths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContourParams {
    pub tool_diameter: f64,
    /// Thickness of material deliberately left on the finished faces for
    /// a later finishing pass.
    pub stock_to_leave: f64,
    pub clearance_z: f64,
    /// Plane tab hops travel in, between the cut and `clearance_z`.
    pub retract_z: f64,
    /// First cut level; material is engaged from here down.
    pub top_z: f64,
    pub bottom_z: f64,
    pub stepdown: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Climb travels each offset loop with the region's canonical
    /// orientation (material on the left); conventional reverses it.
    pub climb: bool,
    pub lead: Lead,
    /// Chord tolerance for arcs: bounds how far a fitted arc may sit
    /// from the tessellation it replaces (both directions), and is the
    /// chord error arcs flatten to wherever a law needs segments.
    pub tolerance: f64,
    /// Tabs: `tab_count` bridges of `tab_width`, holding the part until
    /// the cut drops below `bottom_z + tab_height`.
    pub tab_count: u32,
    pub tab_width: f64,
    pub tab_height: f64,
}

impl ContourParams {
    fn validate(&self) -> Result<(), GenError> {
        let finite = [
            self.tool_diameter,
            self.stock_to_leave,
            self.clearance_z,
            self.retract_z,
            self.top_z,
            self.bottom_z,
            self.stepdown,
            self.feed,
            self.plunge_feed,
            self.tolerance,
            self.tab_width,
            self.tab_height,
        ]
        .into_iter()
        .all(f64::is_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.tool_diameter <= 0.0 || self.stepdown <= 0.0 {
            return Err(GenError::Invalid("diameter and stepdown must be positive".into()));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        if self.stock_to_leave < 0.0 {
            return Err(GenError::Invalid("stock to leave must not be negative".into()));
        }
        if self.top_z < self.bottom_z {
            return Err(GenError::Invalid("cut top below cut bottom".into()));
        }
        if self.clearance_z < self.top_z {
            return Err(GenError::Invalid("clearance plane below the first cut level".into()));
        }
        if self.retract_z < self.top_z {
            // Hops travel at the retract plane; one at or below the
            // material top would drag across solid - a gouge by
            // definition, refused by name. Above clearance is legal in
            // old files and gets clamped, not refused (see `contour`).
            return Err(GenError::Invalid("retract plane sits inside the material".into()));
        }
        if self.tolerance <= 0.0 {
            return Err(GenError::Invalid("tolerance must be positive".into()));
        }
        match self.lead {
            Lead::None => {}
            Lead::Linear { length } if length >= 0.0 && length.is_finite() => {}
            Lead::Arc { radius } if radius > 0.0 => {}
            _ => return Err(GenError::Invalid("bad lead-in parameters".into())),
        }
        if self.tab_count > 0 && (self.tab_width <= 0.0 || self.tab_height < 0.0) {
            return Err(GenError::Invalid("tabs need positive width".into()));
        }
        Ok(())
    }

    /// Cut levels from `top_z` down to `bottom_z`, always ending exactly
    /// at the bottom (same snap rule as facing rows).
    fn levels(&self) -> Vec<f64> {
        cut_levels(self.top_z, self.bottom_z, self.stepdown)
    }
}

/// Cut levels from `top` down to `bottom`, snapped so the last level is
/// `bottom` exactly. Shared by depth-stepping operations.
fn cut_levels(top: f64, bottom: f64, stepdown: f64) -> Vec<f64> {
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
struct Motion {
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
    fn new(clearance_z: f64, retract_z: Option<f64>, feed: f64, plunge_feed: f64) -> Self {
        Self {
            clearance_z,
            hop_z: retract_z.unwrap_or(clearance_z).min(clearance_z),
            feed,
            plunge_feed,
        }
    }
}

fn run_levels(
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

/// Offset-contour `region` (a material cross-section): every level walks
/// the cutter-center loops of the region grown by `r + leave`, so the
/// tool edge grazes the boundary with `leave = 0` and stops a finishing
/// allowance short of it otherwise. The plan phrases this as
/// `- (r - leave)`; that is the same curve written from the void side -
/// here the region is material, so both outer walls and island/hole
/// walls come out of one positive offset. Loop order is deterministic
/// (largest absolute area first, canonically oriented, and traversed
/// forward for climb, reversed for conventional).
///
/// Between subpaths (tab hops) and between loops of one level the tool
/// retracts to the hop plane - `retract_z`, clamped to `clearance_z` so
/// a legacy retract parked above clearance cannot make the hop climb
/// higher than the clearance move it replaced, with the clamp echoed in
/// the panel - rapids across in air, and plunges again. A level change
/// still lifts to full `clearance_z`: the new level engages material the
/// previous one never touched. Rapids inside the footprint below the
/// safe plane are the verifier's business (step 7), flagged, never
/// hidden.
pub fn contour(region: &Region, p: &ContourParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    if region.is_empty() {
        return Err(GenError::Empty("empty footprint".into()));
    }
    let r = p.tool_diameter * 0.5;
    let center = region.offset(r + p.stock_to_leave)?;
    if center.is_empty() {
        return Err(GenError::Empty("offset boundary collapsed".into()));
    }
    let zs = p.levels();
    let mut loops: Vec<&Loop> = center.loops().iter().collect();
    loops.sort_by(|a, b| b.area().abs().total_cmp(&a.area().abs()));

    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    let hop = p.retract_z.min(p.clearance_z);
    let mut first_entry = true;
    for &z in &zs {
        let tabbed = p.tab_count > 0 && z < p.bottom_z + p.tab_height;
        for (li, l) in loops.iter().enumerate() {
            // Canonical orientation is material-on-the-left travel;
            // conventional climbing reverses it. Air is the other side.
            let pts: Vec<Point2> = if p.climb {
                l.points().to_vec()
            } else {
                l.points().iter().rev().copied().collect()
            };
            let ring = Ring::new(pts);
            let keeps = if tabbed {
                ring.tab_keeps(p.tab_count as usize, p.tab_width)
            } else {
                vec![(0.0, ring.len)]
            };
            for (k, &(a, e)) in keeps.iter().enumerate() {
                let poly = ring.arc_between(a, e);
                if poly.len() < 2 {
                    continue;
                }
                let walk = fit_arcs(&poly, p.tolerance);
                // The lead meets the wall on the wall's own tangent at
                // the joint - the fitted walk's true circle tangent, not
                // a chord across it, or the join kinks at the very seam
                // the fitter just smoothed.
                let t0 = walk_tangent(&walk, &poly, true);
                let (entry, lead) = lead_in(poly[0], t0, air_side(t0, p.climb), p);
                if k == 0 && li > 0 && !first_entry {
                    // Loop to loop within one level: the tab-hop
                    // template - up to the hop plane, across, plunge.
                    let cur = b.at();
                    b.push(MoveKind::Feed, Point3::new(cur.x, cur.y, hop), Some(p.feed));
                    b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, hop), None);
                } else if k == 0 {
                    if !first_entry {
                        // Level to level: new material engages from the
                        // one plane that is safe over all of it.
                        let cur = b.at();
                        b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
                    }
                    let cur = b.at();
                    b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, cur.z), None);
                } else {
                    let cur = b.at();
                    b.push(MoveKind::Feed, Point3::new(cur.x, cur.y, hop), Some(p.feed));
                    b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, hop), None);
                }
                b.push(MoveKind::Plunge, Point3::new(entry.x, entry.y, z), Some(p.plunge_feed));
                first_entry = false;
                push_segs(&mut b, &lead, z, p.feed);
                push_segs(&mut b, &walk, z, p.feed);
                let n = poly.len();
                let t1 = walk_tangent(&walk, &poly, false);
                push_segs(
                    &mut b,
                    &lead_out(poly[n - 1], t1, air_side(t1, p.climb), p),
                    z,
                    p.feed,
                );
            }
        }
    }
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);

    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// Contour along an open chain: a run whose ends stop mid-edge instead of
/// a loop around material. The cutter center rides the chain's own
/// points: plunge at one end, feed through every vertex at each level,
/// retract at the other. Climb travels the chain in stored order,
/// conventional reversed, and the same air-side rule sends the lead-in
/// and lead-out off the ends - an open run has no material to its side to
/// keep off, so the ends themselves are where the lead lives. The tab
/// fields do not apply (there is nothing left held between passes), and
/// the level ladder, clearance plane and lead geometry are `contour`'s
/// reused.
///
/// Half the tool overhangs the run on either side; a verifier told about
/// a surrounding region reads that as the wall it is, and says so.
pub fn contour_along_chain(
    pts: &[Point2],
    p: &ContourParams,
) -> Result<Toolpath, GenError> {
    p.validate()?;
    if pts.len() < 2 {
        return Err(GenError::Empty(
            "a centerline run needs at least two points".into(),
        ));
    }
    let poly: Vec<Point2> = if p.climb {
        pts.to_vec()
    } else {
        let mut v = pts.to_vec();
        v.reverse();
        v
    };
    let zs = p.levels();
    let walk = fit_arcs(&poly, p.tolerance);
    let t0 = walk_tangent(&walk, &poly, true);
    let (entry, lead) = lead_in(poly[0], t0, air_side(t0, p.climb), p);
    let n = poly.len();
    let t1 = walk_tangent(&walk, &poly, false);
    let out = lead_out(poly[n - 1], t1, air_side(t1, p.climb), p);

    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    for (k, &z) in zs.iter().enumerate() {
        if k > 0 {
            let cur = b.at();
            b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
        }
        let cur = b.at();
        b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, cur.z), None);
        b.push(MoveKind::Plunge, Point3::new(entry.x, entry.y, z), Some(p.plunge_feed));
        push_segs(&mut b, &lead, z, p.feed);
        push_segs(&mut b, &walk, z, p.feed);
        push_segs(&mut b, &out, z, p.feed);
    }
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);

    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// Which side of travel is air: canonical (climb) travel keeps material on
/// the left, so air is right; reversed travel flips both.
fn air_side(t: Point2, climb: bool) -> Point2 {
    if climb {
        Point2::new(t.y, -t.x)
    } else {
        Point2::new(-t.y, t.x)
    }
}

fn unit(dx: f64, dy: f64) -> Point2 {
    let m = (dx * dx + dy * dy).sqrt();
    if m == 0.0 || !m.is_finite() {
        Point2::new(1.0, 0.0)
    } else {
        Point2::new(dx / m, dy / m)
    }
}

/// One piece of tool motion: a straight feed walk through the listed
/// points (exactly how generators have always walked polylines), or one
/// arc move ending at `to` about `center`. A [`Seg`] list describes a
/// walk starting where the builder's cursor already is, so endpoints
/// chain exactly.
#[derive(Clone, Debug, PartialEq)]
pub enum Seg {
    Line(Vec<Point2>),
    Arc { center: Point2, ccw: bool, to: Point2 },
}

/// Chord tolerance for arc fitting where no parameter carries one
/// (waterline rings, and the depth kinds without a `tolerance` field).
/// An arc may only replace a polyline if both sit within this of each
/// other - the same budget a chord's gouge allowance already spends.
pub const ARC_FIT_TOL: f64 = 0.01;

/// Longest vertex run a single candidate arc may span: bounds the
/// greedy walk on pathological input (no honest tessellation needs it).
const ARC_FIT_MAX_RUN: usize = 4096;

/// How many still-too-straight windows a run may look ahead for its
/// first claim of curvature. A finely tessellated circle only proves
/// "this is a curve" once the window spans enough of it - two vertices
/// into a 1000-gon look flat - and a greedy that gave up at the first
/// flat window would never fit such a rim at all. Straight runs pay
/// this cost once per start vertex and then walk on as lines; no
/// honest tessellation that a `tol`-budgeted arc could replace needs
/// more than a handful of these (sagitta grows quadratically in the
/// window), so the cap only ever bounds pathological flat input.
const ARC_FIT_STRAIGHT_LOOKAHEAD: usize = 64;

/// Walk a polyline's pieces: lines feed through their points, arcs get
/// one [`PathBuilder::push_arc`] each. At cut level `z` with `feed`.
fn push_segs(b: &mut PathBuilder, segs: &[Seg], z: f64, feed: f64) {
    for seg in segs {
        match seg {
            Seg::Line(qs) => {
                for q in qs {
                    b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), Some(feed));
                }
            }
            Seg::Arc { center, ccw, to } => {
                b.push_arc(MoveKind::Feed, Point3::new(to.x, to.y, z), Some(feed), *center, *ccw);
            }
        }
    }
}

/// Travel direction of a fitted walk at its front vertex (`front`) or
/// its last one: the true circle tangent where that end rides an arc,
/// the adjacent-vertex chord otherwise (a straight's chord IS its
/// tangent, and the polyline fallback covers degenerate walks). A lead
/// must leave or join the wall on this vector for the join to be smooth.
fn walk_tangent(segs: &[Seg], poly: &[Point2], front: bool) -> Point2 {
    let n = poly.len();
    if n < 2 {
        return Point2::new(1.0, 0.0);
    }
    let chord = if front {
        unit(poly[1].x - poly[0].x, poly[1].y - poly[0].y)
    } else {
        unit(poly[n - 1].x - poly[n - 2].x, poly[n - 1].y - poly[n - 2].y)
    };
    let arc = match (front, segs.first(), segs.last()) {
        (true, Some(Seg::Arc { center, ccw, .. }), _) => Some((*center, *ccw, poly[0])),
        (false, _, Some(Seg::Arc { center, ccw, to })) => Some((*center, *ccw, *to)),
        _ => None,
    };
    match arc {
        Some((c, ccw, p)) => {
            let (dx, dy) = (p.x - c.x, p.y - c.y);
            let m = (dx * dx + dy * dy).sqrt();
            if m == 0.0 || !m.is_finite() {
                chord
            } else if ccw {
                Point2::new(-dy / m, dx / m)
            } else {
                Point2::new(dy / m, -dx / m)
            }
        }
        None => chord,
    }
}

/// Replace maximal arc-fittable runs of a polyline with arc moves,
/// keeping everything else point-for-point.
///
/// A run earns arcs only if both directions agree within `tol`: every
/// vertex lies within `tol` of one circle (the arc is no lie about the
/// tessellation), and every chord lies within `tol` of that circle's
/// arc (the arc is no surprise against the verified polygon) - so
/// trading chords for the arc moves the tool nowhere the plan didn't
/// already tolerate. Near-straight runs (all deviations within `tol` of
/// the end chord) stay lines on principle: a huge-radius "arc" along a
/// straight would re-describe motion the golden already spells point by
/// point - but a window that is merely too short to have proven its
/// curvature yet extends the run instead of ending it (bounded by
/// [`ARC_FIT_STRAIGHT_LOOKAHEAD`]), because a fine tessellation of a
/// real circle looks flat until the chord spans enough of it. Runs that
/// genuinely fail - corners, coarse tessellations, backtracks - end the
/// run, and a run that claimed nothing advances one vertex at a time,
/// which is exactly the old walk, so polygonal paths are byte-identical
/// through this function.
///
/// Sweeps over half a turn split at a vertex into <= pi pieces (the
/// builder refuses zero-sweep arcs, so an unsplit full circle would
/// erase its ring here); endpoints are always polyline points, so
/// chaining is exact by construction. Consumers fit at use; nothing
/// about a [`crate::geometry::Features`] digest ever depends on this.
pub fn fit_arcs(pts: &[Point2], tol: f64) -> Vec<Seg> {
    let tol = if tol.is_finite() && tol > 0.0 { tol } else { ARC_FIT_TOL };
    let n = pts.len();
    let mut segs = Vec::new();
    let mut s = 0usize;
    while s + 1 < n {
        let cap = (s + ARC_FIT_MAX_RUN).min(n - 1);
        let mut best: Option<(usize, Point2, bool, f64)> = None;
        let mut e = s + 2;
        let mut flat = 0;
        while e <= cap {
            match arc_over(&pts[s..=e], tol) {
                Over::Fit(center, ccw, total) => {
                    best = Some((e, center, ccw, total));
                    e += 1;
                }
                // Too straight to have said anything yet: extend, up
                // to the lookahead's word.
                Over::Straight if flat < ARC_FIT_STRAIGHT_LOOKAHEAD => {
                    flat += 1;
                    e += 1;
                }
                Over::Straight | Over::Fail => break,
            }
        }
        match best {
            Some((e, center, ccw, total)) => {
                // Split at vertices into sub-sweeps of at most half a
                // turn; a closing full circle becomes two arcs meeting at
                // the far vertex, so the builder never sees the
                // zero-sweep single arc that would erase the ring.
                let k = (total / PI).ceil().max(1.0) as usize;
                for m in 1..=k {
                    let idx = if m == k { e } else { s + (e - s) * m / k };
                    segs.push(Seg::Arc {
                        center,
                        ccw,
                        to: pts[idx],
                    });
                }
                s = e;
            }
            None => {
                segs.push(Seg::Line(vec![pts[s + 1]]));
                s += 1;
            }
        }
    }
    segs
}

/// What a window of at least 3 points says about sitting on one circle
/// (see [`fit_arcs`]).
enum Over {
    /// A circle within `tol`, both directions: (center, ccw, total
    /// sweep).
    Fit(Point2, bool, f64),
    /// Flatter than `tol` end to end - the window cannot tell arc from
    /// line yet, and the run's next, longer window might.
    Straight,
    /// Not one circle within `tol`.
    Fail,
}

/// The verdict on whether a run of >= 3 points sits on one circle
/// within `tol` in both directions, and if so which circle (see
/// [`fit_arcs`]). `Straight` is the honest answer when no vertex leaves
/// the end chord by more than `tol`: there is nothing there an arc
/// could describe better than the chords already do.
fn arc_over(run: &[Point2], tol: f64) -> Over {
    let (a, b) = (run[0], run[run.len() - 1]);
    let dev = |p: Point2| chord_dev(p, a, b);
    // Farthest interior vertex from the end chord anchors the circle.
    let mut pivot = None;
    let mut pd = 0.0;
    for p in &run[1..run.len() - 1] {
        let d = dev(*p);
        if !d.is_finite() {
            return Over::Fail;
        }
        if d > pd {
            pd = d;
            pivot = Some(*p);
        }
    }
    // Curvature must exceed the tolerance to justify an arc at all -
    // but a short window on a finely tessellated curve is no refusal,
    // just not yet evidence.
    let Some(pivot) = pivot.filter(|_| pd > tol) else {
        return Over::Straight;
    };
    // A run closing a full circle has an end chord of zero length - no
    // usable anchor at `b`. Second anchor: the vertex farthest from the
    // a-pivot line, which for an honest tessellation is a quarter turn
    // out. Either anchor pair defines the circle - and the turning
    // direction, from the same triple it was fitted on; the validation
    // loops are the actual judges.
    let (c, r, ccw) = match circle3(a, pivot, b) {
        Some((c, r)) => (
            c,
            r,
            ((pivot.x - a.x) * (b.y - pivot.y) - (pivot.y - a.y) * (b.x - pivot.x)) > 0.0,
        ),
        None if dist(a, b) <= tol => {
            let mut second = None;
            let mut sd = 0.0;
            for p in &run[1..] {
                let d = chord_dev(*p, a, pivot);
                if d > sd && d.is_finite() {
                    sd = d;
                    second = Some(*p);
                }
            }
            let Some(anchor) = second.filter(|_| sd > tol) else {
                return Over::Straight;
            };
            let Some((c, r)) = circle3(a, pivot, anchor) else {
                return Over::Fail;
            };
            (
                c,
                r,
                ((pivot.x - a.x) * (anchor.y - pivot.y) - (pivot.y - a.y) * (anchor.x - pivot.x))
                    > 0.0,
            )
        }
        None => return Over::Fail,
    };
    if !r.is_finite() || r <= tol {
        return Over::Fail;
    }
    // Direction one: vertices on the circle.
    for p in run {
        let d = (((p.x - c.x).powi(2) + (p.y - c.y).powi(2)).sqrt() - r).abs();
        if d > tol {
            return Over::Fail;
        }
    }
    // Direction two: chords hug the arc; a step against the claimed
    // direction lands near a full turn here and fails the same test.
    let sag_bound = 1.0 - tol / r;
    let mut total = 0.0;
    for w in run.windows(2) {
        let t0 = (w[0].y - c.y).atan2(w[0].x - c.x);
        let t1 = (w[1].y - c.y).atan2(w[1].x - c.x);
        let step = if ccw { t1 - t0 } else { t0 - t1 };
        if !step.is_finite() {
            return Over::Fail;
        }
        let step = step.rem_euclid(2.0 * PI);
        if (step * 0.5).cos() < sag_bound {
            return Over::Fail;
        }
        total += step;
    }
    if total <= 0.0 || total > 2.0 * PI + tol / r {
        return Over::Fail;
    }
    Over::Fit(c, ccw, total)
}

/// Distance from `p` to segment `a`-`b`.
fn chord_dev(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    if dd <= f64::EPSILON {
        return ((p.x - a.x).powi(2) + (p.y - a.y).powi(2)).sqrt();
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0);
    ((p.x - (a.x + t * dx)).powi(2) + (p.y - (a.y + t * dy)).powi(2)).sqrt()
}

/// Circle through three points: center and radius, or None when they
/// are (near-)collinear or land outside the finite horizon.
fn circle3(a: Point2, b: Point2, c: Point2) -> Option<(Point2, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if !d.is_finite() || d.abs() <= f64::EPSILON {
        return None;
    }
    let (a2, b2, c2) = (
        a.x * a.x + a.y * a.y,
        b.x * b.x + b.y * b.y,
        c.x * c.x + c.y * c.y,
    );
    let ux = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
    let uy = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
    if !ux.is_finite() || !uy.is_finite() {
        return None;
    }
    let o = Point2::new(ux, uy);
    let r = dist(a, o);
    if !r.is_finite() {
        return None;
    }
    Some((o, r))
}

/// (plunge point, segments from it onto the path, ending exactly at
/// `s`).
fn lead_in(s: Point2, t: Point2, air: Point2, p: &ContourParams) -> (Point2, Vec<Seg>) {
    match p.lead {
        Lead::None => (s, Vec::new()),
        Lead::Linear { length } => (
            Point2::new(s.x - t.x * length, s.y - t.y * length),
            vec![Seg::Line(vec![s])],
        ),
        Lead::Arc { radius } => {
            let o = Point2::new(s.x + air.x * radius, s.y + air.y * radius);
            let e = Point2::new(
                s.x + (air.x - t.x) * radius,
                s.y + (air.y - t.y) * radius,
            );
            // Quarter arc e -> s about o: ccw iff s bears left of the
            // start radius. The chord error at p.tolerance only appears
            // when something downstream flattens the move.
            let ccw = ((e.x - o.x) * (s.y - o.y) - (e.y - o.y) * (s.x - o.x)) > 0.0;
            (e, vec![Seg::Arc { center: o, ccw, to: s }])
        }
    }
}

/// Segments carrying the tool off the path end (not including it).
fn lead_out(e: Point2, t: Point2, air: Point2, p: &ContourParams) -> Vec<Seg> {
    match p.lead {
        Lead::None => Vec::new(),
        Lead::Linear { length } => vec![Seg::Line(vec![Point2::new(
            e.x + t.x * length,
            e.y + t.y * length,
        )])],
        Lead::Arc { radius } => {
            let o = Point2::new(e.x + air.x * radius, e.y + air.y * radius);
            let q = Point2::new(
                e.x + (air.x + t.x) * radius,
                e.y + (air.y + t.y) * radius,
            );
            let ccw = ((e.x - o.x) * (q.y - o.y) - (e.y - o.y) * (q.x - o.x)) > 0.0;
            vec![Seg::Arc { center: o, ccw, to: q }]
        }
    }
}

/// A closed polyline parametrized by arc length, for tab windows.
struct Ring {
    pts: Vec<Point2>,
    /// Cumulative length at each vertex; the closing edge runs from
    /// `cum[n-1]` to `len`.
    cum: Vec<f64>,
    len: f64,
}

impl Ring {
    fn new(pts: Vec<Point2>) -> Ring {
        let mut p: Vec<Point2> = Vec::with_capacity(pts.len());
        for q in pts {
            if p.last().is_none_or(|r| *r != q) {
                p.push(q);
            }
        }
        if p.len() > 1 && p.first() == p.last() {
            p.pop();
        }
        let n = p.len();
        let mut cum = vec![0.0; n];
        for i in 1..n {
            cum[i] = cum[i - 1] + dist(p[i - 1], p[i]);
        }
        let len = if n > 1 {
            cum[n - 1] + dist(p[n - 1], p[0])
        } else {
            0.0
        };
        Ring { pts: p, cum, len }
    }

    fn at(&self, s: f64) -> Point2 {
        let n = self.pts.len();
        if n == 0 || self.len <= 0.0 {
            return self.pts.first().copied().unwrap_or(Point2::new(0.0, 0.0));
        }
        let sm = s.rem_euclid(self.len);
        let mut i = n - 1;
        while i > 0 && self.cum[i] > sm {
            i -= 1;
        }
        let (a, b) = (self.pts[i], self.pts[(i + 1) % n]);
        let seg = if i + 1 < n { self.cum[i + 1] - self.cum[i] } else { self.len - self.cum[i] };
        let t = if seg > 0.0 { ((sm - self.cum[i]) / seg).clamp(0.0, 1.0) } else { 0.0 };
        Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
    }

    /// Vertices of the ring sampled from arc position `a` to `b`
    /// (b - a <= len), endpoints interpolated. Vertex i recurs at
    /// `cum[i] + m*len` for every lap m; the walk emits them ordered by
    /// absolute position, which matters for intervals that wrap the
    /// parametrization seam.
    fn arc_between(&self, a: f64, b: f64) -> Vec<Point2> {
        let mut out = vec![self.at(a)];
        let n = self.pts.len();
        let l = self.len;
        if n == 0 || l <= 0.0 || b <= a {
            return out;
        }
        let eps = l * 1e-12;
        // b - a <= len, so each vertex recurs in at most one lap here.
        let m_lo = (a / l).floor() as i64 - 1;
        let m_hi = (b / l).floor() as i64 + 1;
        for m in m_lo..=m_hi {
            for i in 0..n {
                let pos = self.cum[i] + m as f64 * l;
                if pos > a + eps
                    && pos < b - eps
                    && out.last().is_none_or(|q| *q != self.pts[i])
                {
                    out.push(self.pts[i]);
                }
            }
        }
        let e = self.at(b);
        if out.last().is_none_or(|q| *q != e) {
            out.push(e);
        }
        out
    }

    /// Arc intervals to actually cut: the circle minus `count` tab
    /// windows of `width`, centered at equal spacing. Windows wider than
    /// their spacing merge; a fully tabbed ring keeps nothing (the loop
    /// contributes no motion at that level).
    fn tab_keeps(&self, count: usize, width: f64) -> Vec<(f64, f64)> {
        let l = self.len;
        if count == 0 || l <= 0.0 {
            return vec![(0.0, l.max(0.0))];
        }
        let spacing = l / count as f64;
        let hw = width * 0.5;
        if hw >= spacing * 0.5 {
            return Vec::new();
        }
        let eps = l * 1e-12;
        let mut ws: Vec<(f64, f64)> = Vec::new();
        for i in 0..count {
            let c = (i as f64 + 0.5) * spacing;
            let (mut w0, mut w1) = (c - hw, c + hw);
            while w0 < 0.0 {
                w0 += l;
                w1 += l;
            }
            while w0 >= l {
                w0 -= l;
                w1 -= l;
            }
            if w1 <= l {
                ws.push((w0, w1));
            } else {
                ws.push((w0, l));
                ws.push((0.0, w1 - l));
            }
        }
        ws.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut merged: Vec<(f64, f64)> = Vec::with_capacity(ws.len());
        for w in ws {
            match merged.last_mut() {
                Some(last) if w.0 <= last.1 + eps => last.1 = last.1.max(w.1),
                _ => merged.push(w),
            }
        }
        let mut keeps: Vec<(f64, f64)> = Vec::with_capacity(merged.len() + 1);
        let mut cur = 0.0;
        for w in &merged {
            if w.0 > cur + eps {
                keeps.push((cur, w.0));
            }
            cur = cur.max(w.1);
        }
        if cur < l - eps {
            keeps.push((cur, l));
        }
        // Keep intervals at both ends of the parametrization are one
        // arc crossing the seam.
        if keeps.len() > 1 && keeps[0].0 <= eps && keeps[keeps.len() - 1].1 >= l - eps {
            let last = keeps.pop().unwrap_or((0.0, 0.0));
            keeps[0] = (last.0, keeps[0].1 + l);
        }
        keeps
    }
}

fn dist(a: Point2, b: Point2) -> f64 {
    ((a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y)).sqrt()
}

/// True when `p` sits on some loop edge, within a nanometer - exact-zero
/// float tests cannot work here: a computed point on an edge (a chord's
/// midpoint) has round-off of a few ulps against the cross product, and
/// boundary points otherwise read as neither inside nor on. The tolerance
/// is far under the micron quantization of the geometry and far over the
/// cancellation error.
pub fn on_boundary(region: &Region, p: Point2) -> bool {
    const TOL: f64 = 1e-9;
    for l in region.loops() {
        let pts = l.points();
        let n = pts.len();
        for i in 0..n {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            let d = Point2::new(b.x - a.x, b.y - a.y);
            let c = Point2::new(p.x - a.x, p.y - a.y);
            let cross = c.x * d.y - c.y * d.x;
            let dd = d.x * d.x + d.y * d.y;
            if dd == 0.0 || cross * cross > TOL * TOL * dd {
                continue;
            }
            let u = (c.x * d.x + c.y * d.y) / dd;
            if (-TOL..=1.0 + TOL).contains(&u) {
                return true;
            }
        }
    }
    false
}

// ---- pocketing ----

/// How a pocket is filled: straight rows back and forth, or closed rings
/// marching inward.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PocketStrategy {
    Zigzag,
    Waterline,
    /// Waterline where the pocket's outer boundary is round enough for
    /// rings to be the honest shape, Zigzag otherwise. The resolution is
    /// [`PocketStrategy::resolve`] - the panel quotes it verbatim, so an
    /// Auto pocket never hides which path it is cutting.
    Auto,
}

impl PocketStrategy {
    /// What Auto picks for a cutter-center ring: fitted arcs must carry
    /// at least 90% of the boundary's length AND agree on a center
    /// within a quarter stepover - a family of true circles, not corners
    /// that happen to round - and the stepover must still be inside
    /// waterline's reach (rings a whole stepover apart leave a band no
    /// ring's edge reaches when the step exceeds one radius; zigzag
    /// clears the same floor honestly). Any other strategy answers as
    /// itself.
    pub fn resolve(self, outer_ring: &[Point2], stepover: f64, tool_diameter: f64, tol: f64) -> Self {
        match self {
            PocketStrategy::Auto
                if ring_is_circular(outer_ring, tol, stepover)
                    && stepover <= tool_diameter * 0.5 =>
            {
                PocketStrategy::Waterline
            }
            PocketStrategy::Auto => PocketStrategy::Zigzag,
            s => s,
        }
    }
}

/// The circularity law behind [`PocketStrategy::resolve`]: fit the ring
/// with the arc fitter's own tolerance budget and ask what came back.
/// Arc length must cover at least 0.9 of the total; fitted centers must
/// span no more than `stepover / 4` (a concentric circle family agrees
/// on one center; a rounded rectangle's corner arcs do not).
pub fn ring_is_circular(poly: &[Point2], tol: f64, stepover: f64) -> bool {
    if poly.len() < 3 {
        return false;
    }
    // Loop points are vertex lists whose last edge implicitly wraps to the
    // first; the fitter sees plain sequences, so close the ring explicitly
    // before asking it what shape the whole circuit is.
    let mut closed = poly.to_vec();
    if dist(closed[0], *closed.last().unwrap()) > 0.0 {
        closed.push(closed[0]);
    }
    let segs = fit_arcs(&closed, tol);
    let mut arc_len = 0.0;
    let mut line_len = 0.0;
    let mut centers: Vec<Point2> = Vec::new();
    let mut prev = closed[0];
    for seg in &segs {
        match seg {
            Seg::Line(pts) => {
                for q in pts {
                    line_len += dist(prev, *q);
                    prev = *q;
                }
            }
            Seg::Arc { center, to, .. } => {
                // Chord length for the share: an arc is longer than its
                // chord, but coverage compares like-for-like runs, and
                // the 0.9 margin absorbs the difference.
                arc_len += dist(prev, *to);
                centers.push(*center);
                prev = *to;
            }
        }
    }
    let total = arc_len + line_len;
    if total <= 0.0 || !total.is_finite() {
        return false;
    }
    if arc_len / total < 0.9 {
        return false;
    }
    let mut span = 0.0f64;
    for (i, a) in centers.iter().enumerate() {
        for b in centers[i..].iter() {
            span = span.max(dist(*a, *b));
        }
    }
    span <= stepover * 0.25
}

/// 2D pocketing: clear every bit of material inside a region down to
/// `bottom_z`. Islands (region holes) are never entered; the cutter
/// center stays inside the region eroded by the tool radius, so the tool
/// edge never crosses a wall it is meant to leave standing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PocketParams {
    pub tool_diameter: f64,
    /// Distance between successive rows (zigzag) or rings (waterline).
    /// Validated against the tool radius: a wider step leaves bands the
    /// path never reaches, and that is refused rather than accepted
    /// silently.
    pub stepover: f64,
    pub stock_to_leave: f64,
    pub clearance_z: f64,
    /// First cut level; material is engaged from here down.
    pub top_z: f64,
    pub bottom_z: f64,
    pub stepdown: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Zigzag: the first row runs -X to +X. Waterline: rings are
    /// travelled with material on the left of the cut.
    pub climb: bool,
    pub strategy: PocketStrategy,
    /// Between-ring hop plane within one level. `None` hops to
    /// clearance - every pre-M1.5.2 pocket's shape, kept byte-exact.
    pub retract_z: Option<f64>,
}

impl PocketParams {
    fn validate(&self) -> Result<(), GenError> {
        let mut finite = [
            self.tool_diameter,
            self.stock_to_leave,
            self.clearance_z,
            self.top_z,
            self.bottom_z,
            self.stepdown,
            self.stepover,
            self.feed,
            self.plunge_feed,
        ]
        .into_iter()
        .all(f64::is_finite);
        finite &= self.retract_z.is_none_or(f64::is_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.tool_diameter <= 0.0 || self.stepdown <= 0.0 {
            return Err(GenError::Invalid("diameter and stepdown must be positive".into()));
        }
        if self.stepover <= 0.0 {
            return Err(GenError::Invalid("stepover must be positive".into()));
        }
        // Two parallel cut lines a stepover apart cover every point
        // between them (distance at most stepover/2 to the nearer one),
        // and zigzag puts grazing rows on the extreme coverage centers,
        // so stepover <= diameter covers the region. Waterline's innermost
        // ring can stand a whole stepover from the material it collapses
        // around, so it must stay within one radius. Auto is bounded by
        // the looser law: its waterline branch carries the radius bound
        // inside `resolve`, and anything else runs zigzag.
        let reach = match self.strategy {
            PocketStrategy::Zigzag | PocketStrategy::Auto => self.tool_diameter,
            PocketStrategy::Waterline => self.tool_diameter * 0.5,
        };
        if self.stepover > reach {
            return Err(GenError::Invalid(
                "stepover leaves bands the tool cannot reach".into(),
            ));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        if self.stock_to_leave < 0.0 {
            return Err(GenError::Invalid("stock to leave must not be negative".into()));
        }
        if self.top_z < self.bottom_z {
            return Err(GenError::Invalid("cut top below cut bottom".into()));
        }
        if self.clearance_z < self.top_z {
            return Err(GenError::Invalid("clearance plane below the first cut level".into()));
        }
        if self.retract_z.is_some_and(|rz| rz < self.top_z) {
            return Err(GenError::Invalid("retract plane sits inside the material".into()));
        }
        Ok(())
    }
}

/// Pocket `region` (the material to remove; its loops' interiors are
/// islands to leave standing). The cutter-center area is the region
/// eroded by `r + leave` - the opposite sign to contouring, because here
/// the tool must stay inside the material rather than outside it. Every
/// feed point lies in that area, so the tool edge never crosses a wall.
///
/// Transitions: between levels the tool lifts to `clearance_z`, rapids
/// over and plunges again; within a level a straight connection between
/// two cut segments is kept as a Feed exactly when [`chord_clears`]
/// proves it never leaves the cutter-center area (boundary grazing counts
/// as staying in - it is the tangency wall passes are built on), and
/// otherwise it becomes lift-rapid-plunge. Rapids inside the footprint
/// below the safe plane are the verifier's business (step 7).
pub fn pocket(region: &Region, p: &PocketParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    if region.is_empty() {
        return Err(GenError::Empty("empty pocket region".into()));
    }
    let r = p.tool_diameter * 0.5;
    let center = region.offset(-(r + p.stock_to_leave))?;
    if center.is_empty() {
        // Material exists that no legal tool position can reach.
        return Err(GenError::Invalid("pocket is narrower than the tool".into()));
    }
    // Auto commits to a shape here, from the region's own outline - the
    // outermost loop (largest area; holes and islands never decide the
    // pocket's strategy). The panel quotes the same law on the same
    // points, so what was cut is what was said.
    let strategy = match p.strategy {
        PocketStrategy::Auto => {
            let outer = region
                .loops()
                .iter()
                .max_by(|a, b| a.area().abs().total_cmp(&b.area().abs()))
                .map(|l| l.points())
                .unwrap_or(&[]);
            p.strategy.resolve(outer, p.stepover, p.tool_diameter, ARC_FIT_TOL)
        }
        s => s,
    };
    let passes = match strategy {
        PocketStrategy::Zigzag | PocketStrategy::Auto => zigzag_passes(&center, p)?,
        PocketStrategy::Waterline => waterline_passes(&center, p)?,
    };
    let zs = cut_levels(p.top_z, p.bottom_z, p.stepdown);

    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    // Waterline rings circle around material, so they get the arc fitter
    // (rings straight enough to be lines stay exactly that); zigzag rows
    // are straight by construction and keep the plain walk.
    let fit = match strategy {
        PocketStrategy::Zigzag | PocketStrategy::Auto => None,
        PocketStrategy::Waterline => Some(ARC_FIT_TOL),
    };
    let motion = Motion::new(p.clearance_z, p.retract_z, p.feed, p.plunge_feed);
    run_levels(&mut b, &passes, &zs, &motion, fit);
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);

    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// Serpentine rows across the cutter-center area, each scanline split
/// where the even-odd parity says material stands (islands fall out of
/// the interval walk for free). Rows band [lo.y, hi.y] at `stepover` with
/// the extremes kept: a row on the area's own boundary is the pass whose
/// edge grazes the wall, and dropping it would leave the wall's last band
/// standing.
fn zigzag_passes(center: &Region, p: &PocketParams) -> Result<Vec<Vec<Point2>>, GenError> {
    let (lo, hi) = match center.bounds() {
        Some(b) => b,
        None => return Err(GenError::Empty("empty pocket region".into())),
    };
    let ys = band(lo.y, hi.y, 0.0, p.stepover);
    let mut passes: Vec<Vec<Point2>> = Vec::new();
    let mut cur: Vec<Point2> = Vec::new();
    for (i, &y) in ys.iter().enumerate() {
        let segs = scan_row(center, y, y == hi.y);
        let left_to_right = p.climb == (i % 2 == 0);
        for k in 0..segs.len() {
            let (x0, x1) = segs[if left_to_right { k } else { segs.len() - 1 - k }];
            let (s, e) = if left_to_right {
                (Point2::new(x0, y), Point2::new(x1, y))
            } else {
                (Point2::new(x1, y), Point2::new(x0, y))
            };
            let join = match cur.last() {
                Some(&prev) => chord_clears(center, prev, s),
                None => false,
            };
            if join {
                // The corner matters: the connection feeds prev -> s (the
                // tested chord) and only then along the row to e. Pushing
                // only e would cut prev -> e, a diagonal the join test
                // never proved.
                cur.push(s);
                cur.push(e);
            } else {
                if cur.len() >= 2 {
                    passes.push(std::mem::take(&mut cur));
                }
                cur = vec![s, e];
            }
        }
    }
    if cur.len() >= 2 {
        passes.push(cur);
    }
    Ok(passes)
}

/// Concentric rings marching inward: ring k is the cutter-center area
/// eroded by `k * stepover`, travelled closed. Offsets are taken from
/// `center` at absolute distances (never buffer-of-buffer), so micron
/// quantization cannot compound across rings.
fn waterline_passes(center: &Region, p: &PocketParams) -> Result<Vec<Vec<Point2>>, GenError> {
    let (lo, hi) = match center.bounds() {
        Some(b) => b,
        None => return Err(GenError::Empty("empty pocket region".into())),
    };
    // Erosion by more than the diagonal can never be non-empty; the cap
    // only fires if some Clipper path stops shrinking.
    let cap = (dist(lo, hi) / p.stepover + 10.0).max(16.0) as u32;
    let mut passes: Vec<Vec<Point2>> = Vec::new();
    let mut cur: Vec<Point2> = Vec::new();
    for k in 0..=cap {
        let ring = if k == 0 {
            center.clone()
        } else {
            center.offset(-f64::from(k) * p.stepover)?
        };
        if ring.is_empty() {
            if cur.len() >= 2 {
                passes.push(cur);
            }
            return Ok(passes);
        }
        let mut loops: Vec<&Loop> = ring.loops().iter().collect();
        loops.sort_by(|a, b| b.area().abs().total_cmp(&a.area().abs()));
        for l in loops {
            let pts: Vec<Point2> = if p.climb {
                l.points().to_vec()
            } else {
                l.points().iter().rev().copied().collect()
            };
            if pts.len() < 2 {
                continue;
            }
            let join = match cur.last() {
                Some(&prev) => chord_clears(center, prev, pts[0]),
                None => false,
            };
            if !join {
                if cur.len() >= 2 {
                    passes.push(std::mem::take(&mut cur));
                }
                cur = Vec::new();
            }
            cur.extend_from_slice(&pts);
            cur.push(pts[0]);
        }
    }
    Err(GenError::Invalid(
        "waterline rings did not converge; check the stepover".into(),
    ))
}

/// Intersect the horizontal line `y` with `region`'s loops and pair the
/// crossings by even-odd parity: the cut intervals of one scanline.
/// Crossing rule matches `Region::contains` exactly (lower endpoint
/// inclusive); when the row is the region's top edge it flips to
/// upper-inclusive so the grazing wall pass still finds its intervals.
fn scan_row(region: &Region, y: f64, upper_inclusive: bool) -> Vec<(f64, f64)> {
    let mut xs: Vec<f64> = Vec::new();
    for l in region.loops() {
        let pts = l.points();
        let n = pts.len();
        for i in 0..n {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            let (ylo, yhi) = if a.y <= b.y { (a.y, b.y) } else { (b.y, a.y) };
            let hit = if upper_inclusive {
                ylo < y && y <= yhi
            } else {
                ylo <= y && y < yhi
            };
            if hit {
                xs.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
            }
        }
    }
    xs.sort_by(f64::total_cmp);
    let mut out = Vec::new();
    let mut k = 0;
    while k + 1 < xs.len() {
        // A zero-width pair is a tangent vertex or the closing double
        // count of a vertex shared by two edges: nothing to cut.
        if xs[k + 1] - xs[k] > 1e-9 {
            out.push((xs[k], xs[k + 1]));
        }
        k += 2;
    }
    out
}

/// Slotting: a straight channel between two XY points, cleared in rows
/// parallel to its axis. The channel is the stadium swept by a disc of
/// radius `width / 2` along `a` -> `b`; shrinking a stadium is narrowing
/// one, so the cutter-center area is in closed form - the stadium of
/// half-width `h = width / 2 - r - leave` on the same endpoints, no
/// Clipper round trip. `h < 0` means no tool position keeps the edge
/// inside the walls; `h == 0` is the everyday full-width slot - one row
/// on the centerline, the tool's own edges sweeping the walls.
///
/// Rows band the width `[-h, h]` at `stepover` (same snap law as facing
/// and pocket rows), and the row at offset `d` runs `a - q u + d n` to
/// `b + q u + d n` with `q = sqrt(h^2 - d^2)`, so each row reaches the
/// round ends exactly as far as it can. The center area is convex, so
/// the serpentine joins with lateral feeds and no in-cut rapids are
/// even possible.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotParams {
    pub tool_diameter: f64,
    /// Width of the finished channel.
    pub width: f64,
    pub stock_to_leave: f64,
    pub clearance_z: f64,
    /// Hop plane for moves that stay inside already-cut material. None
    /// hops to `clearance_z` (the legacy shape). A slot is one continuous
    /// serpentine per level, so the hop never actually engages here; the
    /// plane is validated like everywhere else so a stale value cannot
    /// hide inside a file that later gains multi-pass rows.
    pub retract_z: Option<f64>,
    /// First cut level; material is engaged from here down.
    pub top_z: f64,
    pub bottom_z: f64,
    pub stepdown: f64,
    pub stepover: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// The first row runs from `a` toward `b`; rows alternate from there.
    pub climb: bool,
}

impl SlotParams {
    fn validate(&self) -> Result<(), GenError> {
        let mut finite = [
            self.tool_diameter,
            self.width,
            self.stock_to_leave,
            self.clearance_z,
            self.top_z,
            self.bottom_z,
            self.stepdown,
            self.stepover,
            self.feed,
            self.plunge_feed,
        ]
        .into_iter()
        .all(f64::is_finite);
        finite &= self.retract_z.is_none_or(f64::is_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.tool_diameter <= 0.0 || self.stepdown <= 0.0 {
            return Err(GenError::Invalid("diameter and stepdown must be positive".into()));
        }
        if self.stepover <= 0.0 {
            return Err(GenError::Invalid("stepover must be positive".into()));
        }
        if self.stepover > self.tool_diameter {
            return Err(GenError::Invalid(
                "stepover leaves bands the tool cannot reach".into(),
            ));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        if self.width <= 0.0 {
            return Err(GenError::Invalid("slot width must be positive".into()));
        }
        if self.stock_to_leave < 0.0 {
            return Err(GenError::Invalid("stock to leave must not be negative".into()));
        }
        if self.top_z < self.bottom_z {
            return Err(GenError::Invalid("cut top below cut bottom".into()));
        }
        if self.clearance_z < self.top_z {
            return Err(GenError::Invalid("clearance plane below the first cut level".into()));
        }
        if self.retract_z.is_some_and(|rz| rz < self.top_z) {
            return Err(GenError::Invalid("retract plane sits inside the material".into()));
        }
        Ok(())
    }
}

/// Mill the slot between `a` and `b` (see [`SlotParams`] for the shape
/// and the row law). One serpentine pass per level; `run_levels` lifts
/// and plunges only between levels.
pub fn slot(a: Point2, b: Point2, p: &SlotParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len == 0.0 || !len.is_finite() {
        return Err(GenError::Invalid("slot endpoints must be distinct".into()));
    }
    let r = p.tool_diameter * 0.5;
    let h = p.width * 0.5 - r - p.stock_to_leave;
    if h < 0.0 {
        return Err(GenError::Invalid("slot is narrower than the tool".into()));
    }
    let u = Point2::new(dx / len, dy / len);
    let n = Point2::new(-u.y, u.x);
    let ds = band(-h, h, 0.0, p.stepover);
    let mut pass: Vec<Point2> = Vec::with_capacity(2 * ds.len());
    for (i, &d) in ds.iter().enumerate() {
        let q = (h * h - d * d).max(0.0).sqrt();
        let s = Point2::new(a.x - u.x * q + n.x * d, a.y - u.y * q + n.y * d);
        let e = Point2::new(b.x + u.x * q + n.x * d, b.y + u.y * q + n.y * d);
        // Even rows run a -> b under climb; odd rows return. The join is
        // then a short lateral step at the far end, never a diagonal.
        let (s, e) = if p.climb == (i % 2 == 0) { (s, e) } else { (e, s) };
        pass.push(s);
        pass.push(e);
    }

    let zs = cut_levels(p.top_z, p.bottom_z, p.stepdown);
    let mut bld = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    let motion = Motion::new(p.clearance_z, p.retract_z, p.feed, p.plunge_feed);
    run_levels(&mut bld, &[pass], &zs, &motion, None);
    let cur = bld.at();
    bld.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);

    let path = bld.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// 2D tracing: the cutter center runs exactly on the region's loops at
/// successive depths - the drawn line itself, engraved (the plan's
/// "buffer 0"). Wall finishing is contour's job; how the tool EDGE gets
/// onto the line (G41/G42, or an r-offset at the post) is the same
/// recorded-compensation question contour already defers to M1.5.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceParams {
    pub clearance_z: f64,
    /// Hop plane between loops of the same level (engraved lines already
    /// cut their own groove, so a low hop is safe there). None hops to
    /// `clearance_z` - the legacy shape, byte-stable goldens.
    pub retract_z: Option<f64>,
    /// First cut level; material is engaged from here down.
    pub top_z: f64,
    pub bottom_z: f64,
    pub stepdown: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Canonical (material-on-the-left) travel; conventional reverses
    /// each loop.
    pub climb: bool,
}

impl TraceParams {
    fn validate(&self) -> Result<(), GenError> {
        let mut finite = [
            self.clearance_z,
            self.top_z,
            self.bottom_z,
            self.stepdown,
            self.feed,
            self.plunge_feed,
        ]
        .into_iter()
        .all(f64::is_finite);
        finite &= self.retract_z.is_none_or(f64::is_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.stepdown <= 0.0 {
            return Err(GenError::Invalid("stepdown must be positive".into()));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        if self.top_z < self.bottom_z {
            return Err(GenError::Invalid("cut top below cut bottom".into()));
        }
        if self.clearance_z < self.top_z {
            return Err(GenError::Invalid("clearance plane below the first cut level".into()));
        }
        if self.retract_z.is_some_and(|rz| rz < self.top_z) {
            return Err(GenError::Invalid("retract plane sits inside the material".into()));
        }
        Ok(())
    }
}

/// Trace every loop of `region` - outer loops and holes alike are just
/// lines to engrave - ordered by `|area|` descending for determinism,
/// each traversal closed so the ring's cut joins at its start.
pub fn trace(region: &Region, p: &TraceParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    if region.is_empty() {
        return Err(GenError::Empty("empty trace region".into()));
    }
    let mut loops: Vec<&Loop> = region.loops().iter().collect();
    loops.sort_by(|x, y| y.area().abs().total_cmp(&x.area().abs()));
    let mut passes: Vec<Vec<Point2>> = Vec::with_capacity(loops.len());
    for l in &loops {
        let mut pass: Vec<Point2> = if p.climb {
            l.points().to_vec()
        } else {
            l.points().iter().rev().copied().collect()
        };
        if let Some(&start) = pass.first() {
            pass.push(start);
        }
        passes.push(pass);
    }

    let zs = cut_levels(p.top_z, p.bottom_z, p.stepdown);
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    // Trace engraves the region's own vertices - the point is to ride
    // the boundary as picked, so no arc fitter runs here.
    let motion = Motion::new(p.clearance_z, p.retract_z, p.feed, p.plunge_feed);
    run_levels(&mut b, &passes, &zs, &motion, None);
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);

    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// True when the straight chord `a` -> `b` never passes outside the
/// closed region: both ends in it, no loop edge properly crossing the
/// open chord, and the chord midpoint inside or on the boundary. Without
/// a proper crossing the open chord lies in one connected piece of the
/// plane complement, so the midpoint decides for all of it - and a chord
/// running along a boundary edge is exactly the tangency a wall pass is,
/// so grazing reads as clear.
pub fn chord_clears(region: &Region, a: Point2, b: Point2) -> bool {
    if (!region.contains(a) && !on_boundary(region, a))
        || (!region.contains(b) && !on_boundary(region, b))
    {
        return false;
    }
    if a == b {
        return true;
    }
    let d = Point2::new(b.x - a.x, b.y - a.y);
    let m = Point2::new(a.x + d.x * 0.5, a.y + d.y * 0.5);
    let eps = 1e-9;
    for l in region.loops() {
        let pts = l.points();
        let n = pts.len();
        for i in 0..n {
            let c = pts[i];
            let e = pts[(i + 1) % n];
            let g = Point2::new(e.x - c.x, e.y - c.y);
            let ca = Point2::new(c.x - a.x, c.y - a.y);
            let den = d.x * g.y - d.y * g.x;
            if den == 0.0 {
                // Parallel to this edge: touching it is decided by the
                // midpoint test below (a graze along a boundary line is
                // exactly what wall and hole-ceiling passes are).
                continue;
            }
            let t = (ca.x * g.y - ca.y * g.x) / den;
            let u = (ca.x * d.y - ca.y * d.x) / den;
            if !(eps < t && t < 1.0 - eps) {
                continue;
            }
            if u > eps && u < 1.0 - eps {
                return false; // crosses the open edge: a real crossing
            }
            if u >= -eps && u <= 1.0 + eps {
                // The chord goes through a loop vertex. That cuts the
                // boundary only if the two edges meeting there are on
                // opposite sides of the chord line; both on one side is
                // tangency (a fillet touching the straight it joins).
                let j = if u < 0.5 { i } else { (i + 1) % n };
                let p0 = pts[(j + n - 1) % n];
                let p2 = pts[(j + 1) % n];
                let h0 = (p0.x - a.x) * d.y - (p0.y - a.y) * d.x;
                let h2 = (p2.x - a.x) * d.y - (p2.y - a.y) * d.x;
                if h0 * h2 < 0.0 {
                    return false;
                }
            }
        }
    }
    // No proper crossing: the open chord lives in one connected piece of
    // the plane minus the loops, and the midpoint decides which. This
    // also decides every parallel/graze case (on_boundary is tolerant).
    region.contains(m) || on_boundary(region, m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_snaps_to_both_coverage_centers() {
        assert_eq!(band(0.0, 20.0, 5.0, 5.0), vec![5.0, 10.0, 15.0]);
        assert_eq!(band(0.0, 20.0, 5.0, 12.0), vec![5.0, 15.0]);
        assert_eq!(band(0.0, 20.0, 5.0, 7.0), vec![5.0, 12.0, 15.0]);
        // Thinner than one tool: the single center row, still covering.
        assert_eq!(band(0.0, 6.0, 5.0, 3.0), vec![3.0]);
    }

    fn ring(n: usize) -> Ring {
        // Unit circle-ish square: 4 vertices, perimeter 4 (0,0)-(1,0)-(1,1)-(0,1).
        let _ = n;
        Ring::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ])
    }

    #[test]
    fn ring_walks_by_arc_length() {
        let r = ring(0);
        assert_eq!(r.len, 4.0);
        assert_eq!(r.at(0.5), Point2::new(0.5, 0.0));
        assert_eq!(r.at(1.5), Point2::new(1.0, 0.5));
        assert_eq!(r.at(4.5), r.at(0.5), "wraps");
        let closed = r.arc_between(0.0, r.len);
        assert_eq!(&closed[..4], &r.pts[..], "full ring = all vertices");
        assert_eq!(closed[4], closed[0], "closes exactly on the start");
        assert_eq!(closed.len(), 5);
        let mid = r.arc_between(0.5, 2.5);
        assert_eq!(
            mid,
            vec![
                Point2::new(0.5, 0.0),
                Point2::new(1.0, 0.0),
                Point2::new(1.0, 1.0),
                Point2::new(0.5, 1.0),
            ]
        );
    }

    #[test]
    fn tab_windows_leave_even_keeps_and_merge_across_the_seam() {
        let r = ring(0);
        // Two tabs (width 1) centered at s = 1 and 3: the keeps between
        // and around them; the seam keeps merge into one crossing arc.
        assert_eq!(r.tab_keeps(2, 1.0), vec![(3.5, 4.5), (1.5, 2.5)]);
        let k = r.tab_keeps(2, 0.5);
        assert_eq!(k, vec![(3.25, 4.75), (1.25, 2.75)]);
        // Tabs touching end to end leave nothing to cut.
        assert_eq!(r.tab_keeps(4, 1.0 + 1e-9), Vec::new());
    }

    fn sq(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
        Loop::new(vec![
            Point2::new(x0, y0),
            Point2::new(x1, y0),
            Point2::new(x1, y1),
            Point2::new(x0, y1),
        ])
        .expect("test loops are valid")
    }

    #[test]
    fn scan_row_agrees_with_contains() {
        // 40x40 plate with a 10x10 island dead center.
        let region = Region::new(vec![sq(0.0, 0.0, 40.0, 40.0), sq(15.0, 15.0, 25.0, 25.0)])
            .expect("nested loops");
        let segs = scan_row(&region, 20.0, false);
        assert_eq!(segs, vec![(0.0, 15.0), (25.0, 40.0)]);
        for &(a, b) in &segs {
            assert!(region.contains(Point2::new((a + b) * 0.5, 20.0)));
        }
        assert!(!region.contains(Point2::new(20.0, 20.0)), "island interior");

        // The island's local minimum row (y = 15) has both bottom edges
        // counted by the lower-inclusive rule and splits like any other
        // straddling row; only a local maximum bridges. At y = 25 the
        // island contributes no crossings, and the one interval's points
        // over the island's span graze its top wall without entering it.
        assert_eq!(scan_row(&region, 15.0, false), vec![(0.0, 15.0), (25.0, 40.0)]);
        assert_eq!(scan_row(&region, 25.0, false), vec![(0.0, 40.0)]);

        // The topmost row needs the upper-inclusive flip to find its
        // crossings at all.
        assert_eq!(scan_row(&region, 40.0, false), Vec::new());
        assert_eq!(scan_row(&region, 40.0, true), vec![(0.0, 40.0)]);
    }

    #[test]
    fn chord_clears_separates_grazing_from_crossing() {
        let region = Region::new(vec![sq(0.0, 0.0, 40.0, 40.0), sq(15.0, 15.0, 25.0, 25.0)])
            .expect("nested loops");
        let p = |x: f64, y: f64| Point2::new(x, y);
        // Through the island: refused.
        assert!(!chord_clears(&region, p(5.0, 20.0), p(35.0, 20.0)));
        // Along the outer wall: the tangency a row connector uses.
        assert!(chord_clears(&region, p(40.0, 5.0), p(40.0, 35.0)));
        // Inside material, no crossings.
        assert!(chord_clears(&region, p(5.0, 5.0), p(10.0, 35.0)));
        // Starting outside: refused.
        assert!(!chord_clears(&region, p(-1.0, 5.0), p(35.0, 20.0)));
        // Boundary corners read as closed-inside.
        assert!(chord_clears(&region, p(40.0, 40.0), p(40.0, 30.0)));
    }
}
