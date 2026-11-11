//! The flowline stage of the finishing family (M5.1 tail): the
//! raster's laws with a direction that TURNS. Straight rasters run
//! one chord across the whole model; a flow pass follows a direction
//! field `(ux, uy)(x, y)` the caller reads off the surface - lines
//! that stay with the part's own grain instead of crossing it.
//!
//! Every other law is the raster's, reused verbatim: the ball's
//! column still answers for height (`support_at`), the march's
//! refinement, run-edge bisection and discontinuity break still make
//! the pieces honest, the cos-law stepover still authors spacing,
//! and the steep flanks still close through `rings_and_tail`'s
//! waterline jurisdiction. What is new is only the seeding: lines
//! cannot be laid on a parallel lattice when they curve, so a grid
//! of candidate seeds walks the footprint and a seed is spent where
//! an already-traced line came closer than a fraction of the
//! commanded stepover - the spacing promise is then kept within the
//! grid's quantization, and the envelope verdict (never the
//! spacing) is what says nothing gouges.
//!
//! Provenance: flowline (field-following) toolpaths for bladed and
//! channelled freeforms are published multi-axis finishing practice;
//! seeding by exclusion against traced curves is the standard
//! construction. Written from those sources and our own design
//! notes. No vendor material consulted.

use std::collections::HashMap;

use crate::cl::{PathBuilder, Toolpath, lattice};
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::march::{emit_runs, march_curve};
use super::params::{Finish3DParams, refinement_floor, scallop_stepdown, scallop_stepover, validate};
use super::raster::rings_and_tail;
use super::support::{build_sups, plunge_support};

/// The seed lattice and the traced-line exclusion, both as a
/// fraction of the commanded stepover. The law they keep together
/// is the cusp: every ground point must stand within HALF a step
/// of some traced line (the straight raster's promise exactly),
/// and a point is never farther from a line than its seed is plus
/// half the seed spacing - so seeds a `SEED_FRAC` apart, claimed
/// within `CLAIM_FRAC` of a line, keep
/// `CLAIM_FRAC + SEED_FRAC/2 <= 0.5`. On straight ground the
/// rows are `SEED_FRAC` apart and every one is traced: the cusp
/// lands at a QUARTER of tolerance there, and the denser promise
/// is the price of a field that turns (where line families
/// converge, the exclusion keeps them at least `CLAIM_FRAC`
/// apart, so the count cannot run away). The measure behind the
/// law: a diverging (radial) family, the worst the lattice can
/// quantize, leaves ground 0.56 of a step from every line at
/// `CLAIM_FRAC = 0.5` - past the promise - and within it at 0.25,
/// where straight ground is untouched (its rows, a half-step
/// apart, answer to `SEED_FRAC`, not to the claim).
const SEED_FRAC: f64 = 0.5;
const CLAIM_FRAC: f64 = 0.25;
/// A raster family's line-count roof, in seeds: the guard that
/// keeps a stepover asking for hours of path a named refusal.
const MAX_SEEDS: f64 = 65536.0;
/// Integrations per half-line, the closed-orbit roof: a field that
/// circles (the ring of a torus flows forever) ends its line here
/// and lets the next seed pick the far side up.
const MAX_STEPS: usize = 16384;

