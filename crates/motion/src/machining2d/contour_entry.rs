//! Contour level engagement: the bore-side entry arms (moved here from
//! `contour.rs` unchanged), the wall-side arc entry, and the closed-ring
//! seam law that keeps a lead-in and its lead-out from crossing.
//!
//! Provenance: the vertical-plane arc lead-in of general machining
//! practice (RS-274 / ISO 6983 circular interpolation in a vertical
//! plane, posted as chords - see `entry_arc`) and the standard
//! multi-depth profile transition of published CAM practice: lead out,
//! retract to the retract plane, traverse, feed back in - or stay down
//! and link at feed where the link runs in air. Legality is
//! `lead_fit`'s sampled room law. No vendor material consulted.

use std::f64::consts::TAU;

use crate::cl::{MoveKind, PathBuilder};
use crate::entry::{Entry3D, dive_room, helix_down};
use crate::entry_arc::{arc_lead, push_arc_lead, seg_dir};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;

use super::arcs::{Seg, walk_tangent};
use super::contour::{ContourParams, Lead, air_side};
use super::fillet::Legal;
use super::lead_fit::{plan_leads, segs_hold, slack};
use super::levels::{Cert, Motion, dive_link, substituted_dive};
use super::ring::dist;
use super::slots::chord_clears;

/// Everything a level's engagement reads, borrowed from `contour`.
pub(super) struct Engage<'a> {
    pub p: &'a ContourParams,
    pub center: &'a Region,
    pub cert: &'a Cert<'a>,
    pub plan: &'a [Vec<Point2>],
    pub m: &'a Motion,
    pub legal: &'a Legal<'a>,
}

/// The bore side (`Side::Inside`, untabbed): the eroded region states
/// the bounded air exactly, so every arm certifies against it with the
/// pocket ladders' own machinery - the dive stands in the floor's room,
/// never dwells on the finished wall.
pub(super) fn bore_entry(
    b: &mut PathBuilder,
    eng: &Engage,
    e2: Point2,
    z: f64,
    prev_z: f64,
    lead: &[Seg],
    walk: &[Seg],
) {
    let Engage { p, center, cert, plan, m, .. } = *eng;
    match p.entry {
        Entry3D::Plunge => match dive_room(center, 0.0) {
            Some(d) => {
                let cur = b.at();
                if cur.x != d.x || cur.y != d.y {
                    b.push(
                        MoveKind::Rapid,
                        Point3::new(d.x, d.y, p.clearance_z),
                        None,
                    );
                }
                b.push(
                    MoveKind::Plunge,
                    Point3::new(d.x, d.y, z),
                    Some(p.plunge_feed),
                );
                dive_link(b, cert, plan, d, e2, z, m);
            }
            None => substituted_dive(
                b,
                cert,
                plan,
                e2,
                z,
                m,
                "the disc fits nowhere here",
            ),
        },
        Entry3D::Ramp => {
            // One chord test covers the whole
            // interpolated descent: the region is
            // one at every plane, and the
            // destination's room bounds every
            // plane above it (M8.1's superset law,
            // here at its simplest - a planar
            // contour's single center).
            let cur = b.at();
            let leg = Point2::new(cur.x, cur.y);
            if chord_clears(center, leg, e2) {
                b.push(
                    MoveKind::Rapid,
                    Point3::new(leg.x, leg.y, prev_z),
                    None,
                );
                b.push(MoveKind::Feed, Point3::new(e2.x, e2.y, z), Some(p.feed));
            } else {
                substituted_dive(
                    b,
                    cert,
                    plan,
                    e2,
                    z,
                    m,
                    "the ramp's chord is blocked",
                );
            }
        }
        Entry3D::Arc => {
            // The circular ramp onto the lead-in's
            // own first move, certified by the
            // ramp's chord law (see `entry_arc`).
            let first = lead.first().or(walk.first());
            let lead_arc = first
                .and_then(|s| seg_dir(e2, s))
                .ok_or("the first cut has no direction to be tangent to")
                .and_then(|d| {
                    let ok = |s, q| chord_clears(center, s, q);
                    arc_lead(e2, d, prev_z, z, p.tool_diameter, p.tolerance, ok)
                });
            match lead_arc {
                Ok(l) => push_arc_lead(b, &l, p.clearance_z, prev_z, p.feed),
                Err(why) => substituted_dive(b, cert, plan, e2, z, m, why),
            }
        }
        Entry3D::Helix => {
            let cur = b.at();
            let leg = Point2::new(cur.x, cur.y);
            // The found point is the orbit's
            // CENTER; the dive starts at its near
            // side, `rh` west - one radius of
            // center-room certifies the whole
            // swept tube exactly (see
            // `entry::dive_room`, M2.5 F's dome;
            // the 2-radius ask over-refused the
            // channel, M8.2's slot).
            match dive_room(center, p.helix_radius + 1e-3) {
                Some(c) => {
                    let d = Point2::new(c.x - p.helix_radius, c.y);
                    if leg.x != d.x || leg.y != d.y {
                        b.push(
                            MoveKind::Rapid,
                            Point3::new(d.x, d.y, p.clearance_z),
                            None,
                        );
                    }
                    b.push(MoveKind::Rapid, Point3::new(d.x, d.y, prev_z), None);
                    helix_down(b, d, prev_z, z, p.helix_radius, p.feed);
                    dive_link(b, cert, plan, d, e2, z, m);
                }
                None => substituted_dive(
                    b,
                    cert,
                    plan,
                    e2,
                    z,
                    m,
                    "the widened disc fits nowhere at this level",
                ),
            }
        }
    }
}

