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
//! laws hold about the spindle the machine turns. The reach and holder
//! laws (issue #84, `drill_reach`) judge the chain in the same frame; the finished chain, back in the
//! world, is judged against the part's solid (issue #117, `cross_part`).
//!
//! Provenance: a lathe's or mill-turn's live-tool radial position is
//! measured from the spindle centreline (RS-274 / ISO 6983 lathe
//! convention, any turning handbook); the frame shift is plain
//! translation. Our own design notes; no vendor material consulted.

use crate::cl::{lattice, Toolpath};
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
    // `spindle_reading` reads nothing for exactly three reasons; name
    // the one that holds.
    let why = match crate::multiaxis::axis::fit(facets) {
        Err(e) => e.to_string(),
        Ok(ax) => {
            let n = ax.dir.dot(ax.dir).sqrt();
            if n.is_finite() && n > 0.0 {
                "its axis lies near the table".to_string()
            } else {
                "the fitted axis has no usable direction".to_string()
            }
        }
    };
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
    tool_name: &str,
) -> Result<Toolpath, GenError> {
    let (ox, oy) = (origin[0], origin[1]);
    if !(ox.is_finite() && oy.is_finite()) {
        return Err(GenError::Invalid("the work offset's origin is not usable".into()));
    }
    plunges_inward(p.holes, ox, oy)?;
    // Z is the spindle's own coordinate - no shift. A Z top known lets
    // the kernel route an opposite-station travel over the end face
    // (M5.2 G); a solid with no vertices says so honestly.
    let st = super::cross_stock::standoff(facets, stock, ox, oy);
    let on_origin = ox == 0.0 && oy == 0.0;
    let local: Vec<CrossHole> = if on_origin {
        p.holes.to_vec()
    } else {
        p.holes
            .iter()
            .map(|h| CrossHole { origin: Point3::new(h.origin.x - ox, h.origin.y - oy, h.origin.z), ..*h })
            .collect()
    };
    let (local, extra) = match st.raw {
        Some(raw) => super::cross_stock::to_raw_mouths(&local, &st, raw, p.tool)?,
        None => (local, Vec::new()),
    };
    // Issue #84: no hole deeper from the stock surface than the flute
    // (the wider of drill and shank sets where the stock surface is).
    super::drill_reach::cross_reach(&local, &extra, p.holes, p.cycle, p.tool, tool_name)?;
    // A shank wider than the drill raises the planes to clear it.
    let (clearance_d, hop_d) = super::cross_stock::body_clear(p.clearance_d, p.hop_d, p.tool);
    let mut t = drilling::cross_drill(&CrossParams {
        clearance_d,
        hop_d,
        stock_r: st.r.max(1.0),
        stock_z_max: st.z_max,
        holes: &local,
        ..p
    })
    .map_err(|e| in_world(e, &local, p.holes))?;
    super::cross_stock::rapids_clear(&t, &st, p.tool)?;
    super::drill_reach::cross_holder(&t, facets, stock, (ox, oy), p.tool, tool_name)?;
    if on_origin {
        // Issue #117: what the chain leaves is judged against the part.
        super::cross_part::part_clear(&t, facets, p.tool, p.holes)?;
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
    super::cross_part::part_clear(&t, facets, p.tool, p.holes)?;
    Ok(t)
}

/// Every hole must plunge TOWARD the spindle line: the kernel stands a
/// station on the mouth's own side at the hop radius, which for a
/// plunge pointing away from the axis is a point beyond the part's far
/// side - and the rapid in to the mouth would run through the material.
/// (A mill-turn block off its work offset has such faces: the ones
/// nearer the spindle line.) Read in the spindle frame - the mouth's
/// radial offset from world Z through (`ox`, `oy`) dotted with the
/// plunge - and refused by name in world coordinates. A non-finite
/// hole falls through to the kernel's own malformed-list refusal.
fn plunges_inward(holes: &[CrossHole], ox: f64, oy: f64) -> Result<(), GenError> {
    for h in holes {
        let out = (h.origin.x - ox) * h.dir.x + (h.origin.y - oy) * h.dir.y;
        if out > 1e-9 {
            return Err(GenError::Invalid(format!(
                "cross hole at ({}, {}, {}) plunges along ({}, {}, {}), away from the spindle (world Z                  through ({ox}, {oy})): its station would stand beyond the part's far side and the                  rapid in would cross the material - drill it from the face toward the spindle, or                  move the work offset",
                h.origin.x, h.origin.y, h.origin.z, h.dir.x, h.dir.y, h.dir.z
            )));
        }
    }
    Ok(())
}

/// Carry a kernel refusal from the spindle frame back to the world: the
/// kernel names a hole by its (raw or lattice-snapped) mouth, as an XYZ
/// or an XY point; the first hole whose point the message carries has
/// it rewritten to the world mouth the user modelled.
fn in_world(e: GenError, local: &[CrossHole], world: &[CrossHole]) -> GenError {
    let fix = |s: String| -> String {
        for (l, w) in local.iter().zip(world) {
            let wo = w.origin;
            let (w3, w2) = (format!("({}, {}, {})", wo.x, wo.y, wo.z), format!("({}, {})", wo.x, wo.y));
            let snap = Point3::new(lattice(l.origin.x), lattice(l.origin.y), lattice(l.origin.z));
            let mut out = s.clone();
            for lo in [l.origin, snap] {
                out = out.replace(&format!("({}, {}, {})", lo.x, lo.y, lo.z), &w3);
                out = out.replace(&format!("({}, {})", lo.x, lo.y), &w2);
            }
            if out != s {
                return out;
            }
        }
        s
    };
    match e {
        GenError::Invalid(s) => GenError::Invalid(fix(s)),
        GenError::Empty(s) => GenError::Empty(fix(s)),
        // #261 part 2: the spindle frame is a pure translation (see
        // `back` above), so the spot rides the same world - local shift
        // every mouth does. No hole to read it from: the spot is dropped,
        // never shown in the wrong frame.
        GenError::At { msg, at } => {
            let msg = fix(msg);
            match local.first().zip(world.first()) {
                Some((l, w)) => GenError::invalid_at(
                    msg,
                    Point3::new(
                        at.x + (w.origin.x - l.origin.x),
                        at.y + (w.origin.y - l.origin.y),
                        at.z + (w.origin.z - l.origin.z),
                    ),
                ),
                None => GenError::Invalid(msg),
            }
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hole(x: f64, y: f64) -> CrossHole {
        CrossHole {
            origin: Point3::new(x, y, 5.0),
            dir: Point3::new(-1.0, 0.0, 0.0),
            radius: 2.0,
            entry_d: 0.0,
            floor_d: -8.0,
            through: false,
        }
    }

    #[test]
    fn a_located_kernel_refusal_lands_on_the_world_mouth() {
        // #261 part 2: the kernel ran 10 mm left of the world (ox = 10),
        // so its spot and its words both move back by that shift.
        let (local, world) = ([hole(20.0, 0.0)], [hole(30.0, 0.0)]);
        let e = GenError::invalid_at("a Drill cycle at (20, 0) plunges into solid", Point3::new(20.0, 0.0, 5.0));
        let w = in_world(e, &local, &world);
        assert_eq!(w.location(), Some(Point3::new(30.0, 0.0, 5.0)));
        assert!(w.to_string().contains("at (30, 0)"), "{w}");
        // No hole to read the shift from: the spot is dropped, never shown in the wrong frame.
        let lone = in_world(GenError::invalid_at("x", Point3::new(1.0, 2.0, 3.0)), &[], &[]);
        assert_eq!(lone, GenError::Invalid("x".into()));
    }
}
