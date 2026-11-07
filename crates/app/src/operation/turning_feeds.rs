//! Issue #146: a boring bar's head and shank judged on its cutting FEEDS in the meridian (turning) plane against the material standing at each instant of the ride - the shop rule that a tool's body never rubs the work, only its edge cuts (machine-tool handbooks), read about the spindle as a body of revolution; our own design, no vendor material consulted.

use crate::cl::{MoveKind, Toolpath};
use crate::turning::profile::Frame;
use crate::verify::Warning;

use super::drill_prior::PriorCell;
use super::kinds::Section;
use super::turning_credit::{pilots, Pilot};
use super::turning_outline::Revolved;
use super::turning_rapids::{faced_end, kerf_of, seg_dist, spread, Meridian, SignedMeridian};
use super::OpParams;

/// The sampling pitch along a feed and up the bar (mm).
const STEP: f64 = 0.5;
/// Most stations one feed, or one piece of the bar, is sampled at.
const MAX_SAMPLES: f64 = 2_000.0;

/// A cutting move's meridian segment, end to end.
type Seg = ((f64, f64), (f64, f64));

/// What the bar's body is judged against: the revolved bodies, and
/// what has been cut before the instant read.
struct Scene<'a> {
    part: Revolved<'a>,
    stock: Option<Revolved<'a>>,
    /// The turning ops ahead's kerfs, each at its own nose.
    ahead: Vec<(Seg, f64)>,
    /// This path's own cutting moves' kerfs, by move index.
    own: Vec<Option<Seg>>,
    pilots: Vec<Pilot>,
    /// This bar's nose kerf reach (nose plus margin).
    reach: f64,
    margin: f64,
}

/// The body law on a boring bar's feeds (#146): the rapid law (#103)
/// judges the whole bar on its rapids, the envelope verdict the nose's
/// cut; between them the bar's head (`flute_len` up the axis from the
/// nose) and shank (`shank_len` above it), at the spans
/// [`crate::turning::bar_spans`] gives, ride every plunge, lead and
/// pass unjudged. Here each feed is sampled along its length, and at
/// each instant the body must stand in air: outside the revolved part,
/// and outside the stock less the drills ahead, the turning ops ahead,
/// this path's earlier feeds and this feed's own kerf up to that
/// instant (the nose's sweep - its own cut is never judged here). A
/// body in the part, or in stock still standing, is a
/// [`Warning::ShankHit`] on that move, which holds the post.
pub(super) fn judge_bar_feeds(params: &OpParams, path: &Toolpath, cx: &Meridian, prior: &mut PriorCell, warns: &mut Vec<Warning>) {
    if params.kind().info().section != Section::Turning {
        return;
    }
    let Some(bar) = cx.body.filter(|q| [q.head_d, q.shank_d, q.flute_len, q.shank_len].iter().all(|v| v.is_finite())) else {
        return;
    };
    let Ok(frame) = Frame::about(cx.part) else { return };
    let Some(part) = Revolved::of(cx.part, &frame) else { return };
    let sm = SignedMeridian::of(&frame, path);
    // An unreadable stock still leaves the part to judge against.
    let stock = cx.stock.and_then(|s| Revolved::of(s, &frame));
    let rn = if cx.nose.is_finite() && cx.nose > 0.0 { cx.nose } else { 0.0 };
    let margin = if cx.margin.is_finite() { cx.margin.max(0.0) } else { 0.0 };
    let cutting = |m: &crate::cl::Move| m.kind != MoveKind::Rapid;
    let scene = Scene {
        part,
        stock,
        ahead: cx
            .ahead
            .iter()
            .flat_map(|a| a.moves.iter().filter(|m| cutting(m)).filter_map(|m| kerf_of(m, &sm, cx.z_tol).map(|pq| (pq, a.nose + margin))))
            .collect(),
        own: path.moves.iter().map(|m| if cutting(m) { kerf_of(m, &sm, cx.z_tol) } else { None }).collect(),
        pilots: pilots(cx.drills, &sm),
        reach: rn + margin,
        margin,
    };
    let [head, shank] = crate::turning::bar_spans(bar.head_d, bar.shank_d);
    let (lf, ls) = (bar.flute_len.max(0.0), bar.shank_len.max(0.0));
    // Each span shrunk by the margin, sampled across: outward offsets
    // of the nose centre (+ away from the spindle).
    let across = |(lo, hi): (f64, f64)| -> Vec<f64> {
        let (lo, hi) = (lo + margin, hi - margin);
        if hi < lo {
            return Vec::new();
        }
        let mid = (lo + hi) * 0.5;
        spread((hi - lo) * 0.5).chain(std::iter::once(0.0)).map(|k| mid + k).collect()
    };
    let pieces = [(0.0, lf, across(head)), (lf, lf + ls, across(shank))];
    let mut cap: Option<Option<f64>> = None; // the faced end, read once
    for (i, m) in path.moves.iter().enumerate() {
        if !cutting(m) || !sm.in_plane(m.from, cx.z_tol) || !sm.in_plane(m.to, cx.z_tol) {
            continue;
        }
        if warns.iter().any(|w| matches!(*w, Warning::ShankHit { at, .. } if at == i)) {
            continue; // named already: one word per move
        }
        let (a, b) = (sm.sr(m.from), sm.sr(m.to));
        let verdict = match rubs(a, b, i, &pieces, &scene, None) {
            // Only a stock conviction pays for the program ahead's
            // simulation: the part is never faced away.
            Some(false) => {
                let c = *cap.get_or_insert_with(|| faced_end(prior, &frame, &sm, cx));
                if c.is_some() { rubs(a, b, i, &pieces, &scene, c) } else { Some(false) }
            }
            v => v,
        };
        if let Some(part) = verdict {
            warns.push(Warning::ShankHit { at: i, part });
        }
    }
}