/// The plane the wall-side arc descends from on level `zi`: the level
/// above (its floor), or - on the first level - one stepdown over it,
/// air by the generator's contract (nothing is engaged above `top_z`),
/// held under the hop plane (clearance where the hop plane IS the top).
pub(super) fn wall_arc_top(p: &ContourParams, zs: &[f64], zi: usize, hop_z: f64) -> f64 {
    match zi.checked_sub(1).and_then(|i| zs.get(i)) {
        Some(&above) => above,
        None => {
            let z = zs.first().copied().unwrap_or(p.top_z);
            let cap = if hop_z > z { hop_z } else { p.clearance_z };
            (z + p.stepdown).min(cap)
        }
    }
}

/// The wall-side arc entry (an outside wall, or the ring of a hole in
/// the section): a circular ramp in the vertical plane of the lead-in's
/// first move, tangent to it at `(e2, z)`, descending from `z_top`. Its
/// XY footprint is the straight chord from its start to `e2` (see
/// `entry_arc`), and that chord is proved on the legal side of the grown
/// region by `lead_fit`'s room law - the cutter center never enters the
/// part's tool-radius offset; uncut stock outside the part is fair to
/// cut.
///
/// The approach is one of two standard sequences: `stay` (the tool
/// stands on this same ring's lead-out on the plane above) feeds the
/// link to the arc start on that plane when the link holds the same
/// room law; otherwise the tool lifts to the hop (retract) plane,
/// traverses, drops at rapid only to the top of the cut, and feeds the
/// rest of the way at the plunge feed. Nothing is pushed on `Err`: the
/// caller substitutes and says why.
#[allow(clippy::too_many_arguments)]
pub(super) fn wall_arc(
    b: &mut PathBuilder,
    eng: &Engage,
    e2: Point2,
    z: f64,
    z_top: f64,
    hop_z: f64,
    first: Option<&Seg>,
    stay: bool,
) -> Result<(), &'static str> {
    let p = eng.p;
    let tol = slack(p);
    let holds = |a: Point2, q: Point2| segs_hold(eng.legal, a, &[Seg::Line(vec![q])], tol);
    let dir = first
        .and_then(|s| seg_dir(e2, s))
        .ok_or("the first cut has no direction to be tangent to")?;
    let arc = arc_lead(e2, dir, z_top, z, p.tool_diameter, p.tolerance, holds)?;
    let s = arc.start;
    let cur = b.at();
    let here = Point2::new(cur.x, cur.y);
    if stay && (cur.z - z_top).abs() <= 1e-9 && holds(here, s) {
        b.push(MoveKind::Feed, Point3::new(s.x, s.y, z_top), Some(p.feed));
    } else {
        let lift = cur.z.max(hop_z);
        b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, lift), None);
        b.push(MoveKind::Rapid, Point3::new(s.x, s.y, lift), None);
        let air = z_top.max(p.top_z).min(lift);
        b.push(MoveKind::Rapid, Point3::new(s.x, s.y, air), None);
        if z_top < air {
            b.push(MoveKind::Plunge, Point3::new(s.x, s.y, z_top), Some(p.plunge_feed));
        }
    }
    for q in &arc.fan {
        b.push(MoveKind::Feed, *q, Some(p.feed));
    }
    Ok(())
}

