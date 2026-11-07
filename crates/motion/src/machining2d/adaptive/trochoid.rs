//! Trochoidal clearing along a channel's medial line (issue #5; Otkur
//! & Lazoglu 2007, "Trochoidal milling", Int. J. Mach. Tools Manuf. 47;
//! Rauch, Duc & Hascoet 2009, "Improving trochoidal tool paths
//! generation and implementation using process constraints modelling",
//! IJMTM 49): loops of radius `rho` about centers advancing `step` a
//! loop along the line (`medial`), the step the largest of the spiral's
//! ladder whose loops read the load bound on the planner's stock.
//!
//! The verifier's crossing law (Feed-Feed crossings at a shared depth
//! hold the post) rules out the textbook full-circle trochoid, whose
//! every loop crosses the one before. Here only the FRONT arc of each
//! loop is cut: two equal circles `step` apart cross on their bisector,
//! `step / 2` from either center - near +-90 deg - so front arcs
//! trimmed to `+-(90 deg - step / rho - margin)` never cross; the air
//! return between them is the caller's (a lifted return, `lift_link`).
//! Ours, no vendor material.

use crate::geo2d::Point2;

use super::front::dist;
use super::medial::{Spine, at_len, lengths};
use super::raster::Raster;

/// The front arc stops this far (radians) short of the bisector the
/// next loop's circle crosses it on.
const ARC_MARGIN: f64 = 0.03;

/// A front arc shorter than this half-angle (radians) cuts too little
/// of the loop: the step is too long for the radius.
const MIN_HALF: f64 = 0.5;

/// Loops at most (the Prime Directive's "never hang").
const MAX_LOOPS: usize = 20_000;

/// The loops tried when a step is chosen (the steady state of the
/// load law is reached within a tool diameter of travel).
const TRIAL_LOOPS: usize = 12;

/// The spine's points, a closed one's first point repeated at the end.
fn closed_pts(spine: &Spine) -> Vec<Point2> {
    let mut pts = spine.points().to_vec();
    if let (Spine::Closed(_), Some(&f)) = (spine, pts.first()) {
        pts.push(f);
    }
    pts
}

/// The front arcs (cutter-center polylines, each tessellated at sagitta
/// `sag`) of the trochoid of loop radius `rho` and `step` along
/// `spine`; CCW for climb. `None`: the step is too long for the radius.
pub(crate) fn front_arcs(spine: &Spine, rho: f64, step: f64, climb: bool, sag: f64) -> Option<Vec<Vec<Point2>>> {
    if !(rho > 0.0 && step > 0.0 && sag > 0.0) {
        return None;
    }
    let half = std::f64::consts::FRAC_PI_2 - step / rho - ARC_MARGIN;
    if half < MIN_HALF {
        return None;
    }
    let pts = closed_pts(spine);
    let len = lengths(&pts);
    let total = len.last().copied().unwrap_or(0.0);
    if total <= 1e-6 {
        return None;
    }
    let open = matches!(spine, Spine::Open(_));
    let n = ((total / step).ceil() as usize).clamp(1, MAX_LOOPS);
    let count = if open { n + 1 } else { n.max(1) };
    let dth = 2.0 * (1.0 - (sag / rho).min(1.0)).acos().max(1e-3);
    let segs = ((2.0 * half / dth).ceil() as usize).clamp(4, 4096);
    // The loop centers, evenly spaced (at most `step` apart: a short
    // last advance would lay two near-equal arcs over one another).
    let centers: Vec<Point2> = (0..count).map(|k| at_len(&pts, &len, (k as f64 * total / n as f64).min(total))).collect();
    let mut out = Vec::with_capacity(count);
    for k in 0..count {
        let c = centers[k];
        // Each arc faces the way its center advances (the neighbors'
        // chord): consecutive arcs then sit one advance apart along
        // their own axis, and cross only on the bisector the trim
        // keeps them short of.
        let (a, b) = if open {
            (centers[k.saturating_sub(1)], centers[(k + 1).min(count - 1)])
        } else {
            (centers[(k + count - 1) % count], centers[(k + 1) % count])
        };
        let l = dist(a, b);
        if l <= 1e-9 {
            continue;
        }
        let tg = Point2::new((b.x - a.x) / l, (b.y - a.y) / l);
        let nm = Point2::new(-tg.y, tg.x);
        let dir = if climb { 1.0 } else { -1.0 };
        let arc = (0..=segs)
            .map(|j| {
                let th = dir * (-half + 2.0 * half * j as f64 / segs as f64);
                let (cs, sn) = (th.cos(), th.sin());
                Point2::new(c.x + rho * (cs * tg.x + sn * nm.x), c.y + rho * (cs * tg.y + sn * nm.y))
            })
            .collect();
        out.push(arc);
    }
    Some(out)
}

