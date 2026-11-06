//! Laws (c), (c3) and (e), shank/holder reach against rest material, the mid-turn sweep and cycle reach - the shop practice of comparing cutter reach against wall height (machine-tool handbooks) on our own conservative rest model (split from `engine.rs` by M13.7; structure only).

use super::laws::{boundary_samples, dist2_seg, law_points};
use super::rest::Rest;
use super::types::{Mode, VerifyContext, Warning};

use crate::cl::{is_vertical, MoveKind, Toolpath};
use crate::geo2d::{Point2, Region};

/// (c) Shank/holder reach against rest material, then (c3) its
/// mid-turn sweep twin. Mode::Radial stands it down (M5.2 F2) for
/// the reason Revolve's doctrine names inverted: a horizontal shank
/// lives in the meridian's world, and the enclosure built for it is
/// a vertical column - it over-reports a graze into the bore's own
/// walls, steel the door law already measured as the hole. (c3)'s
/// sweeps ride the same column and stand with it; the kernel's
/// chain self-check and door margins are the band law here.
pub(super) fn band_reach(
    path: &Toolpath,
    cx: &VerifyContext,
    rest: &Rest,
    chord_tol: f64,
    warns: &mut Vec<Warning>,
) {
    let r = cx.tool.radius;
    // Deepest plane the path cuts at (drives the island law of the
    // reach samples and the part label below).
    let z_deepest = path
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .map(|m| m.from.z.min(m.to.z))
        .fold(f64::INFINITY, f64::min);

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

    if !matches!(cx.mode, Mode::Engrave | Mode::Radial | Mode::Rotary) {
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
            let mut zs: Vec<u64> = rest.cut().iter().map(|(_, _, z)| z.to_bits()).collect();
            zs.sort_unstable();
            zs.dedup();
            for k in zs {
                if let Some(s) = rest.islands_at(f, f64::from_bits(k)) {
                    samples.extend(boundary_samples(&s));
                }
            }
        }
        let deepest_island: Option<Option<Region>> =
            cx.model.map(|f| rest.islands_at(f, z_deepest));
        let part_at = |q: Point2| -> bool {
            !cx.region.contains(q)
                || deepest_island
                    .as_ref()
                    .and_then(|o| o.as_ref())
                    .is_some_and(|s| s.contains(q))
        };
        let in_stock = |q: Point2| -> bool { rest.in_stock(q) };
        let surface = |q: Point2| -> f64 { rest.surface(q) };
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
}

/// (e) Cycle reach: a hole deeper than the tool assembly reaches is
/// a shank collision, reported at the hole's first move. The law
/// reads the cycle list on purpose - a vertical plunge at a matched
/// drill sweeps exactly its own disc, so every rim sample sits inside
/// the flute radius and law (c) has nothing to sample in the annulus;
/// the plate top around the hole is part, so `part` is true. An
/// oversized hole deeper than full reach is visible to (c) as well;
/// the same-move report is not repeated.
/// No mode gate: only drilling paths ever carry cycles, so this is a
/// no-op everywhere else.
pub(super) fn cycle_reach(path: &Toolpath, cx: &VerifyContext, warns: &mut Vec<Warning>) {
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
}
