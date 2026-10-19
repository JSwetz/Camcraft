//! Static verification of generated toolpaths: warnings a path can carry
//! before anything is posted.
//!
//! Provenance: segment intersection by the orientation predicate is
//! textbook computational geometry (O'Rourke, "Computational Geometry in
//! C", public material); the tool-reach checks formalize the shop
//! practice of comparing cutter reach against wall height (machine-tool
//! handbooks). The rest-material model below is our own conservative
//! approximation, stated in full where it is used. No vendor source was
//! consulted.
//!
//! Five checks, each with the exact condition that raises it:
//!
//! - [`Warning::Crossing`] - two Feed moves whose Z ranges overlap cross
//!   in XY. Proper crossings only: touching at a shared vertex (a closed
//!   trace loop, a serpentine join) is not a crossing.
//! - [`Warning::Gouge`] - a cutter center closer to a region boundary
//!   than `r - leave - tol`, i.e. the disc bites past the finish surface
//!   it is supposed to touch. Engrave paths (trace) deliberately sit on
//!   boundaries, so `Mode::Engrave` skips it.
//! - [`Warning::ShankHit`] - uncut material inside the shank or holder
//!   annulus above the current cut. Rest material: at a boundary sample
//!   q (region and stock loop vertices and edge midpoints, nudged a hair
//!   inside so membership is unambiguous), material survives above height h iff
//!   `h < m(q)`, where `m(q)` is the stock top when q is part (never cut)
//!   or, when q is removable stock, the deepest cut level whose path
//!   swept q. A band bottomed at `cut_z + flute_len` (shank) or
//!   `cut_z + flute_len + shank_len` (holder) with radius above the
//!   flute's to q, with material still above it, collides. Sampling at
//!   vertices and edge midpoints is conservative, not exact.
//! - [`Warning::RapidInStock`] - a *lateral* rapid below the clearance
//!   plane whose XY path still has material above it. Vertical rapids
//!   are the tool retracting through its own kerf and stay exempt.
//! - [`Warning::ShankHit`] again, from the cycle list: a hole whose
//!   entry-to-bottom span exceeds flute + shank + holder pushes the
//!   assembly below the entry surface no matter what the path looks
//!   like. The move laws cannot see this at a matched drill - the
//!   plunge sweeps exactly its own disc, so the hole's rim samples
//!   fall inside the flute radius where law (c)'s annulus is empty -
//!   which is why the reach law reads the cycles, not the moves.
//!
//! With [`VerifyContext::model`] set (the 3-D pocket, M2.1a) the
//! boundary is depth-dependent: the gouge law adds the model's
//! cross-section strictly above each move's deepest plane, the reach
//! laws sample those sections' walls like any other island, and rest
//! material counts from the deepest plane whose section does not hold
//! the column - cutting around an island removes nothing inside it.
//!
//! Conservative by design: it may warn where nothing is wrong (a wall
//! thinner than the sampling, an exotic holder shape), it must never
//! stay silent about the five classes above. The hard post-gate that
//! blocks on these lands with the post processor (M2.3); until then the
//! operation layer attaches them to the operation for display.
//!
//! Arc moves are flattened to chords before the laws run
//! ([`Move::chords`]) at `gouge_tol`: a chord sits within that
//! tolerance of the arc it samples, which is exactly the slack the
//! radial laws already grant - so no violation beyond tolerance hides
//! in an arc. Warnings keep indexing *moves*; an arc is one move, so
//! swapping chords for arcs never shifts an index.

use std::collections::HashMap;

use crate::cl::{Move, MoveKind, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;

/// The cutting tool and what rides above it. Radii are coaxial with the
/// spindle; lengths stack upward from the cut edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolGeom {
    pub radius: f64,
    /// Usable flute length below the shank.
    pub flute_len: f64,
    pub shank_radius: f64,
    pub shank_len: f64,
    pub holder_radius: f64,
    pub holder_len: f64,
}

