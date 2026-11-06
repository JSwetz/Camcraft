//! HSM adaptive clearing for one planar area (M11.4 C; closes the
//! reopened M2.1 trochoidal/corner-contact items and M2.4's 2D
//! Adaptive spec). The invariant is the load, not the step: every cut
//! the planner emits was measured on its in-process stock (`raster`,
//! the yardstick's own contact law, `quality`) before it was kept.
//!
//! Round 2: where the whole area is one star cell from the entry (the
//! entry is put in its kernel, `kernel`), the plan is ONE morph spiral
//! (`morph`, `star`: Held & Spielberger 2009; Bieterman & Sandstrom
//! 2003) from the orbit to the walls - no link, no crossing, every
//! revolution's step chosen on the stock. Elsewhere the engagement
//! fronts below plan it, and a plan that would cross itself is refused
//! (`guard`, the verifier's crossing law asked first).
//!
//! The fronts' plan, in order:
//! 1. **Bore and spiral** - the spiral starts on the entry's own orbit
//!    (one descent per level, no replunge). Each revolution morphs onto
//!    the next engagement front - the cutter centers `r - a` from the
//!    stock still standing (Stori & Wright 2000, "Constant engagement
//!    tool path generation for convex geometries", J. Manuf. Systems
//!    19(3), taken against the in-process stock), riding the walls
//!    where the front has met them (`loops`) - as ONE spiral (`spiral`:
//!    Held & Spielberger 2009's offset morph), never rings joined by
//!    steps; the step shrinks where the loop is tight (their convex-path
//!    law: small loops out of a bore overload at the nominal width).
//! 2. **Open rounds** (`rounds`) - once the cleared area's boundary is
//!    mostly wall, its nearest cutting run is cut each round, one front
//!    step on, entered and left on Dubins links (`link`); in a channel
//!    that is a crescent arc and a return loop - trochoidal motion
//!    (Otkur & Lazoglu 2007, "Trochoidal milling", Int. J. Mach. Tools
//!    Manuf. 47).
//! 3. **Wall lap** - the strip left on the walls in one rounded climb
//!    lap (`wall`).
//!
//! Where the bound cannot be held, the least overload up to
//! `OVERLOAD_CAP` x the load is cut and named; past it the stock is
//! refused and left (and `pocket_hsm` falls back to Offset rings when
//! too much is left). Every loop is capped; the plan cannot hang.

mod front;
mod guard;
mod kernel;
mod link;
mod loops;
mod morph;
mod raster;
mod rounds;
mod spiral;
mod star;
mod tidy;
mod wall;

use crate::geo2d::{Point2, Region};
use crate::load::{deg_at_rim, load_at_rim};

pub(crate) use kernel::{preferred_seed, widest_point};
pub(crate) use guard::first_crossing;
pub(crate) use loops::boundary_dist;

use super::GenError;
use front::{densify, dist, rotate_to, tail};
use raster::Raster;

/// The worst contact angle a trial walk read, and where.
type Worst = (f64, Point2);
/// A spiral revolution on trial: the ring it reaches, the revolution,
/// the stock it leaves, its worst reading.
type SpiralTrial = (Vec<Point2>, Vec<Point2>, Raster, Worst);
/// An open round on trial: the cut, its link, the stock, the reading.
type RoundTrial = (Vec<Point2>, Option<Vec<Point2>>, Raster, Worst);
/// A wall lap on trial, with the corner rest its rounding leaves.
type LapTrial = (Vec<Point2>, Option<Vec<Point2>>, Raster, Worst, Option<(Point2, f64)>);

/// Step ladder: fractions of the optimal width a round may try.
const STEPS: [f64; 8] = [1.0, 0.85, 0.7, 0.55, 0.45, 0.35, 0.28, 0.22];

/// The spiral's deeper ladder: out of a small bore the convex-path law
/// (Stori & Wright) wants steps far under the nominal width, and a
/// closed revolution there is cheap to plan.
const SPIRAL_STEPS: [f64; 11] = [1.0, 0.85, 0.7, 0.55, 0.45, 0.35, 0.28, 0.22, 0.15, 0.1, 0.07];

