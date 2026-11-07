//! Arc output for the HSM adaptive plan (M11.4 owed): the plan's cutting
//! chords - the spiral turns, the concentric growth revolutions, the
//! ring bands - ride the one arc fitter the pocket family already has
//! (`arcs::fit_arcs`) at the ring budget (a quarter of the chord
//! tolerance - RECOMMENDED "The ring arc-fit budget": the plan's walk is
//! itself a tessellation, so the fit may not stack a second sag on it),
//! and EVERY arc the fitter returns is proved twice or refused by name:
//!
//! 1. inside the legal area, the way the horizontal arc entry is
//!    (`descent_xy`): bracketed by its inscribed chord fan (sagitta at
//!    most [`ARC_FIT_TOL`]) and its circumscribed tangent polyline, both
//!    proved chord by chord in the cutter-centre area (`chord_clears` -
//!    walls and islands alike);
//! 2. across no other feed of the posted path (`arc_cross`): the
//!    verifier's crossing law (law (a), the proper-crossing predicate at
//!    a shared depth)
//!    held on the TRUE arc - its bracket's chords against every other
//!    feed chord - because the plan's revolutions part from their rings
//!    by microns at the landing seams, where an arc's few-micron bow
//!    off its chords is a crossing no region test can see;
//! 3. adding no kink: at each junction the turn between the arc's
//!    tangent and the neighbouring chord may not exceed the turn the
//!    plan's own chords made there (a short, marginally curved run can
//!    fit a circle within the budget whose end tangent still stands
//!    degrees off the next chord - the 5 deg in-cut law is held junction
//!    by junction).
//!
//! A refused arc stays the plan's own chords, point for point, and is
//! counted; nothing silent ever rides off a wall, into an island or
//! across a neighbouring turn. Links, returns, plunges and leads are
//! posted by their own laws and never pass through here.
//!
//! Provenance: circular interpolation per RS-274 / ISO 6983 (G2/G3);
//! the inscribed/circumscribed polygon bound and the orientation
//! predicate of plane geometry (O'Rourke, "Computational Geometry in
//! C"); the uniform-grid broad phase (Franklin 1989). No vendor material
//! consulted.

use std::f64::consts::{FRAC_PI_2, TAU};

use crate::geo2d::{Point2, Region};

use super::arc_biarc::refit;
use super::arcs::{ARC_FIT_TOL, Seg, fit_arcs};
use super::slots::chord_clears;

/// The most chords one arc's proof fan may hold (a sweep of at most half
/// a turn at the fitter's own split never needs it; the cap only bounds
/// a degenerate radius).
const MAX_CHORDS: usize = 512;

/// What the proof did over a plan: arcs kept (biarc arcs included),
/// arcs refused by the area, kink and crossing laws, and the kinked arcs
/// refitted as tangent biarcs.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ProofTally {
    pub arcs: usize,
    pub refused: usize,
    pub kinked: usize,
    pub crossed: usize,
    pub biarcs: usize,
}

impl ProofTally {
    /// The note when any arc was refused (none: nothing to say).
    pub(crate) fn note(&self) -> Option<String> {
        (self.refused + self.kinked + self.crossed > 0).then(|| {
            format!(
                "arc output: of {} fitted arcs, {} could not be proved inside the cutter-centre area, {} would add a kink and {} would cross another feed - those stay chords",
                self.arcs + self.refused + self.kinked + self.crossed,
                self.refused,
                self.kinked,
                self.crossed
            )
        })
    }
}

