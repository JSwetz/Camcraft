//! The contour's bore-side entry arms (moved out of `contour_entry.rs`
//! by M11.3, which gives them the approach parameters - ramp angle and
//! leg, ramp to depth, helix radius range / pitch / sense, arc radius and
//! sweep, plunge height and feeds - and the one-entry law: a plunge or
//! helix that fits ends ON the lead-in's start, and every descent starts
//! just over the material).
//!
//! Provenance: standard CNC entry motions (plunge, linear and zig-zag
//! ramp, helical interpolation, circular ramp) per public G-code manuals
//! and machining handbooks; the legality is the pocket ladders' own
//! (M8.1's superset law, `entry::dive_room`, `entry_arc`). No vendor
//! material consulted.

use crate::approach::{CENTER_CUT_REFUSAL, NOT_CENTER_CUTTING};
use crate::cl::{MoveKind, PathBuilder, lattice};
use crate::entry::{Entry3D, disc_clears, dive_room, helix_about};
use crate::entry_arc::{arc_lead_swept, push_arc_lead, seg_dir};
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::links::{helix_turns, plan_zig_capped, push_zig};

use super::arcs::Seg;
use super::contour_entry::{Engage, flatten};
use super::descent::{Lvl, dive_link, feed_down, substituted_dive};
use super::slots::chord_clears;

/// One bore-side descent's level facts.
pub(super) struct BoreStep {
    pub arm: Entry3D,
    /// A transition helix's Z per turn (0: the entry's law).
    pub pitch: f64,
    pub e2: Point2,
    pub z: f64,
    pub prev_z: f64,
    /// The plane the tool traverses at.
    pub tz: f64,
    /// Is this the path's first engagement (its descent starts over the
    /// top of the cut)?
    pub first: bool,
}

