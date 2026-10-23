//! Horizontal-plane slicing of a triangle soup, and the XY silhouette.
//!
//! Provenance: plane/triangle intersection with the half-open edge rule and
//! cycle assembly by welding chord endpoints is the standard marching-slices
//! formulation from published computational geometry; the welding trick
//! itself (bucket by quantized position so duplicated STL corners count as
//! shared) is the same construction the viewport's feature-edge pass uses.
//! Region semantics (orientation, overlap fusion, crossing decomposition)
//! come from `geo2d::Region::from_slices` - this module only produces the
//! cycles and the diagnostics.
//!
//! Contract: facets are in whatever frame is to be sliced (transform to
//! world before calling; STEP-imported meshes arrive tessellate-only, which
//! is why boundaries come from slicing at all). A triangle straddling the
//! plane contributes exactly one chord; chords welded at their ends assemble
//! into closed cycles. Half-open in z (an edge counts when one endpoint is
//! strictly below and the other at-or-above): a slice at a mesh's z-max
//! returns its boundary, one at z-min returns nothing - callers sample
//! strictly inside.
//!
//! Prime Directive: a mesh whose slice leaves chord ends dangling is open -
//! reported, never half-used. Tangential slices (grazing an edge or vertex)
//! error cleanly too; `project` nudges once before propagating.

use std::collections::HashMap;

use thiserror::Error;

use crate::geo2d::{COORD_LIMIT, GeoError, Loop, Point2, Region};
use crate::geometry::{Point3, Triangle};

/// Weld grid: 1 unit = 1 um, the same quantization the geo2d bridge uses,
/// so welded vertices and Clipper inputs never disagree.
const WELD: f64 = 1e6;

/// Cap on `project`'s Z samples - a silhouette converges on far fewer.
pub const MAX_SAMPLES: usize = 64;

#[derive(Error, Debug, PartialEq)]
pub enum SliceError {
    #[error("invalid slice input: {0}")]
    Invalid(String),
    #[error("the mesh is open at z = {z}: {dangling} dangling chord ends")]
    Open { z: f64, dangling: usize },
    #[error("chords meet 3+ ways at z = {z}: non-manifold contact")]
    NonManifold { z: f64 },
    #[error("flat model: zero Z extent between {min_z} and {max_z}")]
    Flat { min_z: f64, max_z: f64 },
    #[error(transparent)]
    Geo(#[from] GeoError),
}

/// Cross `facets` with the plane z = `z` and return the material region.
/// An empty intersection is an empty region, not an error (z missed the
/// model); bad geometry is always an error, never a partial region.
pub fn slice(facets: &[Triangle], z: f64) -> Result<Region, SliceError> {
    if !z.is_finite() {
        return Err(SliceError::Invalid("non-finite slice height".into()));
    }
    for t in facets {
        for p in [t.a, t.b, t.c] {
            if !p.is_finite() {
                return Err(SliceError::Invalid("non-finite vertex".into()));
            }
            if beyond_horizon(p) {
                return Err(SliceError::Geo(GeoError::OutOfRange));
            }
        }
    }

    // 1. Chords. Edges are canonicalized low-z-first so the two triangles
    //    sharing an edge compute their crossing point with identical float
    //    arithmetic and it welds exactly; the zero-length guard drops
    //    vertex grazes (a plane touching a triangle at one point encloses
    //    nothing).
    let mut pts: Vec<Point2> = Vec::new();
    let mut keys: HashMap<(i64, i64), u32> = HashMap::new();
    let mut segs: Vec<[u32; 2]> = Vec::new();
    for t in facets {
        let mut hits: [Point2; 2] = [Point2::default(); 2];
        let mut n = 0usize;
        for (p, q) in [(t.a, t.b), (t.b, t.c), (t.c, t.a)] {
            let (lo, hi) = if p.z <= q.z { (p, q) } else { (q, p) };
            if lo.z < z && hi.z >= z {
                if n == 2 {
                    // Three crossings of one triangle boundary with one
                    // plane is impossible for finite input; refuse loudly
                    // rather than index out of bounds.
                    return Err(SliceError::Invalid("triangle meets plane 3x".into()));
                }
                // lo.z < z <= hi.z keeps the denominator positive.
                let s = (z - lo.z) / (hi.z - lo.z);
                hits[n] = Point2::new(
                    lo.x + s * (hi.x - lo.x),
                    lo.y + s * (hi.y - lo.y),
                );
                n += 1;
            }
        }
        match n {
            2 => {
                let i = weld(&mut pts, &mut keys, hits[0]);
                let j = weld(&mut pts, &mut keys, hits[1]);
                if i != j {
                    segs.push([i, j]);
                }
            }
            0 => {}
            _ => return Err(SliceError::Invalid(format!("odd chord count at z = {z}"))),
        }
    }

    // 2. Every chord end must be shared by exactly two chords.
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); pts.len()];
    for (k, [i, j]) in segs.iter().enumerate() {
        adj[*i as usize].push(k as u32);
        adj[*j as usize].push(k as u32);
    }
    let mut dangling = 0usize;
    let mut meeting = false;
    for a in &adj {
        match a.len() {
            1 => dangling += 1,
            2 => {}
            0 => {} // grazed vertex, chord skipped: carries no boundary
            _ => meeting = true,
        }
    }
    if dangling > 0 {
        return Err(SliceError::Open { z, dangling });
    }
    if meeting {
        return Err(SliceError::NonManifold { z });
    }

    // 3. Walk the 2-regular graph into closed cycles.
    let mut used = vec![false; segs.len()];
    let mut loops = Vec::new();
    for s in 0..segs.len() {
        if used[s] {
            continue;
        }
        used[s] = true;
        let [tail, head_start] = segs[s];
        let mut head = head_start;
        let mut chain: Vec<Point2> = vec![pts[tail as usize], pts[head_start as usize]];
        for _ in 0..segs.len() {
            if head == tail {
                break;
            }
            let next = adj[head as usize]
                .iter()
                .copied()
                .find(|k| !used[*k as usize]);
            let ns = match next {
                Some(k) => k as usize,
                None => return Err(SliceError::Invalid("cycle walk stalled".into())),
            };
            used[ns] = true;
            let [a, b] = segs[ns];
            head = if a == head { b } else { a };
            chain.push(pts[head as usize]);
        }
        if head != tail {
            return Err(SliceError::Invalid("cycle walk did not close".into()));
        }
        loops.push(Loop::new(chain)?);
    }

    // 4. Normalize: self-touching section outlines decompose into lobes,
    //    nesting decides hole orientation, the non-zero fuse unites shells.
    Ok(Region::from_slices(loops)?)
}