/// One flow pass: the field-following lines over the shallow
/// facets' footprint, then the rings for the flanks. `dir_at` is
/// the caller's field: a unit XY direction at every column (the
/// law's trust: it must answer unit or NaN-poisoned, never panic -
/// non-finite readings are caught here and steered straight).
pub fn flow(
    facets: &[Triangle],
    p: &Finish3DParams,
    dir_at: &dyn Fn(Point2) -> (f64, f64),
) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    let r = p.tool_diameter * 0.5;
    let sups = build_sups(facets, r);
    let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
    let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut mz_min = 1.0f64;
    for s in sups.iter().filter(|s| s.shallow) {
        (x0, y0) = (x0.min(s.xmin), y0.min(s.ymin));
        (x1, y1) = (x1.max(s.xmax), y1.max(s.ymax));
        mz_min = mz_min.min(s.m.z);
    }
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    let mut emitted = false;
    if x0 <= x1 {
        let step = p.stepover.unwrap_or(scallop_stepover(r, p.tolerance) * mz_min);
        // Seeds live on the half-step lattice (SEED_FRAC's law);
        // the INTEGRATION step stays the commanded one - a finer
        // march would buy nothing, the march refines where the
        // surface asks.
        let ss = SEED_FRAC * step;
        let step_positive = step > 0.0;
        let seeds = ((x1 - x0) / ss).ceil() * ((y1 - y0) / ss).ceil();
        if !step_positive || !seeds.is_finite() || seeds > MAX_SEEDS {
            return Err(GenError::Invalid(format!(
                "flow needs {} seeds at stepover {:.4} (cap {}): widen the stepover or the tolerance",
                seeds.max(0.0) as i64,
                step,
                MAX_SEEDS as i64,
            )));
        }
        // The turn law: a step may bend the direction by at most
        // this much, the deviation of the bent polyline (s * th / 8)
        // kept under the same eps_chord budget the straight march
        // gives its chords.
        let turn_max = ((8.0 * p.tolerance * 4.0) / step).min(0.35);
        let trimmed = std::cell::Cell::new(0usize);
        let sup = |q: Point2| {
            let (c, cut) = plunge_support(q.x, q.y, r, p.bottom_z, p.clearance_z + r, &sups);
            if cut {
                trimmed.set(trimmed.get() + 1);
            }
            c
        };
        let delta0 = scallop_stepdown(r, p.tolerance).max(1e-6);
        let eps_chord = p.tolerance * 0.25;
        // Traced territory, bucketed one cell per seed spacing: a
        // seed query inspects the 3x3 around itself and is spent
        // when any segment (or spent-seed point) lies within
        // CLAIM_FRAC*step.
        let mut traced: HashMap<(i64, i64), Vec<(Point2, Point2)>> = HashMap::new();
        let claim = CLAIM_FRAC * step;
        // One column of slack past the far edge: a line that dies
        // inside the last cell still got its seed, and the march's
        // edge bisection trims the slack it walks into air.
        let nx = ((x1 - x0) / ss).ceil() as i64 + 1;
        let ny = ((y1 - y0) / ss).ceil() as i64 + 1;
        let mut row_odd = false;
        for j in 0..ny {
            for i in 0..nx {
                let q = Point2::new(
                    lattice(x0 + ss * (i as f64 + 0.5)),
                    lattice(y0 + ss * (j as f64 + 0.5)),
                );
                if sup(q).is_none() || claimed(&traced, q, claim, ss) {
                    continue;
                }
                let d0 = unit_of(dir_at(q), q);
                let back = integrate(q, -d0.0, -d0.1, dir_at, &sup, step, turn_max);
                let fwd = integrate(q, d0.0, d0.1, dir_at, &sup, step, turn_max);
                // The whole line: the backward half walked out in
                // reverse, the seed, then the forward half - one
                // arc-length polyline for the march.
                let mut line: Vec<Point2> = back.iter().rev().copied().collect();
                line.push(q);
                line.extend(fwd.iter().copied());
                register(&mut traced, &line, ss);
                if line.len() < 2 {
                    continue; // a dead-end seed: spent, nothing cut
                }
                let cum = cumulative(&line);
                let len = *cum.last().unwrap_or(&0.0);
                if len <= 0.0 {
                    continue;
                }
                let seg = line.len() - 1;
                let pos = |t: f64| -> Point2 {
                    if t <= 0.0 {
                        let d = norm2(line[1].x - line[0].x, line[1].y - line[0].y);
                        return Point2::new(line[0].x - d.0 * t, line[0].y - d.1 * t);
                    }
                    if t >= len {
                        let d = norm2(line[seg].x - line[seg - 1].x, line[seg].y - line[seg - 1].y);
                        return Point2::new(line[seg].x + d.0 * (t - len), line[seg].y + d.1 * (t - len));
                    }
                    let mut k = match cum.binary_search_by(|c| c.total_cmp(&t)) {
                        Ok(k) => k,
                        Err(k) => k - 1,
                    };
                    k = k.min(seg - 1);
                    let f = (t - cum[k]) / (cum[k + 1] - cum[k]).max(1e-12);
                    Point2::new(
                        line[k].x + (line[k + 1].x - line[k].x) * f,
                        line[k].y + (line[k + 1].y - line[k].y) * f,
                    )
                };
                let end = len + step; // the march's own end-bisection finds the edge
                let mut pieces = march_curve(
                    0.0,
                    end,
                    &pos,
                    &|a, c| (c - a).abs(),
                    &|_| 1.0,
                    &sup,
                    delta0,
                    p.tolerance,
                    eps_chord,
                    refinement_floor(delta0, p.min_step),
                );
                if row_odd {
                    for pc in pieces.iter_mut() {
                        pc.reverse();
                    }
                }
                emitted |= !pieces.is_empty();
                for pc in pieces {
                    emit_runs(&mut b, p, r, &sups, std::slice::from_ref(&pc), emitted);
                    emitted = true;
                }
                row_odd = !row_odd;
            }
        }
        if trimmed.get() > 0 {
            b.note(format!(
                "flow: {} ground samples refused - their plunge column from clearance crosses the solid; what hides there stands as rest, reported not cut",
                trimmed.get()
            ));
        }
    }
    rings_and_tail(b, p, r, facets, emitted)
}

/// The field's answer made honest: unit and finite, or straight.
/// A poisoned reading is the caller's field missing the column -
/// straight is what the fallback law would have steered anyway.
fn unit_of(d: (f64, f64), _q: Point2) -> (f64, f64) {
    let l = (d.0 * d.0 + d.1 * d.1).sqrt();
    if !l.is_finite() || l <= 1e-12 {
        (1.0, 0.0)
    } else {
        (d.0 / l, d.1 / l)
    }
}

