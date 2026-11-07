//! The cross drill about the posted spindle (issue #67): the radial
//! kernel (`drilling::cross_drill`) measures every standoff - the bar's
//! radius, the station's side, the hop and clearance radii, the chord
//! law between stations - about world Z through world (0, 0). The post
//! programs about world Z through the WORK OFFSET's origin, so a bar
//! moved off the world origin with its offset moved onto it would be
//! drilled about a line it is not on: standoffs measured from the wrong
//! centre, and a program that differs from the on-origin one. Here the
//! job is carried into the spindle's frame (an XY shift by the origin
//! `posting::wcs_context` hands the post, the one the spindle law
//! judges against), run there, and carried back - so the kernel's own
//! laws hold about the spindle the machine turns.
//!
//! Provenance: a lathe's or mill-turn's live-tool radial position is
//! measured from the spindle centreline (RS-274 / ISO 6983 lathe
//! convention, any turning handbook); the frame shift is plain
//! translation. Our own design notes; no vendor material consulted.

use crate::cl::Toolpath;
use crate::drilling::{self, CrossHole, CrossParams};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

/// The plain lathe's cross drill stands on the same reading its axial
/// drill does: a part the axis fit cannot read as revolved has no
/// spindle the law could put it on, and is refused by name - never
/// drilled about a guessed centre. (A mill-turn's cross drill takes a
/// prismatic bar; it is measured about the work offset as it stands.)
pub(super) fn lathe_reads(facets: &[Triangle], origin: [f64; 3]) -> Result<(), GenError> {
    if super::spindle_law::spindle_reading(facets, origin).is_some() {
        return Ok(());
    }
    let why = crate::multiaxis::axis::fit(facets)
        .err()
        .map_or_else(|| "its axis lies near the table".to_string(), |e| e.to_string());
    Err(GenError::Invalid(format!(
        "a lathe's cross drill plunges toward the spindle, and this part cannot be read as \
         turned about one ({why}) - model the part as the turned bar it is, or drill it on a \
         mill-turn"
    )))
}

/// Run the radial kernel about world Z through `origin`: `p` carries the
/// heights, the cycle, the tool and the world-frame hole list; its
/// `stock_r` and `stock_z_max` are recomputed here about that line from
/// the larger of the part's `facets` and the setup's `stock` (issue
/// #77, `cross_stock`: a raw bar wider than the part moves the mouths
/// out to it, and the chain is proven clear of it). An origin on the
/// world Z axis runs the kernel unshifted, and a stock that does not
/// reach past the part (or none) keeps the part-only chain's bits.
pub(super) fn cross_drill(
    p: CrossParams<'_>,
    facets: &[Triangle],
    stock: Option<&[Triangle]>,
    origin: [f64; 3],
) -> Result<Toolpath, GenError> {
    let (ox, oy) = (origin[0], origin[1]);
    if !(ox.is_finite() && oy.is_finite()) {
        return Err(GenError::Invalid("the work offset's origin is not usable".into()));
    }
    // Z is the spindle's own coordinate - no shift. A Z top known lets
    // the kernel route an opposite-station travel over the end face
    // (M5.2 G); a solid with no vertices says so honestly.
    let st = super::cross_stock::standoff(facets, stock, ox, oy);
    let d = p.tool.diameter;
    let on_origin = ox == 0.0 && oy == 0.0;
    let local: Vec<CrossHole> = if on_origin {
        p.holes.to_vec()
    } else {
        p.holes
            .iter()
            .map(|h| CrossHole { origin: Point3::new(h.origin.x - ox, h.origin.y - oy, h.origin.z), ..*h })
            .collect()
    };
    let local = match st.raw {
        Some(raw) => super::cross_stock::to_raw_mouths(&local, raw),
        None => local,
    };
    let mut t = drilling::cross_drill(&CrossParams {
        stock_r: st.r.max(1.0),
        stock_z_max: st.z_max,
        holes: &local,
        ..p
    })?;
    super::cross_stock::rapids_clear(&t, &st, d)?;
    if on_origin {
        return Ok(t);
    }
    // The kernel's chain is straight, cycle-free and computer-
    // compensated; anything else would need its own carry back, so it
    // refuses by name rather than post half-shifted.
    if !t.cycles.is_empty() || !t.comps.is_empty() || t.has_arcs() {
        return Err(GenError::Invalid(
            "internal: the cross-drill chain grew records the spindle shift does not carry".into(),
        ));
    }
    // A plain carry: the kernel snapped what it snaps (the mouths) in
    // its own frame, and re-snapping here would move its unsnapped
    // chord points off the on-origin chain's.
    let back = |q: Point3| Point3::new(q.x + ox, q.y + oy, q.z);
    for m in &mut t.moves {
        m.from = back(m.from);
        m.to = back(m.to);
    }
    if !t.is_chained() || !t.all_finite() {
        return Err(GenError::Invalid("cross-drill chain self-check failed after the spindle shift".into()));
    }
    Ok(t)
}
