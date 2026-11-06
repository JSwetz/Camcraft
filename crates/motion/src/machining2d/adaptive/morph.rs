//! The morph spiral (M11.4 C round 2): where the whole area is a star
//! cell seen from the entry (`star`), one spiral runs from the entry's
//! orbit to the walls with no link and no air - each revolution morphs
//! the ring a little further from the orbit toward the area's boundary,
//! the boundary rounded at a radius that shrinks as the spiral grows,
//! so the inner rings are round and only the last ones carry the walls'
//! corners. Provenance: the smooth spiral of Held & Spielberger (2009,
//! "A smooth spiral tool path for high speed machining of 2D
//! pocket-shaped objects", Computer-Aided Design 41(7)) - one spiral
//! interpolating nested rings from an inner start to the boundary,
//! here parametrised by the polar angle of the star cell; the
//! curvilinear morph from a point to the pocket boundary of Bieterman &
//! Sandstrom (2003, "A curvilinear tool-path method for pocket
//! machining", J. Manuf. Sci. Eng. 125(4)); the step of every
//! revolution chosen against the in-process stock so the contact holds
//! the load, Stori & Wright's (2000) constant-engagement law, and the
//! corner rounding of the last rings that of Pateloup, Duc & Ray (2004,
//! "Corner optimization for pocket machining", Int. J. Mach. Tools
//! Manuf. 44). The schedule, sampling and step search are ours.

use crate::geo2d::Point2;

use super::front::dist;
use super::raster::Raster;
use super::star::Star;
use super::{GenError, Planner, Worst};

/// Revolutions are capped (the Prime Directive's "never hang").
const MAX_REVS: usize = 4000;

/// The smallest share of the morph a revolution may advance.
const DT_MIN: f64 = 2e-4;

/// A revolution may grow its step by at most this factor over the last.
const DT_GROW: f64 = 1.3;

/// Fine angular sampling of a revolution before thinning (degrees).
const FINE_DEG: f64 = 0.1;

/// The thinned chord stays within this of the curve (mm), and is never
/// longer than [`CHORD_MAX`]: a 1 mm curvature radius is then walked at
/// under 4 degrees a chord, which the arc fitter's tolerance smooths.
const SAG: f64 = 3e-4;
const CHORD_MAX: f64 = 0.5;

/// The last revolution reaches the boundary within this share of its
/// turn.
const FINAL_SHARE: f64 = 0.15;

/// The corner radius eases from the widest opening to the last one as
/// the share to this power: the rings stay round long, the walls'
/// corners come in at the end.
const CORNER_EASE: f64 = 1.0;

/// A revolution tries at most this many steps (a grown first guess,
/// then bisection between the best that held and the least that did
/// not).
const STEP_TRIES: usize = 7;

/// When the rest of the morph is within this share of... one step over
/// it, the revolution aims to finish.
const FINISH_SHARE: f64 = 0.7;

/// The inner circle's radius as a share of the cell's shortest ray.
const INNER_SHARE: f64 = 0.6;

/// A revolution on trial: its share of the morph, its points, the stock
/// it leaves, its worst reading.
type RevTrial = (f64, Vec<Point2>, Raster, Worst);

/// Keep a point of the fine curve only where the chord from the last
/// kept one would leave the curve by more than [`SAG`] or run past
/// [`CHORD_MAX`]; first and last are kept.
pub(super) fn thin_curve(fine: &[Point2]) -> Vec<Point2> {
    let n = fine.len();
    if n <= 2 {
        return fine.to_vec();
    }
    let mut out = vec![fine[0]];
    let mut k = 0usize;
    let mut i = 1usize;
    while i < n {
        // Can the chord k -> i stand?
        let (a, b) = (fine[k], fine[i]);
        let len = dist(a, b);
        let ok = len <= CHORD_MAX
            && (k + 1..i).all(|m| {
                let p = fine[m];
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let dd = dx * dx + dy * dy;
                let t = if dd > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
                (p.x - a.x - dx * t).hypot(p.y - a.y - dy * t) <= SAG
            });
        if ok {
            i += 1;
            continue;
        }
        // The chord to i - 1 was the last that stood (i - 1 > k: a one-
        // step chord always stands unless longer than the cap, then it
        // is kept as it is).
        let keep = if i - 1 > k { i - 1 } else { i };
        out.push(fine[keep]);
        k = keep;
        i = keep + 1;
    }
    if out.last().is_none_or(|&l| dist(l, fine[n - 1]) > 1e-9) {
        out.push(fine[n - 1]);
    }
    out
}

