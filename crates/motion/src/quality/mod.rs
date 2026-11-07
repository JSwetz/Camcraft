//! Path quality, measured (M11.4 C): what a roughing path actually does
//! to the tool, read off the stock simulation - never off the planner's
//! nominal step. Per path: the radial engagement (contact angle) along
//! every planar cut, sampled against the material standing the instant
//! before each sample cuts; its maximum, where it happened and its
//! distribution; the curvature breaks (tangent changes past a threshold
//! while the tool is in material) and the tightest in-cut curvature;
//! the air-cut length and time; the retract count.
//!
//! Provenance: the engagement (immersion) angle of peripheral milling
//! as the machining-dynamics literature defines it (Altintas,
//! *Manufacturing Automation*, ch. 2: the arc of the cutter periphery
//! in contact with the workpiece; for a straight wall
//! `acos(1 - 2 ae/D)`), sampled on the existing dexel stock engine
//! (`camcraft_sim::stock_sim::Dexel`, public API only) the way
//! simulation-based engagement studies measure it (e.g. Kloypayan &
//! Lee 2002, material engagement from an in-process stock model). The
//! rim is read a cell and a half inside the cutter (the grid never reads
//! its own cut edge). The sampling law, thresholds and
//! report are ours; no vendor material.

mod report;
mod shape;
#[cfg(test)]
mod tests;

