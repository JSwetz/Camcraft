//! Drilling: hole recognition from the model's facets, and the cycle
//! generator that turns a hole list into a toolpath carrying both the
//! fully expanded move chain and a [`Cycle`] record per hole.
//!
//! Provenance: cycle semantics (drill, chip-break peck, deep-hole with
//! full retract and dwell, rigid tap, bore and back-bore, spot dimple)
//! follow the public RS-274 / ISO 6983 canned-cycle vocabulary; what a
//! hole IS - a closed cylindrical wall in the facet mesh - is general
//! machining knowledge applied to our own mesh model. No vendor data,
//! no post processors, no HSMWorks material consulted.
//!
//! Law of two truths: `moves` is the physical truth (verify, display,
//! simulation, the .cl dump all read it); `cycles` is the post's truth
//! (one canned block per hole, not a re-derivation from the chain).
//! They are produced together, from the same lattice-snapped numbers,
//! so they agree - and [`tests::agreement_moves_and_cycles_describe_the_same_holes`]
//! pins that: per cycle, starting at `first_move`, the moves that stay
//! on the hole's XY are its group, its first is a cut (positioning
//! rapids precede the group), and the group's deepest point is
//! `bottom_z`.
//!
//! Holes are Z-axis only: the whole codebase plunges along Z (2.5-D),
//! and recognition finds walls, not axes.

use std::collections::HashMap;

use crate::cl::{lattice, Cycle, CycleKind, MoveKind, PathBuilder, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::tools::{Tool, ToolKind};

/// One recognized (or hand-entered) hole, in world coordinates. The
/// cutter's own shape makes the diameter when drilling - `radius` is
/// the model's hole, kept for display, the operation region's rim
/// loops, and the gouge law's tangency test.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hole {
    pub xy: Point2,
    pub radius: f64,
    /// The material's entry surface at the hole (highest wall vertex).
    pub entry_z: f64,
    /// The hole's floor (lowest wall vertex); a through hole's floor is
    /// the mesh's bottom.
    pub floor_z: f64,
    pub through: bool,
}

impl Hole {
    fn all_finite(&self) -> bool {
        self.xy.x.is_finite()
            && self.xy.y.is_finite()
            && self.radius.is_finite()
            && self.entry_z.is_finite()
            && self.floor_z.is_finite()
    }

    /// Structural sanity for command construction (the params mirror of
    /// the generator's gate): finite numbers, a positive radius, and a
    /// floor below the entry - a hole that cuts upward is not a hole.
    #[must_use]
    pub fn is_usable(&self) -> bool {
        self.all_finite() && self.radius > 0.0 && self.floor_z < self.entry_z
    }
}

/// Which cycle runs. `Auto` lets the tool's kind and the hole's depth
/// decide (see [`auto_kind`]); the others are the user's explicit word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CycleChoice {
    #[default]
    Auto,
    Spot,
    Drill,
    Peck,
    Tap,
    Bore,
    BackBore,
}

/// Everything the generator needs, with heights already resolved to
/// world Z by the operation layer.
pub struct CycleParams<'a> {
    /// The plane the tool starts and ends every hole above.
    pub clearance_z: f64,
    /// The cycle's R plane: the feed entry height (Top of Part).
    pub r_z: f64,
    /// Retract plane between holes; `None` retracts to clearance.
    pub hop_z: Option<f64>,
    /// Depth override for every hole; `None` uses each hole's floor
    /// (plus exit allowance past a through hole's far wall).
    pub bottom_z: Option<f64>,
    pub cycle: CycleChoice,
    /// Depth per peck; `<= 0` lets the Auto law pick (see [`drill`]).
    pub peck: f64,
    /// Seconds at bottom; recorded on the cycles that honor a dwell.
    pub dwell: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    pub tool: &'a Tool,
    pub holes: &'a [Hole],
}

/// A spot drill's dimple depth below the entry plane, unless the floor
/// is in the way. General spotting practice.
pub const SPOT_DEPTH: f64 = 2.0;
/// How far past a through hole's far wall the drill must exit so the
/// full diameter is clean through. Standard through-drilling allowance.
pub const THROUGH_CLEAR: f64 = 1.0;
/// G73 chip-break hop: up about a millimeter, snap the chip, continue.
/// Published general practice for chip-breaking peck cycles.
pub const CHIP_BREAK: f64 = 1.0;

/// The Auto cycle law: the tool's kind speaks first (a tap taps, a
/// boring bar bores); otherwise it is depth against the diameter -
/// one length or less is a plain drill, up to four diameters a
/// chip-break peck, deeper gets full-retract pecks and the dwell word
/// (G83's shape). Spot is never automatic: it is what the user says it
/// is.
#[must_use]
pub fn auto_kind(tool_kind: ToolKind, depth: f64, diameter: f64) -> CycleKind {
    match tool_kind {
        ToolKind::Tap => CycleKind::Tap,
        ToolKind::Bore => CycleKind::Bore,
        _ => {
            if depth <= diameter {
                CycleKind::Drill
            } else if depth <= 4.0 * diameter {
                CycleKind::Peck
            } else {
                CycleKind::Deep
            }
        }
    }
}

// ---------------------------------------------------------------- recognition