/// The ring family of one morph: the star cell, the entry orbit's
/// radius, the inner circle the rings grow to first (concentric: the
/// round rings Stori & Wright's law steps furthest) and the share of
/// the morph that growth takes.
/// A family of nested rings, one per morph share `t` in `[0, 1]`: the
/// point at angle `th` (the climb sense increases it) of ring `t`;
/// ring 0 is the entry orbit about `seed`.
pub(super) trait RingFamily {
    fn at(&self, seed: Point2, th: f64, t: f64) -> Point2;
}

struct Rings<'a> {
    star: &'a Star,
    a0: f64,
    a_in: f64,
    t1: f64,
}

impl RingFamily for Rings<'_> {
    /// The point at angle `th` of the ring at morph share `t`: up to
    /// `t1` the circle growing from the orbit to the inner circle, past
    /// it the inner circle blended radially with the cell boundary
    /// (about the pole), the boundary rounded at a radius shrinking
    /// from the cell's widest opening to its last.
    fn at(&self, seed: Point2, th: f64, t: f64) -> Point2 {
        let (rr, m) = if t <= self.t1 {
            let u = if self.t1 > 0.0 { t / self.t1 } else { 1.0 };
            (self.a0 + (self.a_in - self.a0) * u, seed)
        } else {
            let u = ((t - self.t1) / (1.0 - self.t1).max(1e-12)).clamp(0.0, 1.0);
            let (lo, hi) = (self.star.min_rho(), self.star.max_rho());
            let rho = hi + (lo - hi) * u.powf(CORNER_EASE);
            let pole = self.star.seed;
            let m = Point2::new(seed.x + (pole.x - seed.x) * u, seed.y + (pole.y - seed.y) * u);
            ((1.0 - u) * self.a_in + u * self.star.at(th, rho), m)
        };
        Point2::new(m.x + rr * th.cos(), m.y + rr * th.sin())
    }
}

