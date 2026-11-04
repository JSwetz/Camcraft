//! Split from `finishing3d.rs` by M5.0.1; structure only.

use crate::geometry::Triangle;
use crate::machining2d::{cut_levels, GenError};

/// Which finishing kinematic produces the path. Waterline is the
/// Z-level ring ladder; Parallel adds the Z-following shallow raster
/// (walls keep riding the rings - the auto-split, see the module
/// laws); ParallelMulti runs the raster twice, one pass per XY axis,
/// so the cusp budget is spent along both directions; Scallop runs
/// it twice along the DIAGONALS - the constant-cusp promise: no
/// linear terrain feature (which in this codebase's prismatic
/// models means none aligned to X or Y) can run parallel to both
/// families, so no feature's cusp escapes a crossing cut. The flat
/// cusp of the crossed pair is the single family's - the kernel
/// measures it, and no analytic sqrt(2) discount is claimed. Radial
/// (M2.2d) sends chords through the shallow terrain's center, one
/// per stepover of arc at the rim; Spiral runs ONE Archimedean
/// curve in from the silhouette - the retract-free continuous cut.
/// Both ride `march_curve`'s laws verbatim - only the placement of
/// the ground lines changes, never what a step promises - and both
/// hand the steep flanks to the same rings. Contour (M2.2e) shrinks
/// the terrain's center-shadow by one stepover per pass - concentric
/// loops, each lifted sample sitting tangent wherever a tangent
/// exists - and Project runs only its boundary: the model's own
/// silhouette, XY kept, Z lifted. Rings close both families.
/// SteepShallow (M2.5 A) is the auto-split spoken as a partition:
/// the raster rides the shallow ground exactly as Parallel does,
/// and the ring ladder keeps its hands OFF shallow tangency - the
/// 45° line stops being a meeting place and becomes a border, so
/// no slope is ridden twice and the flanks answer to their OWN
/// spacing number (`steep_stepover`, the ring ladder's author when
/// stated). Flat (M2.5 A) finishes the floors only: the near-level
/// facets' footprints, shrunk one stepover per pass at constant
/// tip-Z - the shadow family's choreography on the ground the
/// waterline's rest verdict always refused to claim. Pencil and
/// Corner (M2.5 B) are the crease pair: each rides the locus of
/// centers tangent to BOTH faces of a concave edge at once, and the
/// band `p/tan(theta/2)` - the skirt a ball of radius p can never
/// clean - says which of the two speaks: Corner for the tool's own
/// p = r, Pencil for a stated reference's p = R, keeping only where
/// the bigger ball's shadow stands past the tolerance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinishPattern {
    Waterline,
    Parallel,
    ParallelMulti,
    Scallop,
    Radial,
    Spiral,
    Contour,
    Project,
    /// The two-truth split as a border, each side with its number.
    SteepShallow,
    /// Floors only, planar, one pass per floor.
    Flat,
    /// The concave corner loci a reference ball leaves standing,
    /// ridden by this op's smaller ball (M2.5 B): `pencil_reference`
    /// is the pen, and absent means every corner.
    Pencil,
    /// The simultaneous two-face tangent pass with the operation's
    /// own ball - where the tool's own band beats the tolerance
    /// (M2.5 B). `pencil_reference` is inert here.
    Corner,
    /// Parallel's twin with the sloped laws always on (M2.5 C): the
    /// diagonal chain between the raster's lines (`ramp`) and the
    /// ramped first contacts on steep tangents (`entry_ramp`) are
    /// forced on whatever the pane says - the catalog's "sloped
    /// paths" name, assembled from laws this module already owns.
    Ramp,
    /// The spiral with the terrain's own spacing (M2.5 C): one
    /// in-winding curve whose radius advances by the LOCAL scallop
    /// law - wide where the ground tilts, narrow where it's flat -
    /// so the cusp budget is spent evenly instead of at the
    /// footprint's worst tilt.
    MorphSpiral,
    /// Rings of equal GEODESIC distance from the model's open
    /// boundary (M2.5 C): contours that run WITH the surface - down
    /// the wall and across the floor on one continuous loop -
    /// placed by arc-length along the mesh, not by Z. Needs an open
    /// boundary to march from; a closed shell is refused by name.
    Geodesic,
    /// The two families BOTH ride the transition (M2.5 D): the
    /// raster's lines over the shallow ground exactly as Parallel
    /// places them, and the ring ladder kept wherever a loop's
    /// tangency reaches past 30 degrees - so the band between the
    /// floor's law and the wall's law is cut twice, from both
    /// directions, instead of leaving the 45° border's staircase as
    /// the price of one clean split. Flat interiors stay raster-only:
    /// a ring that touches nothing flatter than the band is the
    /// ladder's hands off the floor, as the border law always said.
    Blend,
    /// The ladder's levels joined by ruled lofts (M2.5 D2): the
    /// waterline rings ride as always, and between every consecutive
    /// pair of levels whose loops pair (area rank + centroid within
    /// one tool diameter) one interpolated guide runs, its XY the
    /// loop-to-loop ruling's midpoint and its Z the column's own
    /// answer - halving the spacing quarters the cusp, and where the
    /// levels' geometry disagrees about being one feature, nothing
    /// is paired and the rings alone carry the step, as before.
    Morph,
    /// The flowline pass (M2.5 E): lines that CURVE with the part,
    /// seeded on the lattice and traced along the surface's own
    /// least-curving grain - the curvature field of `multiaxis`'
    /// 5-axis sibling, run with NO axes stamped: a plain XYZ
    /// finish, the tool's axis vertical, the lines the only thing
    /// that bends. Flat ground makes no claim; there the fallback
    /// walks straight, and flow over plain ground IS the parallel
    /// raster - cutting, never stalling.
    Flow,
}

