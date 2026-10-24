//! Split from `finishing3d.rs` by M5.0.1; structure only.

use crate::cl::{PathBuilder, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::params::{Finish3DParams, refinement_floor, scallop_stepdown, scallop_stepover, validate};
use super::support::{build_sups, support_at};
use super::march::{emit_runs, march_curve};
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
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    let mut emitted = false;
    if let Some((c, radius, mz)) = shallow_footprint(&sups) {
        let step = p.stepover.unwrap_or(scallop_stepover(r, p.tolerance) * mz);
        let rad = radius + step;
        const MAX_CHORDS: f64 = 8192.0;
        let n = (std::f64::consts::FRAC_PI_2 * 2.0 * rad / step).ceil().max(1.0);
        placement_guard(step, n, "radial", MAX_CHORDS)?;
        let delta0 = scallop_stepdown(r, p.tolerance).max(1e-6);
        let sup = |q: Point2| support_at(q.x, q.y, r, p.bottom_z, &sups);
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
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
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
        let sup = |q: Point2| support_at(q.x, q.y, r, p.bottom_z, &sups);
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
        let pieces = march_curve(
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
    }
    rings_and_tail(b, p, r, facets, emitted)
}
