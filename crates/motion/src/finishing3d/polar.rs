//! Split from `finishing3d.rs` by M5.0.1; structure only.

use crate::cl::{PathBuilder, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::params::{Finish3DParams, refinement_floor, scallop_stepdown, scallop_stepover, validate};
use super::support::{build_sups, plunge_support, plunge_support_grad};
use super::march::{emit_runs, march_curve, march_curve_blind};
use super::raster::{placement_guard, rings_and_tail, shallow_footprint};
/// Radial finishing (M2.2d): `N = ceil(PI·R/s)` CHORDS through the
/// shallow terrain's footprint center, each rotated `PI/N` from the
/// last - so arc spacing at the rim meets the cos-law stepover, and
/// toward the center the chords overlap ever more (correct, merely
/// redundant: the cusp law is a bound, never a quota). Each chord
/// is one `march_curve` run: validity bisection digs its plunges
/// inside the material edge, the chord budget keeps the string of
/// samples under a quarter of `tol`, and the discontinuity break
/// cuts the chord into pieces at shelf mouths - no cut move spans
/// two surfaces, radial included. Alternating chords run inward,
/// the serpentine law with a different topology. Then the rings
/// take the flanks, as in every raster-family close.
pub fn radial(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    let r = p.tool_diameter * 0.5;
    let sups = build_sups(facets, r);
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    let mut emitted = false;
    if let Some((c, radius, mz)) = shallow_footprint(&sups) {
        let step = p.stepover.unwrap_or(scallop_stepover(r, p.tolerance) * mz);
        let rad = radius + step;
        const MAX_CHORDS: f64 = 8192.0;
        let n = (std::f64::consts::FRAC_PI_2 * 2.0 * rad / step).ceil().max(1.0);
        placement_guard(step, n, "radial", MAX_CHORDS)?;
        let delta0 = scallop_stepdown(r, p.tolerance).max(1e-6);
        let trimmed = std::cell::Cell::new(0usize);
        let holes = HoleCount::new();
        let sup = |q: Point2| {
            let (c, cut) = plunge_support(q.x, q.y, r, p.bottom_z, p.clearance_z + r, &sups);
            if cut {
                trimmed.set(trimmed.get() + 1);
            }
            holes.bite(c.is_none());
            c
        };
        let mut runs: Vec<Vec<(Point2, f64)>> = Vec::new();
        for k in 0..n as i64 {
            let th = std::f64::consts::PI * k as f64 / n;
            let (dx, dy) = (th.cos(), th.sin());
            let pos = |t: f64| Point2::new(c.x + t * dx, c.y + t * dy);
            let mut pieces = march_curve(
                -rad,
                rad,
                &pos,
                &|a, z| (z - a).abs(),
                &|_| 1.0,
                &sup,
                delta0,
                p.tolerance,
                p.tolerance * 0.25,
                refinement_floor(delta0, p.min_step),
            );
            if k % 2 == 1 {
                for pc in pieces.iter_mut() {
                    pc.reverse();
                }
            }
            runs.extend(pieces);
        }
        if !runs.is_empty() {
            emit_runs(&mut b, p, r, &sups, &runs, false);
            emitted = true;
        }
        if trimmed.get() > 0 {
            b.note(format!(
                "radial: {} ground samples refused - their plunge column from clearance crosses the solid; what hides there stands as rest, reported not cut",
                trimmed.get()
            ));
        }
        holes.note(&mut b, "radial");
    }
    rings_and_tail(b, p, r, facets, emitted)
}

/// Spiral finishing (M2.2d): ONE Archimedean curve,
/// `r(θ) = R - s·θ/2π`, in from the silhouette to within a half
/// stepover of the pole - the retract-free continuous cut the
/// raster families pay for in hops and rapids. The march rides the
/// ANGLE parameter: `unit(θ) = 1/max(r, 4·delta0)` keeps the XY
/// chord per sample inside the stepdown budget at every radius
/// (the apex law - near the pole the angle opens wide while the
/// chord holds, and `dist` speaks true millimeters so the break
/// law and the refinement never read the parameter as ground),
/// and the curve ENDS a half-step from the center: the next turn
/// would only retrace the last. A spiral piece that breaks at a
/// shelf mouth re-enters at the hop, like every other ground line.
/// Then the rings take the flanks.
pub fn spiral(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    let r = p.tool_diameter * 0.5;
    let sups = build_sups(facets, r);
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    let mut emitted = false;
    if let Some((c, radius, mz)) = shallow_footprint(&sups) {
        let step = p.stepover.unwrap_or(scallop_stepover(r, p.tolerance) * mz);
        let rad = radius + step;
        const MAX_TURNS: f64 = 8192.0;
        let turns = (rad / step).ceil().max(1.0);
        placement_guard(step, turns, "spiral", MAX_TURNS)?;
        let delta0 = scallop_stepdown(r, p.tolerance).max(1e-6);
        let th_end = std::f64::consts::TAU * (rad - step * 0.5) / step;
        let r_of = |th: f64| rad - step * th / std::f64::consts::TAU;
        let trimmed = std::cell::Cell::new(0usize);
        let holes = HoleCount::new();
        let sup = |q: Point2| {
            let (c, cut) = plunge_support(q.x, q.y, r, p.bottom_z, p.clearance_z + r, &sups);
            if cut {
                trimmed.set(trimmed.get() + 1);
            }
            holes.bite(c.is_none());
            c
        };
        let pos = |th: f64| {
            let rr = r_of(th).max(0.0);
            let (cs, sn) = th.sin_cos();
            Point2::new(c.x + rr * cs, c.y + rr * sn)
        };
        let dist = |a: f64, z: f64| {
            let (pa, pb) = (pos(a), pos(z));
            (pb.x - pa.x).hypot(pb.y - pa.y)
        };
        let unit = |th: f64| 1.0 / r_of(th).max(4.0 * delta0);
        let pieces = march_curve_blind(
            0.0,
            th_end,
            &pos,
            &dist,
            &unit,
            &sup,
            delta0,
            p.tolerance,
            p.tolerance * 0.25,
            refinement_floor(delta0, p.min_step),
        );
        if !pieces.is_empty() {
            emit_runs(&mut b, p, r, &sups, &pieces, false);
            emitted = true;
        }
        if trimmed.get() > 0 {
            b.note(format!(
                "spiral: {} ground samples refused - their plunge column from clearance crosses the solid; what hides there stands as rest, reported not cut",
                trimmed.get()
            ));
        }
        holes.note(&mut b, "spiral");
    }
    rings_and_tail(b, p, r, facets, emitted)
}

/// The morphed spiral (M2.5 C): the spiral's one in-winding curve,
/// but the radius advances by the LOCAL cos-law instead of the
/// footprint's worst tilt - the table `r(θ)` is integrated ahead of
/// the march from the same ground the march will ask, sixteen
/// samples per turn, each step narrowed by `1/√(1+g²)` where `g` is
/// the support's radial gradient measured across the step it is
/// setting. The gradient is clamped at one: past a slope of 1:1 the
/// ground belongs to the rings, and a spiral that kept narrowing
/// there would spend its turns climbing one step at a time. Flat
/// ground answers `g = 0`, and the morph is the spiral's spacing
/// again. Everything else is `spiral`'s verbatim - `march_curve`
/// with the apex `unit` law, the half-step stop at the pole, the
/// column-refusal note, the rings on the flanks.
pub fn morphed_spiral(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    let r = p.tool_diameter * 0.5;
    let sups = build_sups(facets, r);
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    let mut emitted = false;
    if let Some((c, radius, _mz)) = shallow_footprint(&sups) {
        // The flat-ground figure: the morph's whole point is that
        // the tilt is paid locally, so the worst-tilt discount the
        // spiral takes once up front is not also taken here.
        let step = p.stepover.unwrap_or(scallop_stepover(r, p.tolerance));
        let rad = radius + step;
        const MAX_TURNS: f64 = 8192.0;
        const PER_TURN: i64 = 16;
        let dth = std::f64::consts::TAU / PER_TURN as f64;
        let delta0 = scallop_stepdown(r, p.tolerance).max(1e-6);
        let trimmed = std::cell::Cell::new(0usize);
        let holes = HoleCount::new();
        let sup = |q: Point2| {
            let (cc, cut) = plunge_support(q.x, q.y, r, p.bottom_z, p.clearance_z + r, &sups);
            if cut {
                trimmed.set(trimmed.get() + 1);
            }
            holes.bite(cc.is_none());
            cc
        };
        // Integrate the radius table from the rim in. A ground
        // sample that answers nothing (air, or a column already
        // refused) reads as flat: the march's own laws decide what
        // that turn is worth.
        let mut tab: Vec<(f64, f64)> = Vec::new();
        let mut th = 0.0f64;
        let mut rr = rad;
        tab.push((th, rr));
        while rr > step * 0.5 && tab.len() < (MAX_TURNS * PER_TURN as f64) as usize + 2 {
            // The ground's radial gradient AT the node: the slope of
            // the support function where its answer comes from one
            // facet plane, -(m_xy . ray)/m_z (the same currency the
            // spiral's worst-tilt `mz` spends, asked locally). A
            // chord across `support_at` was blind on tessellated
            // domes: a ray crossing a ring crest answers None in the
            // thin annulus where the ball straddles facets, and the
            // chord then reads FLAT on ground that is anything but.
            // Air, and columns the plunge law refuses, still read
            // flat - the march's own laws decide what that turn is
            // worth.
            let grad = |da: f64| -> f64 {
                let (sx, sy) = (th + da).sin_cos();
                plunge_support_grad(
                    Point2::new(c.x + rr * sx, c.y + rr * sy),
                    Point2::new(sx, sy),
                    r,
                    p.bottom_z,
                    p.clearance_z + r,
                    &sups,
                )
                .map_or(0.0, |(_cz, gx)| gx.abs())
            };
            // A polar mesh's radial edge lines are exactly where one
            // facet's plane tangent dies in the neighbouring fan
            // (barycentric knife), and the table's own nodes march
            // along azimuth multiples of the node step - right on
            // those lines. Ask a quarter step over too: on a plane
            // ground the two answers are equal; on a mesh they agree
            // anywhere the support answers at all. The wider reading
            // wins: narrowing is the safe direction of the doubt.
            let g = grad(0.0).max(grad(dth * 0.25));
            // cos(alpha) from the radial gradient, clamped at the
            // 45-degree jurisdiction line (g > 1 belongs to the
            // rings, and the spacing stops narrowing there).
            let loc = step / (1.0 + g * g).sqrt().min(std::f64::consts::SQRT_2);
            th += dth;
            rr = (rr - loc * dth / std::f64::consts::TAU).max(0.0);
            tab.push((th, rr));
        }
        placement_guard(step, th / std::f64::consts::TAU, "morphed spiral", MAX_TURNS)?;
        let th_end = th;
        let r_of = |t: f64| -> f64 {
            // Piecewise linear in theta between the anchors; the
            // table is dense (sixteen per turn) and the march
            // bisects its chords anyway.
            let i = match tab.binary_search_by(|&(a, _)| {
                a.partial_cmp(&t).unwrap_or(std::cmp::Ordering::Less)
            }) {
                Ok(i) => i,
                Err(0) => 0,
                Err(i) => i - 1,
            };
            let (a0, r0) = tab[i];
            let (a1, r1) = tab[(i + 1).min(tab.len() - 1)];
            if a1 == a0 {
                return r0;
            }
            let w = ((t - a0) / (a1 - a0)).clamp(0.0, 1.0);
            r0 + (r1 - r0) * w
        };
        let pos = |t: f64| {
            let rr = r_of(t).max(0.0);
            let (cs, sn) = t.sin_cos();
            Point2::new(c.x + rr * cs, c.y + rr * sn)
        };
        let dist = |a: f64, z: f64| {
            let (pa, pb) = (pos(a), pos(z));
            (pb.x - pa.x).hypot(pb.y - pa.y)
        };
        let unit = |t: f64| 1.0 / r_of(t).max(4.0 * delta0);
        let pieces = march_curve_blind(
            0.0,
            th_end,
            &pos,
            &dist,
            &unit,
            &sup,
            delta0,
            p.tolerance,
            p.tolerance * 0.25,
            refinement_floor(delta0, p.min_step),
        );
        if !pieces.is_empty() {
            emit_runs(&mut b, p, r, &sups, &pieces, false);
            emitted = true;
        }
        if trimmed.get() > 0 {
            b.note(format!(
                "morphed spiral: {} ground samples refused - their plunge column from clearance crosses the solid; what hides there stands as rest, reported not cut",
                trimmed.get()
            ));
        }
        holes.note(&mut b, "morphed spiral");
    }
    rings_and_tail(b, p, r, facets, emitted)
}

/// A march's ground may flip from rest to no-rest mid-curve: the
/// plane-tangent law answers nothing where the ball would straddle
/// a facet fold (a tessellated dome's ring crests, a coarse mesh's
/// ridge lines). The march breaks there and the band stands as
/// uncut rest - which must be SAID (log diagnostics, never
/// silence). This counts the flips, once per continuous stretch of
/// none-ground, so the number reads as holes, not samples.
pub(super) struct HoleCount {
    was_some: std::cell::Cell<bool>,
    n: std::cell::Cell<usize>,
}

impl HoleCount {
    fn new() -> Self {
        Self {
            was_some: std::cell::Cell::new(false),
            n: std::cell::Cell::new(0),
        }
    }
    fn bite(&self, is_none: bool) {
        if is_none {
            if self.was_some.get() {
                self.n.set(self.n.get() + 1);
            }
            self.was_some.set(false);
        } else {
            self.was_some.set(true);
        }
    }
    fn note(&self, b: &mut PathBuilder, family: &str) {
        if self.n.get() > 0 {
            b.note(format!(
                "{family}: {} bands where no facet plane rests a ball - the contact straddles a fold of the mesh; those bands stand as rest, reported not cut (a finer mesh, or offset-surface support, closes them)",
                self.n.get()
            ));
        }
    }
}
