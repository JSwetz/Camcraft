//! Turning boring (M5.2 F): the inside twin of the profile ride -
//! the nose walking a hole's wall, judged by the envelope law.
//!
//! Stage B proved NO GOUGE as a theorem about the max-of-claims
//! offset of the OUTER silhouette; boring is the same theorem read
//! through the mirror: a center curve that stands at distance >= d
//! BELOW every piece of the hole's INNER wall (the min-of-claims
//! twin, [`offset::profile_offset_in`]) is a bar whose swept ball
//! never enters the steel, and the E1 envelope scan - which already
//! measures a nose-stated tool by its centers - is the judge,
//! unchanged. What is new here is only what the wall IS: the
//! silhouette's edge sweep folded with MIN instead of MAX, so a
//! station answers with its innermost material, and the doors that
//! ask the model whether a hole stands open at all.
//!
//! Those doors are the Prime Directive's share of this kernel. A
//! plain bar's min-envelope reads its own skin as a "wall" - the
//! ball would ride the steel, tangent, and the facet verdict would
//! measure nothing wrong - so the axis's air is asked of the mesh
//! itself (a ray-cast parity from the axis point, voted at three
//! angles against the tessellation's grazing), at every vertex
//! station of the window and at the ball's own axial reach past
//! its ends: an op over a bar with no hole, a window that stands
//! partly in solid metal, and a floor or ceiling the nose cannot
//! clear without biting it all refuse by name, long before a path
//! exists to be wrong.
//!
//! The three subjects split as the three files: this one the ride
//! and its numbers, [`doors`] the parity questions the model must
//! answer before a path exists, [`wall`] the MIN-fold sampler that
//! says what the hole's inner wall IS.
//!
//! Provenance: boring as inside radius-compensated profile
//! turning, the ball-center locus at the tool radius from the
//! hole wall, is machine-tool handbook chapter material (as is
//! bar-entering-along-the-axis with a radial lead-out), and the
//! even-odd ray-parity point-in-solid test is Ericson's
//! Real-Time Collision Detection, ch. 5, in its 3D form. The
//! min-fold of our own silhouette sweep, the parity-voted doors
//! and the nose-reach clip are ours, on the same frame and offset
//! machinery the blade kinds ride. No vendor strategy consulted.

mod doors;
mod wall;

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geometry::Triangle;
use crate::machining2d::GenError;

use super::offset;

use doors::bore_doors;
use wall::inner_wall;

/// Boring on a modeled hole: the window that bounds the ride, the
/// feeds, and the bar's ground nose `nose_radius` - which IS the
/// ball the inside envelope offsets, the same number stage E's law
/// made the OD rides' and the verdict's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoreParams {
    pub tolerance: f64,
    pub clearance_z: f64,
    pub top_z: f64,
    pub bottom_z: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    pub nose_radius: f64,
    /// The bar's head (flute) and shank diameters (#103): the insert
    /// sits at the head's edge, and the rapids ride the nose where the
    /// whole bar - [`bar_spans`] plus the nose - is centred on the
    /// spindle, the only posture a bar enters and leaves a hole barely
    /// wider than itself in. Both 0 ride the nose on the axis (a bar of
    /// no stated body).
    pub head_d: f64,
    pub shank_d: f64,
}

/// A boring bar's cross-section about its nose centre in the meridian
/// plane (#103), as signed offsets OUTWARD of the nose (+ away from the
/// spindle): `[head, shank]`. The insert sits at the head's edge, so
/// the head spans [-head_d, 0]. The shank is read both ways a tool
/// record can mean it - its edge at the insert's, or its axis on the
/// head's (then it reaches (shank_d - head_d)/2 PAST the nose) - and
/// the union of the two kept: [-max(shank_d, (head_d + shank_d)/2),
/// max(0, (shank_d - head_d)/2)].
pub fn bar_spans(head_d: f64, shank_d: f64) -> [(f64, f64); 2] {
    let (h, s) = (head_d.max(0.0), shank_d.max(0.0));
    [(-h, 0.0), (-(s.max((h + s) * 0.5)), ((s - h) * 0.5).max(0.0))]
}

/// The nose radius that centres the whole bar (head, shank and the nose
/// disc past the head's edge) on the spindle: `(L - H) / 2` for the
/// bar's reach `L` toward the axis and `H` outward of the nose.
pub fn centred_nose(head_d: f64, shank_d: f64, nose: f64) -> f64 {
    let sp = bar_spans(head_d, shank_d);
    let l = sp.iter().map(|q| -q.0).fold(0.0f64, f64::max);
    let h = sp.iter().map(|q| q.1).fold(nose.max(0.0), f64::max);
    ((l - h) * 0.5).max(0.0)
}

/// The float tie that says "just inside" - the reach tests stand a
/// hair off the exact tangency so a legal tangent ride never
/// reads as a bite.
const HAIR: f64 = 1e-9;

