//! finishing3d.rs - 3D finishing: the waterline (Z-level ball path)
//! and the parallel (Z-following raster) strategies.
//!
//! Provenance: Z-level contouring with a ball-nose cutter is textbook
//! CAM (RS-274-level vocabulary; the rolling-ball/tangent-locus model
//! is published literature). The parallel raster's cusp law - a
//! Z-following stepover of `√(8·r·tol)·cos α` between ball paths
//! offset `s·sec α` apart, the Huang-Oliver scallop formula specialised
//! to a tilted plane - and the point-to-triangle distance test are
//! published geometry (Ericson, Real-Time Collision Detection, for the
//! region test). The construction below - forbidden centers as a
//! facetwise support-function scan, the tangent-support center law
//! `c = q + r·n̂`, and the adaptive chord march - is our own derivation,
//! written from geometry, not from any vendor's code, output forensics,
//! or documentation text.
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

/// The steep/shallow jurisdiction line, as a cosine: facets whose
/// upward normal tilts more than 45° off vertical get no raster
/// support and belong to the rings. The same 45° the stepdown law
/// already promises (`|α| ≤ 45°` keeps the ring cusp inside `tol`),
/// so the two strategies meet exactly at each one's limit - no
/// territory claimed twice, none dropped.
const SPLIT_MZ: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// The numeric skin of the laws: a center that touches a facet sits
/// at distance exactly `r` from it, and float says `r ± 1e-9·r`.
/// Accept contact within this much bite.
const TOUCH_EPS: f64 = 1e-7;

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

/// The shared numeric gate every 3D finish reads before touching
/// geometry. Messages say "3D finish", not one strategy's name - the
/// check belongs to both.
fn validate(p: &Finish3DParams, facets: &[Triangle]) -> Result<(), GenError> {
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
        && p.retract_z.is_none_or(|rz| rz.is_finite()))
    {
        return Err(GenError::Invalid(
            "3D finish needs finite positive tool, tolerance, feeds and heights".into(),
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

/// A facet prepared for the raster's support scan. `m` is the unit
/// normal pointed into the sky (the ball rests on the plane from
/// above whichever way the winding went); `shallow` marks the facets
/// the raster may touch; the padded box rejects far facets per
/// sample without touching their vertices.
struct Sup {
    tri: [Point3; 3],
    m: Point3,
    shallow: bool,
    xmin: f64,
    ymin: f64,
    xmax: f64,
    ymax: f64,
}

fn build_sups(facets: &[Triangle], r: f64) -> Vec<Sup> {
    facets
        .iter()
        .map(|f| {
            let mut m = f.normal();
            if m.z < 0.0 {
                m = Point3::new(-m.x, -m.y, -m.z);
            }
            let tri = [f.a, f.b, f.c];
            Sup {
                tri,
                shallow: m.is_finite() && m.z >= SPLIT_MZ,
                xmin: tri[0].x.min(tri[1].x.min(tri[2].x)) - r,
                xmax: tri[0].x.max(tri[1].x.max(tri[2].x)) + r,
                ymin: tri[0].y.min(tri[1].y.min(tri[2].y)) - r,
                ymax: tri[0].y.max(tri[1].y.max(tri[2].y)) + r,
                m,
            }
        })
        .collect()
}

/// Point-to-triangle squared distance by the classic Voronoi-region
/// decomposition (published closed form - dot products and one
/// barycentric solve, no iteration).
fn dist2_pt_tri(p: Point3, t: &[Point3; 3]) -> f64 {
    let (a, b, c) = (t[0], t[1], t[2]);
    let (abx, aby, abz) = (b.x - a.x, b.y - a.y, b.z - a.z);
    let (acx, acy, acz) = (c.x - a.x, c.y - a.y, c.z - a.z);
    let (apx, apy, apz) = (p.x - a.x, p.y - a.y, p.z - a.z);
    let d1 = abx * apx + aby * apy + abz * apz;
    let d2 = acx * apx + acy * apy + acz * apz;
    let (bpx, bpy, bpz) = (p.x - b.x, p.y - b.y, p.z - b.z);
    let d3 = abx * bpx + aby * bpy + abz * bpz;
    let d4 = acx * bpx + acy * bpy + acz * bpz;
    let (cpx, cpy, cpz) = (p.x - c.x, p.y - c.y, p.z - c.z);
    let d5 = abx * cpx + aby * cpy + abz * cpz;
    let d6 = acx * cpx + acy * cpy + acz * cpz;
    let vc = d1 * d4 - d3 * d2;
    let vb = d5 * d2 - d1 * d6;
    let va = d3 * d6 - d5 * d4;
    // One region per arm; each computes the closest point q, and the
    // arms are exhaustive by the published case proof.
    let q = if d1 <= 0.0 && d2 <= 0.0 {
        a
    } else if d3 >= 0.0 && d4 <= d3 {
        b
    } else if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3).max(1e-300);
        Point3::new(a.x + abx * v, a.y + aby * v, a.z + abz * v)
    } else if d6 >= 0.0 && d5 <= d6 {
        c
    } else if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6).max(1e-300);
        Point3::new(a.x + acx * w, a.y + acy * w, a.z + acz * w)
    } else if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6)).max(1e-300);
        Point3::new(b.x + (c.x - b.x) * w, b.y + (c.y - b.y) * w, b.z + (c.z - b.z) * w)
    } else {
        let denom = (va + vb + vc).max(1e-300);
        let (v, w) = (vb / denom, vc / denom);
        Point3::new(
            a.x + abx * v + acx * w,
            a.y + aby * v + acy * w,
            a.z + abz * v + acz * w,
        )
    };
    let (dx, dy, dz) = (p.x - q.x, p.y - q.y, p.z - q.z);
    (dx * dx + dy * dy + dz * dz).max(0.0)
}

