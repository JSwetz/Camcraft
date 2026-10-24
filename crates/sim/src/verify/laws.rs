//! Split from `verify.rs` by M5.0.1; structure only.

use super::types::{FixturePart, Warning};

use crate::cl::{Move, MoveKind, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
pub use crate::tools::ToolGeom;
pub(super) fn xy(p3: &Point3) -> Point2 {
    Point2::new(p3.x, p3.y)
}

/// Squared XY distance from `p` to segment `a`-`b`.
pub(super) fn dist2_seg(p: Point2, a: Point2, b: Point2) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let dd = dx * dx + dy * dy;
    if dd <= f64::EPSILON {
        let (ex, ey) = (p.x - a.x, p.y - a.y);
        return ex * ex + ey * ey;
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0);
    let (ex, ey) = (a.x + t * dx - p.x, a.y + t * dy - p.y);
    ex * ex + ey * ey
}

pub(super) fn orient(a: Point2, b: Point2, c: Point2) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// Proper transversal crossing of two XY segments: each endpoint pair
/// strictly straddles the other's line. Shared or touching endpoints
/// give a zero orientation and are not crossings.
pub(super) fn proper_cross(a0: Point2, a1: Point2, b0: Point2, b1: Point2) -> bool {
    let (oa, ob) = (orient(a0, a1, b0), orient(a0, a1, b1));
    let (oc, od) = (orient(b0, b1, a0), orient(b0, b1, a1));
    oa != 0.0
        && ob != 0.0
        && oc != 0.0
        && od != 0.0
        && ((oa > 0.0) != (ob > 0.0))
        && ((oc > 0.0) != (od > 0.0))
}

/// XY samples of every boundary: loop vertices plus edge midpoints, so a
/// wall is represented by more than its corners, each nudged a hair
/// toward the loop's vertex mean. The nudge keeps membership off the
/// parity knife-edge: just inside an outer loop is region material, just
/// inside a hole loop is the island - the point a reach law must classify
/// unambiguously.
pub(super) fn boundary_samples(region: &Region) -> Vec<Point2> {
    const NUDGE: f64 = 0.05;
    let mut out = Vec::new();
    for l in region.loops() {
        let pts = l.points();
        if pts.is_empty() {
            continue;
        }
        let (mx, my) = pts.iter().fold((0.0, 0.0), |s, q| (s.0 + q.x, s.1 + q.y));
        let (mx, my) = (mx / pts.len() as f64, my / pts.len() as f64);
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            for q in [a, Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)] {
                let (dx, dy) = (mx - q.x, my - q.y);
                let d = (dx * dx + dy * dy).sqrt();
                out.push(if d > f64::EPSILON {
                    Point2::new(q.x + NUDGE * dx / d, q.y + NUDGE * dy / d)
                } else {
                    q
                });
            }
        }
    }
    out
}

/// XY points a radial law must hold at for one move: exactly the move's
/// `to` for a straight one - byte-for-byte the behavior every straight
/// path always had - and the arc with its chord joints for an arc, which
/// bound it within `chord_tol`: the laws' own radial tolerance is what
/// absorbs that sampling error, so a violation past tolerance is never
/// missed.
pub(super) fn law_points(m: &Move, chord_tol: f64) -> Vec<Point2> {
    match m.arc {
        None => vec![Point2::new(m.to.x, m.to.y)],
        Some(_) => {
            let cs = m.chords(chord_tol);
            let mut v = Vec::with_capacity(cs.len() + 1);
            if let Some(&(a, _)) = cs.first() {
                v.push(a);
            }
            v.extend(cs.into_iter().map(|(_, b)| b));
            v
        }
    }
}