/// What the path means, which decides which laws apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Pocket/slot: material is removed from inside the region.
    Area,
    /// Contour/face: the path trims the region's boundary.
    Boundary,
    /// Trace: the cutter center runs on the boundary itself; laws that
    /// assume tangency do not apply.
    Engrave,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VerifyContext<'a> {
    pub region: &'a Region,
    /// The stock prism footprint; pass it when part islands must be
    /// checked too - otherwise the region stands in for it.
    pub stock: Option<&'a Region>,
    /// The mesh a model-driven op cuts to (M2.1's 3-D pocket): the cut
    /// target changes with depth, so the static region cannot be the
    /// whole law. Given, the gouge law checks each move against the
    /// model's cross-section strictly above that move's deepest plane
    /// (the level law's own islands), and the reach laws sample those
    /// sections' walls as the islands they are. Every 2.5-D op passes
    /// `None` - its laws are byte-identical to what they always were.
    pub model: Option<&'a [crate::geometry::Triangle]>,
    pub tool: &'a ToolGeom,
    pub mode: Mode,
    /// Top of the stock column in Z (the surface the tool starts from).
    pub stock_top_z: f64,
    /// Safe plane: rapids must live at or above it.
    pub clearance_z: f64,
    /// Finish allowance already baked into the path offsets.
    pub stock_to_leave: f64,
    /// Radial tolerance for the gouge law.
    pub gouge_tol: f64,
    /// Margin for "below the clearance plane" and "moved in XY".
    pub z_tol: f64,
}

/// Serde derives: the operation layer persists warnings with the cached
/// path (project v3 `OperationDto.cache`), so the enum is also a file
/// format - variant names are that contract, keep them stable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Warning {
    /// Feed moves `moves[0]` and `moves[1]` cross in XY at shared depth.
    Crossing { moves: [usize; 2] },
    /// Move `move`'s cutter center is inside the finish surface.
    Gouge { at: usize },
    /// Move `move` carries shank/holder through material; `part` marks
    /// whether the offended material is the part/an island (never to be
    /// cut) rather than stock the op simply has not cleared yet.
    ShankHit { at: usize, part: bool },
    /// Rapid `move` travels under the clearance plane through stock.
    RapidInStock { at: usize },
    /// Defensive: a non-finite coordinate (generators gate this out).
    NonFinite { at: usize },
}

impl core::fmt::Display for Warning {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Warning::Crossing { moves } => write!(
                f,
                "feed moves {} and {} cross in XY at shared depth",
                moves[0], moves[1]
            ),
            Warning::Gouge { at } => {
                write!(f, "move {at} cuts past the finish surface")
            }
            Warning::ShankHit { at, part } => write!(
                f,
                "shank/holder at move {at} collides with the {}",
                if *part { "part or an island" } else { "uncut stock" }
            ),
            Warning::RapidInStock { at } => {
                write!(f, "rapid move {at} travels below the clearance plane inside stock")
            }
            Warning::NonFinite { at } => write!(f, "move {at} has a non-finite coordinate"),
        }
    }
}

fn xy(p3: &Point3) -> Point2 {
    Point2::new(p3.x, p3.y)
}

/// Squared XY distance from `p` to segment `a`-`b`.
fn dist2_seg(p: Point2, a: Point2, b: Point2) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let dd = dx * dx + dy * dy;
    if dd <= f64::EPSILON {
        let (ex, ey) = (p.x - a.x, p.y - a.y);
        return ex * ex + ey * ey;
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0);
    let (ex, ey) = (a.x + t * dx - p.x, a.y + t * dy - p.y);
    ex * ex + ey * ey
}

fn orient(a: Point2, b: Point2, c: Point2) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// Proper transversal crossing of two XY segments: each endpoint pair
/// strictly straddles the other's line. Shared or touching endpoints
/// give a zero orientation and are not crossings.
fn proper_cross(a0: Point2, a1: Point2, b0: Point2, b1: Point2) -> bool {
    let (oa, ob) = (orient(a0, a1, b0), orient(a0, a1, b1));
    let (oc, od) = (orient(b0, b1, a0), orient(b0, b1, a1));
    oa != 0.0
        && ob != 0.0
        && oc != 0.0
        && od != 0.0
        && ((oa > 0.0) != (ob > 0.0))
        && ((oc > 0.0) != (od > 0.0))
}