/// The center is clear when every facet keeps at least `r` of ball
/// around it - the same no-gouge law the ring enforces through the
/// forbidden region, asked of one point at a time.
fn ball_clear(c: Point3, sups: &[Sup], r: f64) -> bool {
    for s in sups {
        if c.x < s.xmin || c.x > s.xmax || c.y < s.ymin || c.y > s.ymax {
            continue; // outside the padded footprint: the ball cannot reach it
        }
        if dist2_pt_tri(c, &s.tri) < (r - TOUCH_EPS) * (r - TOUCH_EPS) {
            return false;
        }
    }
    true
}

/// The tangent-center height of a ball resting on one shallow facet
/// above center XY `(x, y)`: the contact `q = (x, y) − r·m_xy` must
/// fall inside the facet's XY projection (a shallow facet's
/// projection is never degenerate), and the center sits at
/// `q_z + r·m_z`. None when this facet owns no tangent here.
fn tangent_z(s: &Sup, x: f64, y: f64, r: f64) -> Option<f64> {
    if !s.shallow {
        return None;
    }
    let (a, b, c) = (s.tri[0], s.tri[1], s.tri[2]);
    let qx = x - r * s.m.x;
    let qy = y - r * s.m.y;
    // Same-sign barycentrics against the XY projection.
    let det = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    let w0 = ((b.x - a.x) * (qy - a.y) - (b.y - a.y) * (qx - a.x)) / det;
    let w1 = ((c.x - a.x) * (a.y - qy) - (c.y - a.y) * (a.x - qx)) / det;
    let w2 = 1.0 - w0 - w1;
    let tol = 1e-9; // barycentric slack: contact may sit on an edge or a vertex
    if w0 < -tol || w1 < -tol || w2 < -tol {
        return None;
    }
    // The plane through the triangle, solved for z (m_z >= SPLIT_MZ).
    let qz = a.z - (s.m.x * (qx - a.x) + s.m.y * (qy - a.y)) / s.m.z;
    (qz.is_finite() && (qz + r * s.m.z).is_finite()).then_some(qz + r * s.m.z)
}

/// The raster's whole law in one question: where does the ball sit
/// above `(x, y)`? The HIGHEST tangent center that keeps the ball
/// clear of every facet - the topmost surface a descending tool
/// meets first - and `None` over air, over unreached overhangs, and
/// over anything whose tangent sits below the programmed floor: the
/// floor outranks the surface, nothing cuts below `bottom_z`.
fn support_at(x: f64, y: f64, r: f64, floor_tip: f64, sups: &[Sup]) -> Option<f64> {
    let mut best: Option<f64> = None;
    for s in sups {
        if !s.shallow || x < s.xmin || x > s.xmax || y < s.ymin || y > s.ymax {
            continue;
        }
        let Some(cz) = tangent_z(s, x, y, r) else { continue };
        if cz - r < floor_tip - TOUCH_EPS {
            continue;
        }
        if best.is_some_and(|b| b >= cz) {
            continue; // below the current best: clearing it cannot raise the answer
        }
        if ball_clear(Point3::new(x, y, cz), sups, r) {
            best = Some(cz);
        }
    }
    best
}

/// Append a sample to the piece being marched. Runs are walked once;
/// only the zero-length duplicate a finished bisection can leave
/// behind is refused.
fn push_run_pt(line: &mut Vec<(Point2, f64)>, q: Point2, c: f64) {
    if line.last().is_none_or(|(lq, _)| lq.x != q.x || lq.y != q.y) {
        line.push((q, c));
    }
}

