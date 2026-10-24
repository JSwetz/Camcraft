//! Split from `machining2d.rs` by M5.0.1; structure only.

use crate::geo2d::{Loop, Point2, Region};

use super::face::GenError;
use super::ring::dist;
use super::slots::chord_clears;
/// Does the open chord `a -> b` properly cross the path through `pts`
/// (closed by a final segment back to `pts[0]` when `closed`)? Proper
/// means strictly inside both segments - a chord meeting the path at a
/// vertex, running along an edge, or grazing a corner is not a crossing.
///
/// This is deliberately exactly `verify`'s Crossing criterion restricted
/// to one chord against a polyline: crossing products are the same strict
/// sign tests on the same coordinates, so a chord this law accepts can
/// never come back as a verify warning, and a chord it rejects would
/// have. Join decisions made with it agree with the audit that judges
/// them afterwards.
pub(crate) fn chord_crosses_path(a: Point2, b: Point2, pts: &[Point2], closed: bool) -> bool {
    let (ux, uy) = (b.x - a.x, b.y - a.y);
    let n = pts.len();
    if ux == 0.0 && uy == 0.0 || n < 2 {
        return false;
    }
    let m = if closed { n } else { n - 1 };
    for i in 0..m {
        let (c, d) = (pts[i], pts[(i + 1) % n]);
        let s1 = ux * (c.y - a.y) - uy * (c.x - a.x);
        let s2 = ux * (d.y - a.y) - uy * (d.x - a.x);
        if s1 == 0.0 || s2 == 0.0 || (s1 > 0.0) == (s2 > 0.0) {
            continue;
        }
        let (vx, vy) = (d.x - c.x, d.y - c.y);
        let t1 = vx * (a.y - c.y) - vy * (a.x - c.x);
        let t2 = vx * (b.y - c.y) - vy * (b.x - c.x);
        if t1 != 0.0 && t2 != 0.0 && (t1 > 0.0) != (t2 > 0.0) {
            return true;
        }
    }
    false
}