/// The spiral rides a cleared-area loop while at least this share of it
/// faces material.
const SPIRAL_FRONT: f64 = 0.6;

/// Where no step of the ladder holds the load, the least overload up
/// to this multiple of it is cut (and named) rather than left standing.
const OVERLOAD_CAP: f64 = 2.0;

/// A contact standing for less than this load (ae/D) is a graze.
const BITE_LOAD: f64 = 0.005;

/// Rounds are capped (the Prime Directive's "never hang").
const MAX_ROUNDS: usize = 3000;

/// Planner grid: the tool diameter over this - the heavy lane's
/// measuring grid, so the plan and the yardstick read one stock.
const GRID_PER_D: f64 = 120.0;

/// The morph spiral's last ring rounds the walls' corners at this
/// share of the tool radius (the named corner scallop).
pub(crate) const CORNER_PER_R: f64 = 0.5;

/// A star cell whose shortest ray is under this share of its longest
/// (a channel) is cleared by the engagement fronts instead.
const STAR_ASPECT: f64 = 0.1;

/// What one planar area is planned with.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AdaptiveSpec {
    /// Tool radius.
    pub r: f64,
    /// Optimal load, ae/D.
    pub load: f64,
    /// Planning tolerance on the contact angle (fraction).
    pub tol: f64,
    pub climb: bool,
    /// Where the entry stood the tool, the bore it left (radius of the
    /// cleared disc, centered at `seed`) and the helix radius it orbited.
    pub seed: Point2,
    pub start: Point2,
    pub bore: f64,
    pub helix_r: f64,
    /// Lead / link arc radius (mm): the smoothing radius.
    pub smooth: f64,
}

/// The plan: cutter-center walks (each starts where the previous one
/// lifted; the first where the entry stood) and its notes.
#[derive(Clone, Debug, Default)]
pub(crate) struct AdaptivePlan {
    pub passes: Vec<Vec<Point2>>,
    pub notes: Vec<String>,
    /// Stock the plan leaves standing in the reach (mm^2, planner grid).
    pub rest: f64,
}

struct Planner<'a> {
    center: &'a Region,
    s: AdaptiveSpec,
    ras: Raster,
    mat: Region,
    rim: f64,
    step: f64,
    seg: f64,
    /// The contact angle a kept cut may read on `rim` (degrees).
    bound: f64,
    /// The contact angle under which a pass is grazing, not biting.
    bite: f64,
    /// The most a fallback cut may read where no step holds the bound.
    cap: f64,
    walk: Vec<Point2>,
    plan: AdaptivePlan,
    worst: (f64, Point2),
}

/// Plan the area `reach` (the material the cutter can reach) from the
/// legal center area `center`.
pub(crate) fn plan(reach: &Region, center: &Region, s: &AdaptiveSpec) -> Result<AdaptivePlan, GenError> {
    if !(s.r > 0.0 && s.load > 0.0 && s.bore >= 0.0 && s.smooth >= 0.0 && s.tol > -0.5) {
        return Err(GenError::Invalid("adaptive plan needs positive radius and load".into()));
    }
    let h = 2.0 * s.r / GRID_PER_D;
    let ras = Raster::from_region(reach, h, s.r)?;
    // The reading law the measuring stock uses (`quality::rim_for`): the
    // rim a cell and a half outside the cutter, on the half facing the
    // motion, and the bound is the angle the optimal load's straight
    // wall reads on that rim (`load::deg_at_rim`).
    let d = 2.0 * s.r;
    let rim = crate::quality::rim_for(d, ras.h);
    let mut p = Planner {
        center,
        s: *s,
        ras,
        mat: reach.clone(),
        rim,
        step: s.r / 10.0,
        seg: s.r / 8.0,
        bound: deg_at_rim(s.load * (1.0 + s.tol), d, rim),
        bite: deg_at_rim(BITE_LOAD, d, rim),
        cap: deg_at_rim(s.load * OVERLOAD_CAP, d, rim),
        walk: vec![s.start],
        plan: AdaptivePlan::default(),
        worst: (0.0, s.start),
    };
    if s.bore > 0.0 {
        p.ras.cut(s.seed, s.seed, s.bore);
        p.mat = p.mat.difference(&Region::sweep(&[vec![s.seed]], s.bore)?)?;
    }
    let star = star_cell(center, s)?;
    if let Some(star) = &star {
        p.orbit();
        p.morph(star, orbit_radius(s))?;
    } else {
        let heading = p.core_spiral()?;
        p.open_rounds(heading)?;
    }
    let mut out = p.plan;
    out.rest = p.ras.standing() as f64 * p.ras.h * p.ras.h;
    if p.walk.len() >= 2 {
        out.passes.push(p.walk);
    }
    if star.is_none() {
        // Kinks the morph seams and blend joints left are spread out
        // (inside the center area only), so the walk reads smooth.
        for pass in &mut out.passes {
            tidy::smooth_walk(pass, center, 6.0, 24);
        }
    }
    let worst = load_at_rim(p.worst.0, d, rim);
    if worst > s.load * (1.0 + s.tol.max(0.0)) {
        out.notes.push(format!(
            "adaptive: worst planned load {:.1}% of D at ({:.2}, {:.2}) exceeds the {:.1}% optimal load by {:.0}%",
            worst * 100.0,
            p.worst.1.x,
            p.worst.1.y,
            s.load * 100.0,
            (worst / s.load - 1.0) * 100.0
        ));
    }
    Ok(out)
}

