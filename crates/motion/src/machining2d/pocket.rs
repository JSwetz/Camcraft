//! Split from `machining2d.rs` by M5.0.1; structure only.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::entry::{engagement_step, Entry3D};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;

use super::face::{GenError, band};
use super::levels::{Motion, cut_levels, run_levels};
use super::arcs::ARC_FIT_TOL;
use super::ring::ring_is_circular;
use super::waterscan::{scan_row, waterline_passes};
use super::slots::chord_clears;
/// How a pocket is filled: straight rows back and forth, or closed rings
/// marching inward.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PocketStrategy {
    Zigzag,
    Waterline,
    /// Constant-engagement rings (M2.1c's law, real for the planar
    /// pocket since M5.1 H): the same ring tiling as Waterline, but
    /// spaced at the engagement-law step `r (1 - cos theta_max)` from
    /// the params' `max_engagement` instead of a free stepover, so
    /// every pass bites the wall by the same bound the cutter's
    /// contact angle was chosen for. Until H this was Waterline's
    /// geometry at the stated stepover - a documented alias; the law
    /// now runs here too, through the same `engagement_step` the 3-D
    /// ladder reads.
    Adaptive,
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
    /// How the ladder crosses a level plane (M5.1 H, the family the
    /// 3-D ladder names in `crate::entry`). `Plunge` is every pocket
    /// posted before this field existed, and the arm's legality
    /// proofs live in `run_levels` the same way `ladder.rs` owns
    /// them for the 3-D family.
    pub entry: Entry3D,
    /// Helix centerline circle radius (mm); validated at most the tool
    /// radius so successive turns overlap and the dive cuts a clean
    /// bore - the 3-D ladder's door, same words.
    pub helix_radius: f64,
    /// The `Adaptive` strategy's engagement bound, in degrees of
    /// instant cutter contact; read only under `Adaptive` (through
    /// [`PocketParams::tiling_step`], still validated finite.
    pub max_engagement: f64,
}

