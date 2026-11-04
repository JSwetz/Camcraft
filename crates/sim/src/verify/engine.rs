//! Split from `verify.rs` by M5.0.1; structure only.

use super::types::{Mode, VerifyContext, Warning};
use super::laws::{bands_vs_fixtures, boundary_samples, dist2_seg, law_points, proper_cross, rapids_vs_fixtures, xy};

use std::collections::HashMap;
use crate::cl::{is_vertical, MoveKind, Toolpath};
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
    // Mode::Revolve stands it down too (M5.2 C): a turning path's
    // radial plunges all share the axis in XY, so every pair of
    // centers in a groove's ladder "crosses" at the very line the
    // part spins on - the XY reading of the predicate says nothing
    // true about a meridian path, which the revolve verdict judges.
    // Mode::Radial stands it down too (M5.2 F2): two live-tool holes
    // drilled at one height across one another ARE the cross-drilled
    // intersection the part asks for - the crossing point is kerf in
    // both bores, not a collision.
    if !matches!(cx.mode, Mode::Envelope | Mode::Revolve | Mode::Radial) {
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
    // The same membership asked as a question about one point, without
    // cloning the plane's region to ask it (M2.5 E): `surface`'s binary
    // search and `part_at` probe thousands of cached slices per program,
    // and a Region clone per probe turned a continuous-Z path (a flow
    // line rides a new plane with every chord) into a minutes-long walk.
    let island_holds = |facets: &[crate::geometry::Triangle], z: f64, q: Point2| -> bool {
        let key = z.to_bits();
        if let Some(hit) = island_cache.borrow().get(&key) {
            return hit.as_ref().is_some_and(|s| s.contains(q));
        }
        let fresh = crate::geometry::slice::material_above(facets, z).ok();
        let holds = fresh.as_ref().is_some_and(|s| s.contains(q));
        island_cache
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| fresh.clone());
        holds
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
    // swept ball instead. Mode::Revolve stands down for the same
    // reason read the other way: a lathe's centers sit ON the
    // section's silhouette by design - the tool tip traces the
    // meridian - so "inside the section" is where a turned path
    // legitimately lives; the revolve verdict owns that geometry.
    // Mode::Radial stands down the same way read again (M5.2 F2): a
    // radial plunge's centers RUN THROUGH the silhouette wall - head
    // on, d = 0, every feed point of the bore inside the finish
    // circle it came to make. Tangency is the mill's law; the door
    // the kernel measured on the mesh is the radial one.
    // Mode::Cone stands down on the same reading from the vertical
    // mill's side (M2.5 E B): a chamfer ride's axis runs ON the wall
    // loop it rules (d = 0 by construction) and a deburr's tip
    // stands inside the corner material the ride exists to remove -
    // both facts are the generatrix law, not near misses. The reach
    // laws stay, unlike Engrave's: the cone's shank is wider than
    // the cone and the wall beside the edge is steel.
    if !matches!(
        cx.mode,
        Mode::Engrave | Mode::Envelope | Mode::Revolve | Mode::Radial | Mode::Cone
    ) {
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
        // The island clause is a monotone threshold in z, not a scan
        // (M2.5 E): `q` sits inside the plane-z island exactly while
        // z < h(q) - the slice IS the part above z - so below the
        // threshold every chord fails the island test whatever its
        // reach, and above it every chord clears iff it reaches. The
        // first success is therefore the first in-reach chord at or
        // above the threshold: binary-search the threshold (log probed
        // planes, no clones - `island_holds`), then walk upward. The
        // old deepest-first `find` returns the same chord, move for
        // move; it just walked every deep chord's island to learn it.
        let mut lo = 0usize;
        let mut hi = cut.len();
        if let Some(f) = cx.model {
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                if island_holds(f, cut[mid].2, q) {
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }
        }
        // The upward test keeps the island clause: above the threshold
        // it never fires (that is the threshold's meaning), so this is
        // the old predicate restated, not an approximation of it.
        let cleared = cut[lo..]
            .iter()
            .find(|(a, b, z)| {
                dist2_seg(q, *a, *b) <= r2
                    && cx.model
                        .is_none_or(|f| !island_holds(f, *z, q))
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

    // (c) Shank/holder reach against rest material. Mode::Radial
    // stands it down (M5.2 F2) for the reason Revolve's doctrine
    // names inverted: a horizontal shank lives in the meridian's
    // world, and the enclosure built for it is a vertical column -
    // it over-reports a graze into the bore's own walls, steel the
    // door law already measured as the hole. (c3)'s sweeps ride the
    // same column and stand with it; the kernel's chain self-check
    // and door margins are the band law here.
    if !matches!(cx.mode, Mode::Engrave | Mode::Radial) {
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
        let deepest_island: Option<Option<Region>> =
            cx.model.map(|f| islands_at(f, z_deepest));
        let part_at = |q: Point2| -> bool {
            !cx.region.contains(q)
                || deepest_island
                    .as_ref()
                    .and_then(|o| o.as_ref())
                    .is_some_and(|s| s.contains(q))
        };
        // Pose (M5.1 stage A): a leaning band is enclosed, exactly as
        // in [`super::bands_vs_fixtures`], in a vertical column that is
        // never smaller - shifted to the band's mid-height point on the
        // leaning axis, widened by its half-length's horizontal spread,
        // its bottom standing at the lower of the two axis-plane ends
        // minus the disc radius. The annulus's inner edge stays about
        // the TIP: the flute's kerf excuses that radius wherever the
        // tip cuts, tilt or no tilt. Over-report a graze, never hide a
        // gouge. The prismatic branch is the law's own history: same
        // h0, same r2, point p unshifted.
        // Moves law (c) convicted, for law (c3) below: a mid-turn
        // sweep of the same band on the same move is the same event
        // reported twice, and (c)'s verdict stands.
        let mut shanked: Vec<usize> = Vec::new();
        for (i, m) in path.moves.iter().enumerate() {
            if m.kind == MoveKind::Rapid {
                continue;
            }
            let pts = law_points(m, chord_tol);
            let lean = m.axis.filter(|a| !is_vertical(a));
            for &(above, band_r, band_len) in &bands {
                if band_r <= r || band_len <= 0.0 {
                    continue;
                }
                let (offx, offy, r_band, h0) = match lean {
                    None => (0.0, 0.0, band_r, m.to.z + above),
                    Some(a) => {
                        let mid = above + band_len * 0.5;
                        let spread = (band_len * 0.5) * (a[0] * a[0] + a[1] * a[1]).sqrt();
                        let za = m.to.z + above * a[2];
                        let zb = m.to.z + (above + band_len) * a[2];
                        (mid * a[0], mid * a[1], band_r + spread, za.min(zb) - band_r)
                    }
                };
                // Z-window skip (M2.5 E): the law convicts only where
                // `h0 < surface(q)`, and no column's surface stands
                // above the stock top (`surface` ends in that `min`,
                // and a cleared column cannot exceed the stock it was
                // cut from). A band whose bottom stands at or above
                // the ceiling has nothing to meet in any column -
                // this is the same predicate read one step earlier,
                // not an approximation: the old loop asked every
                // sample's annulus to learn the answer it could see
                // in the band's own numbers. A continuous-Z path (a
                // flow line) multiplies that blind ask by its plane
                // count; on a level ladder the test rarely fires.
                if h0 >= cx.stock_top_z {
                    continue;
                }
                let r2 = (r_band + 1e-9) * (r_band + 1e-9);
                let mut hit = None;
                'worst: {
                    for p in &pts {
                        let pc = Point2::new(p.x + offx, p.y + offy);
                        for q in &samples {
                            let dd = dist2_seg(*q, pc, pc);
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
                    shanked.push(i);
                    warns.push(Warning::ShankHit { at: i, part });
                }
            }
        }

        // (c3) The mid-turn sweep (M5.1 stage G): the reachable set
        // between a move's commanded poses, on the grid and column
        // laws of [`super::sweep`]. It runs exactly where its twin (c)
        // runs - the same domain, the turn-aware reading - and the
        // price stays honest because every posed kind verifies as
        // Envelope: law (f) measures the BALL, rotation-invariant and
        // pose-blind, and the POSE question of those programs is the
        // band stack's, (c) and (c3) together. The sweep's per-move
        // sample-union filter keeps the grid from costing twenty times
        // on far-field samples. Engrave never poses. Paths no posed
        // generator ever touched hold every pose at home and convict
        // nothing, exactly as before this law existed.
        if cx.mode != Mode::Engrave {
            warns.extend(super::sweep::mid_turn_sweeps(
                path,
                &bands,
                r,
                cx.gouge_tol,
                chord_tol,
                &shanked,
                &samples,
                &in_stock,
                &surface,
                &part_at,
            ));
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
    // Mode::Revolve stands this law down (M5.2 D): the XY column is the
    // mill's geometry - a turning insert is a POINT in XY and its whole
    // form lives in the meridian plane. The pull-out from a thread's
    // last V slides along flanks that coincide with the cut's own
    // surfaces, yet the column above the tip circle is solid core
    // forever - so this law convicts every full-depth threading retract
    // for a collision the meridian scan knows cannot happen. The
    // turning rapids' safety promise is the kernel's own: radial exits
    // from cut stations and traverses at rho_clear, asserted by each
    // family's tests. Fixture steel (d2) still stands - it is a box.
    // Mode::Radial stands down on the same reading (M5.2 F2): the
    // bore is a tunnel a height field cannot see, so the retract
    // sliding out along its own kerf reads as lateral travel under
    // solid stock; the exit-the-hole-along-its-axis and outside-
    // stock_r traverse promises are the kernel's, asserted in
    // drilling's cross tests.
    if !matches!(cx.mode, Mode::Revolve | Mode::Radial) {
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
    // facet truth (M5.2 E), model surface standing inside the swept
    // envelope - the exact distance of each triangle to each ball
    // center, honest on tilts, walls and under-passes where a column
    // floor was none of those; `rest` is distance truth, model
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
        // The envelope ball is the CUT CIRCLE the path rolls: a
        // ground ball's great circle - the tool radius - or a turning
        // insert's smaller nose circle when the tool states one
        // (M5.2 A; `ToolGeom::nose_radius` is 0.0 for every mill
        // kind, so this reads unchanged for all pre-turning paths).
        // A stated nose radius marks the turning convention too: the
        // generator walked the nose CENTER, so the moves are centers
        // already and the scan must not lift them (M5.2 E; a `+r`
        // tilt-lift on an outward-facing cone reads a gouge of
        // r - r/v2 that no ball ever cut).
        let nose_centers = cx.tool.nose_radius.is_finite() && cx.tool.nose_radius > 0.0;
        let r = if nose_centers {
            cx.tool.nose_radius
        } else {
            cx.tool.radius
        };
        let first_cut = path
            .moves
            .iter()
            .position(|m| m.kind != MoveKind::Rapid)
            .unwrap_or(0);
        let verdict = match cx.model.zip(cx.finish_tol) {
            Some((facets, tol)) => match if nose_centers {
                crate::stock_sim::envelope_scan_centers(facets, &path.moves, r, tol)
            } else {
                crate::stock_sim::envelope_scan(facets, &path.moves, r, tol)
            } {
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