/// Is `q`'s ground already claimed? Buckets at one cell per `step`,
/// the 3x3 neighbourhood, point-to-segment distance under `claim`.
fn claimed(traced: &HashMap<(i64, i64), Vec<(Point2, Point2)>>, q: Point2, claim: f64, step: f64) -> bool {
    let c2 = claim * claim;
    let (ci, cj) = (
        (q.x / step).floor() as i64,
        (q.y / step).floor() as i64,
    );
    for di in -1..=1 {
        for dj in -1..=1 {
            if let Some(segs) = traced.get(&(ci + di, cj + dj)) {
                for (a, b) in segs {
                    if dist2_seg(q, *a, *b) < c2 {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// File a traced line's segments into the claim buckets (points
/// enter as degenerate segments, which is how a spent seed keeps
/// its ground).
fn register(traced: &mut HashMap<(i64, i64), Vec<(Point2, Point2)>>, line: &[Point2], step: f64) {
    if line.len() < 2 {
        let k = ((line[0].x / step).floor() as i64, (line[0].y / step).floor() as i64);
        traced.entry(k).or_default().push((line[0], line[0]));
        return;
    }
    for w in line.windows(2) {
        // A segment lands in the bucket of its midpoint; the 3x3
        // query with claim <= step cannot miss what it should see.
        let m = Point2::new((w[0].x + w[1].x) * 0.5, (w[0].y + w[1].y) * 0.5);
        let k = ((m.x / step).floor() as i64, (m.y / step).floor() as i64);
        traced.entry(k).or_default().push((w[0], w[1]));
    }
}

fn norm2(dx: f64, dy: f64) -> (f64, f64) {
    let l = (dx * dx + dy * dy).sqrt();
    if !l.is_finite() || l <= 1e-12 {
        (1.0, 0.0)
    } else {
        (dx / l, dy / l)
    }
}

fn cumulative(line: &[Point2]) -> Vec<f64> {
    let mut cum = Vec::with_capacity(line.len());
    let mut s = 0.0;
    cum.push(0.0);
    for w in line.windows(2) {
        s += ((w[1].x - w[0].x) * (w[1].x - w[0].x) + (w[1].y - w[0].y) * (w[1].y - w[0].y)).sqrt();
        cum.push(s);
    }
    cum
}

/// Point-to-segment squared distance, the same closed form every
/// crossing law in this codebase uses.
fn dist2_seg(q: Point2, a: Point2, b: Point2) -> f64 {
    let (ex, ey) = (b.x - a.x, b.y - a.y);
    let ee = ex * ex + ey * ey;
    let t = if ee > 0.0 {
        (((q.x - a.x) * ex + (q.y - a.y) * ey) / ee).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (dx, dy) = (q.x - a.x - t * ex, q.y - a.y - t * ey);
    dx * dx + dy * dy
}

/// Walk one half-line from `seed` along `(d0x, d0y)`: each step
/// asks the field at the trial point and bends toward it by the
/// bisector, halving the step (thrice) when the bend would exceed
/// the turn law; the half-line ends where the ball's column goes
/// empty or the step budget runs out - the march's edge bisection
/// then finds the true boundary just outside the last sample.
fn integrate(
    seed: Point2,
    d0x: f64,
    d0y: f64,
    dir_at: &dyn Fn(Point2) -> (f64, f64),
    sup: &dyn Fn(Point2) -> Option<f64>,
    step: f64,
    turn_max: f64,
) -> Vec<Point2> {
    let mut out: Vec<Point2> = Vec::new();
    let (mut q, mut d) = (seed, (d0x, d0y));
    for _ in 0..MAX_STEPS {
        let mut s = step;
        let mut advanced = false;
        for _ in 0..4 {
            let nq = Point2::new(lattice(q.x + s * d.0), lattice(q.y + s * d.1));
            if sup(nq).is_none() {
                break;
            }
            let nd = unit_of(dir_at(nq), nq);
            // The bisector of the two unit directions, renormalized.
            let (bx, by) = (d.0 + nd.0, d.1 + nd.1);
            let bl = (bx * bx + by * by).sqrt();
            let turn = if bl <= 1e-12 {
                std::f64::consts::PI // the field said U-turn: too sharp by any law
            } else {
                let cos = (d.0 * nd.0 + d.1 * nd.1).clamp(-1.0, 1.0);
                cos.acos()
            };
            if turn <= turn_max || s <= step * 0.1 {
                let nd = if bl <= 1e-12 { nd } else { (bx / bl, by / bl) };
                out.push(nq);
                (q, d) = (nq, nd);
                advanced = true;
                break;
            }
            s *= 0.5;
        }
        if !advanced {
            break;
        }
    }
    out
}
