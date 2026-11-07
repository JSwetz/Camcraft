//! The swarf stamp's sign and link laws (split out of `stamp.rs` at
//! the AGENTS 400-line seam): which end of a ruling the spindle sits
//! behind ([`ruled_sign`]), where a pose turn happens
//! ([`link_swings`]), and the guard that refuses a turn anywhere else
//! ([`guard_turns`]).
//!
//! The link law is the standard 5-axis one: a pose turn above
//! [`LINK_TURN_DEG`] happens IN AIR. A joint-interpolating controller
//! swings the head through the whole turn while the tip travels the
//! block, and a cut whose pose turns 90 degrees over a 4 mm feed has
//! its ball off the surface for most of that feed (the mid-move
//! verifier's conviction: 0.97 mm of rest at a raster reversal, 2.9
//! mm at the ring/raster hand-off before the sign law). So wherever
//! two consecutive cutting moves would turn past the threshold, the
//! hand-off becomes a link - and the link leaves the part along the
//! CONTACT NORMAL, never up a Z plane. A swarf tool lies with its
//! whole flank along the ruling, in contact with the blade along its
//! length; a vertical lift under that pose drags the flank through
//! the blade's curvature, and under the equator of a tube the ball
//! itself climbs through the solid (the envelope scan's 2.59 mm
//! gouge on the capped blade, where the ball/tip laws saw nothing).
//! The normal at the exit ball centre - the direction out of the
//! nearest surface point, the same read the stamp leaned from - is
//! the one direction every tangent flank leaves the surface along.
//! The link is three moves: a rapid retract under the OUTGOING pose
//! along that normal, by the plunge height (`clearance + r` above the
//! exit centre - the height the Z link used to climb, spent along the
//! normal instead), to a centre the law has proved in air with a
//! flank as long as the retract itself; a rapid swing there, in air,
//! from the outgoing pose to the INCOMING one about that centre; then
//! a plunge under the incoming pose back down the normal onto the
//! SAME ball centre the previous move finished on - the sphere is
//! rotation-invariant about its centre, so the re-entry lands on a
//! surface point already cut (the verifier's posed-contact law reads
//! a plunge at its end only; its solid law sees the swing's swept
//! poses, in air by this law). Where the retract's far end still
//! holds a flank in solid (a concave neighbour) the law retracts
//! twice and four times as far before it refuses by name; a retract
//! whose ball would cross solid on the way out refuses at once - a Z
//! lift under a lying pose is never the fallback.
//!
//! Provenance: retract / reorient in air / re-approach linking along
//! the surface normal is general multi-axis practice; the
//! centre-invariance argument is the stamp's pivot law. Our own
//! design notes; no vendor material.

use crate::cl::{Move, MoveKind, Toolpath};
use crate::finishing3d::closest_pt_tri;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::retip::{along, UP};

/// The widest turn two consecutive cutting moves may state between
/// them, degrees; past it the hand-off is linked through air.
pub const LINK_TURN_DEG: f64 = 30.0;

/// A ruled facet seen within this of head-on (sin 10 degrees) says
/// nothing about the pass direction; the lean law takes it.
pub(super) const SIN_MIN: f64 = 0.173_648_177_666_930_33; // sin(10 deg), frozen

/// The sign of a ruled pose. A ruling direction is a LINE: both
/// signs of `a` are "the ruling", and which end the spindle sits
/// behind is a choice the program must make ONCE and then keep.
/// The sign continuous with the pose held coming in wins (the dot
/// with `held` not negative), then the one continuous with the last lean the program
/// stated (a floor or a rapid between two blade passes holds nothing
/// that discriminates), and only a program that has stated no lean
/// at all takes the deterministic hemisphere (z first - up, never
/// down - then x, then y). The fixed hemisphere alone flipped the
/// blade's poses 180 degrees between consecutive feeds wherever the
/// lean law had landed on the other end (RECOMMENDED.md: "the swarf
/// stamp flips hemispheres mid-program").
pub(super) fn ruled_sign(a: Point3, held: [f64; 3], last_lean: Option<[f64; 3]>) -> [f64; 3] {
    const EPS: f64 = 1e-9;
    let keep = [a.x, a.y, a.z];
    let flip = [-a.x, -a.y, -a.z];
    for w in std::iter::once(held).chain(last_lean) {
        let d = a.x * w[0] + a.y * w[1] + a.z * w[2];
        if d > EPS {
            return keep;
        }
        if d < -EPS {
            return flip;
        }
    }
    for v in [a.z, a.x, a.y] {
        if v < -EPS {
            return flip;
        }
        if v > EPS {
            break;
        }
    }
    keep
}

