//! Split from `verify.rs` by M5.0.1; structure only.

use super::types::{Mode, VerifyContext, Warning};
use super::laws::{bands_vs_fixtures, boundary_samples, dist2_seg, law_points, proper_cross, rapids_vs_fixtures, xy};

use std::collections::HashMap;
use crate::cl::{MoveKind, Toolpath};
use crate::geo2d::{Point2, Region};
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
    // whatever the chord count. The chord budget is shared by laws
    // (b)-(f), so it is bound here, outside the mode gate below.
    let chord_tol = cx.gouge_tol;
    let mut crossed: Vec<[usize; 2]> = Vec::new();
    // Mode::Envelope stands this law down by design (M2.2c): a
    // crossed raster pattern makes two Feed chords cross in XY at
    // shared depth wherever its two families share a point, and the
    // stepover's own overlap already has neighboring lines covering
    // the same material - that intersection is the pattern's
    // substance, not a path defect. The waterline ring never made a
    // proper crossing, so only the multi-direction patterns reveal
    // the predicate's limits. What a harmful chord would be - one
    // biting into the surface or sailing above it - law (f) measures
    // directly, on the swept ball against the model, move by move.
    if cx.mode != Mode::Envelope {
        let mut feeds: Vec<(usize, Point2, Point2, f64, f64)> = Vec::new();
        for (i, m) in path.moves.iter().enumerate() {
            if m.kind != MoveKind::Feed {
                continue;
            }
            for (a, b) in m.chords(chord_tol) {
                feeds.push((i, a, b, m.from.z, m.to.z));
            }
        }
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
        let fresh = crate::geometry::slice::material_above(facets, z).ok();
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
    // Mode::Envelope stands this law down by design: a waterline ring
    // is tangent - its centers hold exactly r to the model at the
    // tangent line while sitting nearer than r to the level silhouette
    // elsewhere - and tangency is not gouging. Law (f) measures the
    // swept ball instead.
    if cx.mode != Mode::Engrave && cx.mode != Mode::Envelope {
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

    // The band stack, law (c) and its fixture twin (c2) both read -
    // [ (bottom offset above the cut, radius, length) ] for shank, then
    // holder.
    let bands = [
        (cx.tool.flute_len, cx.tool.shank_radius, cx.tool.shank_len),
        (
            cx.tool.flute_len + cx.tool.shank_len,
            cx.tool.holder_radius,
            cx.tool.holder_len,
        ),
    ];

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

    // (c2) Bands against fixtures (M3.2 stage B) - the law lives in
    // [`bands_vs_fixtures`] so the post gate can run it without a
    // region. Run for EVERY mode: steel has no tangency courtesy.
    if !cx.fixtures.is_empty() {
        warns.extend(bands_vs_fixtures(
            path,
            cx.tool,
            cx.fixtures,
            cx.spindle,
            chord_tol,
        ));
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

    // (d2) Rapids against fixtures (M3.2 stage B) - the law lives in
    // [`rapids_vs_fixtures`] so the post gate can run it without a
    // region.
    if !cx.fixtures.is_empty() {
        warns.extend(rapids_vs_fixtures(path, cx.tool, cx.fixtures));
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

    // (f) Finish envelope - the measured verdict that replaces law (b)
    // for ball-rolling paths (`stock_sim::envelope_scan`): `gouge` is
    // Dexel truth, material cut below the model wherever a ball's
    // floor ran under the surface; `rest` is distance truth, model
    // standing outside every ball at ridge points - surface points
    // the center path passed twice inside the promise band, where
    // the stepdown law promised tolerance. The band is `tol` wide,
    // so the tolerance is handed to the scan as well as to the gate.
    // Both are
    // judged against `finish_tol` plus the scan's own quantization
    // floor, `cell^2 / (2r)` - the ball's sagitta across one cell,
    // the smallest deviation this measurement can honestly resolve.
    // A verdict tighter than the measurement is a lie in the other
    // direction; the rest this law does NOT report - untouched
    // columns, shelves and crowns and missing-pass gaps - is another
    // op's jurisdiction (rest machining, M2.3), law documented in
    // finishing3d.rs. An Envelope context that cannot measure - no
    // model, no tolerance, an unscanable lattice - has proven
    // nothing, and proves-not reports as a gouge at the first cut,
    // never as silence (the law (b) slice-proof precedent).
    if cx.mode == Mode::Envelope {
        let first_cut = path
            .moves
            .iter()
            .position(|m| m.kind != MoveKind::Rapid)
            .unwrap_or(0);
        let verdict = match cx.model.zip(cx.finish_tol) {
            Some((facets, tol)) => match crate::stock_sim::envelope_scan(facets, &path.moves, r, tol) {
                Ok(rep) => {
                    let gate = tol + rep.cell * rep.cell / (2.0 * r) + 1e-9;
                    if rep.gouge_mm > gate || rep.rest_mm > gate {
                        let mut at = 0usize;
                        let mut best = f64::INFINITY;
                        for (i, m) in path.moves.iter().enumerate() {
                            if m.kind == MoveKind::Rapid {
                                continue;
                            }
                            for (a, b) in m.chords(chord_tol) {
                                let d = dist2_seg(rep.at, a, b);
                                if d < best {
                                    best = d;
                                    at = i;
                                }
                            }
                        }
                        Some(Warning::Envelope {
                            at,
                            gouge_mm: rep.gouge_mm.clamp(0.0, 1e9),
                            rest_mm: rep.rest_mm.clamp(0.0, 1e9),
                        })
                    } else {
                        None
                    }
                }
                Err(_) => Some(Warning::Gouge { at: first_cut }),
            },
            None => Some(Warning::Gouge { at: first_cut }),
        };
        warns.extend(verdict);
    }

    warns
}