/// The march laws, stated once for every Z-following curve family -
/// raster lines, radial chords, the spiral (M2.2d). Samples ride a
/// parameterized curve `pos(t)` from `t0` to `t1`; `unit(t)` is the
/// parameter advance that carries ONE millimeter of XY ground
/// (1.0 for lines, 1/radius for the spiral); `dist(a, b)` is the
/// true XY distance between two parameters - the break law and the
/// step arithmetic speak millimeters, not parameter, so a curve
/// that parameterizes unevenly still promises what a line does.
/// The three refinements that make the march honest: validity
/// bisection at a run's ends, so a plunge starts just INSIDE the
/// material's edge rather than a hair outside it, hanging the ball
/// over the edge to dig; chord bisection, so the sample string over
/// tessellated ridges sinks at most `eps_chord` of center - the
/// same budget split the ring's scan law makes; and the
/// discontinuity break, so a curve whose next sample stands on a
/// DIFFERENT surface (the center leapt more than XY-span + tol)
/// breaks there instead of dragging one move's chord through the
/// shelf's edge - no cut move ever spans two surfaces. Returns the
/// curve's pieces, in travel order, air stretches gone.
#[allow(clippy::too_many_arguments)] // the march kernel: one slot per law, all three callers fill all nine
fn march_curve(
    t0: f64,
    t1: f64,
    pos: &impl Fn(f64) -> Point2,
    dist: &impl Fn(f64, f64) -> f64,
    unit: &impl Fn(f64) -> f64,
    sup: &impl Fn(Point2) -> Option<f64>,
    delta0: f64,
    tol: f64,
    eps_chord: f64,
) -> Vec<Vec<(Point2, f64)>> {
    const MAX_BISECT: usize = 24;
    let delta_min = delta0 / 4096.0;
    let mut line: Vec<(Point2, f64)> = Vec::new();
    let mut pieces: Vec<Vec<(Point2, f64)>> = Vec::new();
    let mut t = t0;
    let mut cz = sup(pos(t));
    if let Some(c) = cz {
        push_run_pt(&mut line, pos(t), c);
    }
    'march: while t <= t1 {
        if cz.is_none() {
            // Skip phase: coarse search for the next edge of
            // material, then bisect the start of the run.
            let tn = (t + 4.0 * delta0 * unit(t)).min(t1);
            if tn <= t {
                break;
            }
            match sup(pos(tn)) {
                None => t = tn,
                Some(c) => {
                    // The stretch between was not cuttable ground -
                    // air or occlusion. Whatever stands here is a
                    // new run, not a continuation of the line:
                    // append-only would ride one lateral Feed
                    // across the gap, through whatever wall or
                    // shelf edge hides the ground. Flush the line.
                    if !line.is_empty() {
                        pieces.push(std::mem::take(&mut line));
                    }
                    let (mut lo, mut hic, mut hicz) = (t, tn, c); // lo invalid, hic valid
                    for _ in 0..MAX_BISECT {
                        let mid = (lo + hic) * 0.5;
                        match sup(pos(mid)) {
                            Some(cm) => {
                                hic = mid;
                                hicz = cm;
                            }
                            None => lo = mid,
                        }
                    }
                    push_run_pt(&mut line, pos(hic), hicz);
                    t = hic;
                    cz = Some(hicz);
                }
            }
        } else {
            // Valid phase: march inside material.
            let mut delta = delta0;
            loop {
                let tn = (t + delta * unit(t)).min(t1);
                // Two ways the parameter stops advancing: the step
                // clamps onto t1 (the spiral's inner end sits exactly
                // there, still valid ground), or float stagnation -
                // `t` grown so far past `t1` that `delta * unit(t)`
                // no longer moves it. Either way the march is over;
                // a plain `break` here would return to the outer
                // condition, which is still true at t == t1, and the
                // loop would spin forever re-clamping the same
                // sample. Break the MARCH, not the step.
                if tn <= t {
                    break 'march;
                }
                match sup(pos(tn)) {
                    None => {
                        // End of the run: bisect to its edge from
                        // the last known valid point.
                        let (mut lo, mut loc, mut hic) = (t, cz.unwrap_or(0.0), tn);
                        for _ in 0..MAX_BISECT {
                            let mid = (lo + hic) * 0.5;
                            match sup(pos(mid)) {
                                Some(cm) => {
                                    lo = mid;
                                    loc = cm;
                                }
                                None => hic = mid,
                            }
                        }
                        push_run_pt(&mut line, pos(lo), loc);
                        t = lo;
                        cz = None;
                        break;
                    }
                    Some(c) if (c - cz.unwrap_or(c)).abs() > dist(t, tn) + tol => {
                        // Surface discontinuity: the center leapt
                        // higher than the terrain could climb over
                        // the same XY span - the next sample stands
                        // on a different surface (a shelf's edge,
                        // the mouth above a hidden floor). One move
                        // across that gap would drag its chord
                        // through material, so the curve BREAKS
                        // here: this piece ends behind, a new one
                        // starts here, and the choreography hands
                        // over at the hop, the ball clear of both
                        // surfaces in between.
                        if !line.is_empty() {
                            pieces.push(std::mem::take(&mut line));
                        }
                        push_run_pt(&mut line, pos(tn), c);
                        t = tn;
                        cz = Some(c);
                    }
                    Some(c) if delta > delta_min => {
                        let cur = cz.unwrap_or(c);
                        let mid = (t + tn) * 0.5;
                        let dev = match sup(pos(mid)) {
                            Some(cm) => (cm - (cur + c) * 0.5).abs(),
                            None => f64::INFINITY, // straddles an edge: refine
                        };
                        if dev > eps_chord {
                            delta *= 0.5;
                            continue;
                        }
                        push_run_pt(&mut line, pos(tn), c);
                        t = tn;
                        cz = Some(c);
                        delta = (delta * 2.0).min(delta0);
                    }
                    Some(c) => {
                        push_run_pt(&mut line, pos(tn), c);
                        t = tn;
                        cz = Some(c);
                    }
                }
                if t >= t1 {
                    break 'march;
                }
            }
        }
    }
    if !line.is_empty() {
        pieces.push(line);
    }
    pieces
}

