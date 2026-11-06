//! Control-side cutter compensation for the contour (M13.11): the
//! door that names what G41/G42 needs of the parameters, and the span
//! builder that records, over a ring already walked at cutter centre,
//! the PART contour the post writes under G41/G42 instead.
//!
//! Provenance: RS-274 / ISO 6983 G41 / G42 / G40 and the published
//! startup / cancel block behaviour of Fanuc-style controls - the offset
//! vector is built over the straight block carrying G41/G42 (its end
//! point lands one radius off the next block's line) and withdrawn over
//! the straight block carrying G40, so both must be linear and at least
//! a radius long; between them the control offsets the programmed
//! contour by the register. The centre moves the generator proved stay
//! the path's truth (the sim and the verifier read them); the
//! programmed geometry rides beside them in [`CompSpan`]. Our own model;
//! no vendor material consulted.

use crate::cl::{CompSide, CompSpan, Move, MoveKind, PathBuilder};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
use crate::links::{Compensation, Link};

use super::arcs::{fit_arcs, Seg};
use super::contour::{ContourParams, Lead, Side};
use super::face::GenError;
use super::ring::dist;

pub(super) fn refused(why: impl std::fmt::Display) -> GenError {
    GenError::Invalid(format!("control compensation {why}"))
}

fn straight(lead: Lead) -> Option<f64> {
    match lead {
        Lead::Linear { length } | Lead::Perpendicular { length } => Some(length),
        _ => None,
    }
}

/// Does `p` ask the control to compensate?
pub(super) fn wanted(p: &ContourParams) -> bool {
    p.links.approach.compensation == Compensation::Control
}

/// The door: every refusal by name, before any geometry is touched.
pub(super) fn door(p: &ContourParams) -> Result<(), GenError> {
    if !wanted(p) {
        return Ok(());
    }
    let r = p.tool_diameter * 0.5;
    for (end, lead) in [("in", p.lead), ("out", p.lead_out_shape())] {
        match straight(lead) {
            None => {
                return Err(refused(format!(
                    "needs a straight lead-{end} (Linear or Perpendicular) of at least the tool radius - \
                     the control builds the offset over that move, and an arc or no lead cannot carry G41/G42"
                )))
            }
            Some(len) if len + 1e-9 < r => {
                return Err(refused(format!(
                    "needs a straight lead-{end} of at least the tool radius ({r:.3} mm) - the lead-{end} is {len:.3} mm"
                )))
            }
            Some(_) => {}
        }
    }
    if p.tab_count > 0 {
        return Err(refused("does not ride tabs - a tab window has no lead to engage on"));
    }
    if p.blend_turns {
        return Err(refused("does not ride corner blends - the control rounds the offset corners itself"));
    }
    if p.links.approach.overlap > 0.0 {
        return Err(refused("does not ride an overlap past the start - the cancel block must leave from the ring's seam"));
    }
    if p.links.level == Link::KeepDown || p.links.pass == Link::KeepDown {
        return Err(refused("needs Clearance or Retract links - a keep-down link engages the next cut without a lead"));
    }
    Ok(())
}

/// The contour the post programs: the region at the stock to leave
/// (the control adds the radius). `None` when the parameters do not
/// ask the control.
pub(super) fn programmed_region(region: &Region, p: &ContourParams) -> Result<Option<Region>, GenError> {
    if !wanted(p) {
        return Ok(None);
    }
    if p.stock_to_leave <= 0.0 {
        return Ok(Some(region.clone()));
    }
    let d = if p.side == Side::Inside { -p.stock_to_leave } else { p.stock_to_leave };
    let prog = region.offset(d)?;
    if prog.is_empty() {
        return Err(refused("found no part contour once the stock to leave was taken off"));
    }
    Ok(Some(prog))
}