impl Planner<'_> {
    fn width(&self, f: f64) -> f64 {
        2.0 * self.s.r * self.s.load * f
    }

    fn note_worst(&mut self, w: (f64, Point2)) {
        if w.0 > self.worst.0 {
            self.worst = w;
        }
    }

    fn take(&mut self, cut: &[Point2]) -> Result<(), GenError> {
        let sw = Region::sweep(&[cut.to_vec()], self.s.r)?;
        // Slivers under half a grid cell, vertices within a tenth of
        // one and dust under four cells go (`tidy`): the planner's own
        // resolution, below what the measuring stock can see.
        let h = self.ras.h;
        self.mat = tidy::tidy(&self.mat.difference(&sw)?, 0.25 * h, 0.1 * h, 4.0 * h * h)?;
        Ok(())
    }

    /// Phase 1: closed fronts around the bore, cut as one spiral.
    /// Returns the heading the spiral ends on.
    fn core_spiral(&mut self) -> Result<Point2, GenError> {
        let s = self.s;
        let mut rings: Vec<Vec<Point2>> = Vec::new();
        // The bore's own orbit is the spiral's ring zero: the tool
        // leaves the entry on a turn, not a radial step. It is walked
        // once flat first - whatever floor the descent left (a helix
        // leaves its last pitch as a ramp) is the bore's to clear,
        // inside the entry's own circle.
        rings.push(self.orbit());
        // Each revolution is planned as what is emitted: the morph from
        // the last ring onto the next front loop, measured on the stock
        // the revolutions before it really left.
        for _ in 0..MAX_ROUNDS {
            let Some(prev) = rings.last().cloned() else { break };
            let mut kept: Option<SpiralTrial> = None;
            for (i, f) in SPIRAL_STEPS.iter().enumerate() {
                let room = loops::cleared_loops(&self.mat, s.r - self.width(*f), self.center, (s.climb, self.seg, s.smooth), (&self.ras, self.rim, 0.5 * self.bound, self.bite))?;
                // The spiral lasts while the cleared area has ONE loop
                // still facing material (crumbs under half a radius
                // aside - the open rounds take those) and that loop is
                // mostly cutting: once the walls carry most of it, the
                // open rounds' return links fly less air.
                let mut live = room.iter().filter(|l| l.front >= 0.5 * s.r);
                let (Some(l), None) = (live.next(), live.next()) else { break };
                if l.strong < SPIRAL_FRONT * l.total {
                    break;
                }
                let ring = rotate_to(&l.pts, prev[0]);
                let rev = spiral::revolution(&prev, &ring);
                let mut trial = self.ras.clone();
                let w = trial.walk(&rev, s.r, self.rim, self.step);
                if w.0 <= self.bound || i + 1 == SPIRAL_STEPS.len() {
                    kept = Some((ring, rev, trial, w));
                    break;
                }
            }
            let Some((ring, rev, trial, w)) = kept else { break };
            self.note_worst(w);
            self.ras = trial;
            self.take(&rev)?;
            self.push_walk(&rev);
            rings.push(ring);
        }
        // The last ring once whole: the spiral's outer edge is that loop.
        let last = rings.last().cloned().unwrap_or_default();
        if rings.len() > 1 {
            let mut trial = self.ras.clone();
            let w = trial.walk(&last, s.r, self.rim, self.step);
            self.note_worst(w);
            self.ras = trial;
            self.take(&last)?;
            self.push_walk(&last);
        }
        let heading = tail(&self.walk);
        Ok(heading)
    }

    /// The bore's own orbit, walked once flat (a helix leaves its last
    /// pitch as a ramp: that floor is the bore's to clear, inside the
    /// entry's own circle), or - out of a tool-size bore - said.
    fn orbit(&mut self) -> Vec<Point2> {
        let s = self.s;
        let ring0 = circle(s.seed, orbit_radius(&s), s.start, s.climb, self.seg);
        if s.helix_r > 1e-6 {
            self.walk.extend(ring0.iter().skip(1).copied());
        } else {
            self.plan.notes.push(format!(
                "entry: the {} bore at ({:.2}, {:.2}) is tool-size - the first turn out of it exceeds the load (a Helix entry holds it)",
                if s.bore > 0.0 { "axial" } else { "missing" },
                s.seed.x,
                s.seed.y
            ));
        }
        ring0
    }

    /// Append a run to the walk, dropping a repeat of the joint.
    fn push_walk(&mut self, run: &[Point2]) {
        for &p in run {
            if self.walk.last().is_none_or(|&l| dist(l, p) > 1e-6) {
                self.walk.push(p);
            }
        }
    }

}