/// Fit `pass` (its first point is where the tool stands) with `tol`,
/// then prove every arc against `legal`; the walk past the first point.
/// An arc the kink law refuses is fitted again as tangent biarcs
/// (`arc_biarc`, issue #11) under the same proofs before it is given up.
pub(crate) fn fit_proved(pass: &[Point2], tol: f64, legal: &Region, tally: &mut ProofTally) -> Vec<Seg> {
    let fitted = fit_arcs(pass, tol);
    judge(pass, fitted, |(i, j), group| {
        let [Seg::Arc { center, ccw, .. }] = group else {
            tally.refused += 1;
            return None;
        };
        let (from, to) = (pass[i], pass[j]);
        if !proved(from, *center, *ccw, to, legal) {
            tally.refused += 1;
            return None;
        }
        if !adds_no_kink(pass, (i, j), *center, *ccw) {
            return match refit(pass, (i, j), tol, legal) {
                Some((segs, n)) => {
                    tally.arcs += n;
                    tally.biarcs += 1;
                    Some(segs)
                }
                None => {
                    tally.kinked += 1;
                    None
                }
            };
        }
        tally.arcs += 1;
        Some(group.to_vec())
    })
    .unwrap_or_else(|| plain(pass, tally))
}

/// The unsigned turn from `u` to `v` (radians; zero where either is no
/// direction).
fn turn(u: Point2, v: Point2) -> f64 {
    let (lu, lv) = (u.x.hypot(u.y), v.x.hypot(v.y));
    if !(lu > 0.0 && lv > 0.0 && lu.is_finite() && lv.is_finite()) {
        return 0.0;
    }
    let c = ((u.x * v.x + u.y * v.y) / (lu * lv)).clamp(-1.0, 1.0);
    c.acos()
}

/// The kink law on the fit: at each end of the arc over `pass[i..=j]`
/// the turn between the arc's tangent and the neighbouring chord may not
/// exceed the turn the plan's own chords made there - fitting never adds
/// a kink the plan did not have (the 5 deg in-cut law is the plan's to
/// hold, and the fit is held to it junction by junction). An end at the
/// pass's own end has no neighbour and nothing to compare.
fn adds_no_kink(pass: &[Point2], (i, j): (usize, usize), center: Point2, ccw: bool) -> bool {
    let tangent = |p: Point2| {
        let (dx, dy) = (p.x - center.x, p.y - center.y);
        if ccw { Point2::new(-dy, dx) } else { Point2::new(dy, -dx) }
    };
    kink_free(pass, (i, j), tangent(pass[i]), tangent(pass[j]))
}

/// The kink law with the fit's own tangents given: `start` the travel
/// at `pass[i]`, `end` the travel at `pass[j]`.
pub(super) fn kink_free(pass: &[Point2], (i, j): (usize, usize), start: Point2, end: Point2) -> bool {
    let chord = |a: usize, b: usize| Point2::new(pass[b].x - pass[a].x, pass[b].y - pass[a].y);
    const SLACK: f64 = 1e-9;
    if j >= pass.len() {
        return false;
    }
    let start_ok = i == 0 || i + 1 > j || {
        let inc = chord(i - 1, i);
        turn(inc, start) <= turn(inc, chord(i, i + 1)) + SLACK
    };
    let end_ok = j + 1 >= pass.len() || j == 0 || {
        let out = chord(j, j + 1);
        turn(end, out) <= turn(chord(j - 1, j), out) + SLACK
    };
    start_ok && end_ok
}

