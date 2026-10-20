//! finishing3d.rs - 3D finishing: the waterline (Z-level ball path).
//!
//! Provenance: Z-level contouring with a ball-nose cutter is textbook
//! CAM (RS-274-level vocabulary; the rolling-ball/tangent-locus model
//! is published literature). The construction below - forbidden
//! centers as a facetwise support-function scan - is our own
//! derivation, written from geometry, not from any vendor's code,
//! output forensics, or documentation text.
//!
//! ## The laws here
//!
//! - **Forbidden centers.** The finished surface is the model itself,
//!   so a ball center may sit exactly where the ball touches the solid
//!   and nowhere it cuts in. The path at a center plane `cz` is
//!   `boundary(S ⊕ ball ∩ plane)`: the union of per-facet "where may
//!   the center be to touch this facet" sets. Walking that boundary
//!   with the ball is waterline contouring.
//! - **Only outer loops are cut paths.** A hole of the forbidden
//!   region is a set of centers farther than `r` from every facet; its
//!   boundary grazes the surface from the side buried inside the
//!   solid's own material - a center no tool can occupy (the cone's
//!   inner wavefront at `R = (base − cz) − r√2` was a 8.58 mm gouge
//!   when walked). The facet-shell fusion is a material union
//!   (`Region::union_all`, not sketch parity), so genuine islands of
//!   the model emerge as separate CCW components and CW loops are
//!   exactly the unreachable wavefronts: skip them. Interior-feature
//!   waterlines (tool inside a slot or bore) need a reachability test
//!   on hole interiors and are deferred - logged in RECOMMENDED.md.
//! - **Facet contributions are exactly convex.** A triangle plus a
//!   ball is convex, and its section at height `cz` is the convex hull
//!   of the vertex-ball circles: a per-direction support scan
//!   suffices. For direction `u` the support is `max over facet points
//!   q` of `q_xy·u + ρ(q_z)`, `ρ = √(r² − (cz − q_z)²)`; that
//!   function is concave on the triangle, so its maximum is at a
//!   vertex, or on an edge at the one point where
//!   `w·E_z = −(E_xy·u)·ρ` - and that stationary point always lies
//!   inside the feasible band `|w| ≤ r`, so when it falls outside the
//!   edge segment the maximum is at that end's vertex, already a
//!   candidate. (A face-interior stationary point would need `u` to
//!   track the facet normal's XY direction, and the contact line's
//!   edge endpoints carry the same support value - the edge and
//!   vertex candidates cover the face exactly.) Six candidates per
//!   facet per direction, no sampling.
//! - **Stepdown from the scallop, not from habit.** Successive rings
//!   leave a cusp on a wall tilted `α` from vertical of about
//!   `Δz² / (8 r cos²α)`, so `Δz = 2√(r·tol)` holds the cusp inside
//!   `tol` for every surface at `|α| ≤ 45°`. Steeper flanks and flat
//!   shelves are the scallop and parallel strategies' jurisdiction
//!   (M2.2b/M2.2c), and the envelope verifier (`verify.rs`,
//!   `Mode::Envelope`) measures - never assumes - what this stepdown
//!   leaves everywhere else.
//! - **Flat shelves are never waterline rest.** A horizontal facet
//!   only enters the forbidden region through radius-0 silhouette
//!   tangents; its interior is reachable by no ring. That is not a
//!   hole in this law, it is the parallel strategy's job description.
//! - **The entry is the tangent point.** A vertical plunge at a
//!   tangent-ring entry never cuts above the ring's level on any
//!   locally convex surface (distance from the lifted center to the
//!   tangent point grows monotonically); overhangs violate the
//!   premise, and the envelope measurement reports that, loudly, as
//!   a gouge. No lead-in arcs for M2.2a - the ring is smooth at its
//!   own entry by construction.
//! - **Level changes climb to `clearance_z`; rings within one level
//!   hop at `retract_z` clamped to clearance** - the shared ladder
//!   law of every depth-stepping operation in this codebase.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::{cut_levels, fit_arcs, GenError, Seg};

/// Everything the waterline needs that the tool and geometry don't
/// already say. Heights arrive resolved to millimeters by the
/// operation layer (same discipline as every other params shape).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Finish3DParams {
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

