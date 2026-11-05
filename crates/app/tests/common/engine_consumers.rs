//! M7.3 retirement march: the column engine's CONSUMERS, asked of
//! both arms. `stock_engine_agreement` proved the deviation verdict;
//! these are the other questions the column answers today - the
//! viewer's sheet, the `above(z)` masks the rest ladder and the
//! re-machining mask read, and the M2.1d feed sampler's per-move
//! engagement - each compared as a located finding, never rounded.
//! Provenance: our own engines, goldens and parity law (M7.1 C).

use camcraft_app::cl::{Move, MoveKind};
use camcraft_app::geo2d::{Point2, Region};
use camcraft_app::machining3d::engagement_on;
use camcraft_app::stock_sim::{Columns, SimOp, SimSurface, Stock, FIELD_VERDICT_EPS};

/// The sheets two arms paint must be the same quads, walls and
/// classes, every vertex within the declared epsilon.
pub fn compare_sheets(at: &str, c: &SimSurface, f: &SimSurface) -> Vec<String> {
    let mut out = Vec::new();
    if c.verts.len() != f.verts.len() || c.tris != f.tris {
        out.push(format!(
            "{at}: sheet shape {} verts / {} tris vs field {} / {}{}",
            c.verts.len(),
            c.tris.len(),
            f.verts.len(),
            f.tris.len(),
            first_divergence(c, f)
        ));
        return out;
    }
    let mut bad = 0usize;
    for (k, (a, b)) in c.verts.iter().zip(&f.verts).enumerate() {
        let d = (a.x - b.x).abs().max((a.y - b.y).abs()).max((a.z - b.z).abs());
        if d.is_nan() || d > FIELD_VERDICT_EPS || c.vcls[k] != f.vcls[k] {
            bad += 1;
            if bad <= 8 {
                out.push(format!(
                    "{at}: sheet vertex {k} ({:.4}, {:.4}, {:.6}) class {} vs field ({:.4}, {:.4}, {:.6}) class {}",
                    a.x, a.y, a.z, c.vcls[k], b.x, b.y, b.z, f.vcls[k]
                ));
            }
        }
    }
    if bad > 8 {
        out.push(format!("{at}: ... {bad} sheet vertices disagree in all"));
    }
    out
}

/// Where two sheets of different shape first part ways (the quad
/// order is the lattice walk on both arms, so the first unequal
/// vertex names the column).
fn first_divergence(c: &SimSurface, f: &SimSurface) -> String {
    for (k, (a, b)) in c.verts.iter().zip(&f.verts).enumerate() {
        let d = (a.x - b.x).abs().max((a.y - b.y).abs()).max((a.z - b.z).abs());
        if d.is_nan() || d > FIELD_VERDICT_EPS {
            return format!(
                "; first differ at vertex {k}: ({:.4}, {:.4}, {:.6}) vs ({:.4}, {:.4}, {:.6})",
                a.x, a.y, a.z, b.x, b.y, b.z
            );
        }
    }
    String::new()
}

/// The planes a mask is asked at: every distinct planar-feed Z the
/// program cut (the rest ladder's exact case - a floor asked at its
/// own height), at most `cap` of them spread across the run, plus the
/// floor plate and midway.
pub fn mask_planes(moves: &[Move], top: f64, floor: f64, cap: usize) -> Vec<f64> {
    let mut zs: Vec<f64> = moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.to.z - m.from.z).abs() <= 1e-6)
        .map(|m| m.to.z)
        .collect();
    zs.sort_by(f64::total_cmp);
    zs.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);
    let mut out: Vec<f64> = if zs.len() <= cap {
        zs
    } else {
        (0..cap).map(|i| zs[i * (zs.len() - 1) / (cap - 1).max(1)]).collect()
    };
    out.push(floor);
    out.push(0.5 * (floor + top));
    out
}