/// One window's leads, `plan_leads`' answer - unless the window is a
/// whole closed ring whose tangential lead-in and lead-out cross each
/// other at the seam. A seam on a corner or a tessellation vertex has
/// two tangents (the walk's end and its start differ by the turn), so
/// the two half arcs - one tangent to each - meet a second time out in
/// the air: the verifier's crossing, and the "loop" a viewer sees. The
/// lead-out is then re-solved as the arc tangent to the walk's END
/// tangent at the seam (G1, as before) that ends exactly where the
/// lead-in starts: two circles through the same two points cross
/// nowhere else, so the pair closes into one clean figure and the next
/// level leaves from the point this one entered at. The room law
/// (`lead_fit`) must still hold it, or the planned pair stands and the
/// note says the crossing remains.
pub(super) fn seam_leads(
    walk: &[Seg],
    poly: &[Point2],
    p: &ContourParams,
    legal: &Legal,
    notes: &mut Vec<String>,
) -> (Point2, Vec<Seg>, Vec<Seg>) {
    let mut said = Vec::new();
    let planned = plan_leads(walk, poly, p, legal, &mut said);
    for n in said {
        say(notes, n);
    }
    let n = poly.len();
    let closed = n > 2 && dist(poly[0], poly[n - 1]) <= 1e-9;
    let tangential = matches!(p.lead, Lead::Tangential { .. });
    if !(closed && tangential && crosses(planned.0, &planned.1, poly[0], &planned.2)) {
        return planned;
    }
    let s = poly[0];
    let e = planned.0;
    let t1 = walk_tangent(walk, poly, false);
    let n1 = air_side(t1, p.climb);
    let ccw = match planned.2.first() {
        Some(Seg::Arc { ccw, .. }) => Some(*ccw),
        _ => None,
    };
    // The circle through s with its center on s's air normal n1 that
    // also passes through e: |rho n1 - (e - s)| = rho.
    let (dx, dy) = (e.x - s.x, e.y - s.y);
    let along = dx * n1.x + dy * n1.y;
    let rho = (dx * dx + dy * dy) / (2.0 * along);
    let out = match ccw {
        Some(ccw) if along > 0.0 && rho.is_finite() && rho > 0.0 => {
            let c = Point2::new(s.x + n1.x * rho, s.y + n1.y * rho);
            vec![Seg::Arc { center: c, ccw, to: e }]
        }
        _ => Vec::new(),
    };
    if !out.is_empty() && segs_hold(legal, s, &out, slack(p)) && !crosses(e, &planned.1, s, &out) {
        say(
            notes,
            format!(
                "lead: the lead-in and lead-out crossed at the ring seam ({:.3}, {:.3}) - the \
                 lead-out now closes on the lead-in's start",
                s.x, s.y
            ),
        );
        return (planned.0, planned.1, out);
    }
    say(
        notes,
        format!(
            "lead: the lead-in and lead-out cross at the ring seam ({:.3}, {:.3}) and no \
             closing lead-out holds the room - kept as planned",
            s.x, s.y
        ),
    );
    planned
}

/// Push `n` unless it was already said.
pub(super) fn say(notes: &mut Vec<String>, n: String) {
    if !notes.contains(&n) {
        notes.push(n);
    }
}

/// Bound on samples per flattened piece.
const MAX_SAMPLES: usize = 4096;
/// Flattening step for the crossing test (mm).
const STEP: f64 = 0.05;

/// `segs` walked from `from`, flattened to a polyline.
fn flatten(from: Point2, segs: &[Seg]) -> Vec<Point2> {
    let mut out = vec![from];
    let mut at = from;
    for seg in segs {
        match seg {
            Seg::Line(qs) => {
                out.extend(qs.iter().copied());
                if let Some(&q) = qs.last() {
                    at = q;
                }
            }
            Seg::Arc { center: c, ccw, to } => {
                let rad = dist(at, *c);
                let a0 = (at.y - c.y).atan2(at.x - c.x);
                let a1 = (to.y - c.y).atan2(to.x - c.x);
                let sweep = if *ccw { a1 - a0 } else { a0 - a1 }.rem_euclid(TAU);
                if rad.is_finite() && sweep.is_finite() {
                    let k = ((sweep * rad / STEP).ceil() as usize).clamp(8, MAX_SAMPLES);
                    let dir = if *ccw { 1.0 } else { -1.0 };
                    for i in 1..k {
                        let a = a0 + dir * sweep * i as f64 / k as f64;
                        out.push(Point2::new(c.x + rad * a.cos(), c.y + rad * a.sin()));
                    }
                }
                out.push(*to);
                at = *to;
            }
        }
    }
    out
}

fn orient(a: Point2, b: Point2, c: Point2) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// Do the lead-in (walked from `a0`) and the lead-out (from `b0`)
/// properly cross? Pieces touching at a shared point - the seam, or the
/// far end of one closed circle - are contact, not crossing.
pub(super) fn crosses(a0: Point2, a: &[Seg], b0: Point2, b: &[Seg]) -> bool {
    let (pa, pb) = (flatten(a0, a), flatten(b0, b));
    let near = |u: Point2, v: Point2| dist(u, v) <= 1e-7;
    pa.windows(2).any(|s| {
        pb.windows(2).any(|t| {
            if near(s[0], t[0]) || near(s[0], t[1]) || near(s[1], t[0]) || near(s[1], t[1]) {
                return false;
            }
            let (d1, d2) = (orient(s[0], s[1], t[0]), orient(s[0], s[1], t[1]));
            let (d3, d4) = (orient(t[0], t[1], s[0]), orient(t[0], t[1], s[1]));
            d1 * d2 < 0.0 && d3 * d4 < 0.0
        })
    })
}
