//! The shape half of the quality report: tangent continuity and
//! curvature of the in-cut motion (provenance in `quality/mod.rs`).

use crate::cl::Move;
use crate::geo2d::Point2;

use super::{PathQuality, planar_feed};

/// Unit XY tangents of a move at its start and its end: the chord
/// direction for a line, the circle's tangent for an XY arc. `None`
/// for a move with no XY extent.
#[must_use]
pub fn tangents(m: &Move) -> Option<(Point2, Point2)> {
    let (dx, dy) = (m.to.x - m.from.x, m.to.y - m.from.y);
    match m.arc {
        Some(arc) if arc.plane.is_xy() => {
            let o = arc.center;
            let t = |x: f64, y: f64| {
                let (rx, ry) = (x - o.x, y - o.y);
                let l = rx.hypot(ry);
                if l <= 1e-12 {
                    return None;
                }
                Some(if arc.ccw {
                    Point2::new(-ry / l, rx / l)
                } else {
                    Point2::new(ry / l, -rx / l)
                })
            };
            Some((t(m.from.x, m.from.y)?, t(m.to.x, m.to.y)?))
        }
        _ => {
            let l = dx.hypot(dy);
            if l <= 1e-9 {
                return None;
            }
            let u = Point2::new(dx / l, dy / l);
            Some((u, u))
        }
    }
}

/// Angle between two unit vectors, degrees.
fn turn_deg(a: Point2, b: Point2) -> f64 {
    (a.x * b.x + a.y * b.y).clamp(-1.0, 1.0).acos().to_degrees()
}

fn xy_len(m: &Move) -> f64 {
    match m.arc {
        Some(arc) if arc.plane.is_xy() => {
            let o = arc.center;
            let r = (m.from.x - o.x).hypot(m.from.y - o.y);
            let a0 = (m.from.y - o.y).atan2(m.from.x - o.x);
            let a1 = (m.to.y - o.y).atan2(m.to.x - o.x);
            let mut sweep = if arc.ccw { a1 - a0 } else { a0 - a1 };
            if sweep <= 0.0 {
                sweep += std::f64::consts::TAU;
            }
            r * sweep
        }
        _ => (m.to.x - m.from.x).hypot(m.to.y - m.from.y),
    }
}

/// Count the curvature breaks of consecutive planar feeds at one Z
/// whose junction is in cut (either side's adjacent sample engages more
/// than `cut_deg`), and the tightest in-cut curvature: an arc's
/// own `1/R`, a line-to-line joint's turn over its mean leg length.
pub(super) fn breaks(moves: &[Move], ends: &[(f64, f64)], (break_deg, cut_deg): (f64, f64), q: &mut PathQuality) {
    for (i, m) in moves.iter().enumerate() {
        if !planar_feed(m) {
            continue;
        }
        let engaged = ends.get(i).is_some_and(|e| e.0.max(e.1) > cut_deg);
        if engaged && let Some(arc) = m.arc
            && arc.plane.is_xy()
        {
            let rr = (m.from.x - arc.center.x).hypot(m.from.y - arc.center.y);
            if rr > 1e-9 {
                q.max_curvature = q.max_curvature.max(1.0 / rr);
            }
        }
        let Some(n) = moves.get(i + 1) else { continue };
        if !planar_feed(n) || (n.from.z - m.to.z).abs() > 1e-9 {
            continue;
        }
        let at_joint = ends.get(i).map_or(0.0, |e| e.1).max(ends.get(i + 1).map_or(0.0, |e| e.0));
        if at_joint <= cut_deg {
            continue;
        }
        let (Some((_, tm)), Some((tn, _))) = (tangents(m), tangents(n)) else {
            continue;
        };
        let turn = turn_deg(tm, tn);
        let legs = (xy_len(m) + xy_len(n)) * 0.5;
        if m.arc.is_none() && n.arc.is_none() && legs > 1e-9 {
            q.max_curvature = q.max_curvature.max(turn.to_radians() / legs);
        }
        if turn > break_deg {
            q.breaks += 1;
            q.break_list.push((m.to, turn));
            if turn > q.worst_break_deg {
                q.worst_break_deg = turn;
                q.worst_break_at = m.to;
            }
        }
    }
}