/// Does the bar's body, its nose riding `a` -> `b` (move `i`), meet
/// material? `Some(true)` the part, `Some(false)` stock only, `None`
/// air throughout. `cap` is the station the program ahead faced to.
fn rubs(a: (f64, f64), b: (f64, f64), i: usize, pieces: &[(f64, f64, Vec<f64>); 2], sc: &Scene, cap: Option<f64>) -> Option<bool> {
    let len = (b.0 - a.0).hypot(b.1 - a.1);
    if !len.is_finite() {
        return None;
    }
    let n = (len / STEP.max(len / MAX_SAMPLES)).ceil().max(1.0) as usize;
    // The insert points away from the axis: the body's side is the
    // nose's own, read at the end that stands farther out.
    let out = if b.1.abs() >= a.1.abs() { b.1 } else { a.1 };
    let toward = if out < 0.0 { -1.0 } else { 1.0 };
    let mut stock_hit = false;
    for k in 0..=n {
        let t = k as f64 / n as f64;
        let at = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        for (h0, h1, offs) in pieces {
            // Stations on one global grid: the part's hole is read once
            // per station (its ray parity is cached by station).
            let (lo, hi) = (at.0 + h0, at.0 + h1);
            let first = (lo / STEP).ceil();
            let m = ((hi / STEP).floor() - first).clamp(0.0, MAX_SAMPLES) as usize;
            for j in 0..=m {
                let s = (first + j as f64) * STEP;
                if s > hi {
                    break;
                }
                for dr in offs {
                    let rho = at.1 + toward * dr;
                    match material(s, rho, sc, cap) {
                        Some(true) if !cut(s, rho, i, a, at, sc) => return Some(true),
                        Some(false) if !stock_hit && !cut(s, rho, i, a, at, sc) => stock_hit = true,
                        _ => {}
                    }
                }
            }
        }
    }
    stock_hit.then_some(false)
}

/// The material at signed meridian point (`s`, `rho`) before any cut
/// is credited: `Some(true)` the part, `Some(false)` the stock.
fn material(s: f64, rho: f64, sc: &Scene, cap: Option<f64>) -> Option<bool> {
    let r = rho.abs();
    if sc.part.solid(s, r, sc.margin) {
        return Some(true);
    }
    let faced = cap.is_some_and(|c| s >= c - sc.margin);
    (!faced && sc.stock.as_ref().is_some_and(|o| o.solid(s, r, sc.margin))).then_some(false)
}

/// Was (`s`, `rho`) cut before the nose reached `now` on move `i`
/// (from `start`)? This path's earlier feeds, this feed up to `now`,
/// the turning ops and drills ahead.
fn cut(s: f64, rho: f64, i: usize, start: (f64, f64), now: (f64, f64), sc: &Scene) -> bool {
    let near = |p: (f64, f64), q: (f64, f64), reach: f64| seg_dist((s, rho), p, q) <= reach || seg_dist((s, -rho), p, q) <= reach;
    sc.pilots.iter().any(|p| p.removed(s, rho.abs()))
        || near(start, now, sc.reach)
        || sc.own.iter().take(i).flatten().any(|&(p, q)| near(p, q, sc.reach))
        || sc.ahead.iter().any(|&((p, q), reach)| near(p, q, reach))
}