/// Facets with a vertical-normal component at or above this are caps,
/// floors, or too steep to be a drilled wall. A 45° chamfer sits at
/// 0.707 - outside by design; a shallower chamfer ring is caught and
/// merged below.
const WALL_NZ: f64 = 0.7;
/// Farthest a fitted vertex may sit from the shared circle before the
/// component is "not a cylinder". 20 um: over the chord noise of our
/// own tessellations, under any real taper.
const FIT_TOL: f64 = 0.02;
/// A hole is a hole, not a pinprick or a silo.
const R_MIN: f64 = 0.05;
const R_MAX: f64 = 500.0;
/// Two wall components this close in XY are one hole stepped in Z
/// (counterbore, shallow chamfer): fold their z-spans, keep the narrow
/// radius - the actual bore.
const MERGE_DXY: f64 = 0.05;
/// A closed rim spans this much of the compass; a flat wall's vertices
/// straddle two opposite rays (span ~180), a slot's arc end 180. Both
/// fail; a full circle passes.
const SPAN_MIN: f64 = std::f64::consts::TAU * 0.75;
/// Coincident-position union-find keys on exact bits: `world_facets`
/// transforms shared vertices identically, so seams weld exactly.
fn vkey(p: &Point3) -> (u64, u64, u64) {
    (p.x.to_bits(), p.y.to_bits(), p.z.to_bits())
}

fn hypot(x: f64, y: f64) -> f64 {
    (x * x + y * y).sqrt()
}