use crate::cl::{Move, MoveKind, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
use crate::stock_sim::{Dexel, StockError};

pub use report::{PathQuality, QualitySpec};
pub use shape::tangents;

/// Height slack for "material stands above this cut": a micron.
const Z_EPS: f64 = 1e-3;

/// Chord sagitta the measurement walks arcs at (mm): the finest
/// tessellation a plan posts (the HSM plan's own chord law,
/// `adaptive::morph`), so an arc is read at least as finely as the
/// chords it replaced. The reading at a sample stands on the stock
/// around it, never on the pitch - but the max over a path is the max
/// over its samples, and a coarser walk on an arc skips the sample the
/// plan's chords would have read (M11.4 arc output: the bore's peak,
/// 0.3 mm past the helix, read 17.3 % at a 2 um walk and 19.5 % at this
/// one on the D/60 grid, the chord plan's own number).
const CHORD_TOL: f64 = crate::machining2d::PLAN_SAG;

/// Contact-angle samples around the cutter periphery.
pub(crate) const RIM_SAMPLES: usize = 360;

/// The measuring stock a planar pocket is judged against: material at
/// `top_z` over `region` only (the cells outside it start below `floor`,
/// because the walls are the part, which a pass grazes but never loads), on
/// the spec's grid over the bounds grown by `margin`.
pub fn stock_for(region: &Region, top_z: f64, floor: f64, margin: f64, spec: &QualitySpec) -> Result<Dexel, StockError> {
    let Some((lo, hi)) = region.bounds() else {
        return Err(StockError::Invalid("empty region has no stock".into()));
    };
    let mut dex = Dexel::block(lo.x - margin, lo.y - margin, hi.x + margin, hi.y + margin, top_z, spec.cell)?;
    let below = floor - 1.0;
    for iy in 0..dex.ny {
        for ix in 0..dex.nx {
            if !region.contains(dex.center(ix, iy)) {
                dex.set(ix + iy * dex.nx, below);
            }
        }
    }
    Ok(dex)
}

/// Area (mm^2) of the cells whose centers lie inside `region` with
/// material still standing above `z`: the rest the path left there.
#[must_use]
pub fn rest_area(dex: &Dexel, region: &Region, z: f64) -> f64 {
    let mut n = 0usize;
    for iy in 0..dex.ny {
        for ix in 0..dex.nx {
            if dex.top_at(ix + iy * dex.nx) > z + Z_EPS && region.contains(dex.center(ix, iy)) {
                n += 1;
            }
        }
    }
    n as f64 * dex.cell * dex.cell
}

/// The radius the contact angle is read on: a cell and a half OUTSIDE
/// the cutter, on the half facing the motion. Out there, the stock the
/// cutter is touching right now still stands (no earlier position of it
/// reached that far ahead), so the reading does not depend on how
/// finely the path is sampled - the arc from the wall it rides to where
/// the uncut stock begins: `acos((r - ae) / rim)` on a straight wall.
#[must_use]
pub fn rim_for(tool_diameter: f64, cell: f64) -> f64 {
    let r = tool_diameter * 0.5;
    r + 1.5 * cell
}

/// Run `path` through `dex` (cutting it as it goes) and measure it.
/// The stock is the caller's: pass a fresh block, read the rest after.
pub fn measure(path: &Toolpath, dex: &mut Dexel, spec: &QualitySpec) -> PathQuality {
    let r = spec.tool_diameter * 0.5;
    let rim = rim_for(spec.tool_diameter, dex.cell);
    let step = (r / 10.0).max(dex.cell);
    let cut_deg = crate::load::deg_at_rim(spec.in_cut_load, spec.tool_diameter, rim);
    let mut q = PathQuality { rim, diameter: spec.tool_diameter, ..PathQuality::default() };
    // The path's named entry bores: every helical descent's circle. A
    // planar cut whose center stays on or inside one is the bore's own
    // cleanup (the descent's last pitch, the orbit it closes on) and is
    // read as entry engagement, not as the clearing's load.
    let bores: Vec<(Point2, f64)> = path
        .moves
        .iter()
        .filter(|m| (m.to.z - m.from.z).abs() > 1e-9)
        .filter_map(|m| {
            let a = m.arc.filter(|a| a.plane.is_xy())?;
            Some((a.center, (m.from.x - a.center.x).hypot(m.from.y - a.center.y)))
        })
        .collect();
    // Per move: engagement at its first and last sample (for breaks).
    let mut ends: Vec<(f64, f64)> = Vec::with_capacity(path.moves.len());
    // (in a descent, it bit stock)
    let mut desc = (false, false);
    // The floor being cut: the lowest plane a planar feed bit since the
    // last rapid. A planar feed above it in air is a lifted return
    // (`lift_len`), not floor air.
    let mut plane: Option<f64> = None;
    for (mi, m) in path.moves.iter().enumerate() {
        if m.kind == MoveKind::Rapid && desc.0 {
            q.descents += usize::from(desc.1);
            desc = (false, false);
        }
        if m.kind == MoveKind::Rapid {
            plane = None;
            let d = dist3(m.from, m.to);
            q.rapid_len += d;
            q.air_time_s += d / spec.rapid_rate * 60.0;
            if m.to.z > m.from.z + 1e-9 {
                q.retracts += 1;
            }
            ends.push((0.0, 0.0));
            q.move_max.push(0.0);
            q.move_air.push(0.0);
            continue;
        }
        let planar = (m.to.z - m.from.z).abs() <= 1e-9;
        // A descent is a run of moves going down; it counts once if it
        // bit stock anywhere on the way (the descents-per-level law).
        let down = m.to.z < m.from.z - 1e-9;
        if down && !desc.0 {
            desc = (true, false);
        } else if !down && desc.0 {
            q.descents += usize::from(desc.1);
            desc = (false, false);
        }
        let feed = m.feed.unwrap_or(1.0).max(1e-6);
        let (mut first, mut last, mut peak) = (None, 0.0f64, 0.0f64);
        let air0 = q.air_len;
        let lifted = planar && plane.is_some_and(|z| m.to.z > z + 1e-6);
        let mut bit = false;
        // The chord leaving this move's end, for the vertex heading.
        let out = path.moves.get(mi + 1).and_then(|nx| leaving(m, nx));
        for (a, b) in m.segments3(CHORD_TOL) {
            let len = dist3(a, b);
            let n = ((len / step).ceil() as usize).clamp(1, 1 << 16);
            let mut prev = a;
            for k in 1..=n {
                let t = k as f64 / n as f64;
                let p = Point3::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t);
                let mut dir = heading(m, p, prev);
                if k == n && m.arc.is_none() {
                    dir = vertex_heading((b.x - a.x, b.y - a.y), out, spec.break_deg).unwrap_or(dir);
                }
                let deg = contact_deg(dex, p, dir, rim, spec.wall_below);
                let piece = len / n as f64;
                let bore = bores.iter().any(|&(c, rr)| (p.x - c.x).hypot(p.y - c.y) <= rr + 1e-6);
                if planar && m.kind == MoveKind::Feed && !bore {
                    q.hist[((deg / 10.0).floor() as usize).min(18)] += 1;
                    if deg > q.max_deg {
                        q.max_deg = deg;
                        q.max_at = p;
                    }
                    if deg >= 170.0 {
                        q.slot_len += piece;
                    }
                    if deg > cut_deg {
                        q.cut_len += piece;
                        bit = true;
                    } else if lifted {
                        q.lift_len += piece;
                        q.air_time_s += piece / feed * 60.0;
                    } else {
                        q.air_len += piece;
                        q.air_time_s += piece / feed * 60.0;
                        q.air_at.push(p);
                    }
                } else {
                    q.entry_max_deg = q.entry_max_deg.max(deg);
                    if m.to.z < m.from.z - 1e-9 && deg > cut_deg {
                        desc.1 = true;
                    }
                }
                // Inside a named bore the joint law stands down too.
                let joint = if bore { 0.0 } else { deg };
                first.get_or_insert(joint);
                last = joint;
                peak = peak.max(deg);
                dex.cut_chord(prev, p, r);
                prev = p;
            }
        }
        if bit {
            plane = Some(plane.map_or(m.to.z, |p: f64| p.min(m.to.z)));
        }
        ends.push((first.unwrap_or(0.0), last));
        q.move_max.push(peak);
        q.move_air.push(q.air_len - air0);
    }
    if desc.1 {
        q.descents += 1;
    }
    shape::breaks(&path.moves, &ends, (spec.break_deg, cut_deg), &mut q);
    q
}