/// Tip-Z levels for the pass, top down, ending exactly at `bottom_z`
/// (the shared snap law). The scallop stepdown carries a hard level
/// cap: a tolerance so fine that the rings would number in the
/// hundreds of thousands is REFUSED, not quietly coarsened - silent
/// coarsening would make the stated tolerance a lie.
fn levels(p: &Finish3DParams, r: f64) -> Result<Vec<f64>, GenError> {
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
fn dirs(r: f64, tol: f64) -> Vec<(f64, f64)> {
    let want = std::f64::consts::PI * (2.0 * r / tol).sqrt();
    let n = (want.ceil() as usize).clamp(128, 16384);
    (0..n)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / n as f64;
            (a.cos(), a.sin())
        })
        .collect()
}

/// The forbidden-center polygon one facet contributes at center plane
/// `cz` - the support scan of `facet ⊕ ball ∩ plane`, exact per the
/// module's second law. Returns the empty vector (no contribution)
/// when the facet's height band misses the plane, or if every
/// direction somehow came up candidate-free (the band test makes that
/// impossible; the fallback keeps the law total without an expect).
fn facet_polygon(f: &Triangle, r: f64, cz: f64, ds: &[(f64, f64)]) -> Vec<Point2> {
    let v = [f.a, f.b, f.c];
    let zmin = v[0].z.min(v[1].z).min(v[2].z);
    let zmax = v[0].z.max(v[1].z).max(v[2].z);
    if zmax < cz - r || zmin > cz + r {
        return Vec::new();
    }
    let r2 = r * r;
    let mut pts = Vec::with_capacity(ds.len());
    for &(ux, uy) in ds {
        // The argmax candidates for this direction: in-band vertices,
        // plus each non-horizontal edge's stationary point when it
        // lands inside the segment. Six at most, no allocation.
        let mut cand = [f.a; 6];
        let mut nc = 0usize;
        for q in v {
            if (cz - q.z).abs() <= r {
                cand[nc] = q;
                nc += 1;
            }
        }
        for e in 0..3 {
            let (q0, q1) = (v[e], v[(e + 1) % 3]);
            let ez = q1.z - q0.z;
            if ez == 0.0 {
                continue; // horizontal edge: its endpoints own the max
            }
            // g'(t) = 0 along the edge means w* = -a*r*sign(ez) /
            // sqrt(ez^2 + a^2) with a = E_xy.u; |w*| <= r by
            // inspection, so t* is inside the height band whenever
            // it is inside the segment.
            let a = (q1.x - q0.x) * ux + (q1.y - q0.y) * uy;
            let wst = -a * r * ez.signum() / (ez * ez + a * a).sqrt();
            let t = (cz - q0.z - wst) / ez;
            if t > 0.0 && t < 1.0 && nc < cand.len() {
                let qz = q0.z + ez * t;
                if (cz - qz).abs() <= r {
                    cand[nc] = Point3::new(
                        q0.x + (q1.x - q0.x) * t,
                        q0.y + (q1.y - q0.y) * t,
                        qz,
                    );
                    nc += 1;
                }
            }
        }
        let mut best: Option<(f64, Point2)> = None;
        for q in &cand[..nc] {
            let rho = (r2 - (cz - q.z) * (cz - q.z)).max(0.0).sqrt();
            let p = Point2::new(q.x + rho * ux, q.y + rho * uy);
            let s = p.x * ux + p.y * uy;
            if best.is_none_or(|(b, _)| s > b) {
                best = Some((s, p));
            }
        }
        match best {
            Some((_, p)) => pts.push(p),
            None => return Vec::new(),
        }
    }
    convex_hull(&pts)
}