/// The radius the morph spiral's last ring rounds the walls' corners
/// at: [`CORNER_PER_R`] of the tool radius, or under half the area's
/// inscribed radius where the area is narrower (a channel's opening must
/// not vanish).
pub(crate) fn corner_radius(center: &Region, r: f64) -> f64 {
    let rho = r * CORNER_PER_R;
    widest_point(center).map_or(rho, |w| rho.min(0.45 * w.1))
}

/// The radius of the entry's orbit: the helix's, or a hair out of a
/// tool-size bore.
fn orbit_radius(s: &AdaptiveSpec) -> f64 {
    if s.helix_r > 1e-6 { s.helix_r } else { (0.05 * s.r).min(s.bore.max(1e-3)) }
}

/// The star cell the morph spiral runs over, when the whole center area
/// is one (seen from the seed, round enough that its rays agree within
/// [`STAR_ASPECT`]); `None` sends the plan to the engagement fronts.
fn star_cell(center: &Region, s: &AdaptiveSpec) -> Result<Option<star::Star>, GenError> {
    let rho_c = corner_radius(center, s.r);
    let radii: Vec<f64> = (0..12).map(|k| rho_c * 1.6f64.powi(k)).collect();
    let orbit = (s.seed, orbit_radius(s));
    // The rays leave from the seed (`kernel::preferred_seed` put it
    // where they are most alike, when the entry let it choose).
    let cell = star::Star::new(center, s.seed, &radii, orbit)?;
    Ok(cell.filter(|c| c.aspect() >= STAR_ASPECT && (c.min_rho() - rho_c).abs() < 1e-9))
}

/// A closed circle about `c` of radius `rr` starting at `start`'s angle,
/// travelled counter-clockwise for climb (material outside on the right)
/// and clockwise otherwise.
fn circle(c: Point2, rr: f64, start: Point2, climb: bool, seg: f64) -> Vec<Point2> {
    let a0 = (start.y - c.y).atan2(start.x - c.x);
    let n = ((std::f64::consts::TAU * rr / seg.max(1e-6)).ceil() as usize).clamp(72, 4096);
    let dir = if climb { 1.0 } else { -1.0 };
    let pts: Vec<Point2> = (0..=n)
        .map(|k| {
            let a = a0 + dir * std::f64::consts::TAU * k as f64 / n as f64;
            Point2::new(c.x + rr * a.cos(), c.y + rr * a.sin())
        })
        .collect();
    densify(&pts, seg, false)
}

#[cfg(test)]
mod tests;