/// The tool's heading at sample `p` of `m`, the direction its contact
/// window faces: on an XY arc (planar or helical) the circle's tangent
/// there; on a straight, the chord walked (`prev -> p`; at the chord's
/// end vertex `vertex_heading` turns it onto the curve). The chord of
/// an arc's walk lags the tangent by half its angular step, and the
/// window turned that far backward reads standing stock behind the
/// perpendicular that the cutter never touches - a 0.26 mm walk at a
/// 2.3 mm radius over-read the HSM bore by 1.7 % D (M11.4 arc output)
/// against the same plan posted as chords. The true heading reads an
/// arc exactly as the chords it replaced, at any walk pitch.
fn heading(m: &Move, p: Point3, prev: Point3) -> (f64, f64) {
    match m.arc.filter(|a| a.plane.is_xy()) {
        Some(arc) => {
            let (dx, dy) = (p.x - arc.center.x, p.y - arc.center.y);
            if !(dx.is_finite() && dy.is_finite()) || dx.hypot(dy) <= 1e-12 {
                return (p.x - prev.x, p.y - prev.y);
            }
            if arc.ccw { (-dy, dx) } else { (dy, -dx) }
        }
        None => (p.x - prev.x, p.y - prev.y),
    }
}

/// The XY direction a planar straight feed `m` hands on to the next
/// move `nx` at its end: `nx`'s start tangent when `nx` continues it (a
/// planar feed at the same height starting where `m` ends), else `None`.
fn leaving(m: &Move, nx: &Move) -> Option<(f64, f64)> {
    let joined = (nx.from.x - m.to.x).hypot(nx.from.y - m.to.y) <= 1e-6 && (nx.from.z - m.to.z).abs() <= 1e-9;
    if !(planar_feed(m) && planar_feed(nx) && joined) {
        return None;
    }
    let (t0, _) = shape::tangents(nx)?;
    Some((t0.x, t0.y))
}

