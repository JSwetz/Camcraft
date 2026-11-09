//! How far an arc's true path can stand off the chords the fixture
//! laws sweep (#491). The box laws (c2)/(d2) and the posed sweep read
//! an arc as its inscribed chords ([`Move::segments3`]); the circle
//! bulges outward past every chord, so a band swept along the chords at
//! its bare radius misses steel standing in that sliver. Each chord is
//! padded here by what the arc can add to it:
//!
//! - **Across and along.** Take a chord of half-angle `psi` on a circle
//!   of radius `r`, in its own frame (chord along `t`, outward normal
//!   `n`). The true point at sweep fraction `s` stands off the chord's
//!   point at the same fraction by at most `r (1 - cos psi)` along `n`
//!   (the sagitta) and `r (psi - sin psi)` along `t`, for `psi <=
//!   pi/2`: with `u = 2s - 1`, the offsets are `r (cos(u psi) - cos
//!   psi)` and `r (sin(u psi) - u sin psi)`, and sine is concave on
//!   `[0, pi/2]`. A chord over a half turn or more (a circle under half
//!   the chord tolerance gets one chord) is bounded by the diameter,
//!   `2 r`. The bound is fraction-matched, so the posed sweep (which
//!   carries the pose by fraction) reads it as it stands.
//! - **Horizontal and vertical.** The box laws' bands are vertical
//!   columns with a strict z window, so the pad is split. In XY the
//!   whole deviation is horizontal, and a helix's z between two joints
//!   is linear in the sweep, the same as the chord's. In XZ or YZ only
//!   the deviation's horizontal part pads the radius. The z window
//!   takes the arc's true z range between the joints: the builder
//!   splits every vertical arc at its circle's top and bottom
//!   ([`crate::cl::PathBuilder::push_arc_in`]), so between two joints
//!   z is monotone and the joints bound it. An arc that turns past an
//!   extremum anyway (one built without the builder) has that piece's
//!   window carried to the extremum. A z pad on a monotone piece would
//!   break the strict-dip law: a lead arc landing tangent on a mounting
//!   face would report flush contact.
//! - **[`ARC_PAD`].** Arc centres stand on the 1 um lattice while the
//!   endpoints stay exact ([`crate::cl::ArcMove`]), so the circle the
//!   controller cuts can stand that far off the chord circle. Every arc
//!   chord's pad takes it, as the mesh law's chord reading does.
//!
//! A straight move pads nothing: its pieces, windows and verdicts are
//! byte-identical. An arc whose centre or radius is not a number fails
//! closed (a NaN pad, which the band and capsule predicates read as a
//! hit).
//!
//! Provenance: the sagitta of a circular segment and the offset between
//! an arc and its chord at a matched sweep fraction are elementary plane
//! geometry; the lattice is the CL contract's. Our own design; no vendor
//! source consulted.

use std::f64::consts::PI;

use crate::cl::{ArcPlane, Move};
use crate::geometry::Point3;

/// Pad on every arc chord beyond its measured deviation, mm: the arc
/// centre stands on the 1 um lattice, so the endpoints can sit up to
/// that far off the circle the controller cuts.
pub(super) const ARC_PAD: f64 = 1e-3;

/// One move's circle as its chords were cut ([`crate::cl::arc_chords`]'s
/// law: equal steps from the start angle, in the travel direction).
#[derive(Clone, Copy, Debug)]
pub(super) enum Bulge {
    /// A straight move: no pad.
    Straight,
    /// An arc that cannot be read (a centre or radius that is not a
    /// number): every pad is NaN, so the laws fail closed.
    Unread,
    /// An arc the chord law reads as its straight chord (zero radius,
    /// zero sweep): only the lattice pad.
    Degenerate,
    /// The circle: in-plane centre, radius, start angle and the signed
    /// angle each chord turns.
    Circle { plane: ArcPlane, cu: f64, cv: f64, r: f64, a0: f64, step: f64 },
}

