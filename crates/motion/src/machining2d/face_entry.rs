//! The facer's air entry (M11.3 owed (4)): when a facing entry would
//! plunge a tool that is not centre-cutting onto the stock - facing an
//! inner patch of a wider block - the entry runs on back along the first
//! row's own direction until the tool disc clears the stock's outline by
//! [`AIR_MARGIN`], so the plunge falls through air and the row feeds in
//! sideways. The run-on may not touch a forbidden zone (an exclude
//! boundary, the far side of an open boundary, part material standing
//! above the cut) and must find air within a sane reach; otherwise the
//! caller keeps the named refusal.
//!
//! Provenance: the handbook rule that a non-centre-cutting end mill
//! enters from outside the material (or ramps/helixes) and never plunges;
//! the search along the row is this crate's own geometry. No vendor
//! material consulted.

use crate::cl::{MoveKind, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::plunge_law::{disc_overlaps, first_solid_plunge, Solid};

/// How far the tool disc stands clear of the stock outline at an air
/// plunge (mm) - a small allowance for stock that is not quite where
/// its model says.
pub const AIR_MARGIN: f64 = 1.0;

/// How far past the entry the search for air goes before giving up: the
/// stock outline's own diagonal plus a tool diameter and the margin -
/// anything farther cannot be "just outside the stock".
fn reach(solid: &Solid, tool_r: f64) -> Option<f64> {
    let (lo, hi) = solid.footprint?.bounds()?;
    let d = (hi.x - lo.x).hypot(hi.y - lo.y) + 2.0 * tool_r + AIR_MARGIN;
    d.is_finite().then_some(d)
}

/// The run-on that moves `path`'s first plunge into solid out to air:
/// `Ok(None)` when no plunge lands on the stock (nothing to do), `Ok(Some(s))`
/// with the extra lead `s` (mm) along the reversed first-row direction,
/// or `Err(why)` naming why no air entry exists - unseen stock (no
/// outline, so solid everywhere), no row to run back along, no air
/// within reach, or a forbidden zone in the way (`blocked`, its label).
pub fn air_lead(
    path: &Toolpath,
    tool_r: f64,
    solid: &Solid,
    blocked: &[(&str, &Region)],
) -> Result<Option<f64>, String> {
    let Some((xy, _)) = first_solid_plunge(path, tool_r, solid) else {
        return Ok(None);
    };
    if !(tool_r.is_finite() && tool_r > 0.0) {
        return Err("the tool radius is not usable".into());
    }
    let Some(reach) = reach(solid, tool_r) else {
        return Err("the stock outline is unknown, so no point beside it is known to be air".into());
    };
    // The row the plunge enters: the first XY feed after it.
    let at = path.moves.iter().position(|m| {
        m.kind == MoveKind::Plunge && (m.to.x - xy.x).hypot(m.to.y - xy.y) < 1e-9
    });
    let row = at.and_then(|i| path.moves.get(i + 1)).filter(|m| m.kind == MoveKind::Feed);
    let Some(row) = row else {
        return Err("the entry has no row to run back along".into());
    };
    let (dx, dy) = (row.to.x - row.from.x, row.to.y - row.from.y);
    let n = dx.hypot(dy);
    if !(n.is_finite() && n > 1e-9) {
        return Err("the entry has no row to run back along".into());
    }
    let back = Point2::new(-dx / n, -dy / n);
    let along = |s: f64| Point2::new(xy.x + back.x * s, xy.y + back.y * s);
    let h = (tool_r * 0.25).clamp(0.05, 1.0);
    let steps = (reach / h).ceil().min(1e6) as usize;
    let Some(s) = (1..=steps).map(|k| k as f64 * h).find(|&s| !solid.under(along(s), tool_r + AIR_MARGIN)) else {
        return Err(format!("no point within {reach:.1} mm back along the first row clears the stock"));
    };
    // The swept run-on, sampled at `h` and read with the disc grown by
    // `h` - so no sliver between samples escapes the check. The original
    // entry disc is the facer's own (its band law already read it).
    let samples = (s / h).ceil() as usize;
    for k in 1..=samples {
        let p = along((k as f64 * h).min(s));
        if let Some((label, _)) = blocked.iter().find(|(_, z)| disc_overlaps(z, p, tool_r + h)) {
            return Err(format!("the run-on to air along the first row crosses {label}"));
        }
    }
    Ok(Some(s))
}