/// The arcs walked on `ras` (each from its own first point: the lifted
/// return brought the tool there over cleared stock); the worst reading
/// and where.
pub(crate) fn walk_arcs(ras: &mut Raster, arcs: &[Vec<Point2>], (r, rim, step): (f64, f64, f64)) -> (f64, Point2) {
    let mut worst = (0.0f64, Point2::default());
    for a in arcs {
        let w = ras.walk(a, r, rim, step);
        if w.0 > worst.0 {
            worst = w;
        }
    }
    worst
}

/// The largest step of `ladder` (mm, longest first) whose first loops
/// read under `bound` on `ras`, with its arcs; `None` where none does.
pub(crate) fn choose_step(
    ras: &Raster,
    spine: &Spine,
    (rho, climb, sag): (f64, bool, f64),
    ladder: &[f64],
    (r, rim, walk, bound): (f64, f64, f64, f64),
) -> Option<(f64, Vec<Vec<Point2>>)> {
    for &s in ladder {
        let Some(arcs) = front_arcs(spine, rho, s, climb, sag) else { continue };
        let mut trial = ras.clone();
        let head = &arcs[..arcs.len().min(TRIAL_LOOPS)];
        if walk_arcs(&mut trial, head, (r, rim, walk)).0 <= bound {
            return Some((s, arcs));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo2d::{Loop, Region};
    use crate::load::{DEFAULT_OPTIMAL_LOAD, deg_at_rim, load_at_rim};

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
        let pts = vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)];
        let Ok(l) = Loop::new(pts) else { panic!("loop") };
        l
    }

    /// The 22 x 60 channel (the spiral's GAP fixture).
    #[test]
    fn trochoid_front_arcs_hold_the_load_never_cross_and_stay_in_the_center_area() {
        let Ok(center) = Region::new(vec![rect(5.0, 5.0, 55.0, 17.0)]) else { panic!("region") };
        let (s, _, n) = held(&center);
        assert!(n > 10 && s > 0.1, "step {s}, {n} loops");
    }

    /// The user's 14 mm band round the 60 x 50 part (issue #13, refused
    /// by the spiral's budget): a closed medial line, the same laws.
    #[test]
    fn trochoid_on_the_users_band_holds_the_load_and_never_crosses() {
        let Ok(part) = Region::new(vec![rect(0.0, 0.0, 60.0, 50.0)]) else { panic!("part") };
        let (Ok(o), Ok(i)) = (part.offset(9.0), part.offset(5.0)) else { panic!("offsets") };
        let Ok(band) = o.difference(&i) else { panic!("band") };
        let (s, _, n) = held(&band);
        assert!(n > 100 && s > 0.05, "step {s}, {n} loops");
    }

    /// The trochoid planned on `center` for a D10 at the default load:
    /// its front arcs never cross one another (the verifier's law), never
    /// leave the center area, and - the step chosen on the planner's
    /// stock - read the spiral's load bound. Returns (step, load, loops).
    fn held(center: &Region) -> (f64, f64, usize) {
        let Ok(Ok((spine, c))) = super::super::medial::spine(center) else { panic!("spine") };
        let rho = spine.points().iter().map(|&p| super::super::loops::boundary_dist(center, p)).fold(c, f64::min) - 0.05 * c;
        assert!(rho > 0.6 * c, "rho {rho} of {c}");
        let (r, d) = (5.0, 10.0);
        let Ok(reach) = center.offset(r) else { panic!("reach") };
        let h = d / 60.0;
        let Ok(mut ras) = Raster::from_region(&reach, h, r) else { panic!("ras") };
        let rim = crate::quality::rim_for(d, h);
        let bound = deg_at_rim(DEFAULT_OPTIMAL_LOAD * 1.05, d, rim);
        let c0 = spine.points()[0];
        ras.cut(c0, c0, r + rho);
        let ladder: Vec<f64> = super::super::SPIRAL_STEPS.iter().map(|f| d * DEFAULT_OPTIMAL_LOAD * f).collect();
        let Some((s, arcs)) = choose_step(&ras, &spine, (rho, true, 0.002), &ladder, (r, rim, r / 10.0, bound)) else {
            panic!("no step holds the load")
        };
        let x = super::super::guard::first_crossing(&arcs);
        assert!(x.is_none(), "front arcs cross at {x:?} (step {s}, rho {rho}, {} arcs)", arcs.len());
        assert!(arcs.iter().flatten().all(|&p| center.contains(p) || super::super::loops::boundary_dist(center, p) < 1e-6));
        let worst = walk_arcs(&mut ras, &arcs, (r, rim, r / 10.0));
        let load = load_at_rim(worst.0, d, rim);
        assert!(load <= DEFAULT_OPTIMAL_LOAD * 1.10, "step {s}: load {:.1}% at {:?}", load * 100.0, worst.1);
        eprintln!("trochoid: rho {rho:.2}, step {s:.3}, {} loops, load {:.1}% D", arcs.len(), load * 100.0);
        (s, load, arcs.len())
    }
}