/// Andrew monotone-chain hull, CCW, collinear points dropped. The
/// input came from a support scan of a convex set, so this only ever
/// trims float noise - but sorting first makes the output independent
/// of the scan's starting direction either way.
fn convex_hull(pts: &[Point2]) -> Vec<Point2> {
    let mut p = pts.to_vec();
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    p.dedup_by(|a, b| a.x == b.x && a.y == b.y);
    if p.len() < 3 {
        return Vec::new();
    }
    let cross = |o: &Point2, a: &Point2, b: &Point2| {
        (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
    };
    let mut lower: Vec<Point2> = Vec::with_capacity(p.len());
    for q in &p {
        while lower.len() >= 2
            && cross(&lower[lower.len() - 2], &lower[lower.len() - 1], q) <= 0.0
        {
            lower.pop();
        }
        lower.push(*q);
    }
    let mut upper: Vec<Point2> = Vec::with_capacity(p.len());
    for q in p.iter().rev() {
        while upper.len() >= 2
            && cross(&upper[upper.len() - 2], &upper[upper.len() - 1], q) <= 0.0
        {
            upper.pop();
        }
        upper.push(*q);
    }
    lower.pop(); // each chain repeats the far endpoint of the other
    upper.pop();
    lower.extend(upper);
    if lower.len() < 3 {
        return Vec::new(); // collinear after all: no area, no loop
    }
    lower
}

/// The forbidden-center region at one center plane: every facet's
/// convex contribution fused in one Clipper pass. The law here is
/// `Region::union_all`, NOT `Region::new`: facet contributions of one
/// solid are all material, while `new` reads nested loops as holes by
/// shell parity - a law for sketched geometry. A box's bottom-facet
/// footprint genuinely sits inside its walls' offset square at low
/// levels; under parity it became a "hole", the ring walker drew a
/// ring around it, and the ball rode inside corner material (the
/// M2.2a gouge that minted this entry). Overlapping or interpenetrating
/// solids need no special case either way; duplicates now double-wind
/// as material instead of cancelling.
fn level_region(
    facets: &[Triangle],
    r: f64,
    cz: f64,
    ds: &[(f64, f64)],
) -> Result<Region, GenError> {
    let mut loops: Vec<Loop> = Vec::new();
    for f in facets {
        if !(f.a.is_finite() && f.b.is_finite() && f.c.is_finite()) {
            return Err(GenError::Invalid("model has a non-finite facet".into()));
        }
        let hull = facet_polygon(f, r, cz, ds);
        // A band-skimming facet can collapse to a segment or a point
        // (a disk tangent from the side); it forbids no area, so it
        // is no loop. That is a geometry fact, not a failure.
        if let Ok(l) = Loop::new(hull) && l.area().abs() > 1e-12 {
            loops.push(l);
        }
    }
    if loops.is_empty() {
        return Ok(Region::empty());
    }
    Ok(Region::union_all(loops)?)
}

/// Walk a fitted ring as tip-Z `z` feed moves (the private twin of
/// machining2d's `push_segs` - same Seg enum, same chaining law).
fn push_walk(b: &mut PathBuilder, segs: &[Seg], z: f64, feed: f64) {
    for seg in segs {
        match seg {
            Seg::Line(qs) => {
                for q in qs {
                    b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), Some(feed));
                }
            }
            Seg::Arc { center, ccw, to } => {
                b.push_arc(MoveKind::Feed, Point3::new(to.x, to.y, z), Some(feed), *center, *ccw);
            }
        }
    }
}

/// The waterline. `facets` is the model's triangulation; rings are
/// cut at tip-Z levels from `top_z` down to `bottom_z` at the scallop
/// stepdown; every ring is an OUTER loop of the forbidden-center
/// region (interior wavefronts are unreachable, by the law above),
/// traversed by the climb XOR with arcs fitted to the stated
/// tolerance. The result either chains through valid geometry or
/// refuses - never a half-truth.
pub fn waterline(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
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
        && p.retract_z.is_none_or(|rz| rz.is_finite()))
    {
        return Err(GenError::Invalid(
            "waterline needs finite positive tool, tolerance, feeds and heights".into(),
        ));
    }
    if facets.is_empty() {
        return Err(GenError::Empty("model has no facets".into()));
    }
    let r = p.tool_diameter * 0.5;
    let zs = levels(p, r)?;
    let ds = dirs(r, p.tolerance);
    let hop = p.retract_z.unwrap_or(p.clearance_z).min(p.clearance_z);

    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    let mut last_tip_z: Option<f64> = None;
    for &z_tip in &zs {
        let region = level_region(facets, r, z_tip + r, &ds)?;
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
            let walk = fit_arcs(&poly, p.tolerance);
            let entry = poly[0];
            let cur = b.at();
            match last_tip_z {
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
            push_walk(&mut b, &walk, z_tip, p.feed);
            last_tip_z = Some(z_tip);
        }
    }
    if last_tip_z.is_none() {
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