/// Degrees between two unit poses.
pub(super) fn turn_deg(p: [f64; 3], q: [f64; 3]) -> f64 {
    (p[0] * q[0] + p[1] * q[1] + p[2] * q[2]).clamp(-1.0, 1.0).acos().to_degrees()
}

/// The link law over a stamped path: every cutting move whose pose
/// turns more than [`LINK_TURN_DEG`] off the pose held coming in, and
/// whose tip starts below the clearance plane (a move already in air
/// swings where it is), is preceded by the retract-swing-re-enter
/// link described in the module header, along the contact normal read
/// on `facets` at the exit ball centre. `r` is the ball radius,
/// `clearance` the op's clearance plane (an upright tip height: it
/// sets the retract LENGTH, `clearance + r` above the exit centre,
/// and says which moves are in air), `plunge_feed` the re-entry's
/// feed. The cut moves themselves are kept, each re-entered at its
/// own ball centre under its own pose. Returns the linked path and
/// the number of links written.
pub fn link_swings(
    path: &Toolpath,
    facets: &[Triangle],
    r: f64,
    clearance: f64,
    plunge_feed: f64,
) -> Result<(Toolpath, usize), GenError> {
    if !(r.is_finite() && r > 0.0 && clearance.is_finite()) {
        return Err(GenError::Invalid(format!(
            "the swarf link law needs a finite ball radius and clearance, got r {r}, clearance {clearance}"
        )));
    }
    let boxes: Vec<[f64; 6]> = facets
        .iter()
        .map(|f| {
            [
                f.a.x.min(f.b.x.min(f.c.x)),
                f.a.y.min(f.b.y.min(f.c.y)),
                f.a.z.min(f.b.z.min(f.c.z)),
                f.a.x.max(f.b.x.max(f.c.x)),
                f.a.y.max(f.b.y.max(f.c.y)),
                f.a.z.max(f.b.z.max(f.c.z)),
            ]
        })
        .collect();
    let mut out: Vec<Move> = Vec::with_capacity(path.moves.len() + 16);
    let mut held: Option<[f64; 3]> = None;
    let mut links = 0usize;
    for (i, m) in path.moves.iter().enumerate() {
        let p = held.unwrap_or(UP);
        let q = m.axis.unwrap_or(UP);
        let swings = m.kind != MoveKind::Rapid && turn_deg(p, q) > LINK_TURN_DEG;
        if swings && m.from.z < clearance - 1e-9 {
            // The ball centre the previous move finished on, read under
            // the pose it held.
            let c = along(m.from, p, r);
            let h0 = clearance + r - c.z;
            if h0 <= 1e-9 {
                return Err(GenError::Invalid(format!(
                    "the swarf link law retracts by the clearance height (Z {clearance:.3}), which \
                     lies under the cut at Z {:.3} - raise the clearance",
                    c.z
                )));
            }
            let (n, d0) = contact_normal(c, facets, &boxes);
            let h = retract_length(c, n, d0, h0, (p, q), r, facets, &boxes).ok_or_else(|| {
                GenError::Invalid(format!(
                    "swarf pose link at move {i} (ball centre {:.2}, {:.2}, {:.2}): the ball finds no \
                     air along the contact normal [{:.3} {:.3} {:.3}] (a wall stands across it within \
                     {:.1} mm) - refused: a pose turn past {LINK_TURN_DEG:.0} deg happens in air",
                    c.x, c.y, c.z, n[0], n[1], n[2], (r * 0.5).max(1e-3)
                ))
            })?;
            let c_out = along(c, n, h);
            let tip_out = along(c_out, p, -r);
            let tip_swung = along(c_out, q, -r);
            let tip_in = along(c, q, -r);
            let mut retract = Move::new(MoveKind::Rapid, m.from, tip_out, None);
            retract.axis = held;
            out.push(retract);
            let mut swing = Move::new(MoveKind::Rapid, tip_out, tip_swung, None);
            swing.axis = m.axis;
            out.push(swing);
            let mut plunge = Move::new(MoveKind::Plunge, tip_swung, tip_in, Some(plunge_feed));
            plunge.axis = m.axis;
            out.push(plunge);
            let mut cut = *m;
            cut.from = tip_in;
            out.push(cut);
            links += 1;
        } else {
            out.push(*m);
        }
        held = m.axis.or(held);
    }
    let mut linked = Toolpath::new(out);
    linked.notes = path.notes.clone();
    if !linked.all_finite() {
        return Err(GenError::Invalid("the swarf link law left the finite horizon".into()));
    }
    Ok((linked, links))
}