/// The bore side (`Side::Inside`, untabbed): the eroded region states
/// the bounded air exactly, so every arm certifies against it with the
/// pocket ladders' own machinery. At clearance, the entry's own arm and
/// a zero plunge height this posts the pre-M11.3 descent.
pub(super) fn bore_entry(b: &mut PathBuilder, eng: &Engage, s: &BoreStep, lead: &[Seg], walk: &[Seg]) -> Option<Point2> {
    let Engage { p, center, cert, plan, m, .. } = *eng;
    let a = &p.links.approach;
    let BoreStep { arm, e2, z, prev_z, tz, .. } = *s;
    // The material's top over this level: the plane above, or the top
    // of the cut on the first engagement; descents start that high plus
    // the plunge height (above it: rapid).
    let mat_top = if s.first || prev_z >= p.clearance_z { p.top_z.min(prev_z) } else { prev_z };
    let from = if s.first || prev_z >= p.clearance_z { (mat_top + a.plunge_height).min(prev_z) } else { prev_z };
    let lvl = Lvl { cen: cert, passes: plan, pass: &[], walk: None, mat_top };
    let fd = a.descent_feed(p.feed);
    let pf = a.plunge(p.plunge_feed);
    let cur = b.at();
    let leg = Point2::new(cur.x, cur.y);
    let sub = |b: &mut PathBuilder, why: &str| {
        if !a.center_cutting {
            b.note_once(&format!("{CENTER_CUT_REFUSAL} - {why}, on the bore side at ({:.3}, {:.3})", e2.x, e2.y));
        }
        substituted_dive(b, &lvl, e2, z, m, why, tz)
    };
    if !a.center_cutting && arm == Entry3D::Plunge {
        b.note_once(&format!("{NOT_CENTER_CUTTING} - the bore side's plunge entry"));
        return sub(b, "the plunge arm");
    }
    match arm {
        Entry3D::Plunge => {
            let at_e = disc_clears(center, e2, (p.tool_diameter * 0.05).max(0.01));
            match if at_e { Some(e2) } else { dive_room(center, 0.0) } {
                Some(d) => {
                    if leg.x != d.x || leg.y != d.y {
                        b.push(MoveKind::Rapid, Point3::new(d.x, d.y, tz), None);
                    }
                    feed_down(b, d, z, mat_top, m, pf);
                    dive_link(b, &lvl, d, e2, z, m)
                }
                None => sub(b, "the disc fits nowhere here"),
            }
        }
        Entry3D::Ramp if a.ramp_zigs() => {
            // M11.3 B: zig-zag at the angle along the lead-in and the
            // ring - every point on a path the room law proved.
            let path = flatten(e2, &[lead, walk].concat());
            let zf = if a.ramp_to_depth { (p.top_z + a.plunge_height).min(prev_z) } else { from };
            match plan_zig_capped(&path, zf - z, a.ramp_angle, a.ramp_max_len) {
                Some((pre, n)) => {
                    if leg.x != e2.x || leg.y != e2.y {
                        b.push(MoveKind::Rapid, Point3::new(e2.x, e2.y, tz), None);
                    }
                    b.push(MoveKind::Rapid, Point3::new(e2.x, e2.y, zf), None);
                    push_zig(b, &pre, n, zf, z, fd);
                    None
                }
                None => sub(b, "the lead-in and ring are too short for the entry ramp's angle and length"),
            }
        }
        Entry3D::Ramp => {
            // One chord test covers the whole interpolated descent (M8.1's
            // superset law at its simplest - a planar contour's single
            // center); the leg limit takes the chord's last stretch.
            if chord_clears(center, leg, e2) {
                let st = a.ramp_start(leg, e2);
                let st = if chord_clears(center, st, e2) { st } else { leg };
                let zf = if a.ramp_to_depth { (p.top_z + a.plunge_height).min(prev_z) } else { from };
                if st.x != leg.x || st.y != leg.y {
                    b.push(MoveKind::Rapid, Point3::new(st.x, st.y, cur.z), None);
                }
                b.push(MoveKind::Rapid, Point3::new(st.x, st.y, zf), None);
                b.push(MoveKind::Feed, Point3::new(e2.x, e2.y, z), Some(fd));
                None
            } else {
                sub(b, "the ramp's chord is blocked")
            }
        }
        Entry3D::Arc => {
            // The circular ramp onto the lead-in's own first move,
            // certified by the ramp's chord law (see `entry_arc`).
            let first = lead.first().or(walk.first());
            let lead_arc = first
                .and_then(|s| seg_dir(e2, s))
                .ok_or("the first cut has no direction to be tangent to")
                .and_then(|d| {
                    let ok = |s, q| chord_clears(center, s, q);
                    let r = a.arc_nominal(p.tool_diameter);
                    arc_lead_swept(e2, d, from, z, r, a.arc_sweep_rad(), p.tolerance, ok)
                });
            match lead_arc {
                Ok(l) => {
                    push_arc_lead(b, &l, tz, from, fd, pf);
                    None
                }
                Err(why) => sub(b, why),
            }
        }
        Entry3D::Helix => {
            // M11.3: the orbit placed to close ON the lead-in's start
            // where a center a radius off it has the room, else the
            // floor's widest room with a link; radii down to the minimum.
            let pitch_of = |r: f64| if s.pitch > 0.0 { s.pitch } else { a.entry_pitch(r) };
            let mut why = "the widened disc fits nowhere at this level";
            for r in a.helix_radii(p.helix_radius) {
                let Some(t) = helix_turns(from - z, r, pitch_of(r)) else {
                    why = "the helix pitch asks too many turns";
                    continue;
                };
                let at_e = (0..8).find_map(|k| {
                    let (sn, cs) = (f64::from(k) * std::f64::consts::FRAC_PI_4).sin_cos();
                    let c = Point2::new(lattice(e2.x + r * cs), lattice(e2.y + r * sn));
                    disc_clears(center, c, r + 1e-3).then_some((e2, c))
                });
                let placed = at_e.or_else(|| {
                    dive_room(center, r + 1e-3).map(|c| {
                        let d = Point2::new(c.x - r, c.y);
                        (d, Point2::new(lattice(d.x + r), lattice(d.y)))
                    })
                });
                if let Some((d, c)) = placed {
                    if leg.x != d.x || leg.y != d.y {
                        b.push(MoveKind::Rapid, Point3::new(d.x, d.y, tz), None);
                    }
                    if r < p.helix_radius {
                        b.note_once(&format!(
                            "entry: the helix fits only under its radius - flown at {r:.3} mm, within the minimum radius"
                        ));
                    }
                    b.push(MoveKind::Rapid, Point3::new(d.x, d.y, from), None);
                    helix_about(b, d, c, from, z, t, fd, !a.helix_cw);
                    return dive_link(b, &lvl, d, e2, z, m);
                }
            }
            sub(b, why)
        }
    }
}
