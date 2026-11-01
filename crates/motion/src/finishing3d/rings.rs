//! Split from `finishing3d.rs` by M5.0.1; structure only.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{Loop, Point2};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::{fit_arcs, GenError};

use super::params::{Finish3DParams, SPLIT_MZ, dirs, levels, validate};
use super::scan::{level_region, push_walk};
use super::support::closest_pt_tri;

/// The contact normal at a tangent center: the unit vector from the
/// nearest facet point toward the center. `None` when the nearest
/// point IS the center (degenerate) or the vector isn't finite. The
/// caller reads steepness off its tilt - the ramp only wants normals
/// that lean past the 45° split (walls, not floors).
fn contact_normal(facets: &[Triangle], c: Point3) -> Option<Point3> {
    let mut best: Option<Point3> = None;
    let mut best_d2 = f64::INFINITY;
    for f in facets {
        let q = closest_pt_tri(c, &[f.a, f.b, f.c]);
        let dx = c.x - q.x;
        let dy = c.y - q.y;
        let dz = c.z - q.z;
        let d2 = dx * dx + dy * dy + dz * dz;
        if d2 < best_d2 {
            best_d2 = d2;
            best = Some(q);
        }
    }
    let q = best?;
    let (dx, dy, dz) = (c.x - q.x, c.y - q.y, c.z - q.z);
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    if !(len.is_finite() && len > 1e-12) {
        return None;
    }
    Some(Point3::new(dx / len, dy / len, dz / len))
}

/// Does a straight center move from `a` to `b` keep the ball clear of
/// every facet to within `tol` of its radius? The threshold is
/// `r - tol`, not `r`: a tessellated ring boundary sits a chord-sag
/// inside the true tangent locus, so a center exactly on the tangent
/// already measures a hair under `r` (the law probe's own currency).
fn leg_clears(facets: &[Triangle], a: Point3, b: Point3, r: f64, tol: f64) -> bool {
    let limit = r - tol;
    // Only facets whose box can reach the swept volume need distance
    // tests: the leg's own box grown by the radius. A 100k-facet model
    // meets ~nothing here; the certify stays O(few) per ring.
    let (xlo, xhi) = (a.x.min(b.x) - r, a.x.max(b.x) + r);
    let (ylo, yhi) = (a.y.min(b.y) - r, a.y.max(b.y) + r);
    let (zlo, zhi) = (a.z.min(b.z) - r, a.z.max(b.z) + r);
    let near: Vec<[Point3; 3]> = facets
        .iter()
        .filter(|f| {
            let fxlo = f.a.x.min(f.b.x.min(f.c.x));
            let fxhi = f.a.x.max(f.b.x.max(f.c.x));
            let fylo = f.a.y.min(f.b.y.min(f.c.y));
            let fyhi = f.a.y.max(f.b.y.max(f.c.y));
            let fzlo = f.a.z.min(f.b.z.min(f.c.z));
            let fzhi = f.a.z.max(f.b.z.max(f.c.z));
            fxlo <= xhi && fxhi >= xlo && fylo <= yhi && fyhi >= ylo && fzlo <= zhi && fzhi >= zlo
        })
        .map(|f| [f.a, f.b, f.c])
        .collect();
    let steps = 8;
    for k in 0..=steps {
        let t = k as f64 / steps as f64;
        let p = Point3::new(
            a.x + (b.x - a.x) * t,
            a.y + (b.y - a.y) * t,
            a.z + (b.z - a.z) * t,
        );
        for tri in &near {
            let q = closest_pt_tri(p, tri);
            let dx = p.x - q.x;
            let dy = p.y - q.y;
            let dz = p.z - q.z;
            if (dx * dx + dy * dy + dz * dz).sqrt() < limit {
                return false;
            }
        }
    }
    true
}
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
            // The plane the air moves park above the ring entry:
            // the ramp's approach column certifies from its centers.
            let approach_z = match *last {
                None => cur.z,
                Some(lz) if lz != z_tip => p.clearance_z,
                Some(_) => hop,
            };
            // M8.2 B: the ramped first contact. The plunge is the
            // law's own entry; `entry_ramp` is the opt-in that
            // engages a steep wall along its contact normal instead -
            // from a center one radius out along that normal, down a
            // certified column and in along a certified diagonal.
            // `None` below is silent (a gentle tangent: the plunge IS
            // the ramp there, no reason to talk); `Err` is a refusal
            // the user asked about, and it says its reason.
            let ramp = if p.entry_ramp {
                let c = Point3::new(entry.x, entry.y, z_tip + r);
                match contact_normal(facets, c) {
                    None => Some(Err("no contact direction at the ring entry".to_string())),
                    Some(n) => {
                        let tilt = (n.x * n.x + n.y * n.y).sqrt();
                        if n.z <= 0.25 {
                            Some(Err(
                                "the contact faces down, or too near-horizontal to lift a ramp off it; an entry along such a normal is a lateral in-feed at depth".into(),
                            ))
                        } else if tilt < SPLIT_MZ {
                            None
                        } else {
                            let cs = Point3::new(c.x + r * n.x, c.y + r * n.y, c.z + r * n.z);
                            if cs.z > approach_z + r {
                                Some(Err("the ramp start sits above the approach plane".into()))
                            } else if !leg_clears(
                                facets,
                                Point3::new(cs.x, cs.y, approach_z + r),
                                cs,
                                r,
                                p.tolerance,
                            ) {
                                Some(Err("the vertical approach bites the wall".into()))
                            } else if !leg_clears(facets, cs, c, r, p.tolerance) {
                                Some(Err("the diagonal bites the wall".into()))
                            } else {
                                Some(Ok(cs))
                            }
                        }
                    }
                }
            } else {
                None
            };
            let ramped = match ramp {
                Some(Ok(cs)) => Some(cs),
                Some(Err(why)) => {
                    b.note(format!("ramp entry at z {z_tip:.4} refused: {why}"));
                    None
                }
                None => None,
            };
            if let Some(cs) = ramped {
                // Air to the start column at the approach plane...
                match *last {
                    None => b.push(MoveKind::Rapid, Point3::new(cs.x, cs.y, cur.z), None),
                    Some(lz) => {
                        if lz != z_tip {
                            b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
                            b.push(MoveKind::Rapid, Point3::new(cs.x, cs.y, p.clearance_z), None);
                        } else {
                            b.push(MoveKind::Feed, Point3::new(cur.x, cur.y, hop), Some(p.feed));
                            b.push(MoveKind::Rapid, Point3::new(cs.x, cs.y, hop), None);
                        }
                    }
                }
                // ...down the certified column (if there is air left
                // to descend), then in along the certified diagonal
                // to the tangent: the ring walk starts where it
                // always did, already cutting.
                if b.at().z > cs.z - r + 1e-9 {
                    b.push(
                        MoveKind::Plunge,
                        Point3::new(cs.x, cs.y, cs.z - r),
                        Some(p.plunge_feed),
                    );
                }
                b.push(
                    MoveKind::Feed,
                    Point3::new(entry.x, entry.y, z_tip),
                    Some(p.plunge_feed),
                );
            } else {
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
            }
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