/// The ladder's own choreography: curve to curve inside one pass
/// hops at `hop` (the retract FEEDed up, so the envelope sees the
/// column it vacates); pass to pass climbs to clearance with
/// rapids, like a level change among rings; the very first entry
/// just travels over at its own height, like the first ring.
fn emit_runs(
    b: &mut PathBuilder,
    p: &Finish3DParams,
    r: f64,
    runs: &[Vec<(Point2, f64)>],
    continuing: bool,
) {
    let hop = p.retract_z.unwrap_or(p.clearance_z).min(p.clearance_z);
    for (i, run) in runs.iter().enumerate() {
        let (x0, y0) = (run[0].0.x, run[0].0.y);
        let z0 = run[0].1 - r;
        let cur = b.at();
        if i == 0 && !continuing {
            b.push(MoveKind::Rapid, Point3::new(x0, y0, cur.z.max(z0)), None);
        } else if i == 0 {
            let trav = p.clearance_z.max(z0);
            b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, trav), None);
            b.push(MoveKind::Rapid, Point3::new(x0, y0, trav), None);
        } else {
            let trav = hop.max(z0);
            b.push(MoveKind::Feed, Point3::new(cur.x, cur.y, trav), Some(p.feed));
            b.push(MoveKind::Rapid, Point3::new(x0, y0, trav), None);
        }
        b.push(MoveKind::Plunge, Point3::new(x0, y0, z0), Some(p.plunge_feed));
        for (q, czz) in run {
            b.push(MoveKind::Feed, Point3::new(q.x, q.y, czz - r), Some(p.feed));
        }
    }
}

/// One Z-following raster pass: lines perpendicular to `(ux, uy)`
/// across the shallow facets' padded bounds, stepped by the cos-law
/// (or an explicit `stepover`), each walked by `march_curve`'s laws
/// and delivered by `emit_runs`' choreography. Serpentine within
/// the pass: every piece of an odd line runs back, so the next
/// line starts where this one's travel ended.
/// Returns whether it emitted anything.
fn raster_pass(
    b: &mut PathBuilder,
    p: &Finish3DParams,
    r: f64,
    sups: &[Sup],
    ux: f64,
    uy: f64,
    continuing: bool,
) -> Result<bool, GenError> {
    let (mut umin, mut umax, mut vmin, mut vmax) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    let mut mz_min = 1.0f64;
    for s in sups.iter().filter(|s| s.shallow) {
        let (vx0, vy0) = (-uy, ux);
        for (px, py) in [
            (s.xmin, s.ymin),
            (s.xmax, s.ymin),
            (s.xmax, s.ymax),
            (s.xmin, s.ymax),
        ] {
            umin = umin.min(px * ux + py * uy);
            umax = umax.max(px * ux + py * uy);
            vmin = vmin.min(px * vx0 + py * vy0);
            vmax = vmax.max(px * vx0 + py * vy0);
        }
        mz_min = mz_min.min(s.m.z);
    }
    if umin > umax {
        return Ok(false); // all-steep model: the rings alone carry it
    }
    let (vx, vy) = (-uy, ux);
    let step = p.stepover.unwrap_or(scallop_stepover(r, p.tolerance) * mz_min);
    const MAX_LINES: f64 = 8192.0;
    // Spelled through a binding so the NaN case reads what it is:
    // `step > 0.0` is false when step is not a number at all, and a
    // numberless spacing is refusal, never a silent skip.
    let step_positive = step > 0.0;
    if !step_positive || (vmax > vmin && step < (vmax - vmin) / MAX_LINES) {
        return Err(GenError::Invalid(format!(
            "raster needs {}+ lines at stepover {:.4} (cap {}): widen the stepover or the tolerance",
            ((vmax - vmin) / step.max(1e-12)) as i64,
            step,
            MAX_LINES as i64,
        )));
    }
    let delta0 = scallop_stepdown(r, p.tolerance).max(1e-6);
    let eps_chord = p.tolerance * 0.25;
    let at = |t: f64, v: f64| Point2::new(ux * t + vx * v, uy * t + vy * v);
    let sup = |q: Point2| support_at(q.x, q.y, r, p.bottom_z, sups);

    let n_lines = ((vmax - vmin) / step).floor() as i64 + 1;
    let mut runs: Vec<Vec<(Point2, f64)>> = Vec::new();
    for j in 0..n_lines.max(1) {
        let v = vmin + step * j.min(n_lines - 1) as f64;
        let pos = |t: f64| at(t, v);
        let mut pieces = march_curve(
            umin,
            umax,
            &pos,
            &|a, c| (c - a).abs(),
            &|_| 1.0,
            &sup,
            delta0,
            p.tolerance,
            eps_chord,
        );
        if j % 2 == 1 {
            for pc in pieces.iter_mut() {
                pc.reverse();
            }
        }
        runs.extend(pieces);
    }
    if runs.is_empty() {
        return Ok(false);
    }
    emit_runs(b, p, r, &runs, continuing);
    Ok(true)
}