/// The ring restarted at the midpoint of its longest straight edge:
/// the seam a control-compensated ring needs. At a vertex seam the
/// offset the control builds over the engage block (square to the
/// first cut) and the one it withdraws over the cancel block (square
/// to the last) differ, so the tool would leave the ring off the
/// proved centre path; mid-run the two coincide and the engage,
/// the offset walk and the cancel reproduce the centre moves exactly.
pub(super) fn seam_mid_run(pts: &[Point2]) -> Vec<Point2> {
    let n = pts.len();
    if n < 3 {
        return pts.to_vec();
    }
    let mut best = (0usize, -1.0f64);
    for i in 0..n {
        let d = dist(pts[i], pts[(i + 1) % n]);
        if d > best.1 {
            best = (i, d);
        }
    }
    let (i, _) = best;
    let (a, b) = (pts[i], pts[(i + 1) % n]);
    let mut v = Vec::with_capacity(n + 1);
    v.push(Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5));
    v.extend((1..=n).map(|k| pts[(i + k) % n]));
    v
}

/// The closest point of `prog`'s boundary to `q`: (loop index, edge
/// index, the point, the distance).
fn closest(prog: &Region, q: Point2) -> Option<(usize, usize, Point2, f64)> {
    let mut best: Option<(usize, usize, Point2, f64)> = None;
    for (li, l) in prog.loops().iter().enumerate() {
        let pts = l.points();
        let n = pts.len();
        for i in 0..n {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let len2 = dx * dx + dy * dy;
            let t = if len2 > 0.0 { (((q.x - a.x) * dx + (q.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
            let c = Point2::new(a.x + dx * t, a.y + dy * t);
            let d = dist(c, q);
            if best.is_none_or(|(_, _, _, bd)| d < bd) {
                best = Some((li, i, c, d));
            }
        }
    }
    best
}

/// What a span needs from the walk it replaces.
pub(super) struct SpanCtx<'a> {
    pub prog: &'a Region,
    pub p: &'a ContourParams,
}

/// Record the span over the moves `first..=last` of `b` - `first` the
/// lead-in move that lands on the wall, `last` the lead-out move that
/// leaves it, the closed ring walked between them at one Z - as the
/// programmed geometry: the same lead-in ending on the part contour,
/// the part contour around (arcs fitted at the op's tolerance), the
/// same lead-out leaving from it. Every refusal names its place.
pub(super) fn push_span(b: &mut PathBuilder, ctx: &SpanCtx, first: usize, last: usize) -> Result<(), GenError> {
    let p = ctx.p;
    let r = p.tool_diameter * 0.5;
    let moves = b.moves();
    let (Some(m_in), Some(m_out)) = (moves.get(first), moves.get(last)) else {
        return Err(refused("found no lead-in or lead-out move to engage on"));
    };
    if first >= last || m_in.arc.is_some() || m_out.arc.is_some() {
        return Err(refused("needs straight lead moves to engage and cancel on"));
    }
    let (m_in, m_out) = (*m_in, *m_out);
    let xy = |q: Point3| Point2::new(q.x, q.y);
    let (s_c, e_c) = (xy(m_in.to), xy(m_out.from));
    let z = m_in.to.z;
    let tol = p.tolerance.max(1e-6);
    for (end, m) in [("in", m_in), ("out", m_out)] {
        let len = dist(xy(m.from), xy(m.to));
        if len + 1e-9 < r {
            return Err(refused(format!(
                "needs a straight lead-{end} of at least the tool radius ({r:.3} mm) at ({:.3}, {:.3}) - the room law left {len:.3} mm",
                s_c.x, s_c.y
            )));
        }
    }
    if dist(s_c, e_c) > 1e-6 {
        return Err(refused(format!("needs a closed ring - the walk from ({:.3}, {:.3}) does not return to its seam", s_c.x, s_c.y)));
    }
    if moves[first + 1..last].iter().any(|m| m.from.z != z || m.to.z != z || m.kind != MoveKind::Feed) {
        return Err(refused("needs one planar feed walk between its leads"));
    }
    let Some((li, ei, s_p, d)) = closest(ctx.prog, s_c) else {
        return Err(refused("found no part contour to program"));
    };
    // The seam law: the ring's first and last cuts are straight and
    // the centre stands square to both at the seam, so the offset the
    // control builds on the engage block is the one it withdraws on
    // the cancel block - and the tool runs the proved centre moves.
    let off = Point2::new(s_c.x - s_p.x, s_c.y - s_p.y);
    for (which, m) in [("first", moves[first + 1]), ("last", moves[last - 1])] {
        let t = Point2::new(m.to.x - m.from.x, m.to.y - m.from.y);
        let len = t.x.hypot(t.y);
        let square = len > 0.0 && (t.x * off.x + t.y * off.y).abs() / len <= tol + 0.02;
        if m.arc.is_some() || !square {
            return Err(refused(format!(
                "needs the ring seam on a straight run - at ({:.3}, {:.3}) the {which} cut is not square to the offset",
                s_c.x, s_c.y
            )));
        }
    }
    // The programmed contour already carries the stock to leave, so
    // the centre stands exactly one radius off it.
    let want = r;
    if (d - want).abs() > tol + 0.02 {
        return Err(refused(format!(
            "found the part contour {d:.3} mm from the cutter centre at ({:.3}, {:.3}), not the tool radius {want:.3} mm",
            s_c.x, s_c.y
        )));
    }
    let Some(l) = ctx.prog.loops().get(li) else {
        return Err(refused("lost its loop"));
    };
    let pts = l.points();
    let n = pts.len();
    if n < 3 {
        return Err(refused("needs a ring of at least three points"));
    }
    // Forward: s_p -> pts[ei+1] -> ... -> pts[ei] -> s_p. Reversed when
    // the centre walk leaves its seam the other way.
    let t_c = moves.get(first + 1).map(|m| Point2::new(m.to.x - m.from.x, m.to.y - m.from.y));
    let fwd: Vec<Point2> = (1..=n).map(|k| pts[(ei + k) % n]).collect();
    let rev: Vec<Point2> = (0..n).map(|k| pts[(ei + n - k) % n]).collect();
    let first_dir = |v: &[Point2]| v.iter().find(|q| dist(**q, s_p) > 1e-9).map(|q| Point2::new(q.x - s_p.x, q.y - s_p.y));
    let along = match (t_c, first_dir(&fwd)) {
        (Some(t), Some(f)) => t.x * f.x + t.y * f.y,
        _ => 1.0,
    };
    let ring = if along >= 0.0 { fwd } else { rev };
    let mut poly = Vec::with_capacity(n + 2);
    poly.push(s_p);
    for q in ring {
        if poly.last().is_none_or(|l| dist(*l, q) > 1e-9) {
            poly.push(q);
        }
    }
    if poly.last().is_none_or(|l| dist(*l, s_p) > 1e-9) {
        poly.push(s_p);
    }
    let walk = fit_arcs(&poly, p.tolerance);
    let feed = moves.get(first + 1).and_then(|m| m.feed).unwrap_or(p.feed);
    let mut programmed = Vec::with_capacity(walk.len() + 2);
    let s3 = Point3::new(s_p.x, s_p.y, z);
    programmed.push(Move::new(MoveKind::Feed, m_in.from, s3, m_in.feed));
    let mut at = s3;
    for seg in &walk {
        match seg {
            Seg::Line(qs) => {
                for q in qs {
                    let to = Point3::new(q.x, q.y, z);
                    if to != at {
                        programmed.push(Move::new(MoveKind::Feed, at, to, Some(feed)));
                        at = to;
                    }
                }
            }
            Seg::Arc { center, ccw, to } => {
                let to = Point3::new(to.x, to.y, z);
                if to != at {
                    programmed.push(Move::new(MoveKind::Feed, at, to, Some(feed)).with_arc(*center, *ccw));
                    at = to;
                }
            }
        }
    }
    if at != s3 {
        programmed.push(Move::new(MoveKind::Feed, at, s3, Some(feed)));
    }
    programmed.push(Move::new(MoveKind::Feed, s3, m_out.to, m_out.feed));
    // Climb travel keeps the material on the left of the cutter (this
    // generator's law), so the centre stands right of the part contour.
    let side = if p.climb { CompSide::Right } else { CompSide::Left };
    let span = CompSpan { side, radius: r, first, last, programmed };
    if !span.is_sane(b.moves()) {
        return Err(refused(format!("could not chain its programmed contour at ({:.3}, {:.3})", s_c.x, s_c.y)));
    }
    b.push_comp(span);
    Ok(())
}
