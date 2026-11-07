//! The lathe spindle law (issue #62): a turning op's axis of revolution
//! is fitted from the part's facets, but the lathe post writes every
//! diameter about the work offset's origin and every Z as world Z - it
//! programs a spindle that IS world Z through that origin. A part whose
//! fitted axis is not that line (a few degrees of lean, or a millimetre
//! off the origin) would be turned about a spindle it is not on: a taper,
//! an eccentric or a coned face the model does not have, and a lean in
//! one meridian plane gives the post nothing to notice. So the door is
//! here, twice: before a move is computed (`collect_job`), and again at
//! the post (`posting::program_ops`) against the origin the post is
//! about to receive - a work offset moved after generation must never
//! post a program measured from the old one. Off the spindle by more
//! than the post can write, the op is refused by name with the lean and
//! the offset it measured, and the act that fixes it.
//!
//! The measure is the whole rigid displacement, not the axis line alone:
//! every part vertex is read twice - (radius, height) about the fitted
//! axis, the generator's reading, and (radius, Z) about world Z through
//! the origin, the post's - and the largest disagreement over the part
//! is what is bounded. A lean `a` moves a point at radius `R` by `R*a`
//! in Z (a face posted as a cone), on top of the axis drift over the
//! part's height; a thin wide flange is judged by its radius, a long
//! bar by its length.
//!
//! Provenance: a lathe's programmed X is the diameter about the spindle
//! centreline and Z is measured along it (RS-274 / ISO 6983 lathe
//! convention, any turning handbook); the tolerance is the posts' own
//! output lattice. Our own design notes; no vendor material consulted.

use flecs_ecs::prelude::*;

use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

/// How far any part point may read differently about the fitted axis
/// than about the spindle, in mm, per posted word: half the last digit
/// of the diameter word in radius (X is written at 0.001 mm of
/// diameter) and half the Z word's 0.001 mm - so a part that passes
/// posts every word within one last digit of what a perfectly placed
/// part would. The fit of an honest revolve sits far below it (the
/// non-uniform, f32-stored revolve in `lathe_spindle_law` measures it).
pub const SPINDLE_TOL: f64 = 0.0005;
/// The up-component below which the turning frame itself refuses the
/// axis as lying (`turning::profile`'s LEAN_MIN): such a model keeps
/// that refusal's own wording, so this door stands aside for it.
const LYING: f64 = 0.0175;

/// What the law measured on one part: the fitted axis against world Z
/// through the work offset's origin.
#[derive(Clone, Copy, Debug)]
pub struct SpindleReading {
    /// The fitted axis' angle off world Z, degrees.
    pub lean_deg: f64,
    /// The axis line's XY distance from the origin at `mid_z`, mm.
    pub offset_mid: f64,
    /// The part's mid-height (world Z), where `offset_mid` is read.
    pub mid_z: f64,
    /// The worst radius disagreement over the part's vertices, mm.
    pub radial: f64,
    /// The worst height disagreement over the part's vertices, mm.
    pub axial: f64,
}

impl SpindleReading {
    /// The worst disagreement of either word; NaN stays NaN (refused).
    #[must_use]
    pub fn worst(&self) -> f64 {
        if self.radial.is_nan() || self.axial.is_nan() {
            return f64::NAN;
        }
        self.radial.max(self.axial)
    }
}