/// The XY silhouette: material at *any* height, as a union of slices
/// sampled evenly strictly inside the Z extent. This is the footprint a
/// contour path or a collision check asks of a solid. `samples` is clamped
/// to [`MAX_SAMPLES`]; a slice that fails on a tangential coincidence is
/// retried once at a nudge - if it fails again the error is real.
pub fn project(facets: &[Triangle], samples: usize) -> Result<Region, SliceError> {
    if facets.is_empty() {
        return Ok(Region::empty());
    }
    let (mut zmin, mut zmax) = (f64::INFINITY, f64::NEG_INFINITY);
    for t in facets {
        for p in [t.a, t.b, t.c] {
            if !p.is_finite() {
                return Err(SliceError::Invalid("non-finite vertex".into()));
            }
            zmin = zmin.min(p.z);
            zmax = zmax.max(p.z);
        }
    }
    let span = zmax - zmin;
    if !span.is_finite() {
        return Err(SliceError::Invalid("non-finite Z extent".into()));
    }
    if span <= 0.0 {
        return Err(SliceError::Flat { min_z: zmin, max_z: zmax });
    }
    let n = samples.clamp(1, MAX_SAMPLES);
    let nudge = span.max(1.0) * 1e-3;
    let mut acc: Option<Region> = None;
    for k in 0..n {
        let z = zmin + (k as f64 + 0.5) * (span / n as f64);
        let region = match slice(facets, z) {
            Ok(r) => r,
            Err(first_err) => match slice(facets, z + nudge) {
                Ok(r) => r,
                Err(_) => return Err(first_err),
            },
        };
        if region.is_empty() {
            continue;
        }
        acc = Some(match acc {
            None => region,
            Some(a) => a.union(&region)?,
        });
    }
    Ok(acc.unwrap_or_default())
}

fn weld(pts: &mut Vec<Point2>, keys: &mut HashMap<(i64, i64), u32>, p: Point2) -> u32 {
    let key = ((p.x * WELD).round() as i64, (p.y * WELD).round() as i64);
    if let Some(id) = keys.get(&key) {
        return *id;
    }
    let id = pts.len() as u32;
    pts.push(p);
    keys.insert(key, id);
    id
}

fn beyond_horizon(p: Point3) -> bool {
    p.x.abs() >= COORD_LIMIT || p.y.abs() >= COORD_LIMIT || p.z.abs() >= COORD_LIMIT
}

/// The model's cross-section strictly above `z`, nudge-retried when a
/// plane grazes a tessellated shelf (reading an errored slice as
/// "no material" would be the silent wrong answer; the retry lands
/// above the shelf instead, failing safe). Three probes, one
/// millionth-of-a-millimetre apart, first survivor wins. Home of the
/// islands law since M4.1 R4 - `machining3d::material_above` wraps
/// this, the verifier's island cache calls it directly: one law,
/// three readers, at the layer that owns the mesh (M2.1a).
pub fn material_above(facets: &[Triangle], z: f64) -> Result<Region, SliceError> {
    const EPS: f64 = 1e-6;
    let mut err = None;
    for k in 0..3 {
        let probe = z + f64::from(k + 1) * EPS;
        match slice(facets, probe) {
            Ok(r) => return Ok(r),
            Err(e) => err = Some(e),
        }
    }
    Err(err.unwrap_or(SliceError::Invalid("cannot section the model".into())))
}