/// The heading at the vertex ending a straight chord `inc` (issue #10):
/// a chord-posted curve's tangent there is the bisector of the chord
/// arriving and the one leaving (the centred difference), where the
/// arriving chord alone lags it by half the turn - the same backward
/// window the arc heading fixed (M11.4 arc output), read here on chord
/// plans. A turn past `limit` (the break law's threshold) is a corner,
/// not a tessellated curve, and keeps the arriving chord; so do the
/// ends of a pass (`out` = `None`).
fn vertex_heading(inc: (f64, f64), out: Option<(f64, f64)>, limit: f64) -> Option<(f64, f64)> {
    let (ox, oy) = out?;
    let (li, lo) = (inc.0.hypot(inc.1), ox.hypot(oy));
    if !(li > 1e-12 && lo > 1e-12 && li.is_finite() && lo.is_finite()) {
        return None;
    }
    let (ix, iy, ox, oy) = (inc.0 / li, inc.1 / li, ox / lo, oy / lo);
    let turn = (ix * ox + iy * oy).clamp(-1.0, 1.0).acos().to_degrees();
    // A NaN limit reads as a corner: the arriving chord stays.
    (turn <= limit).then_some((ix + ox, iy + oy))
}

/// Contact angle at `p` moving along `dir` (XY; degrees): the longest
/// arc of the FRONT half of the rim standing in material above the cut
/// plane, times 180/samples. A move with no XY direction (a plunge)
/// reads the whole rim, times 360 - a buried tool. With
/// `QualitySpec::wall_below`, a column below that height is the part's
/// wall: it extends an arc that holds stock (a strip left on the wall
/// reads the width it really has, as a straight wall at that width
/// does) but never makes an arc by itself (a tool touching walls only
/// cuts nothing).
fn contact_deg(dex: &Dexel, p: Point3, dir: (f64, f64), rim: f64, wall: Option<f64>) -> f64 {
    let l = dir.0.hypot(dir.1);
    let (base, span) = if l > 1e-12 {
        (dir.1.atan2(dir.0) - std::f64::consts::FRAC_PI_2, std::f64::consts::PI)
    } else {
        (0.0, std::f64::consts::TAU)
    };
    let hits = (0..RIM_SAMPLES).map(|k| {
        let a = base + (k as f64 + 0.5) * span / RIM_SAMPLES as f64;
        let s = Point2::new(p.x + rim * a.cos(), p.y + rim * a.sin());
        match dex.col(s).map(|i| dex.top_at(i)) {
            Some(t) if t > p.z + Z_EPS => Rim::Stock,
            Some(t) if wall.is_some_and(|w| t < w) => Rim::Wall,
            _ => Rim::Open,
        }
    });
    longest_arc(hits, l <= 1e-12) as f64 * span.to_degrees() / RIM_SAMPLES as f64
}

/// What one rim sample stands in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rim {
    Open,
    Wall,
    Stock,
}

/// The longest run of `true` (cyclic when `wrap`): the contact ARC. A
/// cutter riding between two finished walls touches each with a
/// zero-depth band the outside rim reads as a few degrees; those are
/// two arcs, not one cut, and the longest arc is what the flutes see.
pub fn longest_run(hits: impl Iterator<Item = bool>, wrap: bool) -> usize {
    longest_arc(hits.map(|h| if h { Rim::Stock } else { Rim::Open }), wrap)
}

/// The longest run of non-open samples that holds stock (cyclic when
/// `wrap`): wall samples extend an arc of stock, never stand alone.
pub(crate) fn longest_arc(hits: impl Iterator<Item = Rim>, wrap: bool) -> usize {
    let v: Vec<Rim> = hits.collect();
    let n = v.len();
    if n > 0 && v.iter().all(|&h| h != Rim::Open) {
        return if v.contains(&Rim::Stock) { n } else { 0 };
    }
    let (mut best, mut cur, mut stock) = (0usize, 0usize, false);
    let laps = if wrap { 2 } else { 1 };
    for i in 0..n * laps {
        match v[i % n] {
            Rim::Open => {
                cur = 0;
                stock = false;
            }
            h => {
                cur += 1;
                stock |= h == Rim::Stock;
                if stock {
                    best = best.max(cur.min(n));
                }
            }
        }
    }
    best
}

fn dist3(a: Point3, b: Point3) -> f64 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2) + (b.z - a.z).powi(2)).sqrt()
}

/// Is this a planar feed (the moves the break law reads)?
fn planar_feed(m: &Move) -> bool {
    m.kind == MoveKind::Feed && (m.to.z - m.from.z).abs() <= 1e-9
}