/// The raster strategies (Parallel, ParallelMulti, Scallop): the
/// shallow terrain cut Z-following in each direction the pattern
/// asks for - once (Parallel), across the axes (ParallelMulti), or
/// across the diagonals (Scallop); then the waterline rings run the
/// steep flanks at the same scallop ladder - the auto-split, in one
/// toolpath, with the two strategies meeting at the shared 45°
/// jurisdiction line.
pub fn parallel(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    let r = p.tool_diameter * 0.5;
    let sups = build_sups(facets, r);
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    // The crossing pair's directions. Scallop's are ±45 degrees:
    // the axes are already taken by ParallelMulti, and on prismatic
    // terrain the diagonals are the only directions no single-axis
    // raster ever crosses head-on.
    let d = std::f64::consts::FRAC_1_SQRT_2;
    let crossed = matches!(p.pattern, FinishPattern::ParallelMulti | FinishPattern::Scallop);
    let (ax, ay, bx, by) = match p.pattern {
        FinishPattern::Scallop => (d, d, d, -d),
        FinishPattern::ParallelMulti => (1.0, 0.0, 0.0, 1.0),
        _ => (1.0, 0.0, 0.0, 0.0),
    };
    let mut emitted = raster_pass(&mut b, p, r, &sups, ax, ay, false)?;
    if crossed {
        emitted |= raster_pass(&mut b, p, r, &sups, bx, by, emitted)?;
    }
    // Then the rings, from wherever the raster left the tool: the
    // Some(clearance) marker makes the first ring's level-change
    // choreography climb up before anything else.
    rings_and_tail(b, p, r, facets, emitted)
}