/// XY samples of every boundary: loop vertices plus edge midpoints, so a
/// wall is represented by more than its corners, each nudged a hair
/// toward the loop's vertex mean. The nudge keeps membership off the
/// parity knife-edge: just inside an outer loop is region material, just
/// inside a hole loop is the island - the point a reach law must classify
/// unambiguously.
fn boundary_samples(region: &Region) -> Vec<Point2> {
    const NUDGE: f64 = 0.05;
    let mut out = Vec::new();
    for l in region.loops() {
        let pts = l.points();
        if pts.is_empty() {
            continue;
        }
        let (mx, my) = pts.iter().fold((0.0, 0.0), |s, q| (s.0 + q.x, s.1 + q.y));
        let (mx, my) = (mx / pts.len() as f64, my / pts.len() as f64);
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            for q in [a, Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)] {
                let (dx, dy) = (mx - q.x, my - q.y);
                let d = (dx * dx + dy * dy).sqrt();
                out.push(if d > f64::EPSILON {
                    Point2::new(q.x + NUDGE * dx / d, q.y + NUDGE * dy / d)
                } else {
                    q
                });
            }
        }
    }
    out
}

/// XY points a radial law must hold at for one move: exactly the move's
/// `to` for a straight one - byte-for-byte the behavior every straight
/// path always had - and the arc with its chord joints for an arc, which
/// bound it within `chord_tol`: the laws' own radial tolerance is what
/// absorbs that sampling error, so a violation past tolerance is never
/// missed.
fn law_points(m: &Move, chord_tol: f64) -> Vec<Point2> {
    match m.arc {
        None => vec![Point2::new(m.to.x, m.to.y)],
        Some(_) => {
            let cs = m.chords(chord_tol);
            let mut v = Vec::with_capacity(cs.len() + 1);
            if let Some(&(a, _)) = cs.first() {
                v.push(a);
            }
            v.extend(cs.into_iter().map(|(_, b)| b));
            v
        }
    }
}

