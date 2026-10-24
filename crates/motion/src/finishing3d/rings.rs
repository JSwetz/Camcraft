//! Split from `finishing3d.rs` by M5.0.1; structure only.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{Loop, Point2};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::{fit_arcs, GenError};

use super::params::{Finish3DParams, dirs, levels, validate};
use super::scan::{level_region, push_walk};
/// The ring ladder proper: one pass of the levels loop, appended to
/// `b`. `last` is the tip-Z of the path's last cut (or `None` when
/// the builder has never cut) - the choreography reads it exactly
/// like the waterline's own loop did, so a parallel path can hand
/// over after its raster with `last = Some(clearance_z)` and climb
/// in from there.
pub(super) fn emit_rings(
    b: &mut PathBuilder,
    p: &Finish3DParams,
    r: f64,
    facets: &[Triangle],
    zs: &[f64],
    ds: &[(f64, f64)],
    last: &mut Option<f64>,
) -> Result<bool, GenError> {
    let hop = p.retract_z.unwrap_or(p.clearance_z).min(p.clearance_z);
    let mut any = false;
    for &z_tip in zs {
        let region = level_region(facets, r, z_tip + r, ds)?;
        if region.is_empty() {
            continue; // no surface reaches this plane within a ball
        }
        // Outer loops only. A hole of the forbidden region is a set of
        // centers farther than r from every facet - and the ring around
        // such a hole grazes the surface from the far side, the side
        // buried inside the solid's own material. The cone proved it: at
        // low levels the region is an annulus between the tangent circle
        // the tool can actually reach (R = slope-offset, from outside)
        // and an inner wavefront (R = (base − cz) − r√2, from inside),
        // and a walk of the CW inner loop plunged the ball into live
        // material (the 8.58 mm gouge at (7.56, −1.06)). Genuine islands
        // of the model are separate CCW components, never CW holes - so
        // filtering on orientation keeps islands (two domes, twin rings,
        // hops between them) and discards unreachable wavefronts.
        let mut loops: Vec<&Loop> = region.loops().iter().filter(|l| l.is_ccw()).collect();
        loops.sort_by(|a, b| b.area().abs().total_cmp(&a.area().abs()));
        for l in loops {
            let stored: &[Point2] = l.points();
            if stored.len() < 2 {
                continue;
            }
            let mut poly: Vec<Point2> = if p.climb {
                stored.to_vec()
            } else {
                stored.iter().rev().copied().collect()
            };
            poly.push(poly[0]); // close the ring for the fitter
            // The same chord law as the raster's: every XY shortcut of
            // the plan holds to tolerance/4, not the full tolerance.
            // The tessellated boundary already sits a chord-sag inside
            // the true offset locus; a ring fitted at the full budget
            // stacks its shortcut on top of that sag, and at a corner
            // - where the vertex-sphere's radius changes fast - the
            // Z-direction envelope sees more than the XY figure of
            // speech promised (the tilted wedge's low corner, 0.144 mm
            // at a full-tolerance fit).
            let walk = fit_arcs(&poly, p.tolerance * 0.25);
            let entry = poly[0];
            let cur = b.at();
            match *last {
                None => {
                    b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, cur.z), None);
                }
                Some(lz) => {
                    if lz != z_tip {
                        // Level to level: the new level engages
                        // material the previous one never touched -
                        // climb to the one plane safe over all of it.
                        b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
                        b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, p.clearance_z), None);
                    } else {
                        // Ring to ring within one level: the hop.
                        b.push(MoveKind::Feed, Point3::new(cur.x, cur.y, hop), Some(p.feed));
                        b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, hop), None);
                    }
                }
            }
            b.push(
                MoveKind::Plunge,
                Point3::new(entry.x, entry.y, z_tip),
                Some(p.plunge_feed),
            );
            push_walk(b, &walk, z_tip, p.feed);
            *last = Some(z_tip);
            any = true;
        }
    }
    Ok(any)
}

/// The waterline: the ring ladder alone. One closed cut at each level
/// of the scallop stepdown ladder, wherever a ball of radius `r`
/// resting tip-deep at that Z touches the surface.
pub fn waterline(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    let r = p.tool_diameter * 0.5;
    let zs = levels(p, r)?;
    let ds = dirs(r, p.tolerance);
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    let mut last: Option<f64> = None;
    if !emit_rings(&mut b, p, r, facets, &zs, &ds, &mut last)? {
        return Err(GenError::Empty(
            "no surface reaches within one ball radius of the requested depths".into(),
        ));
    }
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("waterline path left the finite horizon".into()));
    }
    Ok(path)
}