impl Bulge {
    /// The bulge of move `m`, cut into `n` chords.
    pub(super) fn of(m: &Move, n: usize) -> Self {
        let Some(arc) = m.arc else { return Bulge::Straight };
        let pl = arc.plane;
        let (cu, cv) = pl.center_uv(arc.center);
        let (u0, v0, _) = pl.uvw(m.from);
        let (u1, v1, _) = pl.uvw(m.to);
        let r = (u0 - cu).hypot(v0 - cv).max((u1 - cu).hypot(v1 - cv));
        if !(cu.is_finite() && cv.is_finite() && r.is_finite()) {
            return Bulge::Unread;
        }
        let a0 = (v0 - cv).atan2(u0 - cu);
        let a1 = (v1 - cv).atan2(u1 - cu);
        let d = if arc.ccw { a1 - a0 } else { a0 - a1 };
        if !d.is_finite() {
            return Bulge::Unread;
        }
        let d = d.rem_euclid(2.0 * PI);
        if r <= f64::EPSILON || d <= 0.0 {
            return Bulge::Degenerate;
        }
        let step = (if arc.ccw { d } else { -d }) / n.max(1) as f64;
        Bulge::Circle { plane: pl, cu, cv, r, a0, step }
    }

    /// `(across, along)`: the most the true path stands off a chord
    /// along its outward normal and along itself (module header).
    fn offsets(r: f64, step: f64) -> (f64, f64) {
        let psi = 0.5 * step.abs();
        if psi <= 0.5 * PI {
            (r * (1.0 - psi.cos()), r * (psi - psi.sin()))
        } else {
            (2.0 * r, 2.0 * r)
        }
    }

    /// The most the true tip stands off the chord's point at the same
    /// sweep fraction, in 3-D, [`ARC_PAD`] included: the posed sweep's
    /// capsule growth.
    pub(super) fn deviation(&self) -> f64 {
        match *self {
            Bulge::Straight => 0.0,
            Bulge::Unread => f64::NAN,
            Bulge::Degenerate => ARC_PAD,
            Bulge::Circle { r, step, .. } => {
                let (across, along) = Self::offsets(r, step);
                across.hypot(along) + ARC_PAD
            }
        }
    }

    /// The radius pad for a vertical band swept along the chord `p`-`q`:
    /// the horizontal part of the deviation, [`ARC_PAD`] included.
    pub(super) fn horizontal(&self, p: Point3, q: Point3) -> f64 {
        let Bulge::Circle { plane, r, step, .. } = *self else { return self.deviation() };
        let (across, along) = Self::offsets(r, step);
        if plane.is_xy() {
            return across.hypot(along) + ARC_PAD;
        }
        let ((pu, pv, _), (qu, qv, _)) = (plane.uvw(p), plane.uvw(q));
        let len = (qu - pu).hypot(qv - pv);
        if len.is_nan() || len <= 0.0 {
            return across.hypot(along) + ARC_PAD;
        }
        let (tu, tv) = ((qu - pu) / len, (qv - pv) / len);
        // The horizontal in-plane axis: X is v in XZ, Y is u in YZ. The
        // in-plane normal is the chord turned a quarter, so its
        // horizontal share is the chord's vertical one.
        let (t_h, n_h) = if plane == ArcPlane::Xz { (tv, tu) } else { (tu, tv) };
        along * t_h.abs() + across * n_h.abs() + ARC_PAD
    }

    /// The tip's true z range over chord `j` (`p`-`q`): the joints', or
    /// for a vertical arc that turns past its circle's top or bottom
    /// inside the chord, carried to it (module header).
    pub(super) fn z_span(&self, j: usize, p: Point3, q: Point3) -> (f64, f64) {
        let (mut lo, mut hi) = (p.z.min(q.z), p.z.max(q.z));
        let Bulge::Circle { plane, cu, cv, r, a0, step } = *self else { return (lo, hi) };
        // z is u in XZ (top at angle 0) and v in YZ (top at pi/2).
        let (cz, top) = match plane {
            ArcPlane::Xy => return (lo, hi),
            ArcPlane::Xz => (cu, 0.0),
            ArcPlane::Yz => (cv, 0.5 * PI),
        };
        let start = a0 + step * j as f64;
        let span = step.abs();
        let eps = 1e-9 * span;
        for (angle, ez) in [(top, cz + r), (top + PI, cz - r)] {
            let off = (if step >= 0.0 { angle - start } else { start - angle }).rem_euclid(2.0 * PI);
            if off <= eps || off >= span - eps {
                continue;
            }
            // An extremum within the lattice pad of a joint is the
            // builder's own split, snapped to the lattice: the
            // Z-monotone law reads it, as every endpoint-z law does.
            if ez < lo - ARC_PAD {
                lo = ez - ARC_PAD;
            }
            if ez > hi + ARC_PAD {
                hi = ez + ARC_PAD;
            }
        }
        (lo, hi)
    }
}
