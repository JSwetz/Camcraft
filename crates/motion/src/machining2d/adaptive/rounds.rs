//! The open rounds and the wall lap of the adaptive plan (M11.4 C;
//! provenance in `adaptive/mod.rs`): once the cleared area meets the
//! walls, the nearest cutting segment of its boundary is cut each round
//! (one front step on), entered and left on Dubins links; what the
//! fronts cannot reach is the wall lap's.

use crate::geo2d::Point2;

use super::front::{dist, head, rotate_to, tail};
use super::{GenError, LapTrial, MAX_ROUNDS, Planner, RoundTrial, STEPS, link, loops, wall};

impl Planner<'_> {
    /// Phase 2 and 3: open fronts, nearest first, then the wall laps.
    pub(super) fn open_rounds(&mut self, mut heading: Point2) -> Result<(), GenError> {
        let s = self.s;
        let mut last_start: Option<Point2> = None;
        // Segments that could not be cut within the load, or cut nothing:
        // never offered again (the plan cannot spin on them).
        let mut dead: Vec<Point2> = Vec::new();
        let min_cells = (0.25 * s.r * s.r * s.load / (self.ras.h * self.ras.h)).max(4.0) as usize;
        for _ in 0..MAX_ROUNDS {
            let cur = *self.walk.last().unwrap_or(&s.start);
            if self.mat.area() <= self.ras.h * self.ras.h {
                return Ok(());
            }
            let mut kept: Option<RoundTrial> = None;
            let mut tried: Option<Point2> = None;
            for f in STEPS {
                let room = loops::cleared_loops(&self.mat, s.r - self.width(f), self.center, (s.climb, self.seg, s.smooth), (&self.ras, self.rim, 0.5 * self.bound, self.bite))?;
                let Some(cut) = self.nearest_cut(&room, cur, last_start, &dead) else { continue };
                tried.get_or_insert(cut[0]);
                let link = link::dubins(
                    &self.ras,
                    self.center,
                    (cur, heading),
                    (cut[0], head(&cut)),
                    s.smooth,
                    (self.rim, self.step, self.bound),
                );
                let mut trial = self.ras.clone();
                // The link is cut too: what it bites counts like the cut.
                let lw = link.as_ref().map_or((0.0, cur), |l| trial.walk(l, s.r, self.rim, self.step));
                let cw = trial.walk(&cut, s.r, self.rim, self.step);
                let w = if lw.0 > cw.0 { lw } else { cw };
                let held = w.0 <= self.bound;
                // Past the bound but under the cap: the least overload of
                // the ladder is kept as a fallback (named via `worst`).
                if held || (w.0 <= self.cap && kept.as_ref().is_none_or(|k| w.0 < k.3.0)) {
                    kept = Some((cut, link, trial, w));
                }
                if held {
                    break;
                }
            }
            let Some(start) = tried else { return self.wall(heading) };
            let Some((cut, link, trial, w)) = kept else {
                // No step of the ladder held the load here: refuse the
                // segment (its stock is left, and said, not slotted).
                dead.push(start);
                continue;
            };
            if self.ras.standing().saturating_sub(trial.standing()) < min_cells {
                dead.push(cut[0]);
                continue;
            }
            self.note_worst(w);
            self.ras = trial;
            match link {
                Some(l) => {
                    self.take(&l)?;
                    self.walk.extend(l.into_iter().skip(1));
                }
                None => self.lift(cut[0]),
            }
            self.take(&cut)?;
            self.walk.extend(cut.iter().skip(1).copied());
            heading = tail(&cut);
            last_start = cut.first().copied();
        }
        self.plan.notes.push(format!("adaptive: round cap ({MAX_ROUNDS}) reached - wall lap takes the rest"));
        self.wall(heading)
    }

    /// The cutting segment nearest `cur` among the cleared-area loops
    /// that still face material: a front run with a wall-run margin on
    /// each side (`ClearLoop::segments`), or a whole loop (closed,
    /// rotated to start nearest). The front the last cut worked weighs
    /// most: a segment starting next to the previous one's start is the
    /// same front one step on, and finishing a front before crossing to
    /// another is what keeps the links short.
    fn nearest_cut(&self, room: &[loops::ClearLoop], cur: Point2, last: Option<Point2>, dead: &[Point2]) -> Option<Vec<Point2>> {
        let margin = self.s.smooth;
        let ride = 4.0 * self.s.smooth;
        let anchor = last.unwrap_or(cur);
        let mut best: Option<(f64, Vec<Point2>)> = None;
        for l in room.iter().filter(|l| l.front > self.ras.h * 2.0) {
            for seg in l.segments(margin, ride) {
                let closed = seg.len() > 2 && dist(seg[0], seg[seg.len() - 1]) < 1e-9;
                let cut = if closed { rotate_to(&seg, cur) } else { seg };
                if dead.iter().any(|&d| dist(d, cut[0]) < 0.5 * self.s.r) {
                    continue;
                }
                let d = dist(cut[0], anchor) + 0.25 * dist(cut[0], cur);
                if best.as_ref().is_none_or(|b| d < b.0) {
                    best = Some((d, cut));
                }
            }
        }
        best.map(|b| b.1)
    }

    /// No legal stay-down link: lift here, plunge at `at` (which the
    /// leads put in cleared stock where they could).
    fn lift(&mut self, at: Point2) {
        let clear = self.ras.disc_clear(at, self.rim);
        self.plan.notes.push(format!(
            "adaptive: no smooth stay-down link to ({:.2}, {:.2}) - retract and re-enter{}",
            at.x,
            at.y,
            if clear { "" } else { " (into standing material)" }
        ));
        let done = std::mem::replace(&mut self.walk, vec![at]);
        if done.len() >= 2 {
            self.plan.passes.push(done);
        }
    }

    /// Phase 3: one rounded climb lap per center loop that still touches
    /// material, its corners rounded at the tightest radius of the ladder
    /// that holds the load; the corner rest the rounding leaves is
    /// stated, and a lap no radius makes legal is refused - its stock
    /// left and said, never slotted.
    fn wall(&mut self, heading: Point2) -> Result<(), GenError> {
        let s = self.s;
        let ladder: Vec<_> = [0.25, 0.5, 1.0].iter().map(|f| wall::laps(self.center, s.climb, s.r * f, self.seg)).collect();
        let n = ladder.first().map_or(0, |l| l.0.len());
        let mut hd = heading;
        let mut pulled_worst: Option<(Point2, f64)> = None;
        for k in 0..n {
            let at = *self.walk.last().unwrap_or(&s.start);
            let mut kept = None;
            let mut seen = 0.0f64;
            for (laps, pulled) in &ladder {
                let Some(lap) = laps.get(k).map(|l| rotate_to(l, at)) else { continue };
                if self.ras.peek(&lap, self.rim, self.step) <= self.bite {
                    break; // nothing left on this wall
                }
                let mut trial = self.ras.clone();
                let link = link::dubins(&trial, self.center, (at, hd), (lap[0], head(&lap)), s.smooth, (self.rim, self.step, self.bound));
                let lw = link.as_ref().map_or((0.0, at), |l| trial.walk(l, s.r, self.rim, self.step));
                let cw = trial.walk(&lap, s.r, self.rim, self.step);
                let w = if lw.0 > cw.0 { lw } else { cw };
                seen = seen.max(w.0);
                let held = w.0 <= self.bound;
                if held || (w.0 <= self.cap && kept.as_ref().is_none_or(|k: &LapTrial| w.0 < k.3.0)) {
                    let worst_pull = pulled.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1));
                    kept = Some((lap, link, trial, w, worst_pull));
                }
                if held {
                    break;
                }
            }
            let Some((lap, link, trial, w, pull)) = kept else {
                if seen > 0.0 {
                    self.plan.notes.push(format!(
                        "adaptive: a wall lap would load the tool to {:.0}% of D - its strip is left for a finishing pass",
                        crate::load::load_at_rim(seen, 2.0 * s.r, self.rim) * 100.0
                    ));
                }
                continue;
            };
            self.note_worst(w);
            self.ras = trial;
            match link {
                Some(l) => self.walk.extend(l.into_iter().skip(1)),
                None => self.lift(lap[0]),
            }
            self.walk.extend(lap.iter().skip(1).copied());
            hd = tail(&lap);
            if pull.is_some_and(|p| pulled_worst.is_none_or(|q| p.1 > q.1)) {
                pulled_worst = pull;
            }
        }
        if let Some((v, e)) = pulled_worst.filter(|(_, e)| *e > 1e-3) {
            self.plan.notes.push(format!(
                "adaptive: wall corners rounded to keep the load - up to {e:.2} mm corner rest (worst at ({:.2}, {:.2})) left for the finishing pass",
                v.x, v.y
            ));
        }
        Ok(())
    }
}