impl Planner<'_> {
    /// One revolution from share `t` to `t + dt` starting at angle
    /// `th0`, in the climb sense. The last one (`fin`) reaches the
    /// boundary within [`FINAL_SHARE`] of its turn, rides it for the
    /// rest and once more over that share: the crescent its quick
    /// reach left is cut, and the wall ride after it bites the full
    /// step (a slow reach would leave a thin crescent - wall-side air).
    fn revolution(&self, rings: &dyn RingFamily, (t, dt): (f64, f64), th0: f64, fin: bool) -> Vec<Point2> {
        let dir = if self.s.climb { 1.0 } else { -1.0 };
        let n = (360.0 / FINE_DEG).round() as usize;
        let fs = FINAL_SHARE;
        // The extra share stops short of where the reach met the wall: the
        // two never touch (a shared wall point read twice is a crossing).
        let (span, extra) = if fin { (fs, (0.85 * fs * n as f64).floor() as usize) } else { (1.0, 0) };
        let mut fine = Vec::with_capacity(n + extra + 1);
        for k in 0..=n + extra {
            let f = (k as f64 / n as f64 / span).min(1.0);
            let th = th0 + dir * std::f64::consts::TAU * k as f64 / n as f64;
            fine.push(rings.at(self.s.seed, th, t + dt * f));
        }
        thin_curve(&fine)
    }

    /// One revolution of `d` from `t` cut on a copy of the stock (the
    /// last when it reaches the end of the morph, `rem` away).
    /// `whole`: walk it to the end even past the bound (a fallback that
    /// is kept); otherwise a failing trial stops at its first overload.
    fn rev_trial(&self, rings: &dyn RingFamily, (t, d): (f64, f64), th0: f64, (rem, whole): (f64, bool)) -> RevTrial {
        let fin = d >= rem - 1e-12;
        let rev = self.revolution(rings, (t, d.min(rem)), th0, fin);
        let mut trial = self.ras.clone();
        let limit = if whole { f64::INFINITY } else { self.bound };
        let w = trial.walk_until(&rev, (self.s.r, self.rim, self.step), limit);
        (d.min(rem), rev, trial, w)
    }

    /// The morph spiral over the star cell, from the orbit of radius
    /// `a0` (the walk stands on it at polar angle `th0`) to the cell's
    /// boundary, then one lap on it.
    pub(super) fn morph(&mut self, star: &Star, a0: f64) -> Result<(), GenError> {
        let s = self.s;
        let th0 = (s.start.y - s.seed.y).atan2(s.start.x - s.seed.x);
        let span = (star.reach() - a0).max(1e-6);
        // The inner circle: a share of the shortest ray, so the sides
        // still have room to step when the corners morph in.
        let a_in = (INNER_SHARE * star.shortest()).max(a0);
        let t1 = (a_in - a0) / span;
        let rings = Rings { star, a0, a_in, t1 };
        self.spiral_over(&rings, span, th0, star.seed)?;
        // The last ring rounds the walls' corners: what it pulls off a
        // corner of the center area is the scallop left there, named.
        let rho = star.min_rho();
        let pole = star.seed;
        let pulled = self
            .center
            .loops()
            .iter()
            .flat_map(|l| l.points().iter())
            .map(|&v| (v.y - pole.y).atan2(v.x - pole.x))
            .zip(self.center.loops().iter().flat_map(|l| l.points().iter()))
            .map(|(th, &v)| (dist(v, pole) - star.at(th, rho), v))
            .fold((0.0f64, pole), |a, b| if b.0 > a.0 { b } else { a });
        if pulled.0 > 1e-3 {
            self.plan.notes.push(format!(
                "adaptive: wall corners rounded at R{rho:.2} to keep the load - up to {:.2} mm corner scallop (worst at ({:.2}, {:.2})) left for the finishing pass",
                pulled.0, pulled.1.x, pulled.1.y
            ));
        }
        Ok(())
    }

    /// The spiral over a ring family: from ring 0 (where the walk
    /// stands, at angle `th0`) to ring 1 and one lap on it, every
    /// revolution's step the largest the stock lets hold the load;
    /// `span` is the most a ring moves over the whole family (mm).
    pub(super) fn spiral_over(&mut self, rings: &dyn RingFamily, span: f64, th0: f64, at: Point2) -> Result<(), GenError> {
        let s = self.s;
        // No revolution steps further than the optimal width anywhere.
        let dt_cap = (self.width(1.0) / span).min(1.0);
        let mut t = 0.0f64;
        let mut dt = 0.25 * dt_cap;
        let mut overloaded = false;
        for _ in 0..MAX_REVS {
            let rem = 1.0 - t;
            if rem <= 1e-12 {
                break;
            }
            // Aim at the largest step that holds the load (grown at most
            // DT_GROW over the last), finishing outright when the rest
            // is near one step; a step that would leave a sliver of the
            // morph splits the rest in two instead (a last revolution
            // shaving microns rides the walls in air).
            let mut hi = (dt * DT_GROW).min(dt_cap);
            if hi >= FINISH_SHARE * rem {
                hi = rem;
            }
            let mut tries: Vec<RevTrial> = Vec::new();
            let mut good: Option<RevTrial> = None;
            let mut lo = 0.0f64;
            let mut cand = hi;
            for k in 0..STEP_TRIES {
                let trial = self.rev_trial(rings, (t, cand.min(rem)), th0, (rem, false));
                if trial.3.0 <= self.bound {
                    lo = cand;
                    good = Some(trial);
                    if k == 0 {
                        break;
                    }
                } else {
                    hi = cand;
                    tries.push(trial);
                }
                cand = if good.is_some() { 0.5 * (lo + hi) } else { hi * 0.6 };
                if cand < DT_MIN || (good.is_some() && hi - lo < 0.08 * hi) {
                    break;
                }
            }
            if let Some(g) = &good
                && g.0 < rem
                && rem - g.0 < 0.5 * g.0
            {
                let half = self.rev_trial(rings, (t, 0.5 * rem), th0, (rem, false));
                if half.3.0 <= self.bound {
                    good = Some(half);
                }
            }
            let (d, rev, trial, w) = match good {
                Some(k) => k,
                None => {
                    // No step holds the load: the largest step within a
                    // whisker of the least overload is cut, and named.
                    overloaded = true;
                    let least = tries.iter().map(|k| k.3.0).fold(f64::INFINITY, f64::min);
                    let Some(i) = tries.iter().position(|k| k.3.0 <= least * 1.02 + 1e-9) else { break };
                    // The trials stopped at their first overload: the one
                    // kept is walked whole.
                    self.rev_trial(rings, (t, tries[i].0), th0, (rem, true))
                }
            };
            self.note_worst(w);
            self.ras = trial;
            self.push_walk(&rev);
            t += d;
            dt = d;
        }
        if overloaded {
            self.plan.notes.push(format!(
                "adaptive: out of the {} at ({:.2}, {:.2}) no spiral step holds the load - the least overload was cut",
                if s.helix_r > 1e-6 { "helix bore" } else { "tool-size axial bore" },
                at.x,
                at.y
            ));
        }
        Ok(())
    }
}