/// Bands against fixtures (M3.2 stage B): the shank and holder cylinders,
/// tested against the fixture box list instead of rest material - a vise
/// jaw in a band's swept path is a crash exactly the way uncut stock in it
/// is. The cylinders are SWEPT along each move's xy path (a straight move
/// is one segment, an arc its chord segments), never sampled at law
/// points: an 8 mm jaw standing between two far endpoints is exactly the
/// thing this law exists for, and erring toward HIT means not sampling
/// past it. The shank band and holder band stand at [min(from,to).z +
/// above, + band_len]; above the holder, the spindle housing band when
/// the machine file states its body geometry; below them all, the flute
/// column at the tool radius over [min(from,to).z, + flute_len].
///
/// The law (c) annulus is empty over the flute because the flute cuts its
/// own kerf in STOCK - but a fixture standing in that window was never
/// meant to be cut, and nothing else looks at it. Z overlap is strict
/// beyond tolerance (the swept-cylinder rule, `fixture::band_hits_box`:
/// a jaw milled flush is contact without penetration, not a crash); a
/// band reaching within the radius of the box counts, XY touching
/// included. One report per move, first band struck, worst-case order
/// flute-free: shank, holder, then the spindle housing. Region-free by
/// construction - the post's live gate runs it against a cached path
/// without re-deriving any cut geometry (the M2.3 law: workholding is
/// checked here and now, never baked into a stamp).
pub fn bands_vs_fixtures(
    path: &Toolpath,
    tool: &ToolGeom,
    fixtures: &[crate::fixture::FixtureBox],
    spindle: Option<(f64, f64)>,
    chord_tol: f64,
) -> Vec<Warning> {
    let mut warns = Vec::new();
    if fixtures.is_empty() {
        return warns;
    }
    // The spindle housing band rides on the holder's top: its bottom
    // offset is the holder's offset plus the holder length, so the stack
    // can never double-count a slice of air or gap over the tool. It is a
    // fixture law only - against stock the housing stands down by the kerf
    // reasoning that already excuses the holder band there.
    let spindle = spindle.map(|(sr, sl)| {
        (
            FixturePart::Spindle,
            tool.flute_len + tool.shank_len + tool.holder_len,
            sr,
            sl,
        )
    });
    let bands = [
        (FixturePart::Shank, tool.flute_len, tool.shank_radius, tool.shank_len),
        (
            FixturePart::Holder,
            tool.flute_len + tool.shank_len,
            tool.holder_radius,
            tool.holder_len,
        ),
    ];
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind == MoveKind::Rapid {
            continue;
        }
        let z_deep = m.from.z.min(m.to.z);
        // The move's xy trace as segments: one for a straight, the
        // chords for an arc - what the cylinders actually sweep.
        let segs: Vec<(f64, f64, f64, f64)> = match &m.arc {
            None => vec![(m.from.x, m.from.y, m.to.x, m.to.y)],
            Some(_) => m
                .chords(chord_tol)
                .into_iter()
                .map(|(a, b)| (a.x, a.y, b.x, b.y))
                .collect(),
        };
        let mut struck = None;
        for (part, above, band_r, band_len) in bands.into_iter().chain(spindle) {
            // Unlike law (c), a band THINNER than the flute still
            // collides here: (c) skips it because the flute's kerf in
            // stock covers that radius, and steel has no such kerf.
            if band_len <= 0.0 {
                continue;
            }
            let (bz0, bz1) = (z_deep + above, z_deep + above + band_len);
            if fixtures.iter().any(|b| {
                segs.iter().any(|&(ax, ay, bx, by)| {
                    crate::fixture::band_hits_box(ax, ay, bx, by, bz0, bz1, band_r, b)
                })
            }) {
                struck = Some(part);
                break; // once per move, first (worst) element struck
            }
        }
        if struck.is_none() {
            let (bz0, bz1) = (z_deep, z_deep + tool.flute_len);
            if fixtures.iter().any(|b| {
                segs.iter().any(|&(ax, ay, bx, by)| {
                    crate::fixture::band_hits_box(ax, ay, bx, by, bz0, bz1, tool.radius, b)
                })
            }) {
                struck = Some(FixturePart::Flute);
            }
        }
        if let Some(part) = struck {
            warns.push(Warning::FixtureHit {
                at: i,
                fixture_part: part,
            });
        }
    }
    warns
}

/// Rapids against fixtures (M3.2 stage B): the tool disc swept along a
/// rapid's whole segment against every box of the setup's fixtures. No
/// clearance gate and no vertical exemption - law (d) stands those down
/// because the air under the clearance plane and the kerf below a retract
/// are REMOVED MATERIAL; a fixture is steel that was never meant to move,
/// and a rapid dipping through a jaw or a clamp arm is a crash wherever it
/// runs. The z law is the strict-dip rule of `fixture::rapid_hits_box`: a
/// retract lifting off exactly the mounting surface stays silent (mating
/// surfaces contact by design); anything deeper than float dust reports.
/// First hit per move, once. Region-free - see [`bands_vs_fixtures`] for
/// why that matters.
pub fn rapids_vs_fixtures(
    path: &Toolpath,
    tool: &ToolGeom,
    fixtures: &[crate::fixture::FixtureBox],
) -> Vec<Warning> {
    let mut warns = Vec::new();
    if fixtures.is_empty() {
        return warns;
    }
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind != MoveKind::Rapid {
            continue;
        }
        if fixtures
            .iter()
            .any(|b| crate::fixture::rapid_hits_box(&m.from, &m.to, tool.radius, b))
        {
            warns.push(Warning::FixtureHit {
                at: i,
                fixture_part: FixturePart::Rapid,
            });
        }
    }
    warns
}
