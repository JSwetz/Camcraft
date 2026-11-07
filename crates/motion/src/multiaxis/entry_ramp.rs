//! Sloped entries for the posed raster kinds (#15, M11.3 owed): Rotary
//! Parallel's radial infeeds and Swarf's first raster entry, which the
//! finish's ramp retry never reached (the radial relink rebuilt every
//! infeed as a radial plunge; the raster's first descent is a straight
//! drop the ring-only `entry_ramp` law does not touch).
//!
//! Both entries become a straight ramp onto the pass at the finish's
//! `ramp_angle`, arriving along the pass's own first direction:
//! Rotary Parallel descends from the retract cylinder in the landing's
//! meridian while advancing along the axis; Swarf drops in air to the
//! Top of Part Z behind the first line's start and ramps down the
//! line's tangent onto it. Each ramp is PROVED clear of the part before
//! it replaces the plunge ([`segment_clear`]: the ball centre sampled
//! at a spacing whose Lipschitz margin covers the gaps), and where no
//! proof exists the plunge stays - the centre-cutting law then refuses
//! a tool that is not centre-cutting by name, exactly as before.
//!
//! Provenance: ramped (inclined) entry of non-centre-cutting end mills
//! from machining handbooks; the 1-Lipschitz distance sampling proof is
//! elementary analysis. No vendor material consulted.

use crate::cl::{Move, MoveKind, Toolpath};
use crate::finishing3d::closest_pt_tri;
use crate::geometry::{Point3, Triangle};

/// The steepest ramp these entries take (degrees off the pass plane):
/// a posed lead can tilt the axis toward the ramp, and the plunge law's
/// cone is 30 degrees about the axis, so a 45-degree ramp keeps a
/// 15-degree lean of margin.
const MAX_RAMP_DEG: f64 = 45.0;

/// The ramp's run for a drop of `h` at the finish's `ramp_deg`
/// (clamped to `(1, MAX_RAMP_DEG]`); None for a drop not worth a ramp.
pub(super) fn ramp_run(h: f64, ramp_deg: f64) -> Option<f64> {
    if !(h.is_finite() && h > 1e-6 && ramp_deg.is_finite()) {
        return None;
    }
    let a = ramp_deg.clamp(1.0, MAX_RAMP_DEG).to_radians();
    let l = h / a.tan();
    (l.is_finite() && l > 0.0).then_some(l)
}

/// Facet boxes, one per triangle, for the distance shell reject.
pub(super) fn boxes(facets: &[Triangle]) -> Vec<[f64; 6]> {
    facets
        .iter()
        .map(|t| {
            let (a, b, c) = (t.a, t.b, t.c);
            [
                a.x.min(b.x).min(c.x),
                a.y.min(b.y).min(c.y),
                a.z.min(b.z).min(c.z),
                a.x.max(b.x).max(c.x),
                a.y.max(b.y).max(c.y),
                a.z.max(b.z).max(c.z),
            ]
        })
        .collect()
}

/// Does a ball of radius `r` whose CENTRE runs `a -> b` stay at least
/// `r - tol` from every facet? Distance to a set is 1-Lipschitz, so
/// samples spaced `s` that each stand `r - tol + s/2` off prove the
/// whole segment. Non-finite input fails closed.
pub(super) fn segment_clear(a: Point3, b: Point3, r: f64, tol: f64, facets: &[Triangle], bx: &[[f64; 6]]) -> bool {
    let min = r - tol.max(0.0);
    if !(a.is_finite() && b.is_finite() && min.is_finite() && min > 0.0) {
        return false;
    }
    let d = b - a;
    let len = d.dot(d).sqrt();
    let k = ((len / (r * 0.05).max(1e-3)).ceil() as usize).clamp(1, 8192);
    let need = min + 0.5 * len / k as f64;
    (0..=k).all(|j| {
        let t = j as f64 / k as f64;
        nearest(Point3::new(a.x + d.x * t, a.y + d.y * t, a.z + d.z * t), facets, bx, need) >= need
    })
}

/// Distance from `s` to the nearest facet, saturated at `min`.
fn nearest(s: Point3, facets: &[Triangle], bx: &[[f64; 6]], min: f64) -> f64 {
    let mut best = min;
    for (f, b) in facets.iter().zip(bx) {
        let gx = (b[0] - s.x).max(0.0).max(s.x - b[3]);
        let gy = (b[1] - s.y).max(0.0).max(s.y - b[4]);
        let gz = (b[2] - s.z).max(0.0).max(s.z - b[5]);
        if (gx * gx + gy * gy + gz * gz).sqrt() >= best {
            continue;
        }
        let q = closest_pt_tri(s, &[f.a, f.b, f.c]);
        let dv = s - q;
        best = best.min(dv.dot(dv).sqrt());
    }
    best
}