/// The shared close of every raster-family strategy: the steep
/// flanks ride the waterline rings from wherever the shallow pass
/// left the tool, then the clearance exit, the emptiness refusal
/// and the finiteness gate. `parallel`, `radial` and `spiral` all
/// end here, so no strategy can drift from the ladder.
fn rings_and_tail(
    mut b: PathBuilder,
    p: &Finish3DParams,
    r: f64,
    facets: &[Triangle],
    emitted: bool,
) -> Result<Toolpath, GenError> {
    let zs = levels(p, r)?;
    let ds = dirs(r, p.tolerance);
    let mut last = Some(p.clearance_z);
    let mut emitted = emitted;
    emitted |= emit_rings(&mut b, p, r, facets, &zs, &ds, &mut last)?;
    if !emitted {
        return Err(GenError::Empty(
            "no surface reaches within one ball radius of the requested depths".into(),
        ));
    }
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// The shallow terrain's polar stage: its footprint center, the
/// circumscribed radius over the bbox corner (one stepover of
/// margin added by the caller, so the rim's ground is reached),
/// and the smallest shallow tilt - the cos-law's `m_z`. `None`
/// when every facet is steep: the rings alone carry the model,
/// exactly as the raster answers to that case.
fn shallow_footprint(sups: &[Sup]) -> Option<(Point2, f64, f64)> {
    let (mut x0, mut x1, mut y0, mut y1) =
        (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    let mut mz = 1.0f64;
    for s in sups.iter().filter(|s| s.shallow) {
        x0 = x0.min(s.xmin);
        x1 = x1.max(s.xmax);
        y0 = y0.min(s.ymin);
        y1 = y1.max(s.ymax);
        mz = mz.min(s.m.z);
    }
    if x0 > x1 {
        return None;
    }
    let c = Point2::new((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    Some((c, (x1 - c.x).hypot(y1 - c.y), mz))
}

/// The placement refusals, shared: a spacing without a number is
/// refusal, never a silent skip (the `step_positive` law of the
/// raster, spoken in polar words); and a family needing more
/// ground lines than the cap is the user's stepover asking for
/// hours of path - widen it, or the tolerance, and try again.
fn placement_guard(step: f64, lines: f64, family: &str, cap: f64) -> Result<(), GenError> {
    let step_positive = step > 0.0;
    if !step_positive || lines > cap {
        return Err(GenError::Invalid(format!(
            "{family} needs {} ground lines at stepover {:.4} (cap {cap}): widen the stepover or the tolerance",
            lines.max(0.0) as i64,
            step,
        )));
    }
    Ok(())
}

/// Radial finishing (M2.2d): `N = ceil(PI·R/s)` CHORDS through the
/// shallow terrain's footprint center, each rotated `PI/N` from the
/// last - so arc spacing at the rim meets the cos-law stepover, and
/// toward the center the chords overlap ever more (correct, merely
/// redundant: the cusp law is a bound, never a quota). Each chord
/// is one `march_curve` run: validity bisection digs its plunges
/// inside the material edge, the chord budget keeps the string of
/// samples under a quarter of `tol`, and the discontinuity break
/// cuts the chord into pieces at shelf mouths - no cut move spans
/// two surfaces, radial included. Alternating chords run inward,
/// the serpentine law with a different topology. Then the rings
/// take the flanks, as in every raster-family close.
pub fn radial(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    let r = p.tool_diameter * 0.5;
    let sups = build_sups(facets, r);
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    let mut emitted = false;
    if let Some((c, radius, mz)) = shallow_footprint(&sups) {
        let step = p.stepover.unwrap_or(scallop_stepover(r, p.tolerance) * mz);
        let rad = radius + step;
        const MAX_CHORDS: f64 = 8192.0;
        let n = (std::f64::consts::FRAC_PI_2 * 2.0 * rad / step).ceil().max(1.0);
        placement_guard(step, n, "radial", MAX_CHORDS)?;
        let delta0 = scallop_stepdown(r, p.tolerance).max(1e-6);
        let sup = |q: Point2| support_at(q.x, q.y, r, p.bottom_z, &sups);
        let mut runs: Vec<Vec<(Point2, f64)>> = Vec::new();
        for k in 0..n as i64 {
            let th = std::f64::consts::PI * k as f64 / n;
            let (dx, dy) = (th.cos(), th.sin());
            let pos = |t: f64| Point2::new(c.x + t * dx, c.y + t * dy);
            let mut pieces = march_curve(
                -rad,
                rad,
                &pos,
                &|a, z| (z - a).abs(),
                &|_| 1.0,
                &sup,
                delta0,
                p.tolerance,
                p.tolerance * 0.25,
            );
            if k % 2 == 1 {
                for pc in pieces.iter_mut() {
                    pc.reverse();
                }
            }
            runs.extend(pieces);
        }
        if !runs.is_empty() {
            emit_runs(&mut b, p, r, &runs, false);
            emitted = true;
        }
    }
    rings_and_tail(b, p, r, facets, emitted)
}

/// Spiral finishing (M2.2d): ONE Archimedean curve,
/// `r(θ) = R - s·θ/2π`, in from the silhouette to within a half
/// stepover of the pole - the retract-free continuous cut the
/// raster families pay for in hops and rapids. The march rides the
/// ANGLE parameter: `unit(θ) = 1/max(r, 4·delta0)` keeps the XY
/// chord per sample inside the stepdown budget at every radius
/// (the apex law - near the pole the angle opens wide while the
/// chord holds, and `dist` speaks true millimeters so the break
/// law and the refinement never read the parameter as ground),
/// and the curve ENDS a half-step from the center: the next turn
/// would only retrace the last. A spiral piece that breaks at a
/// shelf mouth re-enters at the hop, like every other ground line.
/// Then the rings take the flanks.
pub fn spiral(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    let r = p.tool_diameter * 0.5;
    let sups = build_sups(facets, r);
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    let mut emitted = false;
    if let Some((c, radius, mz)) = shallow_footprint(&sups) {
        let step = p.stepover.unwrap_or(scallop_stepover(r, p.tolerance) * mz);
        let rad = radius + step;
        const MAX_TURNS: f64 = 8192.0;
        let turns = (rad / step).ceil().max(1.0);
        placement_guard(step, turns, "spiral", MAX_TURNS)?;
        let delta0 = scallop_stepdown(r, p.tolerance).max(1e-6);
        let th_end = std::f64::consts::TAU * (rad - step * 0.5) / step;
        let r_of = |th: f64| rad - step * th / std::f64::consts::TAU;
        let sup = |q: Point2| support_at(q.x, q.y, r, p.bottom_z, &sups);
        let pos = |th: f64| {
            let rr = r_of(th).max(0.0);
            let (cs, sn) = th.sin_cos();
            Point2::new(c.x + rr * cs, c.y + rr * sn)
        };
        let dist = |a: f64, z: f64| {
            let (pa, pb) = (pos(a), pos(z));
            (pb.x - pa.x).hypot(pb.y - pa.y)
        };
        let unit = |th: f64| 1.0 / r_of(th).max(4.0 * delta0);
        let pieces = march_curve(
            0.0,
            th_end,
            &pos,
            &dist,
            &unit,
            &sup,
            delta0,
            p.tolerance,
            p.tolerance * 0.25,
        );
        if !pieces.is_empty() {
            emit_runs(&mut b, p, r, &pieces, false);
            emitted = true;
        }
    }
    rings_and_tail(b, p, r, facets, emitted)
}

/// The center-shadow of the shallow terrain: the union of every
/// shallow facet's XY triangle TRANSLATED by `r·m_xy` - the set of
/// XY points where a ball can rest tangent to that facet AT SOME
/// height (a plane's tangent centers form the plane shifted by the
/// contact radius along its lean; the XY shadow of that sheet is the
/// shifted triangle). Steep facets contribute nothing - the march
/// reads no tangent from them, and the rings own the flanks, exactly
/// the jurisdiction every other family keeps. Holes of the shadow
/// are genuine islands (or air the tool sees from above); a lifted
/// sample there answers through `support_at`'s own ball-clear law,
/// so a wavefront that cannot be cut simply breaks - never a lie.
fn shadow_region(sups: &[Sup], r: f64) -> Result<Region, GenError> {
    let mut loops: Vec<Loop> = Vec::new();
    for s in sups.iter().filter(|s| s.shallow) {
        let pts: Vec<Point2> = s
            .tri
            .iter()
            .map(|v| Point2::new(v.x + r * s.m.x, v.y + r * s.m.y))
            .collect();
        // Orientation decided by the signed sum, not by faith: a
        // flipped winding still enters the union CCW, and `Region`
        // then reads holes by nesting parity like every other union.
        let area: f64 = (0..3).map(|i| {
            pts[i].x * pts[(i + 1) % 3].y - pts[(i + 1) % 3].x * pts[i].y
        }).sum::<f64>()
            * 0.5;
        let pts = if area < 0.0 {
            let mut v = pts;
            v.reverse();
            v
        } else {
            pts
        };
        if let Ok(l) = Loop::new(pts) && l.area().abs() > 1e-12 {
            loops.push(l);
        }
    }
    if loops.is_empty() {
        return Ok(Region::empty());
    }
    Ok(Region::union_all(loops)?)
}

/// One closed loop, marched: arc length along the polygon becomes
/// the parameter (cumulative chords, summed in walk order so the
/// numbers are the traveler's), so `march_curve`'s laws speak
/// millimeters exactly as they do on a raster line. The march ends
/// where the ground ends: a fully-valid loop comes back CLOSED -
/// the clamp sample at `t = L` lands on the entry point again, and
/// `push_run_pt`'s law keeps it as the closing move, not a
/// duplicate; a loop that breaks at a shelf mouth stays a piece.
fn march_loop(
    pts: &[Point2],
    sup: &impl Fn(Point2) -> Option<f64>,
    delta0: f64,
    tol: f64,
    eps_chord: f64,
) -> Vec<Vec<(Point2, f64)>> {
    let n = pts.len();
    if n < 3 {
        return Vec::new();
    }
    let mut cum: Vec<f64> = Vec::with_capacity(n + 1);
    cum.push(0.0);
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let next = cum[i] + (b.x - a.x).hypot(b.y - a.y);
        if !next.is_finite() {
            return Vec::new();
        }
        cum.push(next);
    }
    let len = cum[n];
    if !len.is_finite() || len <= 0.0 {
        return Vec::new();
    }
    let pos = |t: f64| {
        let j = match cum.binary_search_by(|c| c.total_cmp(&t)) {
            Ok(j) => j.min(n - 1),
            Err(j) => j.saturating_sub(1).min(n - 1),
        };
        let (t0, t1) = (cum[j], cum[j + 1]);
        let u = if t1 > t0 {
            ((t - t0) / (t1 - t0)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Point2::new(
            pts[j].x + (pts[(j + 1) % n].x - pts[j].x) * u,
            pts[j].y + (pts[(j + 1) % n].y - pts[j].y) * u,
        )
    };
    march_curve(
        0.0,
        len,
        &pos,
        &|a, b| {
            let (pa, pb) = (pos(a), pos(b));
            (pb.x - pa.x).hypot(pb.y - pa.y)
        },
        &|_| 1.0,
        sup,
        delta0,
        tol,
        eps_chord,
    )
}

/// The shared core of the two around-the-model families (M2.2e):
/// `shadow` is the terrain's center-shadow; `max_rings` bounds the
/// shrink (Project: boundary only, `Some(0)`; Contour: every pass
/// until the region collapses, `None`). Loops march outermost-first
/// (then by entry point, so the order is a fact about the model,
/// not about clipper's memory), and the rings take the flanks at
/// the close, as in every family.
fn shadow_pass(
    b: &mut PathBuilder,
    p: &Finish3DParams,
    r: f64,
    sups: &[Sup],
    shadow: &Region,
    max_rings: Option<usize>,
    family: &str,
) -> Result<bool, GenError> {
    if shadow.is_empty() {
        return Ok(false); // all-steep model: the rings alone carry it
    }
    let mz = sups
        .iter()
        .filter(|s| s.shallow)
        .fold(1.0f64, |acc, s| acc.min(s.m.z));
    let step = p.stepover.unwrap_or(scallop_stepover(r, p.tolerance) * mz);
    if max_rings.is_none() {
        // The spacing law speaks only where spacing is used: the
        // boundary-only family (Project) marches its one rim whatever
        // `stepover` says - the Waterline's own "as if this enum had
        // never arrived" precedent - while the shrinking family must
        // answer for the hours a hairline stepover is asking for.
        let (mut x0, mut x1, mut y0, mut y1) =
            (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
        for l in shadow.loops() {
            for q in l.points() {
                x0 = x0.min(q.x);
                x1 = x1.max(q.x);
                y0 = y0.min(q.y);
                y1 = y1.max(q.y);
            }
        }
        let diag = (x1 - x0).hypot(y1 - y0);
        const MAX_RINGS: f64 = 8192.0;
        let est = (diag * 0.5 / step).ceil().max(1.0);
        placement_guard(step, est, family, MAX_RINGS)?;
    }
    const MAX_RINGS_CAP: usize = 8192;
    let delta0 = scallop_stepdown(r, p.tolerance).max(1e-6);
    let sup = |q: Point2| support_at(q.x, q.y, r, p.bottom_z, sups);
    let mut runs: Vec<Vec<(Point2, f64)>> = Vec::new();
    let mut reg = shadow.clone();
    let mut k = 0usize;
    while !reg.is_empty() {
        let mut loops: Vec<&Loop> = reg.loops().iter().collect();
        loops.sort_by(|a, b| {
            b.area().abs().total_cmp(&a.area().abs()).then_with(|| {
                let (pa, pb) = (a.points()[0], b.points()[0]);
                pa.x.total_cmp(&pb.x).then_with(|| pa.y.total_cmp(&pb.y))
            })
        });
        for l in loops {
            let stored: &[Point2] = l.points();
            if stored.len() < 3 {
                continue;
            }
            let poly: Vec<Point2> = if p.climb {
                stored.to_vec()
            } else {
                stored.iter().rev().copied().collect()
            };
            runs.extend(march_loop(&poly, &sup, delta0, p.tolerance, p.tolerance * 0.25));
        }
        k += 1;
        // The guard above already refused a family needing more than
        // the cap; this bound is the belt - geometry that refuses to
        // collapse (a numerically immortal sliver) must still leave
        // the loop with an honest long path, never forever.
        if max_rings.is_some_and(|m| k > m) || k > MAX_RINGS_CAP + 2 {
            break;
        }
        reg = reg.offset(-step)?;
    }
    if runs.is_empty() {
        return Ok(false);
    }
    emit_runs(b, p, r, &runs, false);
    Ok(true)
}

/// Contour finishing (M2.2e): concentric loops of the terrain's
/// center-shadow, shrunk one stepover per pass until the region
/// collapses at its medial axis. The XY spacing of successive loops
/// is exactly the offset, so on the ground they meet the same cusp
/// law a raster's line spacing meets - and where a loop crosses a
/// shelf mouth the march's break law cuts it into pieces that
/// re-enter at the hop, never a chord dragged through an edge.
/// Then the rings take the flanks, as in every family close.
pub fn contour(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    let r = p.tool_diameter * 0.5;
    let sups = build_sups(facets, r);
    let shadow = shadow_region(&sups, r)?;
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    let emitted = shadow_pass(&mut b, p, r, &sups, &shadow, None, "contour")?;
    rings_and_tail(b, p, r, facets, emitted)
}

/// Project finishing (M2.2e): the model's own silhouette, XY kept
/// and Z lifted onto the surface - the boundary's contact line,
/// where the tool grazes the part's edge wherever that edge climbs.
/// It is the contour family with `max_rings = 0`: same shadow, same
/// march laws, one loop set. The rings below it finish the flanks
/// the lifted rim only touches at a point.
pub fn project(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    let r = p.tool_diameter * 0.5;
    let sups = build_sups(facets, r);
    let shadow = shadow_region(&sups, r)?;
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    let emitted = shadow_pass(&mut b, p, r, &sups, &shadow, Some(0), "project")?;
    rings_and_tail(b, p, r, facets, emitted)
}

/// The ring ladder proper: one pass of the levels loop, appended to
/// `b`. `last` is the tip-Z of the path's last cut (or `None` when
/// the builder has never cut) - the choreography reads it exactly
/// like the waterline's own loop did, so a parallel path can hand
/// over after its raster with `last = Some(clearance_z)` and climb
/// in from there.
fn emit_rings(
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
