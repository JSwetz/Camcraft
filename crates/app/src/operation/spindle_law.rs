//! The lathe spindle law (issue #62): a turning op's axis of revolution
//! is fitted from the part's facets, but the lathe post writes every
//! diameter about the work offset's origin and every Z as world Z - it
//! programs a spindle that IS world Z through that origin. A part whose
//! fitted axis is not that line (a few degrees of lean, or a millimetre
//! off the origin) would be turned about a spindle it is not on: a taper
//! or an eccentric the model does not have, and a lean in one meridian
//! plane gives the post nothing to notice. So the door is here, before
//! a move is computed: off the spindle line by more than the post can
//! write, the op is refused by name with the lean and the offset it
//! measured, and the act that fixes it (align the part or the offset).
//!
//! Provenance: a lathe's programmed X is the diameter about the spindle
//! centreline and Z is measured along it (RS-274 / ISO 6983 lathe
//! convention, any turning handbook); the tolerance is the posts' own
//! output lattice. Our own design notes; no vendor material consulted.

use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

/// How far the fitted axis may stand from the spindle line anywhere
/// over the part's height, in mm of RADIUS: half the posted diameter
/// word's last digit (X is written at 0.001 mm of diameter), so a part
/// that passes posts the same digits a perfectly placed part would -
/// and the fit of an honest revolve is exact far below it.
pub const SPINDLE_TOL: f64 = 0.0005;
/// The up-component below which the turning frame itself refuses the
/// axis as lying (`turning::profile`'s LEAN_MIN): such a model keeps
/// that refusal's own wording, so this door stands aside for it.
const LYING: f64 = 0.0175;

/// Refuse by name a part whose fitted axis is not world Z through
/// `origin` (the work offset's world position) within [`SPINDLE_TOL`]
/// over the facets' Z span. A model the fit cannot read as revolved, or
/// whose axis lies near the table, passes here untouched: the turning
/// generators refuse those by their own names, in their own order.
pub(super) fn check(facets: &[Triangle], origin: [f64; 3], slot: &str) -> Result<(), GenError> {
    let Ok(ax) = crate::multiaxis::axis::fit(facets) else {
        return Ok(());
    };
    let d = ax.dir;
    let n = d.dot(d).sqrt();
    if !n.is_finite() || n <= 0.0 {
        return Ok(());
    }
    let mut u = Point3::new(d.x / n, d.y / n, d.z / n);
    if u.z < 0.0 {
        u = Point3::new(-u.x, -u.y, -u.z);
    }
    if u.z < LYING {
        return Ok(());
    }
    let (lo, hi) = facets
        .iter()
        .flat_map(|t| [t.a.z, t.b.z, t.c.z])
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), z| (lo.min(z), hi.max(z)));
    if !lo.is_finite() || !hi.is_finite() {
        return Ok(());
    }
    // The axis' XY where it crosses a world Z plane, off the origin's.
    let off = |z: f64| {
        let s = (z - ax.origin.z) / u.z;
        let x = ax.origin.x + u.x * s - origin[0];
        let y = ax.origin.y + u.y * s - origin[1];
        (x * x + y * y).sqrt()
    };
    let worst = off(lo).max(off(hi));
    if worst.is_finite() && worst <= SPINDLE_TOL {
        return Ok(());
    }
    let lean = u.z.clamp(-1.0, 1.0).acos().to_degrees();
    let mid = 0.5 * (lo + hi);
    Err(GenError::Invalid(format!(
        "the part's axis of revolution is not the lathe spindle: it leans {lean:.3} degrees \
         off world Z and stands {:.3} mm off the {slot} origin at Z{mid:.3} (the lathe programs \
         diameters about world Z through the work offset, {SPINDLE_TOL} mm allowed) - turned \
         as posted it would cut a taper or an eccentric; align the part's axis with world Z \
         and move the work offset onto it",
        off(mid)
    )))
}
