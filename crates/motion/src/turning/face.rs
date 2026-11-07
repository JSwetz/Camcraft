//! Facing: the station ladder of radial passes (M5.2 A).
//!
//! The law this kernel stands on: the CL point is the insert nose
//! CIRCLE'S CENTER, held at `station + nose_radius` along the axis,
//! so the nose ball is TANGENT to the facing plane at every point of
//! the pass - it cuts exactly the flat it is aimed at and can never
//! cross it. No gouging is therefore a construction, not a check:
//! a pass at station s removes precisely the band [s, s+2r], which
//! is why an infeed over twice the nose radius is REFUSED (adjacent
//! stations would leave an untouched sheet between them) rather
//! than trusted to the verifier to catch afterwards.
//!
//! The passes all run INWARD, from the outer band to the axis: the
//! spun work presents its fresh face to the tool only on the cut
//! move, and the retract slides the ball tangentially off the flat
//! it just made - the way a facing cut is actually taken. Rapids
//! travel at `r_max + stepdown + nose`, the rotary rough's own
//! clearance law read radially, where convexity of the distance-to-
//! axis makes `r_max` an exact bound from the vertices alone.
//!
//! Provenance: lathe facing as radial passes at axial stations -
//! standard turning practice from machine-tool handbooks and the
//! RS-274 G1 vocabulary; the ball-tangent no-gouge construction is
//! our own, on the same nose-envelope law the verifier's envelope
//! scan measures with. No vendor strategy consulted.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geometry::Triangle;
use crate::machining2d::GenError;

use super::profile;

/// The facing operation as the kernel reads it: three world heights
/// (the rapids plane, the claimed top of the work, and the plane
/// the last pass must leave flat), the RADIAL ladder's infeed, the
/// two feeds, and the insert's nose radius - the ball everything
/// turns on. There is no stepover because none is missed: one pass
/// per station runs the profile's whole width, the stepdown IS the
/// spacing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FacingParams {
    pub clearance_z: f64,
    pub top_z: f64,
    pub facing_z: f64,
    pub stepdown: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    pub nose_radius: f64,
}

/// A rung cap with room to spare: a finer promise than one pass in
/// 20 nanometres of 32-bit float is nobody's facing - refuse the
/// hang before it starts.
const MAX_RUNGS: f64 = 100_000.0;