/// Swarf's first raster entry, led in (#15): the plan's first cutting
/// move, when it is a straight drop onto the first line, becomes an
/// air drop to `top` behind the line's start and a ramp down the
/// line's own first direction onto it - both proved clear of the part
/// for the ball (`r`, centres `r` up Z: the plan's tip convention).
/// Returns the path and whether it led in; an entry with no proof, no
/// drop below `top`, or no lateral first cut is returned untouched.
pub(super) fn lead_first_entry(base: &Toolpath, facets: &[Triangle], r: f64, top: f64, ramp_deg: f64, tol: f64, feed: f64) -> (Toolpath, bool) {
    let ms = &base.moves;
    let Some(i) = ms.iter().position(|m| m.kind != MoveKind::Rapid) else {
        return (base.clone(), false);
    };
    let m = &ms[i];
    let drop = m.arc.is_none() && (m.to.x - m.from.x).hypot(m.to.y - m.from.y) <= 1e-9 && m.to.z < m.from.z;
    let next = ms.get(i + 1).filter(|n| n.kind != MoveKind::Rapid && n.arc.is_none());
    let (Some(n), true) = (next, drop) else {
        return (base.clone(), false);
    };
    let (tx, ty) = (n.to.x - n.from.x, n.to.y - n.from.y);
    let tl = tx.hypot(ty);
    let land = m.to;
    let h = top.min(m.from.z) - land.z;
    let Some(l) = ramp_run(h, ramp_deg).filter(|_| tl.is_finite() && tl > 1e-9) else {
        return (base.clone(), false);
    };
    let s = Point3::new(land.x - tx / tl * l, land.y - ty / tl * l, land.z + h);
    let hi = Point3::new(s.x, s.y, m.from.z);
    let up = |q: Point3| Point3::new(q.x, q.y, q.z + r);
    let bx = boxes(facets);
    let proved = segment_clear(up(m.from), up(hi), r, tol, facets, &bx)
        && segment_clear(up(hi), up(s), r, tol, facets, &bx)
        && segment_clear(up(s), up(land), r, tol, facets, &bx);
    if !proved {
        return (base.clone(), false);
    }
    let mut out: Vec<Move> = Vec::with_capacity(ms.len() + 2);
    out.extend(ms[..i].iter().cloned());
    out.push(Move::new(MoveKind::Rapid, m.from, hi, None));
    out.push(Move::new(m.kind, hi, s, m.feed));
    out.push(Move::new(MoveKind::Feed, s, land, Some(feed)));
    out.extend(ms[i + 1..].iter().cloned());
    let mut path = Toolpath::new(out);
    path.notes = base.notes.clone();
    path.cycles = base.cycles.clone();
    (path, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    /// A flat square plate at z = 0, 20 mm on a side about the origin.
    fn plate() -> Vec<Triangle> {
        vec![
            Triangle::new(p(-10.0, -10.0, 0.0), p(10.0, -10.0, 0.0), p(10.0, 10.0, 0.0)),
            Triangle::new(p(-10.0, -10.0, 0.0), p(10.0, 10.0, 0.0), p(-10.0, 10.0, 0.0)),
        ]
    }

    #[test]
    fn the_lipschitz_proof_passes_air_and_fails_a_gouge() {
        let f = plate();
        let bx = boxes(&f);
        assert!(segment_clear(p(-5.0, 0.0, 3.1), p(5.0, 0.0, 3.1), 3.0, 0.01, &f, &bx));
        assert!(!segment_clear(p(-5.0, 0.0, 3.0), p(5.0, 0.0, 2.5), 3.0, 0.01, &f, &bx));
        assert!(!segment_clear(p(f64::NAN, 0.0, 3.0), p(5.0, 0.0, 3.0), 3.0, 0.01, &f, &bx));
    }

    #[test]
    fn swarf_first_entry_leads_in_down_the_lines_tangent_or_stays_put() {
        // A drop from clearance onto x = -12 (off the plate's edge, the
        // tip at z = -2 below its top), then the first line along +x.
        let f = plate();
        let r = 3.0;
        let mv = |k, a, b| Move::new(k, a, b, Some(100.0));
        let base = Toolpath::new(vec![
            Move::new(MoveKind::Rapid, p(0.0, 0.0, 25.0), p(-14.0, 0.0, 25.0), None),
            mv(MoveKind::Plunge, p(-14.0, 0.0, 25.0), p(-14.0, 0.0, -2.0)),
            mv(MoveKind::Feed, p(-14.0, 0.0, -2.0), p(-13.0, 0.0, -2.0)),
        ]);
        let (led, ok) = lead_first_entry(&base, &f, r, 0.0, 45.0, 0.01, 500.0);
        assert!(ok, "air behind the start: the entry leads in");
        let ramp = &led.moves[3];
        assert_eq!((ramp.to, ramp.kind), (p(-14.0, 0.0, -2.0), MoveKind::Feed));
        assert!((ramp.from.x + 16.0).abs() < 1e-9 && ramp.from.z == 0.0, "45-degree ramp from 2 mm back: {:?}", ramp.from);
        // A shelf at z = 4 behind a landing on the plate: a 10-degree ramp
        // from the Top of Part Z (5) would cut through it - no proof, the
        // plunge stays.
        let mut f = f;
        f.push(Triangle::new(p(-10.0, -10.0, 4.0), p(-1.0, -10.0, 4.0), p(-1.0, 10.0, 4.0)));
        f.push(Triangle::new(p(-10.0, -10.0, 4.0), p(-1.0, 10.0, 4.0), p(-10.0, 10.0, 4.0)));
        let onto = Toolpath::new(vec![
            Move::new(MoveKind::Rapid, p(0.0, 0.0, 25.0), p(5.0, 0.0, 25.0), None),
            mv(MoveKind::Plunge, p(5.0, 0.0, 25.0), p(5.0, 0.0, 0.0)),
            mv(MoveKind::Feed, p(5.0, 0.0, 0.0), p(6.0, 0.0, 0.0)),
        ]);
        let (same, ok) = lead_first_entry(&onto, &f, r, 5.0, 10.0, 0.01, 500.0);
        assert!(!ok && same.moves == onto.moves, "no proof, no change");
    }
}