/// Walk `segs` over `pass`, handing each arc group to `keep` with its
/// vertex span: what it returns stands in for the group, `None` hands
/// the span back as the plan's own vertices. A group is the arcs up to
/// one ending on a vertex of `pass` - one arc of the single fit, or a
/// biarc whose joint lies between vertices. Every group ends on a vertex,
/// so the walk's cursor is an index into the pass; `None` when it does
/// not (never expected).
pub(super) fn judge(pass: &[Point2], segs: Vec<Seg>, mut keep: impl FnMut((usize, usize), &[Seg]) -> Option<Vec<Seg>>) -> Option<Vec<Seg>> {
    let mut out: Vec<Seg> = Vec::with_capacity(segs.len());
    let mut group: Vec<Seg> = Vec::new();
    let mut i = 0usize;
    for seg in segs {
        match seg {
            Seg::Line(qs) => {
                if !group.is_empty() {
                    return None;
                }
                i = i.saturating_add(qs.len());
                push_line(&mut out, qs);
            }
            Seg::Arc { center, ccw, to } => {
                pass.get(i)?;
                group.push(Seg::Arc { center, ccw, to });
                let Some(j) = (i + 1..pass.len()).find(|&k| pass[k] == to) else { continue };
                let span = std::mem::take(&mut group);
                match keep((i, j), &span) {
                    Some(rep) => {
                        for s in rep {
                            match s {
                                Seg::Line(qs) => push_line(&mut out, qs),
                                arc => out.push(arc),
                            }
                        }
                    }
                    None => push_line(&mut out, pass[i + 1..=j].to_vec()),
                }
                i = j;
            }
        }
    }
    group.is_empty().then_some(out)
}

/// The honest answer when the fitter's walk cannot be mapped back onto
/// the pass (never expected): the plan's own chords, every arc refused.
pub(super) fn plain(pass: &[Point2], tally: &mut ProofTally) -> Vec<Seg> {
    tally.refused += 1;
    vec![Seg::Line(pass.get(1..).unwrap_or(&[]).to_vec())]
}

/// Append a straight run, merged onto a preceding one.
pub(super) fn push_line(out: &mut Vec<Seg>, qs: Vec<Point2>) {
    match out.last_mut() {
        Some(Seg::Line(prev)) => prev.extend(qs),
        _ => out.push(Seg::Line(qs)),
    }
}

