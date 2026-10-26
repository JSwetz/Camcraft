//! Multi-axis contour on drive curves (M5.1 stage J): the picked
//! chain, machined where it stands. Where the ladder kinds ask the
//! model for a pattern - rings, rasters, flow lines - this kind is
//! told exactly where to go: the caller hands in 3D polylines that
//! lie ON the part (the selection's edge chains are the house's
//! drive curves), and the pass walks them with the ball tangent to
//! the surface it rides, the axis posed by stage C's lean law at
//! every vertex and dodged by stage F's where the shank speaks.
//!
//! The station law is the contact geometry, winding-honest. At a
//! chain vertex the incident facets group into planes (smooth fans
//! merge; an edge stays two planes, a corner three); the ball that
//! touches every plane at once stands on their bisector at the
//! height `h = r / max_i |cos(c, n_i)|` that puts it exactly a
//! radius off each. Which side of the bisector line is resolved by
//! the mesh itself: a closed, consistently wound shell (edge-
//! pairing closure, signed-volume outwardness) points its normals
//! away from the material and settles it exactly; anything else -
//! an open sheet, a mixed winding - is judged candidate by candidate
//! on where the ball's skin stands: a side that sinks into a facet
//! it should only touch is a ball buried in steel and loses, the
//! side with the clearest air wins, and only a true tie falls to
//! the sky convention and the chain's own continuity. A chain that
//! stands where no side clears is refused by name, never posted.
//!
//! Provenance: 5-axis curve-tracing with normal-lean poses and
//! bisector edge contact is published multi-axis milling practice
//! (ISO 10791-10 test-part tradition, standard CAM manuals); the
//! winding-resolution laws (signed volume, edge pairing) are
//! standard computational geometry. Written from those sources and
//! our own design notes; no vendor material consulted.

use std::collections::HashMap;

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::finishing3d::closest_pt_tri;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::avoid::{escape_axis, shank_hits, Shank};
use super::stamp::ESCAPE_CAP;
use super::tilt::{command_axis, AxisControl};

/// The pass's numbers. There is no stepover and no tolerance here:
/// the curve IS the path - one pass over the picked chain, at the
/// depth the model itself gives the chain.
pub struct CurveParams {
    /// Ball diameter, mm.
    pub tool_diameter: f64,
    /// Air height over the entries and the final exit.
    pub clearance_z: f64,
    /// Hop plane between chains; `None` hops to clearance.
    pub retract_z: Option<f64>,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Reverses each chain's travel, like the ring's climb law.
    pub climb: bool,
}

/// The corner lattice step: two transforms of the same CAD vertex
/// differ by float dust, never by a facet's breadth.
const DUST: f64 = 1e-6;