impl PocketParams {
    pub(super) fn validate(&self) -> Result<(), GenError> {
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
            self.helix_radius,
            self.max_engagement,
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
        // Two parallel cut lines a step apart cover every point between
        // them (distance at most step/2 to the nearer one), and zigzag
        // puts grazing rows on the extreme coverage centers, so a step
        // within the diameter covers the region. Waterline's innermost
        // ring can stand a whole step from the material it collapses
        // around, so it must stay within one radius. Auto is bounded by
        // the looser law: its waterline branch carries the radius bound
        // inside `resolve`, and anything else runs zigzag. The checked
        // number is `tiling_step` - what spaces the passes - so an
        // `Adaptive` plan is honest at its engagement-law step even
        // where the (ignored) stepover field would not be (M5.1 H;
        // the 3-D ladder makes the same distinction the other way:
        // it bounds the field and never coarsens the grid past the law).
        let reach = match self.strategy {
            PocketStrategy::Zigzag | PocketStrategy::Auto => self.tool_diameter,
            // Adaptive rings are Waterline's geometry; the same reach
            // bound keeps their innermost ring honest. The law's step
            // is at most one radius for any angle in the door, so
            // beyond it the door is the check.
            PocketStrategy::Waterline | PocketStrategy::Adaptive => self.tool_diameter * 0.5,
        };
        if self.tiling_step() > reach {
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
        if self.entry == Entry3D::Helix {
            let r = self.tool_diameter * 0.5;
            if self.helix_radius <= 0.0 || self.helix_radius > r {
                return Err(GenError::Invalid(
                    "helix radius must be positive and at most the tool radius".into(),
                ));
            }
        }
        if self.strategy == PocketStrategy::Adaptive {
            // The law's own bounds, same as the 3-D ladder's door: a
            // zero angle is a zero step (an infinite ladder), and past
            // 90 degrees the sagitta passes one radius - past where
            // consecutive rings still overlap.
            if self.max_engagement <= 0.0 || self.max_engagement > 90.0 {
                return Err(GenError::Invalid(
                    "engagement angle must be in (0, 90] degrees".into(),
                ));
            }
        }
        Ok(())
    }

    /// What actually spaces the passes at a level: the engagement-law
    /// step under `Adaptive`, the stated stepover otherwise. The pass
    /// plan and the reach door both ask this one function, mirroring
    /// `Pocket3DParams::tiling_step` (M5.1 H).
    fn tiling_step(&self) -> f64 {
        match self.strategy {
            PocketStrategy::Adaptive => {
                engagement_step(self.tool_diameter * 0.5, self.max_engagement)
            }
            _ => self.stepover,
        }
    }
}

/// Pocket `region` (the material to remove; its loops' interiors are
/// islands to leave standing). The cutter-center area is the region
/// eroded by `r + leave` - the opposite sign to contouring, because here
/// the tool must stay inside the material rather than outside it. Every
/// feed point lies in that area, so the tool edge never crosses a wall.
///
/// Transitions: between levels the tool crosses in with its `entry`
/// family - the dive it always posted, or the ramp diagonal / helical
/// bore the 3-D ladder proves and `run_levels` now shares (M5.1 H);
/// within a level a straight connection between two cut segments is
/// kept as a Feed exactly when [`chord_clears`] proves it never leaves
/// the cutter-center area (boundary grazing counts as staying in - it
/// is the tangency wall passes are built on), and otherwise it becomes
/// lift-rapid-plunge. Rapids inside the footprint below the safe plane
/// are the verifier's business (step 7).
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
    let passes = plan_passes(&center, p.tiling_step(), p.climb, strategy)?;
    let zs = cut_levels(p.top_z, p.bottom_z, p.stepdown);

    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    // Waterline rings circle around material, so they get the arc fitter
    // (rings straight enough to be lines stay exactly that); zigzag rows
    // are straight by construction and keep the plain walk.
    let fit = match strategy {
        PocketStrategy::Zigzag | PocketStrategy::Auto => None,
        PocketStrategy::Waterline | PocketStrategy::Adaptive => Some(ARC_FIT_TOL),
    };
    let motion = Motion::new(p.clearance_z, p.retract_z, p.feed, p.plunge_feed)
        .with_entry(p.entry, p.helix_radius);
    run_levels(&mut b, &passes, &zs, &motion, fit, Some(&center));
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
pub(super) fn zigzag_passes(center: &Region, stepover: f64, climb: bool) -> Result<Vec<Vec<Point2>>, GenError> {
    let (lo, hi) = match center.bounds() {
        Some(b) => b,
        None => return Err(GenError::Empty("empty pocket region".into())),
    };
    let ys = band(lo.y, hi.y, 0.0, stepover);
    let mut passes: Vec<Vec<Point2>> = Vec::new();
    let mut cur: Vec<Point2> = Vec::new();
    for (i, &y) in ys.iter().enumerate() {
        let segs = scan_row(center, y, y == hi.y);
        let left_to_right = climb == (i % 2 == 0);
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
/// The cutter-center passes for one planar area: the shape law both the
/// 2-D pocket and the 3-D pocket's per-level plan share (the strategies
/// band/erode `center` identically; only the numbers come from the
/// operation's params either way).
pub(crate) fn plan_passes(
    center: &Region,
    stepover: f64,
    climb: bool,
    strategy: PocketStrategy,
) -> Result<Vec<Vec<Point2>>, GenError> {
    match strategy {
        PocketStrategy::Zigzag | PocketStrategy::Auto => zigzag_passes(center, stepover, climb),
        // Adaptive is ring tiling; who chose the step is the caller's
        // law (3-D: the engagement bound; here: the plain stepover).
        PocketStrategy::Waterline | PocketStrategy::Adaptive => {
            waterline_passes(center, stepover, climb)
        }
    }
}