/// `v` turned by `t` radians.
fn rot(v: Point2, t: f64) -> Point2 {
    let (s, c) = t.sin_cos();
    Point2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// The arc `from -> to` about `center`, bracketed: the inscribed chord
/// fan (sagitta at most [`ARC_FIT_TOL`]) and the circumscribed tangent
/// polyline, both from `from` to `to`. `None` where the arc has no
/// finite, positive radius or sweep.
pub(super) fn bracket(from: Point2, center: Point2, ccw: bool, to: Point2) -> Option<(Vec<Point2>, Vec<Point2>)> {
    let s0 = Point2::new(from.x - center.x, from.y - center.y);
    let r = s0.x.hypot(s0.y);
    if !(r.is_finite() && r > 0.0) {
        return None;
    }
    let a0 = s0.y.atan2(s0.x);
    let a1 = (to.y - center.y).atan2(to.x - center.x);
    let sweep = (if ccw { a1 - a0 } else { a0 - a1 }).rem_euclid(TAU);
    if !(sweep.is_finite() && sweep > 0.0) {
        return None;
    }
    // The fan's chord angle: the one whose sagitta r (1 - cos(d/2)) is
    // the tolerance.
    let half = if ARC_FIT_TOL >= r { FRAC_PI_2 } else { (1.0 - ARC_FIT_TOL / r).acos() };
    let k = (sweep / (2.0 * half)).ceil().max(1.0);
    if !(k.is_finite() && k <= MAX_CHORDS as f64) {
        return None;
    }
    let k = k as usize;
    let step = if ccw { sweep / k as f64 } else { -sweep / k as f64 };
    let at = |t: f64, scale: f64| {
        let v = rot(s0, t);
        Point2::new(center.x + v.x * scale, center.y + v.y * scale)
    };
    let mut fan = Vec::with_capacity(k + 1);
    fan.push(from);
    fan.extend((1..k).map(|i| at(step * i as f64, 1.0)));
    fan.push(to);
    // The tangents' meeting points at the mid-angles, r / cos(d/2) out.
    let grow = 1.0 / (step.abs() * 0.5).cos();
    let mut hull = Vec::with_capacity(k + 2);
    hull.push(from);
    hull.extend((0..k).map(|i| at(step * (i as f64 + 0.5), grow)));
    hull.push(to);
    Some((fan, hull))
}

/// Both bracketing polylines inside `legal`, chord by chord.
pub(super) fn proved(from: Point2, center: Point2, ccw: bool, to: Point2, legal: &Region) -> bool {
    let Some((fan, hull)) = bracket(from, center, ccw, to) else {
        return false;
    };
    let inside = |pts: &[Point2]| pts.windows(2).all(|w| chord_clears(legal, w[0], w[1]));
    inside(&fan) && inside(&hull)
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::geo2d::Loop;

    pub(crate) fn disc(cx: f64, cy: f64, r: f64, n: usize) -> Region {
        let pts = (0..n)
            .map(|k| {
                let a = k as f64 * TAU / n as f64;
                Point2::new(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();
        let Ok(l) = Loop::new(pts) else { panic!("loop") };
        let Ok(r) = Region::new(vec![l]) else { panic!("region") };
        r
    }

    pub(crate) fn ring(r: f64) -> Vec<Point2> {
        (0..=400)
            .map(|k| {
                let a = k as f64 * TAU / 400.0;
                Point2::new(r * a.cos(), r * a.sin())
            })
            .collect()
    }

    /// A finely tessellated ring well inside the area fits as arcs,
    /// every one proved; the same ring pressed onto the wall is refused
    /// back to its own chords, point for point.
    #[test]
    fn arcs_inside_are_kept_and_arcs_on_the_wall_stay_chords() {
        let legal = disc(0.0, 0.0, 10.0, 720);
        let mut t = ProofTally::default();
        let segs = fit_proved(&ring(6.0), ARC_FIT_TOL * 0.25, &legal, &mut t);
        assert!(t.arcs >= 2 && t.refused == 0, "{t:?}");
        assert!(segs.iter().all(|s| matches!(s, Seg::Arc { .. })), "{segs:?}");
        // On the wall: the circumscribed hull stands outside the area.
        let wall = ring(10.0);
        let mut t = ProofTally::default();
        let segs = fit_proved(&wall, ARC_FIT_TOL * 0.25, &legal, &mut t);
        assert!(t.arcs == 0 && t.refused >= 1, "{t:?}");
        let walked: Vec<Point2> = segs
            .iter()
            .flat_map(|s| match s {
                Seg::Line(q) => q.clone(),
                Seg::Arc { to, .. } => vec![*to],
            })
            .collect();
        assert_eq!(walked, wall[1..].to_vec(), "the plan's own chords, point for point");
    }

    /// The bracket: the fan's vertices sit on the circle, the hull's
    /// stand outside it by the fan's sagitta at most, both end exactly.
    #[test]
    fn the_bracket_squeezes_the_arc() {
        let c = Point2::new(1.0, 2.0);
        let from = Point2::new(6.0, 2.0);
        let to = Point2::new(1.0, 7.0);
        let Some((fan, hull)) = bracket(from, c, true, to) else { panic!("bracket") };
        assert!(fan.len() >= 3 && hull.len() >= 3);
        assert_eq!((fan[0], *fan.last().unwrap_or(&c)), (from, to));
        assert_eq!((hull[0], *hull.last().unwrap_or(&c)), (from, to));
        for p in &fan {
            assert!(((p.x - c.x).hypot(p.y - c.y) - 5.0).abs() < 1e-9);
        }
        for p in &hull[1..hull.len() - 1] {
            let d = (p.x - c.x).hypot(p.y - c.y) - 5.0;
            assert!(d > 0.0 && d <= ARC_FIT_TOL + 1e-9, "{d}");
        }
        for w in fan.windows(2) {
            let m = Point2::new((w[0].x + w[1].x) * 0.5, (w[0].y + w[1].y) * 0.5);
            assert!(5.0 - (m.x - c.x).hypot(m.y - c.y) <= ARC_FIT_TOL + 1e-9);
        }
        assert!(bracket(from, c, true, from).is_none(), "zero sweep");
        assert!(bracket(c, c, true, to).is_none(), "zero radius");
    }
}