/// Read the part about its fitted axis and about the spindle through
/// `origin`. `None` when the fit cannot read the model as revolved or
/// the axis lies near the table: the turning generators refuse those by
/// their own names, in their own order.
#[must_use]
pub fn spindle_reading(facets: &[Triangle], origin: [f64; 3]) -> Option<SpindleReading> {
    let ax = crate::multiaxis::axis::fit(facets).ok()?;
    let d = ax.dir;
    let n = d.dot(d).sqrt();
    if !n.is_finite() || n <= 0.0 {
        return None;
    }
    let mut u = Point3::new(d.x / n, d.y / n, d.z / n);
    if u.z < 0.0 {
        u = Point3::new(-u.x, -u.y, -u.z);
    }
    if u.z < LYING {
        return None;
    }
    let c = ax.origin;
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut radial, mut axial, mut finite) = (0.0f64, 0.0f64, true);
    for p in facets.iter().flat_map(|t| [t.a, t.b, t.c]) {
        let q = p - c;
        let s = q.dot(u);
        // The generator's reading: radius off the fitted axis, and the
        // world height of the axis point at that station (stations are
        // world Z planes read where they meet the axis).
        let (fx, fy, fz) = (q.x - u.x * s, q.y - u.y * s, q.z - u.z * s);
        let rf = (fx * fx + fy * fy + fz * fz).sqrt();
        let zf = c.z + s * u.z;
        // The post's: radius about world Z through the origin, world Z.
        let (dx, dy) = (p.x - origin[0], p.y - origin[1]);
        let rs = (dx * dx + dy * dy).sqrt();
        let (er, ez) = ((rs - rf).abs(), (p.z - zf).abs());
        if !er.is_finite() || !ez.is_finite() {
            finite = false;
            continue;
        }
        radial = radial.max(er);
        axial = axial.max(ez);
        lo = lo.min(p.z);
        hi = hi.max(p.z);
    }
    if !finite || !lo.is_finite() || !hi.is_finite() {
        radial = f64::NAN;
    }
    let mid_z = if lo.is_finite() && hi.is_finite() { 0.5 * (lo + hi) } else { c.z };
    let t = (mid_z - c.z) / u.z;
    let (ox, oy) = (c.x + u.x * t - origin[0], c.y + u.y * t - origin[1]);
    Some(SpindleReading {
        lean_deg: u.z.clamp(-1.0, 1.0).acos().to_degrees(),
        offset_mid: (ox * ox + oy * oy).sqrt(),
        mid_z,
        radial,
        axial,
    })
}

/// Refuse by name a part that reads differently about its fitted axis
/// than about world Z through `origin` (the work offset's world
/// position) by more than [`SPINDLE_TOL`] anywhere on it.
pub(super) fn check(facets: &[Triangle], origin: [f64; 3], slot: &str) -> Result<(), GenError> {
    let Some(r) = spindle_reading(facets, origin) else {
        return Ok(());
    };
    let worst = r.worst();
    if worst.is_finite() && worst <= SPINDLE_TOL {
        return Ok(());
    }
    Err(GenError::Invalid(format!(
        "the part's axis of revolution is not the lathe spindle: it leans {:.3} degrees \
         off world Z and stands {:.3} mm off the {slot} origin at Z{:.3}, moving a part \
         point {worst:.4} mm (the lathe programs diameters about world Z through the work \
         offset, {SPINDLE_TOL} mm allowed) - turned as posted it would cut a taper, an \
         eccentric or a coned face; align the part's axis with world Z and move the work \
         offset onto it",
        r.lean_deg, r.offset_mid, r.mid_z
    )))
}

/// Whether `params` turns about the fitted axis on the document's
/// machine: the turning family, and the lathe's axial Drill.
pub(crate) fn turns(world: &World, params: &super::OpParams) -> bool {
    params.kind().info().family() == super::Family::Turn
        || (matches!(params, super::OpParams::Drill { .. })
            && crate::machine::kind_of(world) == crate::machine::Kind::Lathe)
}

/// The law again at the post, for operation entity `e` named `name`:
/// the work offset may have moved since generation, and the post is
/// about to measure every diameter from where it is NOW. `None` for an
/// op that does not turn, or whose part still sits on the spindle.
pub(crate) fn post_problem(world: &World, name: &str, e: &Entity) -> Option<String> {
    let op = crate::model::operation_of(world, e)?;
    if !turns(world, &op.params) {
        return None;
    }
    let setup = op.setup.as_deref();
    let (slot, origin) = crate::posting::wcs_context(world, setup, &op.params);
    // A material that cannot be read is a source that moved or went:
    // the freshness check right after this names that, as it always has.
    let m = super::material_of(world, setup, &op.src).ok()?;
    let why = check(&m.facets, origin, slot).err()?;
    Some(format!("{name}: {why} - fix it and regenerate before posting"))
}