/// The contact normal at a ball centre: the unit direction out of the
/// nearest surface point (the stamp's own read), and the distance to
/// it. A centre with no surface to read (no facets, or ON a facet)
/// takes world up, the only direction left that is away from nothing.
fn contact_normal(c: Point3, facets: &[Triangle], boxes: &[[f64; 6]]) -> ([f64; 3], f64) {
    let mut best: Option<(f64, Point3)> = None;
    for (f, bx) in facets.iter().zip(boxes) {
        if best.is_some_and(|(bd, _)| box_gap(c, bx) > bd) {
            continue;
        }
        let q = closest_pt_tri(c, &[f.a, f.b, f.c]);
        let d = dist(c, q);
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, q));
        }
    }
    match best {
        Some((d, q)) if d > 1e-9 => ([(c.x - q.x) / d, (c.y - q.y) / d, (c.z - q.z) / d], d),
        Some((d, _)) => (UP, d),
        None => (UP, f64::INFINITY),
    }
}

/// The retract length that proves the link in air. The normal is
/// walked out to four plunge heights, every half-radius, and the walk
/// stops at the wall ahead: the first sample where the ball would
/// stand closer to the part than it stood at the exit. Along the air
/// found, `h0` (the plunge height) is taken first, then twice and
/// four times it, the first at whose far end the flank - a plunge
/// height long, up the outgoing and the incoming pose `(p, q)` -
/// clears the part by the ball's own radius. Where no plunge height
/// is reachable (a bore, a pocket: the wall across stands nearer) the
/// link takes the farthest-standing air the walk found - with the
/// flank proved where it can be, and without it where no length holds
/// a flank (a pose already pointing across a bore holds its flank in
/// the wall during the CUT; the solid law with the real tool stack
/// is that collision's judge, on the swing as on the cut). None only
/// when the ball itself finds no air along the normal.
#[allow(clippy::too_many_arguments)] // one link's flat read
fn retract_length(
    c: Point3,
    n: [f64; 3],
    d0: f64,
    h0: f64,
    (p, q): ([f64; 3], [f64; 3]),
    r: f64,
    facets: &[Triangle],
    boxes: &[[f64; 6]],
) -> Option<f64> {
    let floor = d0.min(r) - 1e-6;
    let step = (r * 0.5).max(1e-3);
    let clear = |a: Point3, b: Point3, min: f64| -> bool {
        let len = dist(a, b);
        let k = ((len / step).ceil() as usize).clamp(1, 4096);
        (0..=k).all(|j| {
            let t = j as f64 / k as f64;
            let s = Point3::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t);
            nearest_within(s, facets, boxes, min) >= min
        })
    };
    let flank_clear = |h: f64| -> bool {
        let c_out = along(c, n, h);
        clear(c_out, along(c_out, p, h0), r) && clear(c_out, along(c_out, q, h0), r)
    };
    // The air along the normal: how far the walk got, and the sample
    // standing farthest from the part on the way.
    let far = h0 * 4.0;
    let k = ((far / step).ceil() as usize).clamp(1, 8192);
    let (mut reach, mut best) = (0.0f64, (0.0f64, d0));
    for j in 1..=k {
        let s = far * j as f64 / k as f64;
        let d = nearest_within(along(c, n, s), facets, boxes, d0 + s + 1.0);
        if d < floor {
            break;
        }
        reach = s;
        if d > best.1 {
            best = (s, d);
        }
    }
    for h in [h0, h0 * 2.0, h0 * 4.0] {
        if h <= reach + 1e-9 && flank_clear(h) {
            return Some(h);
        }
    }
    (best.0 > 1e-9).then_some(best.0)
}

