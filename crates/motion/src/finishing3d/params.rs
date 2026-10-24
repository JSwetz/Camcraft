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
    /// air, a flank crossing), the hop stands, unchanged. First
    /// contacts and between-family transits are never ramped: the
    /// diagonal is a chain between cuts, not a way into a new one.
    pub ramp: bool,
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
    let dz = scallop_stepdown(r, p.tolerance);
    const MAX_LEVELS: f64 = 8192.0;
    if span > 0.0 && dz <= span / MAX_LEVELS {
        return Err(GenError::Invalid(format!(
            "tolerance {:.6} with this tool needs {} levels (cap {}): refine the model or widen the tolerance",
            p.tolerance,
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
