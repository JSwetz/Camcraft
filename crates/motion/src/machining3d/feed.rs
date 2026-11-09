//! Split from `machining3d.rs` by M5.0.1; structure only.

use super::params::{CHORD_TOL, Pocket3DParams};
use super::rest::EPS;

use crate::cl::{Move, MoveKind, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::stock_sim::{self, Dexel};
/// How far below the programmed feed the modulation may take a planar
/// cut move. One quarter is a handbook-style floor for a roughing
/// bite: below it the chip is too thin to evacuate in steel and the
/// slow feed buys nothing the floor pass will not cut anyway. It is a
/// law constant, not a parameter - a path with bites needing less
/// than this wants a smaller stepover, said the strategy, not a
/// crawl said the feed.
pub const FFO_FLOOR: f64 = 0.25;

/// Radial engagement of one planar feed move, as a fraction of the
/// tool diameter (0 = air, 1 = full bite): the perpendicular chord
/// through the move's midpoint, sampled at `2 * HALF + 1` points,
/// read against the sim's standing-material field BEFORE the move
/// cuts it. Sampling the instantaneous field is the whole point - a
/// static per-level region would report the entire pocket cross-section
/// as material and slow every ring to the floor. A move too short to
/// have a direction is treated as a full bite: slowing down is the
/// safe read of an ambiguity. Grid-quantized by construction (the
/// same dexel law everywhere: measure, never derive, from the sim).
///
/// The chord is sampled at two stations - mid-move and move-end - and
/// averaged: the trapezoid of the swept side width. A midpoint-only
/// chord misreads a stepover link, whose fresh material lies *ahead*
/// along travel rather than beside it, as zero engagement; averaging
/// the two stations puts a half-engaged link at ~0.5, which is what
/// it cuts. A move that engages nothing at either station - the
/// zero-depth skin of the first level, air moves - reads exactly 0
/// and is held at the programmed feed (there is no load to pay for).
pub(super) fn engagement(dex: &Dexel, from: Point2, to: Point2, z: f64, r: f64) -> f64 {
    sample_engagement(|q| dex.col(q).map(|i| dex.top_at(i)), from, to, z, r)
}

/// The same sampler read off either stock engine (M7.3): the seam's
/// column reader answers `col`-then-`top_at` on the column arm and the
/// verdict march on the field arm, so the heavy agreement lane asks
/// both engines this law's question and demands one answer. Not on
/// the feed pass's path yet - the column arm still owns `optimize_feed`.
#[must_use]
pub fn engagement_on(cols: &stock_sim::Columns<'_>, from: Point2, to: Point2, z: f64, r: f64) -> f64 {
    sample_engagement(|q| cols.top_over(q), from, to, z, r)
}

/// The chord law of [`engagement`], over any column-top source:
/// `top(q)` is the top standing in `q`'s column, None off the grid.
fn sample_engagement(top: impl Fn(Point2) -> Option<f64>, from: Point2, to: Point2, z: f64, r: f64) -> f64 {
    const HALF: i32 = 8; // r/8 chord resolution, one lattice step per side
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len <= EPS {
        return 1.0;
    }
    // Chords normal to travel: width of cut is measured across the
    // feed direction, along it the tool is a line.
    let (nx, ny) = (-dy / len, dx / len);
    let stations = [
        Point2::new((from.x + to.x) * 0.5, (from.y + to.y) * 0.5),
        Point2::new(from.x + dx * 0.75, from.y + dy * 0.75),
    ];
    let mut acc = 0.0;
    for s in stations {
        let mut hit = 0usize;
        let mut total = 0usize;
        for k in -HALF..=HALF {
            let q = Point2::new(
                s.x + nx * (k as f64) * r / HALF as f64,
                s.y + ny * (k as f64) * r / HALF as f64,
            );
            total += 1;
            if let Some(t) = top(q)
                && t > z + EPS
            {
                hit += 1;
            }
        }
        acc += (hit as f64 / total as f64).clamp(0.0, 1.0);
    }
    (acc * 0.5).clamp(0.0, 1.0)
}

/// The M2.1d feed pass: run the finished path through the sim once,
/// measuring each planar feed move's engagement against the material
/// standing the instant before it, then scale its feed by
/// `median / engagement`, clamped to `[FFO_FLOOR, 1]` of the
/// programmed feed. The median is the self-calibration: whatever the
/// strategy's nominal load, that is what runs at the programmed feed,
/// and only the peaks (full bites, corners, the first ring of a
/// level) are slowed. Never faster, never past the programmed feed -
/// within tool limits, where the op's feed word is the limit. The
/// geometry is untouched: same moves, same order, same points; a
/// law test pins that. Vertical feeds (plunges, ramps, helices) keep
/// their `plunge_feed`/`feed` as assigned - axial engagement is a
/// different curve we do not model here (decision, RECOMMENDED).
pub fn optimize_feed(
    path: &Toolpath,
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
) -> Result<Toolpath, GenError> {
    p.validate()?;
    refuse_posed(&path.moves)?;
    let r = p.tool_diameter * 0.5;
    let (mut dex, _) = sim_of(facets, footprint, p)?;
    let mut moves = path.moves.clone();
    // (index, engagement) of the planar feed moves, measured before
    // the sim advances past them.
    let mut bites: Vec<(usize, f64)> = Vec::new();
    for (i, m) in moves.iter().enumerate() {
        if m.kind == MoveKind::Feed && m.arc.is_none() && (m.to.z - m.from.z).abs() <= EPS {
            bites.push((
                i,
                engagement(
                    &dex,
                    Point2::new(m.from.x, m.from.y),
                    Point2::new(m.to.x, m.to.y),
                    m.to.z,
                    r,
                ),
            ));
        }
        dex.subtract(m, r, CHORD_TOL);
    }
    if !bites.is_empty() {
        // The anchor is the median of the *loaded* bites only: an
        // air cut or a zero-depth skin reading 0 must not drag the
        // whole job's reference load down to nothing (that would
        // silently switch the optimization off - the failure mode a
        // first-run histogram caught). Zero bites stay at the
        // programmed feed below, which is the honest answer: no
        // load, no reason to slow.
        let mut ae: Vec<f64> = bites.iter().map(|&(_, a)| a).filter(|a| *a > EPS).collect();
        if !ae.is_empty() {
            ae.sort_by(|a, b| a.total_cmp(b));
            // Middle of the sorted run; an even count straddles the
            // two central samples and averages them - one value, no
            // branch.
            let mid = ae.len() / 2;
            let median = if ae.len() % 2 == 1 {
                ae[mid]
            } else {
                (ae[mid - 1] + ae[mid]) * 0.5
            };
            for &(i, a) in &bites {
                let ratio = if a <= EPS { 1.0 } else { (median / a).clamp(FFO_FLOOR, 1.0) };
                moves[i].feed = Some(p.feed * ratio);
            }
        }
    }
    let out = Toolpath {
        moves,
        cycles: path.cycles.clone(),
        notes: path.notes.clone(),
        comps: path.comps.clone(),
        threads: path.threads.clone(),
        probes: path.probes.clone(),
        breaks: path.breaks.clone(),
    };
    if !out.all_finite() {
        return Err(GenError::Invalid("feed modulation broke the finite horizon".into()));
    }
    Ok(out)
}

pub(super) fn home(p: &Pocket3DParams) -> Point3 {
    Point3::new(0.0, 0.0, p.clearance_z)
}

pub(super) fn guard_input(facets: &[Triangle], footprint: &Region, p: &Pocket3DParams) -> Result<(), GenError> {
    p.validate()?;
    if facets.is_empty() {
        return Err(GenError::Empty("no model facets to section".into()));
    }
    if footprint.is_empty() {
        return Err(GenError::Empty("empty pocket footprint".into()));
    }
    Ok(())
}

/// The sim for this operation: the footprint's XY box, cells fine
/// enough to see both the tool and the pass gaps (under `Adaptive`
/// those gaps are the engagement-law step, so the grid follows it -
/// `tiling_step`), the stock face at the cut top. A part that stands taller than the cut top is part
/// of the stock there (the setup's stock contains its model, or the
/// operation's span does not reach it - either way the column starts
/// full, never half-invented).
pub(super) fn sim_of(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
) -> Result<(Dexel, Vec<f64>), GenError> {
    let (lo, hi) = footprint
        .bounds()
        .ok_or_else(|| GenError::Invalid("footprint has no bounds".into()))?;
    let cell = (p.tool_diameter * 0.5).min(p.tiling_step() * 0.5);
    let mut dex = Dexel::block(lo.x, lo.y, hi.x, hi.y, p.top_z, cell)?;
    let part = stock_sim::part_surface(facets, dex.x0, dex.y0, dex.cell, dex.nx, dex.ny);
    for (k, &pt) in part.iter().enumerate() {
        if pt > dex.top_at(k) {
            dex.set(k, pt);
        }
    }
    Ok((dex, part))
}

/// The column engine's door, kept at motion's direct `Dexel::subtract`
/// calls too (M13.7): the dexel atom stands upright on +Z, so a posed
/// cutting move lowered through it is a tilted tool cut vertically - a
/// silent wrong stock. Refused by name with the engine's own text
/// ([`stock_sim::POSED_REFUSAL`]) before a single column is touched.
pub(super) fn refuse_posed(moves: &[Move]) -> Result<(), GenError> {
    match stock_sim::first_posed(moves) {
        Some(k) => Err(GenError::Invalid(format!("move {k}: {}", stock_sim::POSED_REFUSAL))),
        None => Ok(()),
    }
}

/// Lower `dex` by every move of `moves` with the upright flat tool of
/// radius `r`, after [`refuse_posed`] - the only way motion subtracts.
pub(super) fn subtract_upright(dex: &mut Dexel, moves: &[Move], r: f64) -> Result<(), GenError> {
    refuse_posed(moves)?;
    for m in moves {
        dex.subtract(m, r, CHORD_TOL);
    }
    Ok(())
}