/// The flank proof of a pose (the swarf stamp's fallback law,
/// `stamp_pose`): the flute's column from the ball centre `c` along
/// `a`, sampled every half radius until the ray leaves the part's
/// shell (`bounds`, the facets' box, grown by `r`), stands at least
/// `r - tol` from every facet - in air to the plan's own tolerance.
/// The centre itself is the ball's contact and is not a sample; a
/// ray standing outside the shell and leaving it is clear; a pose
/// that frames no exit (a non-unit garbage axis) fails closed.
pub(super) fn flank_clear(
    c: Point3,
    a: [f64; 3],
    r: f64,
    tol: f64,
    facets: &[Triangle],
    boxes: &[[f64; 6]],
    bounds: &[f64; 6],
) -> bool {
    let min = (r - tol.max(0.0)).max(1e-6);
    let step = (r * 0.5).max(1e-3);
    let p = [c.x, c.y, c.z];
    let mut exit = f64::INFINITY;
    for i in 0..3 {
        let (lo, hi) = (bounds[i] - r, bounds[i + 3] + r);
        if a[i] > 1e-12 {
            exit = exit.min((hi - p[i]) / a[i]);
        } else if a[i] < -1e-12 {
            exit = exit.min((lo - p[i]) / a[i]);
        } else if p[i] < lo || p[i] > hi {
            return true;
        }
    }
    if !exit.is_finite() {
        return false;
    }
    if exit <= 1e-9 {
        return true;
    }
    let k = ((exit / step).ceil() as usize).clamp(1, 4096);
    (1..=k).all(|j| {
        let s = exit * j as f64 / k as f64;
        nearest_within(along(c, a, s), facets, boxes, min) >= min
    })
}

/// The distance from `s` to the nearest facet, or `min` when no
/// facet stands within it (the shell reject spends nothing on a far
/// facet, and a caller asking "at least `min`?" needs no more).
fn nearest_within(s: Point3, facets: &[Triangle], boxes: &[[f64; 6]], min: f64) -> f64 {
    let mut best = f64::INFINITY;
    for (f, bx) in facets.iter().zip(boxes) {
        if box_gap(s, bx) >= min.min(best) {
            continue;
        }
        let d = dist(s, closest_pt_tri(s, &[f.a, f.b, f.c]));
        if d < best {
            best = d;
            if best < min {
                return best;
            }
        }
    }
    best.min(min)
}

/// The gap from `p` to a facet's box (zero inside it).
fn box_gap(p: Point3, bx: &[f64; 6]) -> f64 {
    let gx = (bx[0] - p.x).max(0.0).max(p.x - bx[3]);
    let gy = (bx[1] - p.y).max(0.0).max(p.y - bx[4]);
    let gz = (bx[2] - p.z).max(0.0).max(p.z - bx[5]);
    (gx * gx + gy * gy + gz * gz).sqrt()
}

fn dist(a: Point3, b: Point3) -> f64 {
    let (dx, dy, dz) = (a.x - b.x, a.y - b.y, a.z - b.z);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// The turn guard: no cutting move turns more than [`LINK_TURN_DEG`]
/// off the pose held coming in unless its tip starts on or above the
/// clearance plane (an air link: the plunge that carries the swing
/// down, or the plan's own entry from clearance). A move stating no
/// pose is upright; rapids inherit and are not judged. The first
/// offender refuses the path by name - a silent head swing in contact
/// is never posted.
pub fn guard_turns(path: &Toolpath, clearance: f64) -> Result<(), GenError> {
    let mut held: Option<[f64; 3]> = None;
    for (i, m) in path.moves.iter().enumerate() {
        let p = held.unwrap_or(UP);
        let q = m.axis.unwrap_or(UP);
        if m.kind != MoveKind::Rapid && m.from.z < clearance - 1e-9 {
            let deg = turn_deg(p, q);
            if deg > LINK_TURN_DEG {
                return Err(GenError::Invalid(format!(
                    "swarf pose swing {deg:.0} deg at move {i} (to {:.2}, {:.2}, {:.2}; \
                     [{:.3} {:.3} {:.3}] to [{:.3} {:.3} {:.3}]) - refused: a pose turn past \
                     {LINK_TURN_DEG:.0} deg happens in air, never on a cut",
                    m.to.x, m.to.y, m.to.z, p[0], p[1], p[2], q[0], q[1], q[2]
                )));
            }
        }
        held = m.axis.or(held);
    }
    Ok(())
}