/// Everything the 3D finishers need that the tool and geometry don't
/// already say. Heights arrive resolved to millimeters by the
/// operation layer (same discipline as every other params shape).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Finish3DParams {
    /// Which kinematic runs; `Waterline` ignores the raster fields,
    /// exactly as if this enum had never arrived.
    pub pattern: FinishPattern,
    /// Raster line spacing override, mm. `None` follows the cos-law
    /// `√(8·r·tol)·min(m_z)` from the tolerance, like the stepdown
    /// does for the rings: the tolerance is the author.
    pub stepover: Option<f64>,
    /// Ball-nose diameter, mm.
    pub tool_diameter: f64,
    /// Surface deviation budget, mm - both directions. It sets the
    /// stepdown and the arc-fit chord budget together: the rings are
    /// promised within `tol` of the true tangent locus.
    pub tolerance: f64,
    pub clearance_z: f64,
    pub retract_z: Option<f64>,
    /// Tip-Z of the first (highest) ring.
    pub top_z: f64,
    /// Tip-Z of the last ring.
    pub bottom_z: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Climb runs rings in stored order (the forbidden region riding
    /// on the left), conventional reverses - the same XOR contour
    /// applies to planar loops.
    pub climb: bool,
    /// Ramp the entries (M2.2e): between two runs of the SAME family,
    /// one straight diagonal Feed replaces the hop-to-clearance
    /// wherever [`diagonal_ok`] can certify it - the ball never sinks
    /// below its promised surface en route and never leaves the
    /// region. Where it can't certify (a hill between the lines, open
    /// air, a flank crossing), the hop stands, unchanged. Between-
    /// family transits are never ramped by this flag: the diagonal
    /// here is a chain between cuts, not a way into a new one. First
    /// contacts are the domain of [`Finish3DParams::entry_ramp`]
    /// (M8.2 B) - the one place a diagonal IS a way into a cut.
    pub ramp: bool,
    /// Ramp the FIRST contacts on steep tangents (M8.2 B): the ring
    /// law promises the vertical plunge never cuts above its level,
    /// but on a flank steeper than the 45° split the ball MEETS the
    /// wall all at once. With this on, such an entry slides in along
    /// its contact normal instead: a certified diagonal Feed from an
    /// air point down to the tangent, engaging the wall in order. The
    /// certification currency is the same as the law's test: sampled
    /// centers never bite past `tolerance`. Where the normal can't be
    /// read, points at or below the horizon (an overhang, or a flank
    /// too near-vertical to lift off - a lateral in-feed at depth is
    /// not an entry), can't start below the clearance plane, or bites
    /// along the way, the vertical plunge stands and the path carries
    /// a note saying why. `false` is v0.4's behavior byte for byte;
    /// the ring walk itself is never touched.
    pub entry_ramp: bool,
    /// Floor for the adaptive march's refinement, mm (M2.2e): the
    /// march bisects a chord whenever its middle sinks past the
    /// quarter budget, down to `delta0/4096` by its own right. Set
    /// this to say NO cut segment shorter than `s` - coarse ground
    /// you chose, not ground the chord law found. The run-edge
    /// bisections are NOT floored: where a cut starts and stops is
    /// the material's business. `None` rides the march's own floor;
    /// Waterline ignores it, exactly as if it had never arrived
    /// (the same jurisdiction the raster fields keep).
    pub min_step: Option<f64>,
    /// The steepest descent a ramp link may carry, degrees (M2.2e):
    /// between two runs, the chain's TRAVERSE is a straight line,
    /// and a drop steeper than `dz = d_xy·tan(angle)` is not a ramp:
    /// it is a plunge in a diagonal coat, and plunges belong to
    /// the run's own first contact. 45° is the default and the
    /// law's own gift: shallow ground is exactly the ground whose
    /// support climbs no more than one in one, so an angle of 45
    /// certifies every traverse the raster's jurisdiction can
    /// present - the default is v17's behavior, spelled. Links
    /// that barely traverse (`d_xy` below the tolerance) are the
    /// vertical case the ramp law already settled, gate untouched.
    pub ramp_angle: f64,
    /// The ring ladder's spacing, mm (M2.5 A): the flank's own
    /// number, stated at the slope the partition draws. Waterline's
    /// ladder has never had one - the scallop stepdown authored it -
    /// and that is the surface-finish tail M2.2 left for this
    /// stage: a `stepover` for the lines, a `steep_stepover` for
    /// the rings. `Some(s)` spaces the levels by `s` exactly (the
    /// same authored-over-law grammar `stepover` keeps for the
    /// raster - the envelope verdict, not the number, is what holds
    /// the promise); `None` - every pre-M2.5 file, and every
    /// pattern other than SteepShallow - rides the scallop law
    /// `2√(r·tol)`, byte for byte, as if this field had never
    /// arrived.
    pub steep_stepover: Option<f64>,
    /// The reference ball's diameter, mm (M2.5 B): Pencil cleans the
    /// corner bands this tool's ball cannot reach but that a bigger
    /// ball of diameter `d` leaves standing - it cuts where
    /// `(d/2 − r)/tan(θ/2)` beats the tolerance. `None` says "no
    /// bigger ball was ever named": every concave corner the tool's
    /// locus rests on is fair game. The number must exceed the op's
    /// own tool diameter - a reference not bigger than the tool
    /// hides under no band, and that is refused by name, not
    /// shrunk. Every other pattern ignores it, exactly as the
    /// raster fields ignore Waterline, inert as if it had never
    /// arrived.
    pub pencil_reference: Option<f64>,
}