/// `above(z)` from both arms: the same cells, so the symmetric
/// difference has no area at all (cells are whole, so any
/// disagreement is at least one cell's area - nothing to round).
pub fn compare_masks(at: &str, c: &Columns<'_>, f: &Columns<'_>, z: f64) -> Vec<String> {
    let (rc, rf) = match (c.above(z), f.above(z)) {
        (Ok(a), Ok(b)) => (a, b),
        (a, b) => return vec![format!("{at}: above({z:.4}) column {:?} vs field {:?}", a.err(), b.err())],
    };
    let xor = |a: &Region, b: &Region| a.difference(b).map(|d| d.area()).unwrap_or(f64::NAN);
    let (lost, gained) = (xor(&rc, &rf), xor(&rf, &rc));
    let mut out = Vec::new();
    if !(lost <= 1e-9 && gained <= 1e-9) {
        // Locate it: the columns whose two tops land on opposite
        // sides of the plane.
        let mut cols = Vec::new();
        for iy in 0..c.ny().min(f.ny()) {
            for ix in 0..c.nx().min(f.nx()) {
                let (a, b) = (c.top_at(ix, iy), f.top_at(ix, iy));
                if (a > z) != (b > z) && cols.len() < 4 {
                    cols.push(format!("[{ix},{iy}] col {:+.3e} field {:+.3e} (from z)", a - z, b - z));
                }
            }
        }
        out.push(format!(
            "{at}: above({z:.4}) area column {:.4} field {:.4}, column-only {lost:.4} field-only {gained:.4}; {}",
            rc.area(),
            rf.area(),
            cols.join(", ")
        ));
    }
    out
}

/// Locate an engagement disagreement: the chord samples (the
/// sampler's own two stations, `2 * 8 + 1` points each) whose column
/// tops land on opposite sides of the sampler's `z + 1e-6` line.
fn split_samples(c: &Columns<'_>, f: &Columns<'_>, a: Point2, b: Point2, z: f64, r: f64) -> String {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dy);
    if len <= 1e-6 {
        return String::new();
    }
    let (nx, ny) = (-dy / len, dx / len);
    let mut out = Vec::new();
    for s in [Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5), Point2::new(a.x + dx * 0.75, a.y + dy * 0.75)] {
        for k in -8..=8 {
            let q = Point2::new(s.x + nx * k as f64 * r / 8.0, s.y + ny * k as f64 * r / 8.0);
            let (tc, tf) = (c.top_over(q), f.top_over(q));
            let hit = |t: Option<f64>| t.is_some_and(|t| t > z + 1e-6);
            if hit(tc) != hit(tf) {
                out.push(format!("q ({:.4}, {:.4}) col {:?} field {:?} cell {:?}", q.x, q.y, tc.map(|t| t - z), tf.map(|t| t - z), c.col(q)));
            }
        }
    }
    out.join(", ")
}

/// A planar feed - the moves the feed pass modulates (its own filter).
fn planar(m: &Move) -> bool {
    m.kind == MoveKind::Feed && m.arc.is_none() && (m.to.z - m.from.z).abs() <= 1e-6
}

/// The feed sampler replayed on both arms: each planar feed's
/// engagement measured against the stock standing the instant before
/// it, then the moves up to the next sample cut on both. The sampler
/// is quantized (34 chord points), so agreement is EXACT: equal
/// fractions, or a located finding. Returns (findings, sampled moves,
/// loaded moves).
pub fn compare_engagement(
    at: &str,
    col: &mut Stock,
    fld: &mut Stock,
    moves: &[Move],
    radius: f64,
    ball: bool,
) -> Result<(Vec<String>, usize, usize), String> {
    let mut out = Vec::new();
    let (mut sampled, mut loaded, mut cut_to) = (0usize, 0usize, 0usize);
    for (i, m) in moves.iter().enumerate() {
        if !planar(m) {
            continue;
        }
        // Bring both stocks to "just before move i".
        if cut_to < i {
            let ops = [SimOp { moves: &moves[cut_to..i], radius, ball, cone: None }];
            col.simulate(&ops).map_err(|e| format!("{at}: column cut: {e}"))?;
            fld.simulate(&ops).map_err(|e| format!("{at}: field cut: {e}"))?;
            cut_to = i;
        }
        let (cc, cf) = (col.columns().map_err(|e| e.to_string())?, fld.columns().map_err(|e| e.to_string())?);
        let (a, b) = (Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y));
        let (ec, ef) = (engagement_on(&cc, a, b, m.to.z, radius), engagement_on(&cf, a, b, m.to.z, radius));
        sampled += 1;
        if ec > 1e-6 {
            loaded += 1;
        }
        if ec != ef {
            out.push(format!(
                "{at}: move {i} ({:.3}, {:.3}, {:.3}) -> ({:.3}, {:.3}): engagement column {ec:.6} vs field {ef:.6}; {}",
                m.from.x, m.from.y, m.from.z, m.to.x, m.to.y,
                split_samples(&cc, &cf, a, b, m.to.z, radius)
            ));
        }
    }
    Ok((out, sampled, loaded))
}