/// Concentric rings marching inward: ring k is the cutter-center area
/// eroded by `k * stepover`, travelled closed. Offsets are taken from
/// `center` at absolute distances (never buffer-of-buffer), so micron
/// quantization cannot compound across rings.
///
/// Chaining the rings into as few passes as the floor allows is a
/// two-phase business. `chord_clears` proves a join chord stays inside
/// the cutter-center area, but on a holed region (an island, a boss) the
/// area's own loops run *through* it: a chord from the outer family to a
/// hole family crosses every ring in between, and `verify` will report
/// each crossing at the cut depth. Phase 1 therefore plans the whole
/// ring family of the level before a single join is made; phase 2
/// chains rings only along chords that clear the area AND cross none of
/// the level's planned rings nor any other join chord (chords built by
/// different chains can cross without touching any loop - the sweep in
/// phase 2 cuts such pairs into separate passes). Rings that cannot be
/// joined honestly run as separate passes, and the caller hops them
/// with rapids and a plunge - the same fallback `chord_clears` alone
/// already forced.
pub(super) fn waterline_passes(center: &Region, stepover: f64, climb: bool) -> Result<Vec<Vec<Point2>>, GenError> {
    let (lo, hi) = match center.bounds() {
        Some(b) => b,
        None => return Err(GenError::Empty("empty pocket region".into())),
    };
    // Erosion by more than the diagonal can never be non-empty; the cap
    // only fires if some Clipper path stops shrinking.
    let cap = (dist(lo, hi) / stepover + 10.0).max(16.0) as u32;
    // Phase 1: every loop of every ring, in (offset, |area| descending)
    // order - the nesting order, so a chain step's honest candidates
    // (the adjacent ring of the same family) come first.
    let mut loops: Vec<(u32, Vec<Point2>)> = Vec::new();
    let mut converged = false;
    for k in 0..=cap {
        let ring = if k == 0 {
            center.clone()
        } else {
            center.offset(-f64::from(k) * stepover)?
        };
        if ring.is_empty() {
            converged = true;
            break;
        }
        let mut ls: Vec<&Loop> = ring.loops().iter().collect();
        ls.sort_by(|a, b| b.area().abs().total_cmp(&a.area().abs()));
        for l in ls {
            let pts: Vec<Point2> = if climb {
                l.points().to_vec()
            } else {
                l.points().iter().rev().copied().collect()
            };
            if pts.len() >= 2 {
                loops.push((k, pts));
            }
        }
    }
    if !converged {
        return Err(GenError::Invalid(
            "waterline rings did not converge; check the stepover".into(),
        ));
    }
    // Phase 2: chain. A chord to a loop's entry point must clear the
    // region and properly cross neither any ring of the level nor any
    // chord of its own chain (earlier join chords are real cut paths
    // too; chords against loops is the global test, so the chain's
    // loops need no separate pass). Candidates are pruned to the
    // neighbouring offsets: a valid chord reaches only the adjacent
    // ring of its family, and anything farther would cross a ring in
    // between - refusing those by geometry, not by search, keeps the
    // level quadratic in rings, not cubic.
    //
    // One pair class cannot be judged at acceptance time: a chord built
    // by a chain seeded later may cross a chord of a finished chain
    // without touching any loop. On a boss, a ring family that shatters
    // into slivers links its entries with chords running along the
    // shattered ring's wall, and those cross the spiral's closing
    // chords - neither chord meets a loop edge anywhere, so acceptance
    // sees nothing. Hence chains are built first, every pair of
    // chords from different chains is then tested, and a crossing is
    // resolved by cutting the later-listed chord: the two loops it
    // linked end up in separate passes and the caller hops between
    // them with a rapid, which cuts nothing. Cutting cannot create a
    // crossing (chord sets only shrink), so this converges.
    let mut used = vec![false; loops.len()];
    let mut chains: Vec<Vec<usize>> = Vec::new();
    for i in 0..loops.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let mut chain = vec![i];
        while let Some(&last) = chain.last() {
            let prev = loops[last].1[0];
            let last_k = loops[last].0;
            let mine = chain_chords(&loops, &chain);
            let mut next = None;
            for j in 0..loops.len() {
                if used[j] || loops[j].0.abs_diff(last_k) > 1 {
                    continue;
                }
                let entry = loops[j].1[0];
                if !chord_clears(center, prev, entry)
                    || loops
                        .iter()
                        .any(|(_, l)| chord_crosses_path(prev, entry, l, true))
                    || mine.iter().any(|&(a, b)| chord_crosses_path(prev, entry, &[a, b], false))
                {
                    continue;
                }
                next = Some(j);
                break;
            }
            let Some(j) = next else { break };
            used[j] = true;
            chain.push(j);
        }
        chains.push(chain);
    }
    loop {
        let mut victim = None;
        let mut chords: Vec<(usize, usize, Point2, Point2)> = Vec::new();
        for (ci, ch) in chains.iter().enumerate() {
            for (w, &(a, b)) in chain_chords(&loops, ch).iter().enumerate() {
                chords.push((ci, w, a, b));
            }
        }
        'sweep: for x in 0..chords.len() {
            for y in x + 1..chords.len() {
                if chords[x].0 == chords[y].0 {
                    continue;
                }
                if chord_crosses_path(chords[x].2, chords[x].3, &[chords[y].2, chords[y].3], false) {
                    // Cut the later-listed chord: x < y in chain-then-
                    // join order, and chords inside one chain cannot
                    // cross by the acceptance test above.
                    victim = Some((chords[y].0, chords[y].1));
                    break 'sweep;
                }
            }
        }
        let Some((ci, w)) = victim else { break };
        let tail: Vec<usize> = chains[ci].split_off(w + 1);
        chains.push(tail);
    }
    let mut passes: Vec<Vec<Point2>> = Vec::new();
    for chain in &chains {
        let mut cur: Vec<Point2> = Vec::new();
        for &l in chain {
            cur.extend_from_slice(&loops[l].1);
            cur.push(loops[l].1[0]);
        }
        if cur.len() >= 2 {
            passes.push(cur);
        }
    }
    Ok(passes)
}

/// The join chords of a chain: one per consecutive loop pair, running
/// from the closing point of the earlier loop - its entry point - to
/// the entry point of the next. These are cut paths exactly like the
/// ring walks between them.
pub(super) fn chain_chords(loops: &[(u32, Vec<Point2>)], chain: &[usize]) -> Vec<(Point2, Point2)> {
    chain
        .windows(2)
        .map(|w| (loops[w[0]].1[0], loops[w[1]].1[0]))
        .collect()
}

/// Intersect the horizontal line `y` with `region`'s loops and pair the
/// crossings by even-odd parity: the cut intervals of one scanline.
/// Crossing rule matches `Region::contains` exactly (lower endpoint
/// inclusive); when the row is the region's top edge it flips to
/// upper-inclusive so the grazing wall pass still finds its intervals.
pub(super) fn scan_row(region: &Region, y: f64, upper_inclusive: bool) -> Vec<(f64, f64)> {
    let mut xs: Vec<f64> = Vec::new();
    for l in region.loops() {
        let pts = l.points();
        let n = pts.len();
        for i in 0..n {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            let (ylo, yhi) = if a.y <= b.y { (a.y, b.y) } else { (b.y, a.y) };
            let hit = if upper_inclusive {
                ylo < y && y <= yhi
            } else {
                ylo <= y && y < yhi
            };
            if hit {
                xs.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
            }
        }
    }
    xs.sort_by(f64::total_cmp);
    let mut out = Vec::new();
    let mut k = 0;
    while k + 1 < xs.len() {
        // A zero-width pair is a tangent vertex or the closing double
        // count of a vertex shared by two edges: nothing to cut.
        if xs[k + 1] - xs[k] > 1e-9 {
            out.push((xs[k], xs[k + 1]));
        }
        k += 2;
    }
    out
}