/// The stepdown the scallop law prescribes: cusp height
/// `Δz²/(8 r cos²α) ≤ tol` for `α ≤ 45°` gives `Δz = 2√(r·tol)`.
/// Non-finite or non-positive inputs collapse to 0.0, which the
/// caller's level law turns into a clean refusal.
#[must_use]
pub fn scallop_stepdown(r: f64, tol: f64) -> f64 {
    if !(r.is_finite() && tol.is_finite()) || r <= 0.0 || tol <= 0.0 {
        return 0.0;
    }
    2.0 * (r * tol).sqrt()
}

/// The raster stepover the same scallop law prescribes sideways: two
/// Z-following ball paths whose tangent centers are `s·sec α` apart in
/// the plane normal to the pass leave a cusp `≈ (s·sec α)²/(8r)` on a
/// facet tilted `α` from horizontal, so `s = √(8·r·tol)·cos α` holds
/// the cusp at `tol`. The caller multiplies by the model's smallest
/// shallow `m_z` - one law, the tilt's worst case. Non-positive or
/// non-finite inputs collapse to 0.0 like the stepdown's.
#[must_use]
pub fn scallop_stepover(r: f64, tol: f64) -> f64 {
    if !(r.is_finite() && tol.is_finite()) || r <= 0.0 || tol <= 0.0 {
        return 0.0;
    }
    (8.0 * r * tol).sqrt()
}

/// The adaptive march's refinement floor, mm: `delta0/4096` by its
/// own right (the quarter-chord bisection has to stop SOMEWHERE),
/// raised - never lowered - by an explicit `min_step`. A NaN floor
/// loses to the auto floor by `f64::max`'s own law, so a nonsense
/// number can never freeze the march; an over-tall floor simply
/// turns refinement off, which is legal coarse ground.
#[must_use]
pub(super) fn refinement_floor(delta0: f64, min_step: Option<f64>) -> f64 {
    let auto = delta0 / 4096.0;
    match min_step {
        Some(s) => auto.max(s),
        None => auto,
    }
}

/// The steep/shallow jurisdiction line, as a cosine: facets whose
/// upward normal tilts more than 45° off vertical get no raster
/// support and belong to the rings. The same 45° the stepdown law
/// already promises (`|α| ≤ 45°` keeps the ring cusp inside `tol`),
/// so the two strategies meet exactly at each one's limit - no
/// territory claimed twice, none dropped.
pub(super) const SPLIT_MZ: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// The Blend's band line, as a cosine: 30° off vertical. Why there:
/// the ring cusp on a facet tilted `α` is `Δz² / (8·r·cos²α)` - at
/// 30° it is `4/3` of the vertical promise, at 45° twice it - so
/// between 30° and the split the two families' numbers are within
/// a factor of the SAME tolerance of each other, and cutting that
/// band twice shaves the border's staircase instead of trading
/// promises. Flatter ground is worth only the raster's line; the
/// rings let it alone, exactly as the border law says.
pub(super) const BLEND_MZ: f64 = 0.866_025_403_784_438_7;