/// Face the work: fit the axis, ladder the stations, walk them top
/// down. Refusals are by name; a clean pass-set is a Toolpath in
/// world coordinates, ready for the envelope verdict.
pub fn facing(facets: &[Triangle], p: &FacingParams) -> Result<Toolpath, GenError> {
    if facets.is_empty() {
        return Err(GenError::Invalid("no facets to face".into()));
    }
    if ![
        p.clearance_z,
        p.top_z,
        p.facing_z,
        p.stepdown,
        p.feed,
        p.plunge_feed,
        p.nose_radius,
    ]
    .iter()
    .all(|v| v.is_finite())
    {
        return Err(GenError::Invalid(
            "facing was asked with a number that is not one".into(),
        ));
    }
    if p.nose_radius <= 0.0 {
        return Err(GenError::Invalid(format!(
            "facing is an arc, not a line - a nose radius of {:.3} is a mathematical point, \
             and a point cannot face",
            p.nose_radius
        )));
    }
    if p.stepdown <= 0.0 {
        return Err(GenError::Invalid("facing needs a positive stepdown".into()));
    }
    if p.stepdown > 2.0 * p.nose_radius {
        return Err(GenError::Invalid(format!(
            "a stepdown of {:.3} is past the nose's reach: a ball of radius {:.3} clears \
             a band {:.3} tall, and the next station up would leave a sheet nothing touches \
             - tighten the stepdown",
            p.stepdown,
            p.nose_radius,
            2.0 * p.nose_radius
        )));
    }
    let f = profile::Frame::about(facets)?;

    // The work's whole-body extents, folded once from the vertices -
    // sound HERE (the body is the hull of all of them; profile.rs's
    // note says where that promise ends).
    let prof = profile::profile_of(facets, &f)?;
    let (r_max, z_top) = (prof.r_max, prof.z_top);
    if r_max <= 0.0 {
        return Err(GenError::Invalid(
            "the work has no radius to face - every facet sits on the axis".into(),
        ));
    }

    // The station ladder, anchored on the facing plane and stopped
    // at the claimed top of the work.
    let s_face = f.station_of(p.facing_z);
    let hi = f.station_of(p.top_z).min(z_top);
    if hi <= s_face {
        return Err(GenError::Empty(
            "nothing to face - the facing plane stands at or above the work's top".into(),
        ));
    }
    let rungs = ((hi - s_face) / p.stepdown).floor() + 1.0;
    if rungs > MAX_RUNGS {
        return Err(GenError::Invalid(format!(
            "the facing ladder would run {:.0} stations - the stepdown is far too fine \
             for the height between the plane and the top",
            rungs
        )));
    }
    let n = rungs as usize;
    // The topmost rung reaches its ball's ceiling at `station + 2r`;
    // the ladder's spacing law (`stepdown <= 2r`, refused above)
    // already guarantees that ceiling clears `hi`, so no separate
    // coverage test is needed here.

    // The infeed radius rung by rung, asked of the TRIANGLES: one
    // whose stations span [lo, top] occupies a pass's window
    // [s, s + 2 nose] exactly for s in [lo - 2 nose, top], and its
    // widest VERTEX bounds the whole triangle (convexity over the
    // hull of its own corners). Vertices filtered into the window
    // would miss a cylinder's side - both rim rings outside, the
    // wall itself at full radius inside - and the rung would then
    // cut only its little disk near the axis and leave an annulus
    // of stubble standing above the facing plane: the silent kind
    // of wrong, which no envelope verdict ever catches. The sweep
    // is events by rung start into a max-heap of (radius, life); a
    // rung's band is the top still alive at it. Overshoot is safe
    // (the bound is the triangle's widest vertex wherever in the
    // window it stands), so the whole-triangle bound is taken as
    // given.
    let mut bands = vec![0.0f64; n];
    {
        let mut events: Vec<(usize, Reach)> = Vec::with_capacity(facets.len());
        for t in facets {
            let (a, b, c) = (f.sr(t.a), f.sr(t.b), f.sr(t.c));
            let lo = a.0.min(b.0).min(c.0);
            let top = a.0.max(b.0).max(c.0);
            let rr = a.1.max(b.1).max(c.1);
            let k0 = ((lo - 2.0 * p.nose_radius - s_face) / p.stepdown).ceil();
            let k1 = ((top - s_face) / p.stepdown).floor();
            if k1 < 0.0 || k0 >= n as f64 || k0 > k1 {
                continue;
            }
            let k0 = k0.max(0.0) as usize;
            let k1 = k1.min((n - 1) as f64) as usize;
            events.push((k0, Reach { radius: rr, end: k1 }));
        }
        events.sort_unstable_by_key(|e| e.0);
        let mut heap: std::collections::BinaryHeap<Reach> =
            std::collections::BinaryHeap::with_capacity(events.len());
        let mut ei = 0usize;
        for (k, band) in bands.iter_mut().enumerate() {
            while ei < events.len() && events[ei].0 <= k {
                heap.push(events[ei].1);
                ei += 1;
            }
            while heap.peek().is_some_and(|e| e.end < k) {
                heap.pop();
            }
            *band = heap.peek().map_or(0.0, |e| e.radius);
        }
    }

    let rho_clear = r_max + p.stepdown + p.nose_radius;
    let center = |s: f64| s + p.nose_radius;
    let mut b = PathBuilder::new(f.point(rho_clear, center(f.station_of(p.clearance_z))));
    for k in (0..n).rev() {
        let s = s_face + (k as f64) * p.stepdown;
        // The controlled axial stationing, outside the whole work.
        b.push(MoveKind::Feed, f.point(rho_clear, center(s)), Some(p.plunge_feed));
        // Where this rung's air begins, from the reach sweep above - fed
        // in, not rapided (#103): the band is the MODEL's reach, and a
        // stock wider than the model stands between it and rho_clear at
        // this rung, below the last pass's floor. A feed through air is
        // slow; a rapid through the bar's corner is a crash.
        let r_band = bands[k];
        b.push(MoveKind::Feed, f.point(r_band + p.nose_radius, center(s)), Some(p.feed));
        // The pass itself: inward, tangent to the station, to the axis.
        b.push(MoveKind::Feed, f.point(0.0, center(s)), Some(p.feed));
        if k > 0 {
            b.push(MoveKind::Rapid, f.point(rho_clear, center(s)), None);
        }
    }
    // Off the finished face and home to the rapids plane.
    b.push(MoveKind::Rapid, f.point(rho_clear, center(s_face)), None);
    b.push(
        MoveKind::Rapid,
        f.point(
            rho_clear,
            center(f.station_of(p.clearance_z)),
        ),
        None,
    );
    Ok(b.finish())
}

/// One triangle's claim on the ladder: its widest radius, alive
/// through rung `end`. The heap is a max-heap on the radius; f64
/// is not `Ord`, but `total_cmp` is a total order on finite ones
/// and a corrupt vertex died at `profile_of`'s gate before any of
/// this was built.
#[derive(Clone, Copy)]
struct Reach {
    radius: f64,
    end: usize,
}

impl PartialEq for Reach {
    fn eq(&self, o: &Self) -> bool {
        self.radius.total_cmp(&o.radius) == std::cmp::Ordering::Equal && self.end == o.end
    }
}

impl Eq for Reach {}

impl Ord for Reach {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.radius
            .total_cmp(&o.radius)
            .then_with(|| self.end.cmp(&o.end))
    }
}

impl PartialOrd for Reach {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