/// Verify a generated path. Quadratic in feed count (crossings), linear
/// per sample for the rest; called by the operation layer after
/// generation, never on a systems hot path.
pub fn verify(path: &Toolpath, cx: &VerifyContext) -> Vec<Warning> {
    let mut warns: Vec<Warning> = Vec::new();
    let r = cx.tool.radius;

    for (i, m) in path.moves.iter().enumerate() {
        if !(m.from.is_finite() && m.to.is_finite()) {
            warns.push(Warning::NonFinite { at: i });
        }
    }

    // (a) Crossing: Feed-Feed pairs, proper XY crossings at shared depth.
    // Each feed contributes its chords (one for a straight move, so a
    // straight path walks exactly the pair set it always did); reports
    // are per move pair, ascending and deduped - indices hold stable
    // whatever the chord count.
    let chord_tol = cx.gouge_tol;
    let mut feeds: Vec<(usize, Point2, Point2, f64, f64)> = Vec::new();
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind != MoveKind::Feed {
            continue;
        }
        for (a, b) in m.chords(chord_tol) {
            feeds.push((i, a, b, m.from.z, m.to.z));
        }
    }
    let mut crossed: Vec<[usize; 2]> = Vec::new();
    for (ai, &(ia, a0, a1, za0, za1)) in feeds.iter().enumerate() {
        for &(ib, b0, b1, zb0, zb1) in feeds.iter().skip(ai + 1) {
            if ia == ib {
                continue; // chords of one move share their joints
            }
            // Two planar chords cross only at a shared plane, exactly
            // as always. When a chord descends (a ramp diagonal, a
            // helix arc chord), a Z-interval that touches the other's
            // plane at a single endpoint is a GRAZE of the already-cut
            // floor at the dive's own start or end depth - which is
            // what every 3-D entry does by construction, over XY the
            // level law already proved open. For a non-planar pair
            // only a shared depth RANGE (two descents whose bands
            // overlap) counts as crossing. (M2.1a's superset law is
            // what makes the graze safe; without it this would be a
            // gouge.)
            let planar_pair = za0 == za1 && zb0 == zb1;
            let z_hit = if planar_pair {
                za0 == zb0
            } else {
                za0.max(za1).min(zb0.max(zb1)) - za0.min(za1).max(zb0.min(zb1)) > 0.0
            };
            if z_hit && proper_cross(a0, a1, b0, b1) {
                crossed.push([ia.min(ib), ia.max(ib)]);
            }
        }
    }
    crossed.sort_unstable();
    crossed.dedup();
    warns.extend(crossed.into_iter().map(|moves| Warning::Crossing { moves }));

    // A model-driven path's islands per plane - the generator's own
    // `material_above`, so one law has two readers (M2.1a). Cached by
    // the plane's bits: the level ladder repeats its planes across all
    // three readers below. `None` means the slice failed; every use of
    // a failed slice fails closed (no proof, no pass).
    let island_cache: std::cell::RefCell<HashMap<u64, Option<Region>>> =
        std::cell::RefCell::new(HashMap::new());
    let islands_at = |facets: &[crate::geometry::Triangle], z: f64| -> Option<Region> {
        let key = z.to_bits();
        if let Some(hit) = island_cache.borrow().get(&key) {
            return hit.clone();
        }
        let fresh = crate::machining3d::material_above(facets, z).ok();
        island_cache
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| fresh.clone());
        fresh
    };
    // Deepest plane the path cuts at (drives the island law of the
    // reach samples and the part label below).
    let z_deepest = path
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .map(|m| m.from.z.min(m.to.z))
        .fold(f64::INFINITY, f64::min);

    // (b) Gouge: tangency law d(center, boundary) >= r - leave - tol,
    // held at every law point of the move (see law_points). For a
    // model-driven path the boundary is level-dependent: the region's
    // silhouette walls plus the model's cross-section strictly above
    // the move's deepest plane. That section is exact for a fixed-Z
    // level pass, and conservative for a ramp or helix, whose whole XY
    // the generator already proved inside the deepest level's center
    // area (the chord and swept-disc laws) - a column that is only an
    // island deeper is not a hole here, and one the ladder must never
    // enter is. A center inside the section is a full collision, not
    // a near miss, so containment joins the distance test.
    if cx.mode != Mode::Engrave {
        for (i, m) in path.moves.iter().enumerate() {
            if m.kind == MoveKind::Rapid {
                continue;
            }
            let section = cx.model.map(|f| islands_at(f, m.from.z.min(m.to.z)));
            if let Some(None) = section {
                warns.push(Warning::Gouge { at: i }); // cannot prove the slice
                continue;
            }
            let section = section.flatten();
            let mut gouged = false;
            for p in law_points(m, chord_tol) {
                let mut d2 = f64::INFINITY;
                for l in cx.region.loops() {
                    let pts = l.points();
                    for k in 0..pts.len() {
                        d2 = d2.min(dist2_seg(p, pts[k], pts[(k + 1) % pts.len()]));
                    }
                }
                if let Some(s) = &section {
                    if s.contains(p) {
                        gouged = true;
                        break;
                    }
                    for l in s.loops() {
                        let pts = l.points();
                        for k in 0..pts.len() {
                            d2 = d2.min(dist2_seg(p, pts[k], pts[(k + 1) % pts.len()]));
                        }
                    }
                }
                if d2.sqrt() < r - cx.stock_to_leave - cx.gouge_tol {
                    gouged = true;
                    break;
                }
            }
            if gouged {
                warns.push(Warning::Gouge { at: i });
            }
        }
    }

    // Cutting (non-rapid) XY segments with the level they cut at: a
    // segment's whole disc sweep clears its column down to `to.z`
    // (plunges are degenerate segments at their XY, arcs contribute
    // their chords - all within chord_tol of the true arc).
    let mut cut: Vec<(Point2, Point2, f64)> = path
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .flat_map(|m| {
            m.chords(chord_tol)
                .into_iter()
                .map(move |(a, b)| (a, b, m.to.z))
        })
        .collect();
    // Deepest plane first, so `surface`'s first reach is its deepest
    // reach - the min the law always asked for, found without a scan.
    cut.sort_by(|a, b| a.2.total_cmp(&b.2));
    // Material footprint for the reach laws: the stock prism when given,
    // the region standing in for it otherwise.
    let in_stock = |q: Point2| -> bool { cx.stock.map_or_else(|| cx.region.contains(q), |s| s.contains(q)) };
    // m(q): the surface height of remaining material in q's column.
    // A model-driven path adds one clause (M2.1a): a chord at plane z
    // cleared q's column only if the section at z does not hold q -
    // cutting around an island removes nothing inside it, and the
    // staircase's top is the shallower plane that did reach. Sections
    // are monotone in z, so the island test is a point-in-region on a
    // cached slice, run only for chords that are already within reach.
    let surface = |q: Point2| -> f64 {
        let r2 = (r + 1e-9) * (r + 1e-9);
        let cleared = cut
            .iter()
            .find(|(a, b, z)| {
                dist2_seg(q, *a, *b) <= r2
                    && cx.model
                        .is_none_or(|f| !islands_at(f, *z).is_some_and(|s| s.contains(q)))
            })
            .map_or(f64::INFINITY, |(_, _, z)| *z);
        cleared.min(cx.stock_top_z)
    };

    // (c) Shank/holder reach against rest material.
    if cx.mode != Mode::Engrave {
        // Both boundaries matter: the region's loops carry the part
        // islands (holes in the region), the stock's carry its outer
        // walls wherever they stand beyond the region. A model-driven
        // path's islands are not holes in its silhouette region - they
        // are computed per plane - so their walls join the sample set
        // at every plane the ladder cuts at (M2.1a). The part flag
        // follows the law that keeps them out of the cut: a column
        // inside the deepest section is material the ladder swore
        // never to chase below, stair left by a shallower pass
        // notwithstanding - reported part, worst case.
        let mut samples = boundary_samples(cx.region);
        if let Some(s) = cx.stock {
            samples.extend(boundary_samples(s));
        }
        if let Some(f) = cx.model {
            let mut zs: Vec<u64> = cut.iter().map(|(_, _, z)| z.to_bits()).collect();
            zs.sort_unstable();
            zs.dedup();
            for k in zs {
                if let Some(s) = islands_at(f, f64::from_bits(k)) {
                    samples.extend(boundary_samples(&s));
                }
            }
        }
        let part_at = |q: Point2| -> bool {
            !cx.region.contains(q)
                || cx.model.is_some_and(|f| {
                    islands_at(f, z_deepest).is_some_and(|s| s.contains(q))
                })
        };
        let bands = [
            (cx.tool.flute_len, cx.tool.shank_radius, cx.tool.shank_len),
            (
                cx.tool.flute_len + cx.tool.shank_len,
                cx.tool.holder_radius,
                cx.tool.holder_len,
            ),
        ];
        for (i, m) in path.moves.iter().enumerate() {
            if m.kind == MoveKind::Rapid {
                continue;
            }
            let pts = law_points(m, chord_tol);
            for &(above, band_r, band_len) in &bands {
                if band_r <= r || band_len <= 0.0 {
                    continue;
                }
                let h0 = m.to.z + above;
                let r2 = (band_r + 1e-9) * (band_r + 1e-9);
                let mut hit = None;
                'worst: {
                    for p in &pts {
                        for q in &samples {
                            let dd = dist2_seg(*q, *p, *p);
                            if dd > r2 || dd <= (r + cx.gouge_tol) * (r + cx.gouge_tol) {
                                continue; // outside this band's annulus
                            }
                            if !in_stock(*q) {
                                continue;
                            }
                            if h0 < surface(*q) {
                                hit = Some(part_at(*q));
                                if hit == Some(true) {
                                    break 'worst; // report the worst offender
                                }
                            }
                        }
                    }
                }
                if let Some(part) = hit {
                    warns.push(Warning::ShankHit { at: i, part });
                }
            }
        }
    }

    // (d) Rapids: lateral travel under the clearance plane inside stock.
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind != MoveKind::Rapid {
            continue;
        }
        let (a, b) = (xy(&m.from), xy(&m.to));
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        if dx * dx + dy * dy <= cx.z_tol * cx.z_tol {
            continue; // vertical retract through the tool's own kerf
        }
        let low = m.from.z.min(m.to.z);
        if low >= cx.clearance_z - cx.z_tol {
            continue;
        }
        let mid = Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        let bad = [a, mid, b].into_iter().any(|q| in_stock(q) && low < surface(q));
        if bad {
            warns.push(Warning::RapidInStock { at: i });
        }
    }

    // (e) Cycle reach: a hole deeper than the tool assembly reaches is
    // a shank collision, reported at the hole's first move. The law
    // reads the cycle list on purpose - a vertical plunge at a matched
    // drill sweeps exactly its own disc, so every rim sample sits inside
    // the flute radius and law (c) has nothing to sample in the annulus;
    // the plate top around the hole is part, so `part` is true. An
    // oversized hole deeper than full reach is visible to (c) as well;
    // the same-move report is not repeated.
    // No mode gate: only drilling paths ever carry cycles, so this is a
    // no-op everywhere else.
    let reach = cx.tool.flute_len + cx.tool.shank_len + cx.tool.holder_len;
    for c in &path.cycles {
        if c.entry_z - c.bottom_z > reach {
            let hit = Warning::ShankHit {
                at: c.first_move,
                part: true,
            };
            if !warns.contains(&hit) {
                warns.push(hit);
            }
        }
    }

    warns
}