/// The numeric skin of the laws: a center that touches a facet sits
/// at distance exactly `r` from it, and float says `r ± 1e-9·r`.
/// Accept contact within this much bite.
pub(super) const TOUCH_EPS: f64 = 1e-7;

/// Tip-Z levels for the pass, top down, ending exactly at `bottom_z`
/// (the shared snap law). The scallop stepdown carries a hard level
/// cap: a tolerance so fine that the rings would number in the
/// hundreds of thousands is REFUSED, not quietly coarsened - silent
/// coarsening would make the stated tolerance a lie.
pub(super) fn levels(p: &Finish3DParams, r: f64) -> Result<Vec<f64>, GenError> {
    let span = p.top_z - p.bottom_z;
    if span < 0.0 {
        return Err(GenError::Invalid("waterline bottom is above its top".into()));
    }
    // The ladder's author: the scallop law, unless the partition's
    // own operation states a flank spacing (M2.5 A) - the same
    // "a stated number outranks a derived one, the verdict keeps
    // the promise" grammar the raster's stepover speaks.
    let dz = match p.pattern {
        FinishPattern::SteepShallow => p.steep_stepover.unwrap_or_else(|| scallop_stepdown(r, p.tolerance)),
        _ => scallop_stepdown(r, p.tolerance),
    };
    const MAX_LEVELS: f64 = 8192.0;
    if span > 0.0 && dz <= span / MAX_LEVELS {
        return Err(GenError::Invalid(format!(
            "stepdown {:.6} with this span needs {} levels (cap {}): refine the spacing or the tolerance",
            dz,
            (span / dz.max(1e-12)).min(i64::MAX as f64) as i64,
            MAX_LEVELS as i64,
        )));
    }
    Ok(cut_levels(p.top_z, p.bottom_z, dz.max(1e-9)))
}

/// Unit directions of the convex scan. The count follows the
/// disk-chord law: the scan's worst miss is a disk sagitta at most
/// `r(1 − cos(π/N))`, held under `tol/2` (the other half of the
/// budget belongs to the stepdown scallop).
pub(super) fn dirs(r: f64, tol: f64) -> Vec<(f64, f64)> {
    let want = std::f64::consts::PI * (2.0 * r / tol).sqrt();
    let n = (want.ceil() as usize).clamp(128, 16384);
    (0..n)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / n as f64;
            (a.cos(), a.sin())
        })
        .collect()
}

/// The shared numeric gate every 3D finish reads before touching
/// geometry. Messages say "3D finish", not one strategy's name - the
/// check belongs to both.
pub(super) fn validate(p: &Finish3DParams, facets: &[Triangle]) -> Result<(), GenError> {
    if !(p.tool_diameter.is_finite()
        && p.tool_diameter > 0.0
        && p.tolerance.is_finite()
        && p.tolerance > 0.0
        && p.feed.is_finite()
        && p.feed > 0.0
        && p.plunge_feed.is_finite()
        && p.plunge_feed > 0.0
        && p.clearance_z.is_finite()
        && p.top_z.is_finite()
        && p.bottom_z.is_finite()
        && p.stepover.is_none_or(|s| s.is_finite() && s > 0.0)
        && p.steep_stepover.is_none_or(|s| s.is_finite() && s > 0.0)
        && p.pencil_reference.is_none_or(|d| d.is_finite() && d > 0.0)
        && p.retract_z.is_none_or(|rz| rz.is_finite())
        && p.min_step.is_none_or(|s| s.is_finite() && s > 0.0)
        // The ramp angle is refused OUTSIDE (0, 90) even while
        // `ramp` is off - an unreachable promise is not an inert
        // number, it is a lie waiting for the checkbox.
        && (0.0..90.0).contains(&p.ramp_angle))
    {
        return Err(GenError::Invalid(
            "3D finish needs finite positive tool, tolerance, feeds, heights and surface-finish numbers".into(),
        ));
    }
    if facets.is_empty() {
        return Err(GenError::Empty("model has no facets".into()));
    }
    for f in facets {
        if !(f.a.is_finite() && f.b.is_finite() && f.c.is_finite()) {
            return Err(GenError::Invalid("model has a non-finite facet".into()));
        }
    }
    Ok(())
}