/// Find the drilled holes in a triangle mesh: closed, round, vertical
/// walls whose facets face their own axis (so raised bosses fall out),
/// merged across Z steps, snapped onto the lattice so re-detection of
/// an unmoved model is bit-stable. Degenerate or empty input yields an
/// empty list - never a panic, never a hang (Prime Directive).
///
/// Runs on raw facets, not the Feature layer: recognition must not
/// inherit (or drift from) feature grouping.
#[must_use]
pub fn recognize(tris: &[Triangle]) -> Vec<Hole> {
    // Global z floor: what makes a hole "through".
    let mut mesh_min_z = f64::INFINITY;
    for t in tris {
        if t.a.is_finite() && t.b.is_finite() && t.c.is_finite() {
            for v in [t.a, t.b, t.c] {
                if v.z < mesh_min_z {
                    mesh_min_z = v.z;
                }
            }
        }
    }
    if !mesh_min_z.is_finite() {
        return Vec::new();
    }

    // Wall facets -> components, welded on exact shared vertex bits.
    let mut ids: HashMap<(u64, u64, u64), usize> = HashMap::new();
    let mut parent: Vec<usize> = Vec::new();
    let mut verts: Vec<Point3> = Vec::new();
    // Per wall facet: its vertex ids, centroid, normal.
    let mut facets: Vec<([usize; 3], Point3, Point3)> = Vec::new();

    for t in tris {
        let (a, b, c) = (t.a, t.b, t.c);
        if !(a.is_finite() && b.is_finite() && c.is_finite()) {
            continue;
        }
        let n = t.normal();
        if !n.is_finite() || n.z.abs() >= WALL_NZ {
            continue;
        }
        let mut v3 = [0usize; 3];
        for (slot, p) in v3.iter_mut().zip([a, b, c]) {
            *slot = match ids.get(&vkey(&p)) {
                Some(&i) => i,
                None => {
                    let i = verts.len();
                    ids.insert(vkey(&p), i);
                    verts.push(p);
                    parent.push(i);
                    i
                }
            };
        }
        let cen = Point3::new((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0, (a.z + b.z + c.z) / 3.0);
        facets.push(([v3[0], v3[1], v3[2]], cen, n));
    }

    // Path-compressed union over each facet's three vertices.
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for (v, _, _) in &facets {
        let (ra, rb) = (find(&mut parent, v[0]), find(&mut parent, v[1]));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
        let (ra, rc) = (find(&mut parent, v[0]), find(&mut parent, v[2]));
        if ra != rc {
            parent[ra.max(rc)] = ra.min(rc);
        }
    }
    let mut by_root: HashMap<usize, Vec<usize>> = HashMap::new();
    for (fi, (v, _, _)) in facets.iter().enumerate() {
        let r = find(&mut parent, v[0]);
        by_root.entry(r).or_default().push(fi);
    }

    let mut holes: Vec<Hole> = Vec::new();
    for fis in by_root.values() {
        // The component's distinct vertices.
        let mut vids: Vec<usize> = Vec::new();
        for &fi in fis {
            for &v in &facets[fi].0 {
                if !vids.contains(&v) {
                    vids.push(v);
                }
            }
        }
        if vids.len() < 6 {
            continue; // a facet pair or a sliver is not a hole wall
        }
        let (mut zmin, mut zmax) = (f64::INFINITY, f64::NEG_INFINITY);
        for &v in &vids {
            zmin = zmin.min(verts[v].z);
            zmax = zmax.max(verts[v].z);
        }
        if zmax - zmin <= 1e-6 {
            continue; // a ring of facets laid flat, not a wall
        }
        // Kasa circle fit on the XY projections; singular (a flat wall's
        // collinear vertices) drops the component here, for free.
        let Some((cx, cy, rad)) = fit_circle(&vids, &verts) else {
            continue;
        };
        if !(R_MIN < rad && rad < R_MAX) {
            continue;
        }
        // Fit quality: every vertex within FIT_TOL of the circle.
        let worst = vids
            .iter()
            .map(|&v| (hypot(verts[v].x - cx, verts[v].y - cy) - rad).abs())
            .fold(0.0f64, f64::max);
        if worst > FIT_TOL {
            continue; // a slot's racetrack fits no circle
        }
        // Closed-rim test: the vertices must wrap more than three
        // quarters of the compass.
        let mut angs: Vec<f64> = vids
            .iter()
            .map(|&v| (verts[v].y - cy).atan2(verts[v].x - cx))
            .collect();
        angs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mut max_gap = angs[0] + std::f64::consts::TAU - angs[angs.len() - 1];
        for w in angs.windows(2) {
            max_gap = max_gap.max(w[1] - w[0]);
        }
        if std::f64::consts::TAU - max_gap <= SPAN_MIN {
            continue;
        }
        // Hole vs boss: on a hole's wall an outward-wound mesh's normals
        // point AT the axis. A raised cylinder's wall points away. Vote
        // by facet; a coin flip refuses.
        let (mut toward, mut away) = (0usize, 0usize);
        for &fi in fis {
            let (_, cen, n) = facets[fi];
            let d = n.x * (cx - cen.x) + n.y * (cy - cen.y);
            if d > 0.0 {
                toward += 1;
            } else if d < 0.0 {
                away += 1;
            }
        }
        if toward <= away {
            continue;
        }
        holes.push(Hole {
            xy: Point2::new(lattice(cx), lattice(cy)),
            radius: lattice(rad),
            entry_z: lattice(zmax),
            floor_z: lattice(zmin),
            through: (zmin - mesh_min_z).abs() <= 1e-6,
        });
    }

    // Merge Z-stepped same-axis components (counterbores, shallow
    // chamfers): one hole, narrowest radius, whole z span.
    let mut merged: Vec<Hole> = Vec::new();
    let mut used = vec![false; holes.len()];
    for i in 0..holes.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let mut h = holes[i];
        loop {
            let mut grew = false;
            for j in 0..holes.len() {
                if !used[j] && hypot(h.xy.x - holes[j].xy.x, h.xy.y - holes[j].xy.y) <= MERGE_DXY {
                    used[j] = true;
                    h = Hole {
                        xy: Point2::new(
                            lattice(h.xy.x.min(holes[j].xy.x)),
                            lattice(h.xy.y.min(holes[j].xy.y)),
                        ),
                        radius: h.radius.min(holes[j].radius),
                        entry_z: h.entry_z.max(holes[j].entry_z),
                        floor_z: h.floor_z.min(holes[j].floor_z),
                        through: false,
                    };
                    h.through = (h.floor_z - mesh_min_z).abs() <= 1e-6;
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        merged.push(h);
    }
    merged.sort_by(|a, b| {
        cmp64(a.xy.y, b.xy.y)
            .then(cmp64(a.xy.x, b.xy.x))
            .then(cmp64(a.radius, b.radius))
    });
    merged
}

fn cmp64(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}

/// Least-squares (Kasa) circle through the projected vertices; `None`
/// when the normal equations are singular - which is exactly the
/// flat-wall case, so the fit itself does the first cut. 3x3 solve
/// with partial pivoting, no panics, no unwraps.
///
/// Conditioning law: Kasa's normal equations degrade quadratically in
/// the distance from the origin (an r = 2 circle at x = 32000 fit
/// *wrong-but-plausibly* before this re-centering). So everything is
/// accumulated against the point cloud's centroid and the center is
/// translated back at the end.
fn fit_circle(vids: &[usize], verts: &[Point3]) -> Option<(f64, f64, f64)> {
    let n = vids.len() as f64;
    let (mut mx, mut my) = (0.0, 0.0);
    for &v in vids {
        mx += verts[v].x;
        my += verts[v].y;
    }
    mx /= n;
    my /= n;
    let (mut sxx, mut sxy, mut syy, mut sx, mut sy) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut sxxx, mut sxyy, mut sxxy, mut syyy) = (0.0, 0.0, 0.0, 0.0);
    for &v in vids {
        let (x, y) = (verts[v].x - mx, verts[v].y - my);
        let (x2, y2) = (x * x, y * y);
        sxx += x2;
        sxy += x * y;
        syy += y2;
        sx += x;
        sy += y;
        sxxx += x * x2;
        sxyy += x * y2;
        sxxy += x2 * y;
        syyy += y * y2;
    }
    // [sxx sxy sx | -sum(x^3+xy^2)]
    // [sxy syy sy | -sum(x^2y+y^3)]
    // [sx  sy  n  | -sum(x^2+y^2) ]   ->  x^2+y^2 + Dx + Ey + F = 0
    // (x, y local; sx and sy are ~0 by construction of the centroid.)
    let mut a = [
        [sxx, sxy, sx, -(sxxx + sxyy)],
        [sxy, syy, sy, -(sxxy + syyy)],
        [sx, sy, n, -(sxx + syy)],
    ];
    for col in 0..3 {
        let mut piv = col;
        for r in col + 1..3 {
            if a[r][col].abs() > a[piv][col].abs() {
                piv = r;
            }
        }
        // Relative to the cloud's own scale: a circle of radius rr has
        // diagonal sums ~ n*rr^2; anything under a nanometer-equivalent
        // of that is a flat (collinear) wall.
        let scale = (sxx + syy) / n.max(1.0);
        if a[piv][col].abs() <= 1e-12 * scale * n.max(1.0) + f64::EPSILON {
            return None;
        }
        a.swap(col, piv);
        for r in 0..3 {
            if r == col {
                continue;
            }
            let f = a[r][col] / a[col][col];
            let pivot_row = a[col]; // Copy; lets the elimination zip rows.
            for (cell, &pv) in a[r][col..4].iter_mut().zip(&pivot_row[col..4]) {
                *cell -= f * pv;
            }
        }
    }
    // Gauss-Jordan left a diagonal matrix: the solution sits in the
    // right-hand column divided by each row's own pivot.
    let sol = |i: usize| -> f64 {
        if a[i][i].abs() > f64::EPSILON {
            a[i][3] / a[i][i]
        } else {
            f64::NAN
        }
    };
    let (d, e, f0) = (sol(0), sol(1), sol(2));
    if !(d.is_finite() && e.is_finite() && f0.is_finite()) {
        return None;
    }
    let (cx, cy) = (-d * 0.5 + mx, -e * 0.5 + my);
    let r2 = (d * d + e * e) * 0.25 - f0;
    if !(r2 > 0.0 && r2.is_finite()) {
        return None;
    }
    Some((cx, cy, r2.sqrt()))
}

// ---------------------------------------------------------------- generator

/// Machine a hole list: a contiguous cut group per hole (positioning
/// rapids sit between the groups) plus one [`Cycle`] record each. The
/// chain is chained and finite by construction; nonsense in,
/// `GenError` out.
pub fn drill(p: &CycleParams) -> Result<Toolpath, GenError> {
    let d = p.tool.diameter;
    if !(d.is_finite() && d > 0.0) || !p.r_z.is_finite() || !p.clearance_z.is_finite() {
        return Err(GenError::Invalid("drill tool or heights not usable".into()));
    }
    if !p.holes.iter().all(|h| h.all_finite() && h.radius > 0.0 && h.floor_z < h.entry_z) {
        return Err(GenError::Invalid("malformed hole list".into()));
    }
    if !(p.feed.is_finite() && p.plunge_feed.is_finite() && p.dwell.is_finite() && p.dwell >= 0.0) {
        return Err(GenError::Invalid("cycle parameters not usable".into()));
    }
    if p.holes.is_empty() {
        return Err(GenError::Empty("no holes to drill".into()));
    }
    let tap_feed = p.tool.rpm * p.tool.pitch;
    let taps = matches!(p.cycle, CycleChoice::Tap)
        || (matches!(p.cycle, CycleChoice::Auto) && p.tool.kind == ToolKind::Tap);
    if taps && !(tap_feed.is_finite() && tap_feed > 0.0) {
        return Err(GenError::Invalid("a tap cycle needs a positive pitch x rpm feed".into()));
    }

    // Snap once; every move and every cycle reads these, so the two
    // truths agree bit for bit, not by luck.
    let hs: Vec<Hole> = p
        .holes
        .iter()
        .map(|h| Hole {
            xy: Point2::new(lattice(h.xy.x), lattice(h.xy.y)),
            radius: lattice(h.radius),
            entry_z: lattice(h.entry_z),
            floor_z: lattice(h.floor_z),
            through: h.through,
        })
        .collect();

    let start_z = p.clearance_z.max(p.r_z);
    let hop = p.hop_z.unwrap_or(p.clearance_z).max(p.r_z);
    let mut b = PathBuilder::new(Point3::new(hs[0].xy.x, hs[0].xy.y, start_z));
    for (i, h) in hs.iter().enumerate() {
        if i > 0 {
            // The previous hole's exit left the tool at (prev_xy, hop):
            // move over, then down onto this hole's R plane.
            b.push(MoveKind::Rapid, Point3::new(h.xy.x, h.xy.y, hop), None);
        }
        b.push(MoveKind::Rapid, Point3::new(h.xy.x, h.xy.y, p.r_z), None);
        let floor_bottom = h.floor_z - if h.through { THROUGH_CLEAR } else { 0.0 };
        let bottom = match p.bottom_z {
            Some(z) => z,
            None if matches!(p.cycle, CycleChoice::Spot) => (p.r_z - SPOT_DEPTH).max(h.floor_z),
            None => floor_bottom,
        };
        if bottom >= p.r_z {
            return Err(GenError::Invalid(format!(
                "hole at ({}, {}) bottoms at {bottom}, not below the R plane {}",
                h.xy.x, h.xy.y, p.r_z
            )));
        }
        let kind = match p.cycle {
            CycleChoice::Auto => auto_kind(p.tool.kind, p.r_z - bottom, d),
            _ => resolve_choice(p),
        };
        let fm = b.moves_len();
        let mut peck = 0.0;
        match kind {
            CycleKind::Spot | CycleKind::Drill => {
                b.push(MoveKind::Plunge, Point3::new(h.xy.x, h.xy.y, bottom), Some(p.plunge_feed));
                b.push(MoveKind::Rapid, Point3::new(h.xy.x, h.xy.y, p.r_z), None);
            }
            CycleKind::Peck | CycleKind::Deep => {
                let auto = if kind == CycleKind::Peck { 3.0 * d } else { d };
                let step = if p.peck > 0.0 { p.peck } else { auto }.max(0.25 * d);
                if !(step.is_finite() && step > 0.0) || (p.r_z - bottom) / step > 4096.0 {
                    return Err(GenError::Invalid("peck plan unusable".into()));
                }
                peck = step;
                // Pecks march DOWN by step: z decreases. The retract
                // (a G73 hop up, or a G83 full return to R) follows
                // every peck but the last, which reaches the bottom.
                let mut t = (p.r_z - step).max(bottom);
                loop {
                    b.push(MoveKind::Plunge, Point3::new(h.xy.x, h.xy.y, t), Some(p.plunge_feed));
                    if t <= bottom {
                        break;
                    }
                    let up = if kind == CycleKind::Peck {
                        (t + CHIP_BREAK).min(p.r_z)
                    } else {
                        p.r_z
                    };
                    b.push(MoveKind::Rapid, Point3::new(h.xy.x, h.xy.y, up), None);
                    t = (t - step).max(bottom);
                }
                if kind == CycleKind::Peck {
                    b.push(MoveKind::Rapid, Point3::new(h.xy.x, h.xy.y, p.r_z), None);
                }
            }
            CycleKind::Tap => {
                b.push(MoveKind::Feed, Point3::new(h.xy.x, h.xy.y, bottom), Some(tap_feed));
                b.push(MoveKind::Feed, Point3::new(h.xy.x, h.xy.y, p.r_z), Some(tap_feed));
            }
            CycleKind::Bore => {
                b.push(MoveKind::Plunge, Point3::new(h.xy.x, h.xy.y, bottom), Some(p.plunge_feed));
                b.push(MoveKind::Rapid, Point3::new(h.xy.x, h.xy.y, p.r_z), None);
            }
            CycleKind::BackBore => {
                b.push(MoveKind::Plunge, Point3::new(h.xy.x, h.xy.y, bottom), Some(p.plunge_feed));
                b.push(MoveKind::Feed, Point3::new(h.xy.x, h.xy.y, p.r_z), Some(p.feed));
            }
        }
        let dwell = match kind {
            CycleKind::Spot | CycleKind::Drill | CycleKind::Deep | CycleKind::Bore => p.dwell,
            _ => 0.0,
        };
        b.push_cycle(Cycle {
            kind,
            xy: h.xy,
            r_z: p.r_z,
            bottom_z: bottom,
            entry_z: h.entry_z,
            peck,
            dwell,
            first_move: fm,
        });
        // Lift to the hop plane for the traverse to the next hole.
        b.push(MoveKind::Rapid, Point3::new(h.xy.x, h.xy.y, hop), None);
    }
    let last = hs[hs.len() - 1];
    b.push(MoveKind::Rapid, Point3::new(last.xy.x, last.xy.y, start_z), None);
    let t = b.finish();
    if !t.is_chained() || !t.all_finite() {
        // The builder cannot produce this; if it ever did, the path is
        // not fit to leave this function.
        return Err(GenError::Invalid("drill chain self-check failed".into()));
    }
    Ok(t)
}

fn resolve_choice(p: &CycleParams) -> CycleKind {
    match p.cycle {
        CycleChoice::Auto => CycleKind::Drill, // never taken: drill() routes Auto through auto_kind
        CycleChoice::Spot => CycleKind::Spot,
        CycleChoice::Drill => CycleKind::Drill,
        CycleChoice::Peck => CycleKind::Peck,
        CycleChoice::Tap => CycleKind::Tap,
        CycleChoice::Bore => CycleKind::Bore,
        CycleChoice::BackBore => CycleKind::BackBore,
    }
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    /// Vertical walls of an XY ring, z in [z0, z1]. Ring order decides
    /// the facing: CCW rings face outward (a plate, a boss), CW rings
    /// face inward (a hole's wall in an outward-wound solid).
    fn ring_walls(poly: &[(f64, f64)], z0: f64, z1: f64) -> Vec<Triangle> {
        let mut f = Vec::new();
        let n = poly.len();
        for i in 0..n {
            let (x0, y0) = poly[i];
            let (x1, y1) = poly[(i + 1) % n];
            let (a, b) = (Point3::new(x0, y0, z0), Point3::new(x1, y1, z0));
            let (c, d) = (Point3::new(x1, y1, z1), Point3::new(x0, y0, z1));
            f.push(Triangle::new(a, b, c));
            f.push(Triangle::new(a, c, d));
        }
        f
    }

    fn circle(cx: f64, cy: f64, r: f64, n: usize) -> Vec<(f64, f64)> {
        (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * (i as f64) / (n as f64);
                (cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    }

    fn circle_cw(cx: f64, cy: f64, r: f64, n: usize) -> Vec<(f64, f64)> {
        let mut p = circle(cx, cy, r, n);
        p.reverse();
        p
    }

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<(f64, f64)> {
        vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
    }

    /// A 20x20x10 plate's walls plus `extra` void/collar walls.
    fn plate(extra: Vec<Triangle>) -> Vec<Triangle> {
        let mut t = ring_walls(&rect(0.0, 0.0, 20.0, 20.0), 0.0, 10.0);
        t.extend(extra);
        t
    }

    fn drill_tool(d: f64) -> Tool {
        Tool {
            kind: ToolKind::Drill,
            diameter: d,
            corner_r: 0.0,
            tip_angle: 118.0,
            pitch: 0.0,
            flute_len: 40.0,
            shank_d: d,
            shank_len: 40.0,
            holder_d: 44.0,
            holder_len: 35.0,
            material_group: "aluminum".into(),
            rpm: 10_000.0,
            feed: 120.0,
        }
    }

    fn hole(xy: (f64, f64), r: f64, entry: f64, floor: f64, through: bool) -> Hole {
        Hole {
            xy: Point2::new(xy.0, xy.1),
            radius: r,
            entry_z: entry,
            floor_z: floor,
            through,
        }
    }

    fn params<'a>(tool: &'a Tool, holes: &'a [Hole]) -> CycleParams<'a> {
        CycleParams {
            clearance_z: 15.0,
            r_z: 10.0,
            hop_z: None,
            bottom_z: None,
            cycle: CycleChoice::Auto,
            peck: 0.0,
            dwell: 0.0,
            feed: 500.0,
            plunge_feed: 100.0,
            tool,
            holes,
        }
    }

    // ---- recognition

    #[test]
    fn recognize_finds_through_and_blind_holes() {
        let mut t = plate(ring_walls(&circle_cw(10.0, 10.0, 2.0, 16), 0.0, 10.0));
        t.extend(ring_walls(&circle_cw(5.0, 5.0, 1.5, 16), 4.0, 10.0));
        let holes = recognize(&t);
        assert_eq!(holes.len(), 2, "{holes:?}");
        let through = holes.iter().find(|h| (h.xy.x - 10.0).abs() < 1e-6).unwrap();
        assert!(through.through);
        assert!((through.radius - 2.0).abs() < 1e-6);
        assert_eq!(through.entry_z, 10.0);
        assert_eq!(through.floor_z, 0.0);
        let blind = holes.iter().find(|h| (h.xy.x - 5.0).abs() < 1e-6).unwrap();
        assert!(!blind.through);
        assert_eq!(blind.floor_z, 4.0);
    }

    #[test]
    fn recognize_refuses_walls_bosses_and_slots() {
        // The plate's own outer wall: straight, normals face away.
        assert!(recognize(&plate(Vec::new())).is_empty(), "outer wall is not a hole");
        // A raised boss: closed round wall, normals face away.
        let mut m = ring_walls(&rect(-20.0, -20.0, 20.0, 20.0), 0.0, 2.0);
        m.extend(ring_walls(&circle(0.0, 0.0, 5.0, 32), 2.0, 8.0)); // CCW = outward-facing
        assert!(recognize(&m).is_empty(), "a boss is not a hole");
        // A racetrack slot: a void, but its vertices fit no circle.
        let mut track: Vec<(f64, f64)> = (1..16)
            .map(|i| {
                let a = -std::f64::consts::PI / 2.0 + std::f64::consts::PI * (i as f64) / 16.0;
                (13.0 + 3.0 * a.cos(), 10.0 + 3.0 * a.sin())
            })
            .collect();
        track.extend((1..16).map(|i| {
            let a = std::f64::consts::PI / 2.0 + std::f64::consts::PI * (i as f64) / 16.0;
            (7.0 + 3.0 * a.cos(), 10.0 + 3.0 * a.sin())
        }));
        let mut m2 = plate(ring_walls(&rect(0.0, 0.0, 40.0, 20.0), 0.0, 10.0));
        m2.extend(ring_walls(&track, 0.0, 10.0));
        assert!(recognize(&m2).is_empty(), "a slot is not a drilled hole");
    }

    #[test]
    fn recognize_merges_a_counterbore() {
        // A wider bore stepped above the main wall: one axis, two z-spans.
        let mut m = plate(ring_walls(&circle_cw(10.0, 10.0, 2.0, 24), 0.0, 8.0));
        m.extend(ring_walls(&circle_cw(10.0, 10.0, 3.0, 24), 8.0, 10.0));
        let h = recognize(&m);
        assert_eq!(h.len(), 1, "counterbore is one hole: {h:?}");
        assert!((h[0].radius - 2.0).abs() < 1e-6, "the narrow wall is the bore");
        assert_eq!(h[0].entry_z, 10.0);
        assert_eq!(h[0].floor_z, 0.0);
        assert!(h[0].through);
    }

    #[test]
    fn recognize_is_stable_under_vertex_noise() {
        let base = plate(ring_walls(&circle_cw(10.0, 10.0, 2.0, 16), 0.0, 10.0));
        let noisy = base
            .iter()
            .map(|t| {
                let jit = |p: Point3| {
                    let k = p.x.to_bits() ^ (p.y.to_bits() << 1) ^ (p.z.to_bits() >> 3);
                    let e = ((k & 0xFFFF) as f64 / 65535.0 - 0.5) * 2e-7;
                    Point3::new(p.x + e, p.y + e, p.z)
                };
                Triangle::new(jit(t.a), jit(t.b), jit(t.c))
            })
            .collect::<Vec<_>>();
        let (a, b) = (recognize(&base), recognize(&noisy));
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert!((x.xy.x - y.xy.x).abs() < 1e-4 && (x.xy.y - y.xy.y).abs() < 1e-4);
            assert!((x.radius - y.radius).abs() < 1e-4);
        }
    }

    #[test]
    fn recognize_degenerate_input_never_panics() {
        assert!(recognize(&[]).is_empty());
        let nan = Triangle::new(
            Point3::new(f64::NAN, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        );
        assert!(recognize(&[nan]).is_empty());
        let mut t = plate(ring_walls(&circle_cw(10.0, 10.0, 2.0, 16), 0.0, 10.0));
        for x in t.iter_mut() {
            x.b = x.a; // collapse every triangle to zero area
        }
        assert!(recognize(&t).is_empty(), "zero-area facets must drop, not divide");
    }

    // ---- generator

    fn one(t: &Toolpath) -> &Cycle {
        assert_eq!(t.cycles.len(), 1);
        &t.cycles[0]
    }

    #[test]
    fn agreement_moves_and_cycles_describe_the_same_holes() {
        // The law this module is named for: per cycle, from first_move
        // the moves that stay on the hole's XY are its group, the group
        // opens with a cut, and its deepest point is bottom_z.
        let tool = drill_tool(4.0);
        let holes = vec![
            hole((5.0, 5.0), 2.0, 10.0, 8.0, false),
            hole((15.0, 15.0), 2.0, 10.0, 0.0, true),
        ];
        let t = drill(&params(&tool, &holes)).unwrap();
        assert_eq!(t.cycles.len(), 2);
        assert!(t.is_chained() && t.all_finite());
        for (i, c) in t.cycles.iter().enumerate() {
            let end = t.cycles.get(i + 1).map_or(t.moves.len(), |n| n.first_move);
            let on_hole: Vec<&crate::cl::Move> = t.moves[c.first_move..end]
                .iter()
                .take_while(|m| (m.to.x - c.xy.x).abs() < 1e-9 && (m.to.y - c.xy.y).abs() < 1e-9)
                .collect();
            assert!(!on_hole.is_empty(), "cycle {i} has no moves at its XY");
            assert_ne!(on_hole[0].kind, MoveKind::Rapid, "cycle must open with a cut");
            let deepest = on_hole.iter().map(|m| m.to.z).fold(f64::INFINITY, f64::min);
            assert!((deepest - c.bottom_z).abs() < 1e-9, "cycle {i}: {deepest} vs {}", c.bottom_z);
            assert!((c.entry_z - 10.0).abs() < 1e-9);
        }
    }

    #[test]
    fn auto_cycle_picks_by_depth() {
        let tool = drill_tool(4.0);
        let cases = [
            (hole((10.0, 10.0), 2.0, 10.0, 7.0, false), CycleKind::Drill),
            (hole((10.0, 10.0), 2.0, 10.0, 3.0, false), CycleKind::Peck),
            (hole((10.0, 10.0), 2.0, 10.0, -10.0, false), CycleKind::Deep),
        ];
        for (h, want) in cases {
            let t = drill(&params(&tool, &[h])).unwrap();
            assert_eq!(one(&t).kind, want, "depth {} on d 4", 10.0 - h.floor_z);
        }
    }

    #[test]
    fn deep_peck_retracts_to_r_between_pecks() {
        let tool = drill_tool(2.0);
        // Depth 9 > 4d: the Deep branch of the auto law.
        let t = drill(&params(&tool, &[hole((10.0, 10.0), 1.0, 10.0, 1.0, false)])).unwrap();
        let c = one(&t);
        assert_eq!(c.kind, CycleKind::Deep);
        assert!((c.peck - 2.0).abs() < 1e-9, "auto G83 peck = 1d");
        assert_eq!(c.dwell, 0.0);
        let plunges = t.moves[c.first_move..]
            .iter()
            .take_while(|m| (m.to.x - 10.0).abs() < 1e-9)
            .filter(|m| m.kind == MoveKind::Plunge)
            .count();
        assert_eq!(plunges, 5, "ceil(9 / peck 2)");
        // Full returns to R between pecks: rapids up to exactly 10.0.
        let ups = t.moves[c.first_move..]
            .iter()
            .filter(|m| m.kind == MoveKind::Rapid && m.to.z == 10.0 && m.from.z < 10.0)
            .count();
        assert!(ups >= 3, "G83 retracts to R between the four pecks: {ups}");
    }

    #[test]
    fn peck_hops_only_chip_break() {
        let tool = drill_tool(2.0);
        let hs = [hole((10.0, 10.0), 1.0, 10.0, 2.0, false)];
        let mut p = params(&tool, &hs);
        p.cycle = CycleChoice::Peck; // depth 8 > 4d would auto-Deep; force the G73 shape
        let t = drill(&p).unwrap();
        let c = one(&t);
        assert_eq!(c.kind, CycleKind::Peck);
        assert!((c.peck - 6.0).abs() < 1e-9, "auto G73 peck = 3d");
        // Hops rise by about CHIP_BREAK, not to R: no rapid reaches 10.0
        // from below except the cycle's own final retract.
        let to_r = t.moves[c.first_move..]
            .iter()
            .filter(|m| m.kind == MoveKind::Rapid && m.to.z == 10.0 && m.from.z < 10.0)
            .count();
        assert!(to_r <= 2, "G73 does not fully retract mid-cycle: {to_r}");
    }

    #[test]
    fn tap_feeds_at_pitch_times_rpm_both_ways() {
        let mut tool = drill_tool(6.0);
        tool.kind = ToolKind::Tap;
        tool.pitch = 1.0;
        tool.rpm = 500.0;
        let t = drill(&params(&tool, &[hole((10.0, 10.0), 3.0, 10.0, 8.0, false)])).unwrap();
        let c = one(&t);
        assert_eq!(c.kind, CycleKind::Tap);
        assert_eq!(c.dwell, 0.0);
        let cut: Vec<&crate::cl::Move> = t.moves[c.first_move..]
            .iter()
            .take_while(|m| m.kind == MoveKind::Feed)
            .collect();
        assert_eq!(cut.len(), 2, "down and out, both feeds");
        assert!(cut.iter().all(|m| m.feed == Some(500.0)), "{:?}", cut.iter().map(|m| m.feed).collect::<Vec<_>>());
        // A pitchless tap cannot have its feed derived: refuse.
        tool.pitch = 0.0;
        assert!(
            matches!(drill(&params(&tool, &[hole((10.0, 10.0), 3.0, 10.0, 8.0, false)])), Err(GenError::Invalid(_))),
            "tap with no pitch must error"
        );
    }

    #[test]
    fn bore_backbore_and_spot_shapes() {
        let mut tool = drill_tool(8.0);
        tool.kind = ToolKind::Bore;
        let h = hole((10.0, 10.0), 4.0, 10.0, 4.0, false);
        let t = drill(&params(&tool, &[h])).unwrap();
        assert_eq!(one(&t).kind, CycleKind::Bore);
        let g = &t.moves[one(&t).first_move..];
        assert_eq!(g[0].kind, MoveKind::Plunge, "feed in");
        assert_eq!(g[1].kind, MoveKind::Rapid, "rapid out");

        let hs = [h];
        let mut p = params(&tool, &hs);
        p.cycle = CycleChoice::BackBore;
        let t = drill(&p).unwrap();
        let g = &t.moves[one(&t).first_move..];
        assert!(g.iter().any(|m| m.kind == MoveKind::Feed && m.to.z > m.from.z), "back-bore retracts on a feed");

        let emt = drill_tool(4.0);
        let mut p = params(&emt, &hs);
        p.cycle = CycleChoice::Spot;
        p.dwell = 1.5;
        let t = drill(&p).unwrap();
        let c = one(&t);
        assert_eq!(c.kind, CycleKind::Spot);
        assert!((c.bottom_z - 8.0).abs() < 1e-9, "spot at r_z - SPOT_DEPTH");
        assert!((c.dwell - 1.5).abs() < 1e-9, "spot honors the dwell word");
        // A spot never digs through a floor standing in its way:
        let shallower = [hole((10.0, 10.0), 4.0, 10.0, 9.0, false)];
        let mut p2 = params(&emt, &shallower);
        p2.cycle = CycleChoice::Spot;
        let t2 = drill(&p2).unwrap();
        assert!((one(&t2).bottom_z - 9.0).abs() < 1e-9, "spot stops at the floor");
    }

    #[test]
    fn through_exit_clears_the_floor() {
        let tool = drill_tool(4.0);
        let t = drill(&params(&tool, &[hole((10.0, 10.0), 2.0, 10.0, 0.0, true)])).unwrap();
        let c = one(&t);
        assert!(c.bottom_z < 0.0);
        assert!((c.bottom_z + THROUGH_CLEAR).abs() < 1e-9);
        assert_eq!(c.entry_z, 10.0);
    }

    #[test]
    fn depth_override_applies_to_every_hole() {
        let tool = drill_tool(4.0);
        let holes = vec![hole((5.0, 5.0), 2.0, 10.0, 8.0, false), hole((15.0, 5.0), 2.0, 10.0, 2.0, false)];
        let mut p = params(&tool, &holes);
        p.bottom_z = Some(-1.0);
        let t = drill(&p).unwrap();
        assert!(t.cycles.iter().all(|c| (c.bottom_z + 1.0).abs() < 1e-9));
        // An override at or above the R plane is nonsense, refused.
        p.bottom_z = Some(10.0);
        assert!(matches!(drill(&p), Err(GenError::Invalid(_))));
    }

    #[test]
    fn empty_and_malformed_lists_error_never_panic() {
        let tool = drill_tool(4.0);
        assert!(matches!(drill(&params(&tool, &[])), Err(GenError::Empty(_))));
        assert!(matches!(
            drill(&params(&tool, &[hole((10.0, 10.0), 2.0, f64::NAN, 8.0, false)])),
            Err(GenError::Invalid(_))
        ));
        assert!(matches!(
            drill(&params(&tool, &[hole((10.0, 10.0), 2.0, 8.0, 8.0, false)])),
            Err(GenError::Invalid(_))
        ));
    }

    #[test]
    fn chain_stays_contiguous_over_many_holes() {
        let tool = drill_tool(3.0);
        let holes: Vec<Hole> = (0..20)
            .map(|i| hole((2.0 + i as f64 * 0.9, 10.0), 1.5, 10.0, 9.0, false))
            .collect();
        let t = drill(&params(&tool, &holes)).unwrap();
        assert!(t.is_chained());
        assert_eq!(t.cycles.len(), 20);
        let mut prev = 0;
        for c in &t.cycles {
            assert!(c.first_move >= prev, "cycle groups must be ordered and disjoint");
            prev = c.first_move;
        }
    }

    #[test]
    fn user_peck_overrides_the_auto_step() {
        let tool = drill_tool(2.0); // depth 8, auto Deep peck = 2
        let h = hole((10.0, 10.0), 1.0, 10.0, 2.0, false);
        let hs = [h];
        let mut p = params(&tool, &hs);
        p.peck = 4.0;
        let t = drill(&p).unwrap();
        assert!((one(&t).peck - 4.0).abs() < 1e-9);
        let plunges = t.moves[one(&t).first_move..]
            .iter()
            .take_while(|m| (m.to.x - 10.0).abs() < 1e-9)
            .filter(|m| m.kind == MoveKind::Plunge)
            .count();
        assert_eq!(plunges, 2);
    }
}