fn dot(a: Point3, b: Point3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn sub(a: Point3, b: Point3) -> Point3 {
    Point3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

fn add(a: Point3, b: Point3) -> Point3 {
    Point3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

fn neg(a: Point3) -> Point3 {
    Point3::new(-a.x, -a.y, -a.z)
}

fn scale(a: Point3, s: f64) -> Point3 {
    Point3::new(a.x * s, a.y * s, a.z * s)
}

fn unit(v: Point3) -> Option<Point3> {
    let l = dot(v, v).sqrt();
    (l > 1e-12 && l.is_finite()).then(|| scale(v, 1.0 / l))
}

fn quant(p: Point3, cell: f64) -> (i64, i64, i64) {
    (
        (p.x / cell).floor() as i64,
        (p.y / cell).floor() as i64,
        (p.z / cell).floor() as i64,
    )
}

/// One plane of a vertex's fan: a unit normal (whatever pole the
/// source dared) and the facet area standing behind it.
struct Plane {
    n: Point3,
    area: f64,
}

/// The fan at a chain point: its planes and the facet indices that
/// vouch for them.
struct Fan {
    planes: Vec<Plane>,
    incident: Vec<usize>,
}

/// The mesh's bounding diagonal, by edge - cheap and finite-safe.
fn extent(facets: &[Triangle]) -> f64 {
    let mut d = 0.0f64;
    for f in facets {
        for (a, b) in [(f.a, f.b), (f.b, f.c), (f.c, f.a)] {
            let e = sub(a, b);
            let l = dot(e, e);
            if l.is_finite() {
                d = d.max(l);
            }
        }
    }
    d.sqrt()
}

/// Is the shell closed and consistently wound, and does its winding
/// read outward (`Some(flip)`, flip = negate every normal)? Edge
/// pairing is the closure (every directed edge once, its reverse
/// once); the signed volume is the outwardness. `None` when no
/// winding law can vouch for this mesh.
fn winding_law(facets: &[Triangle], cell: f64) -> Option<bool> {
    let mut deg: HashMap<(u64, u64), u32> = HashMap::new();
    let mut vol = 0.0f64;
    let key = |p: Point3| -> u64 {
        let (i, j, k) = quant(p, cell);
        (i as u64)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .rotate_left(21)
            ^ (j as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F).rotate_left(7)
            ^ (k as u64).wrapping_mul(0x1656_67B1_9E37_79F9)
    };
    for f in facets {
        let (a, b, c) = (key(f.a), key(f.b), key(f.c));
        if a == b || b == c || a == c {
            return None; // a collapsed corner: no winding claim to trust
        }
        for (u, v) in [(a, b), (b, c), (c, a)] {
            *deg.entry((u, v)).or_insert(0) += 1;
        }
        vol += dot(f.a, cross3(sub(f.b, f.a), sub(f.c, f.a))) / 6.0;
    }
    for f in facets {
        let (a, b, c) = (key(f.a), key(f.b), key(f.c));
        for (u, v) in [(a, b), (b, c), (c, a)] {
            if deg.get(&(u, v)) != Some(&1) || deg.get(&(v, u)) != Some(&1) {
                return None; // an open boundary or a doubled edge
            }
        }
    }
    let v3 = (extent(facets) * DUST).max(1e-9);
    if !vol.is_finite() || vol.abs() <= v3 * v3 * v3 {
        return None; // flat sheet or garbage: there is no side to call out
    }
    Some(vol < 0.0)
}

fn cross3(a: Point3, b: Point3) -> Point3 {
    Point3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

/// The corner map: quantized vertex -> facets meeting there.
fn corner_map(facets: &[Triangle], cell: f64) -> HashMap<(i64, i64, i64), Vec<usize>> {
    let mut m: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    for (i, f) in facets.iter().enumerate() {
        for v in [f.a, f.b, f.c] {
            m.entry(quant(v, cell)).or_default().push(i);
        }
    }
    m
}

/// The fan's view of a facet: unit plane normal (either pole) and
/// area; `None` for a zero-area sliver.
fn facet_plane(f: &Triangle) -> Option<(Point3, f64)> {
    let cr = cross3(sub(f.b, f.a), sub(f.c, f.a));
    let a2 = dot(cr, cr);
    if !a2.is_finite() || a2 <= 0.0 {
        return None;
    }
    Some((scale(cr, 1.0 / a2.sqrt()), 0.5 * a2.sqrt()))
}

/// The planes at a chain point: facets with a corner within reach,
/// merged into plane groups by angular agreement; a point lying on a
/// facet's body (no corner near) stands on that facet's single
/// plane. `None` when the point is off the model altogether.
fn fan_at(
    q: Point3,
    facets: &[Triangle],
    map: &HashMap<(i64, i64, i64), Vec<usize>>,
    cell: f64,
    eps2: f64,
) -> Option<Fan> {
    let (i0, j0, k0) = quant(q, cell);
    let mut hit: Vec<usize> = Vec::new();
    for di in -1..=1 {
        for dj in -1..=1 {
            for dk in -1..=1 {
                if let Some(list) = map.get(&(i0 + di, j0 + dj, k0 + dk)) {
                    for &fi in list {
                        if hit.contains(&fi) {
                            continue;
                        }
                        let f = &facets[fi];
                        if [f.a, f.b, f.c].iter().any(|v| dot(sub(*v, q), sub(*v, q)) <= eps2) {
                            hit.push(fi);
                        }
                    }
                }
            }
        }
    }
    if hit.is_empty() {
        // Not a mesh corner: the point may still ride a facet body
        // or an edge's middle - collect EVERY facet it touches, so
        // an edge-on chain point gets the edge's two-plane fan, not
        // whichever facet the scan happened to like.
        let mut incident: Vec<usize> = Vec::new();
        for (fi, f) in facets.iter().enumerate() {
            let d = sub(q, closest_pt_tri(q, &[f.a, f.b, f.c]));
            if dot(d, d) <= eps2 {
                incident.push(fi);
            }
        }
        if incident.is_empty() {
            return None; // off the model: nothing touches the point
        }
        return Some(group_planes(facets, &incident));
    }
    Some(group_planes(facets, &hit))
}

/// Merge facets into the plane groups their normals describe: like
/// poles of smooth neighbours add their areas, an edge keeps two
/// planes, a corner three; a fan busier than geometry allows folds
/// into the most nearly-parallel plane rather than enumerate a
/// dozen sign combinations the sink audit would judge anyway.
fn group_planes(facets: &[Triangle], incident: &[usize]) -> Fan {
    let mut planes: Vec<Plane> = Vec::new();
    for &fi in incident {
        let Some((nh, area)) = facet_plane(&facets[fi]) else { continue };
        let same = planes.iter().position(|p| dot(p.n, nh).abs() >= 0.98);
        match same {
            Some(i) => {
                let p = &mut planes[i];
                if dot(p.n, nh) < 0.0 {
                    p.n = neg(p.n);
                }
                p.area += area;
            }
            None if planes.len() < 5 => planes.push(Plane { n: nh, area }),
            None => {
                // planes is non-empty here (the <5 arm took the rest)
                let (mut j, mut bd) = (0usize, f64::NEG_INFINITY);
                for (k, p) in planes.iter().enumerate() {
                    let d = dot(p.n, nh).abs();
                    if d > bd {
                        bd = d;
                        j = k;
                    }
                }
                let p = &mut planes[j];
                if dot(p.n, nh) < 0.0 {
                    p.n = neg(p.n);
                }
                p.area += area;
            }
        }
    }
    Fan { planes, incident: incident.to_vec() }
}

/// The ball's resting direction at a fan, with its stand-off height
/// `h`: `CL = q + h * dir`. The winding-verdict mesh answers once;
/// every other mesh is asked of every quadrant, best-air first.
#[allow(clippy::too_many_arguments)] // the flat geometry a rest question needs
fn rest_at(
    q: Point3,
    fan: &Fan,
    facets: &[Triangle],
    boxes: &[[f64; 6]],
    r: f64,
    band: f64,
    flip: Option<bool>,
    prev: Option<Point3>,
) -> Result<(Point3, f64), GenError> {
    let height = |c: Point3| -> Option<f64> {
        let maxd = fan.planes.iter().fold(0.0f64, |m, p| dot(c, p.n).abs().max(m));
        (maxd >= 0.2).then(|| r / maxd)
    };
    let cands: Vec<Point3> = match flip {
        // A shell the winding law vouches for: the oriented fan sum
        // is the answer, and the enumeration never runs.
        Some(flip) => {
            let mut s = Point3::ZERO;
            for p in &fan.planes {
                s = add(s, if flip { neg(p.n) } else { p.n });
            }
            let Some(u) = unit(s) else {
                return Err(GenError::Invalid(
                    "the fan at a drive-curve vertex cancels to nothing - the mesh is a \
                     non-manifold fold at that point".into(),
                ));
            };
            vec![u]
        }
        None => {
            let k = fan.planes.len().min(5);
            let mut v = Vec::new();
            for mask in 0u64..(1u64 << k) {
                let mut s = Point3::ZERO;
                for (i, p) in fan.planes.iter().enumerate() {
                    s = if mask >> i & 1 == 1 { add(s, p.n) } else { add(s, neg(p.n)) };
                }
                if let Some(u) = unit(s)
                    && !v.iter().any(|c: &Point3| dot(*c, u) > 0.999)
                {
                    v.push(u);
                }
            }
            v
        }
    };
    #[derive(Clone, Copy)]
    struct Bid {
        sinks: u32,
        clear: f64,
        sky: f64,
        cont: f64,
        cdir: Point3,
    }
    // A bid wins outright on the first law that separates it from
    // the incumbent: fewer facets the ball's skin crosses; then the
    // clearest air (clamped, so distant geometry cannot bias a
    // tie); then skyward, the house convention for an open sheet;
    // then the chain's own continuity.
    fn wins(a: &Bid, b: &Bid) -> bool {
        let sep = |x: f64, y: f64| (x - y).abs() > 1e-9;
        if a.sinks != b.sinks {
            return a.sinks < b.sinks;
        }
        for (x, y) in [(a.clear, b.clear), (a.sky, b.sky), (a.cont, b.cont)] {
            if sep(x, y) {
                return x > y;
            }
        }
        false
    }
    let mut best: Option<Bid> = None;
    for c in cands {
        let Some(h) = height(c) else { continue };
        if h > 5.0 * r + band {
            continue; // a knife-edge fold; no sane tool rests on it
        }
        let cl = add(q, scale(c, h));
        let mut sinks = 0u32;
        let mut clear = f64::INFINITY;
        for (fi, f) in facets.iter().enumerate() {
            if fan.incident.contains(&fi) {
                continue;
            }
            // Cheap box screen first: a facet whose bounds stand
            // farther than the ball's reach cannot sink it or own
            // the minimum, and drive chains ride real meshes.
            let b = &boxes[fi];
            let (dx, dy, dz) = (
                (b[0] - cl.x).max(cl.x - b[3]).max(0.0),
                (b[1] - cl.y).max(cl.y - b[4]).max(0.0),
                (b[2] - cl.z).max(cl.z - b[5]).max(0.0),
            );
            if dx * dx + dy * dy + dz * dz > (r - band) * (r - band) {
                continue;
            }
            let d = sub(cl, closest_pt_tri(cl, &[f.a, f.b, f.c]));
            let dist = dot(d, d).sqrt();
            if dist < r - band {
                sinks += 1;
            }
            clear = clear.min(dist);
        }
        let bid = Bid {
            sinks,
            clear: clear.min(4.0 * r),
            sky: c.z,
            cont: prev.map(|p| dot(c, p)).unwrap_or(0.0),
            cdir: c,
        };
        best = Some(match &best {
            None => bid,
            Some(cur) if wins(&bid, cur) => bid,
            Some(cur) => *cur,
        });
    }
    best.map(|b| (b.cdir, height(b.cdir).unwrap_or(r)))
        .ok_or_else(|| {
            GenError::Invalid(
                "the drive curve stands where no ball can rest without sinking into the part - \
                 the chain touches material it cannot clear; move it to exposed ground"
                    .into(),
            )
        })
}

/// The multi-axis contour: the drive chains walked with the ball
/// tangent to the model, every cut move posed by stage C's lean law
/// (and, with a `shank`, stage F's dodge within the cone). Rapids
/// state no pose; a ring chain closes itself.
pub fn contour_curve(
    chains: &[Vec<Point3>],
    facets: &[Triangle],
    p: &CurveParams,
    c: &AxisControl,
    shank: Option<&Shank>,
) -> Result<Toolpath, GenError> {
    let r = p.tool_diameter * 0.5;
    if !(r.is_finite()
        && r > 0.0
        && p.clearance_z.is_finite()
        && p.retract_z.is_none_or(f64::is_finite)
        && p.feed.is_finite()
        && p.plunge_feed.is_finite())
    {
        return Err(GenError::Invalid(
            "multi-axis contour needs a finite ball radius, finite heights and finite feeds"
                .into(),
        ));
    }
    if facets.is_empty() {
        return Err(GenError::Empty(
            "multi-axis contour found no model to ride".into(),
        ));
    }
    let diag = extent(facets).max(1e-6);
    let cell = (diag * DUST).max(1e-9);
    let eps2 = (4.0 * cell) * (4.0 * cell);
    let mut live: Vec<(Vec<Point3>, bool)> = Vec::new();
    for (ci, ch) in chains.iter().enumerate() {
        if !ch.iter().all(|q| q.is_finite()) {
            return Err(GenError::Invalid(
                "a drive curve carries a non-finite point".into(),
            ));
        }
        // Collapse coincidental repeats; keep the ring's closing
        // duplicate if the ends meet.
        let mut pts: Vec<Point3> = Vec::with_capacity(ch.len());
        for q in ch {
            if pts.last().is_none_or(|l| dot(sub(*l, *q), sub(*l, *q)) > eps2) {
                pts.push(*q);
            }
        }
        let ring = {
            let e = sub(pts[0], pts[pts.len().saturating_sub(1)]);
            pts.len() >= 3 && dot(e, e) <= eps2
        };
        let distinct = if ring { pts.len() - 1 } else { pts.len() };
        if distinct < 2 {
            eprintln!(
                "multi-axis contour: drive curve {} stands on {} distinct point(s) - nothing \
                 to walk, skipped",
                ci + 1,
                distinct
            );
            continue;
        }
        if ring {
            pts.pop();
        }
        if !p.climb {
            pts.reverse();
        }
        if ring {
            let f = pts[0];
            pts.push(f);
        }
        live.push((pts, ring));
    }
    if live.is_empty() {
        return Err(GenError::Empty(
            "multi-axis contour needs at least one picked edge chain of two distinct points"
                .into(),
        ));
    }
    let map = corner_map(facets, cell);
    let boxes: Vec<[f64; 6]> = facets
        .iter()
        .map(|f| {
            [
                f.a.x.min(f.b.x.min(f.c.x)),
                f.a.y.min(f.b.y.min(f.c.y)),
                f.a.z.min(f.b.z.min(f.c.z)),
                f.a.x.max(f.b.x.max(f.c.x)),
                f.a.y.max(f.b.y.max(f.c.y)),
                f.a.z.max(f.b.z.max(f.c.z)),
            ]
        })
        .collect();
    let flip = winding_law(facets, cell);
    let band = 0.01 + 1e-6 * r;
    let hop_z = p.retract_z.unwrap_or(p.clearance_z);

    let mut b = PathBuilder::new(Point3::ZERO);
    let mut held = [0.0f64, 0.0, 1.0];
    for (ci, (pts, ring)) in live.iter().enumerate() {
        let k = if *ring { pts.len() - 1 } else { pts.len() };
        let mut prev: Option<Point3> = None;
        let mut stations: Vec<(Point3, [f64; 3])> = Vec::with_capacity(k);
        for i in 0..k {
            let q = pts[i];
            let fan = fan_at(q, facets, &map, cell, eps2).ok_or_else(|| {
                GenError::Invalid(format!(
                    "drive curve {} point {} stands off the model surface - re-pick it, or \
                     project it onto the part",
                    ci + 1,
                    i + 1
                ))
            })?;
            let (a, z) = (
                pts[if *ring { (i + k - 1) % k } else if i == 0 { 0 } else { i - 1 }],
                pts[if *ring { (i + 1) % k } else { (i + 1).min(k - 1) }],
            );
            let t = unit(sub(z, a)).or_else(|| unit(sub(*pts.last()?, pts[0]))).unwrap_or(Point3::new(1.0, 0.0, 0.0));
            let (cdir, h) = rest_at(q, &fan, facets, &boxes, r, band, flip, prev)?;
            prev = Some(cdir);
            let axis = command_axis(cdir, t, c)
                .map(|v| [v.x, v.y, v.z])
                .ok_or_else(|| {
                    GenError::Invalid(format!(
                        "the lean law cannot frame a pose at drive curve {} point {}",
                        ci + 1,
                        i + 1
                    ))
                })?;
            let cl = add(q, scale(cdir, h));
            let pose = match shank {
                Some(s) if shank_hits(cl, axis, r, s, facets, &boxes) => {
                    let cone = c.max_tilt_deg.unwrap_or(ESCAPE_CAP).min(ESCAPE_CAP);
                    escape_axis(cl, axis, cdir, held, cone, r, s, facets, &boxes).ok_or_else(|| {
                        GenError::Invalid(format!(
                            "the shank of the tool touches the part at drive curve {} point {} \
                             of the posed path, and no pose inside the max-tilt cone ({cone:.0} \
                             deg) clears it - widen the cone, shorten the tool, or turn shank \
                             avoidance off",
                            ci + 1,
                            i + 1
                        ))
                    })?
                }
                _ => axis,
            };
            held = pose;
            stations.push((cl, pose));
        }
        if let Some((cl0, pose0)) = stations.first() {
            b.set_axis(None);
            b.push(MoveKind::Rapid, Point3::new(cl0.x, cl0.y, hop_z), None);
            b.set_axis(Some(*pose0));
            b.push(MoveKind::Plunge, *cl0, Some(p.plunge_feed));
        }
        for (cl, pose) in stations.iter().skip(1) {
            b.set_axis(Some(*pose));
            b.push(MoveKind::Feed, *cl, Some(p.feed));
        }
        if *ring {
            // Home at the seam: the ring's closing move ends where
            // it started, under the first vertex's pose again.
            if let Some((cl0, pose0)) = stations.first() {
                b.set_axis(Some(*pose0));
                b.push(MoveKind::Feed, *cl0, Some(p.feed));
            }
        }
    }
    let last = b.at();
    b.set_axis(None);
    b.push(MoveKind::Rapid, Point3::new(last.x, last.y, hop_z), None);
    Ok(b.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::cube;

    fn boxy() -> Vec<Triangle> {
        cube(Point3::ZERO, Point3::new(40.0, 40.0, 15.0)).facets
    }

    fn cp() -> CurveParams {
        CurveParams {
            tool_diameter: 6.0,
            clearance_z: 25.0,
            retract_z: None,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        }
    }

    #[test]
    fn a_top_edge_chain_rests_on_the_bisector() {
        // The box's top-front edge (y = 0, z = 15): the ball that
        // touches BOTH planes stands a radius off each - centre
        // (y, z) = (-r, 15 + r) on the 45-degree bisector, its axis
        // leaning off the fold.
        let facets = boxy();
        let chain = vec![vec![
            Point3::new(5.0, 0.0, 15.0),
            Point3::new(20.0, 0.0, 15.0),
            Point3::new(35.0, 0.0, 15.0),
        ]];
        let path = contour_curve(&chain, &facets, &cp(), &AxisControl::default(), None).unwrap();
        let mids: Vec<_> = path.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
        assert_eq!(mids.len(), 2);
        let mid = mids[0].to;
        let r = 3.0;
        let want = Point3::new(20.0, -r, 15.0 + r);
        assert!(
            dot(sub(mid, want), sub(mid, want)).sqrt() < 1e-6,
            "centre {mid:?} not the bisector rest {want:?}"
        );
        let ax = mids[0].axis.expect("a cut move poses");
        assert!(ax[1] < -0.6 && ax[2] > 0.6, "pose {ax:?} should lean off the fold");
    }

    #[test]
    fn a_flat_top_chain_poses_near_vertical() {
        let facets = boxy();
        let chain = vec![vec![
            Point3::new(5.0, 20.0, 15.0),
            Point3::new(35.0, 20.0, 15.0),
        ]];
        let path = contour_curve(&chain, &facets, &cp(), &AxisControl::default(), None).unwrap();
        let last_feed = path
            .moves
            .iter()
            .rev()
            .find(|m| m.kind == MoveKind::Feed)
            .unwrap();
        assert!((last_feed.to.z - (15.0 + 3.0)).abs() < 1e-6);
        let ax = last_feed.axis.unwrap();
        assert!((ax[2] - 1.0).abs() < 1e-9, "a floor curve holds vertical, got {ax:?}");
    }

    #[test]
    fn a_floating_chain_is_named_not_projected() {
        let facets = boxy();
        let chain = vec![vec![
            Point3::new(5.0, 20.0, 40.0),
            Point3::new(35.0, 20.0, 40.0),
        ]];
        let e = contour_curve(&chain, &facets, &cp(), &AxisControl::default(), None).unwrap_err();
        assert!(format!("{e}").contains("off the model"), "{e}");
    }

    #[test]
    fn a_ring_closes_and_rapids_hold_no_pose() {
        let facets = boxy();
        let z = 7.5;
        let mut ring = Vec::new();
        for (x, y) in [
            (0.0, 0.0),
            (40.0, 0.0),
            (40.0, 40.0),
            (0.0, 40.0),
            (0.0, 0.0),
        ] {
            ring.push(Point3::new(x, y, z));
        }
        let path = contour_curve(&[ring], &facets, &cp(), &AxisControl::default(), None).unwrap();
        assert!(path.is_chained());
        assert!(path.moves.iter().all(|m| m.axis.is_none_or(|a| {
            let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
            (l - 1.0).abs() < 1e-9
        })));
        for m in &path.moves {
            if m.kind == MoveKind::Rapid {
                assert!(m.axis.is_none(), "a rapid stated a pose: {m:?}");
            }
        }
        // Four vertical corner edges: the ball rests OUTSIDE each,
        // a radius off both walls, its axis the horizontal bisector
        // - and the ring's last chord brings it home to station 0.
        let feeds: Vec<_> = path.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
        assert_eq!(feeds.len(), 4);
        let plunge = path
            .moves
            .iter()
            .find(|m| m.kind == MoveKind::Plunge)
            .expect("a first station");
        assert!(
            dot(sub(plunge.to, Point3::new(-3.0, -3.0, 7.5)), sub(plunge.to, Point3::new(-3.0, -3.0, 7.5))).sqrt()
                < 1e-6,
            "station 0 rests at {:?}, not outside the (-,-) corner",
            plunge.to
        );
        let ax = feeds[0].axis.unwrap();
        assert!(
            (ax[0] - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-9
                && (ax[1] + std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-9
                && ax[2].abs() < 1e-9,
            "corner-groove bisector should be horizontal-out at (+,-): {ax:?}"
        );
        assert!(
            dot(sub(feeds[3].to, plunge.to), sub(feeds[3].to, plunge.to)).sqrt() < 1e-9,
            "the ring did not come home"
        );
    }
}