/// Turning boring: rapids down the bore's axis, a radial lead-out
/// onto the nose offset of the hole's inner wall, one ascending
/// ride to the window's top, and retract along the axis.
pub fn bore(facets: &[Triangle], p: &BoreParams) -> Result<Toolpath, GenError> {
    if facets.is_empty() {
        return Err(GenError::Invalid("no facets to bore".into()));
    }
    if ![p.clearance_z, p.top_z, p.bottom_z, p.nose_radius, p.tolerance, p.head_d, p.shank_d]
        .iter()
        .all(|v| v.is_finite())
    {
        return Err(GenError::Invalid(
            "boring was asked with a number that is not one".into(),
        ));
    }
    if p.nose_radius <= 0.0 {
        return Err(GenError::Invalid(format!(
            "a boring bar holds its wall with a ground nose - at {:.3} the tip is a \
             mathematical point, and a point cannot hold a round hole round",
            p.nose_radius
        )));
    }
    if p.tolerance <= 0.0 {
        return Err(GenError::Invalid(format!(
            "the bore tolerance must be a positive size, not {}",
            p.tolerance
        )));
    }
    let f = super::profile::Frame::about(facets)?;
    // The chord budget splits as the OD ride's: a quarter for the
    // wall's own sampling, a quarter for the center curve's.
    let wall = inner_wall(facets, &f, p.tolerance * 0.25)?;
    let s_lo = f
        .station_of(p.bottom_z)
        .max(wall[0].0)
        .min(wall[wall.len() - 1].0);
    let s_hi = f
        .station_of(p.top_z)
        .min(wall[wall.len() - 1].0)
        .max(wall[0].0);
    if s_hi <= s_lo {
        return Err(GenError::Empty(
            "nothing to bore - the height window holds no length of the part".into(),
        ));
    }
    let d = p.nose_radius;
    bore_doors(facets, &f, &wall, s_lo, s_hi, d)?;

    // The offset rides the WALL: an axis-end point of a floor or a
    // bottom face is not steel the nose rides - it is the hole's own
    // air boundary on the spindle centerline. profile_offset_in's
    // vertex-circle claim would apply its <=-branch there (a wall
    // radius of ~0 with the center standing above it) and force the
    // center onto the axis: a fit refusal no geometry earns. And the
    // claim is vacuous anyway once the doors above passed - a floor
    // or ceiling the ball cannot clear was just refused by name, so
    // every axis vertex stands at least a nose beyond the window and
    // its sqrt(d^2 - ds^2) is never real there. Dropping the point
    // states that theorem at the one place it could bite.
    let ride_wall: Vec<(f64, f64)> = wall.iter().copied().filter(|q| q.1 > HAIR).collect();
    if ride_wall.is_empty() {
        // The doors called the axis open air and the filter found no
        // steel at all - a slit, not a hole. Refuse by name; never
        // index a path that was never built.
        return Err(GenError::Invalid(
            "nothing to bore - the window holds no wall off the axis".into(),
        ));
    }
    let cl = offset::profile_offset_in(&ride_wall, d, p.tolerance * 0.25, s_lo, s_hi)?;
    let fit = cl.iter().map(|q| q.1).fold(f64::INFINITY, f64::min);
    if fit < 0.0 {
        let w_min = ride_wall
            .iter()
            .filter(|q| q.0 >= s_lo && q.0 <= s_hi)
            .map(|q| q.1)
            .fold(f64::INFINITY, f64::min);
        // #677: names no spot - the whole wall is too small for the bar.
        return Err(GenError::Invalid(format!(
            "the bar is bigger than its own work: a nose of radius {d:.3} cannot stand \
             inside a hole whose wall runs at radius {w_min:.3} - mount a smaller bar \
             or bore a bigger hole",
        )));
    }

    // The ride: in with the bar centred on the spindle, out to the
    // wall at the plunge feed, up the offset, off the wall and back up
    // the same line (#103): the whole bar ([`centred_nose`]: head,
    // shank and the nose past the head's edge) centred on the spindle
    // - never past the ride's own first radius.
    let centred = centred_nose(p.head_d, p.shank_d, p.nose_radius);
    let r_in = centred.min(cl[0].1.max(0.0));
    let r_out = centred.min(cl[cl.len() - 1].1.max(0.0));
    let s_home = f.station_of(p.clearance_z);
    let mut b = PathBuilder::new(f.point(r_in, s_home));
    b.push(MoveKind::Rapid, f.point(r_in, cl[0].0), None);
    b.push(MoveKind::Feed, f.point(cl[0].1, cl[0].0), Some(p.plunge_feed));
    for q in &cl[1..] {
        b.push(MoveKind::Feed, f.point(q.1, q.0), Some(p.feed));
    }
    b.push(
        MoveKind::Rapid,
        f.point(r_out, cl[cl.len() - 1].0),
        None,
    );
    b.push(MoveKind::Rapid, f.point(r_out, s_home), None);
    Ok(b.finish())
}
