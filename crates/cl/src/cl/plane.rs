//! The arc's plane (M11.2 A): G17 XY, G18 XZ, G19 YZ.
//!
//! Provenance: plane selection and the circular-interpolation
//! direction rule of RS-274/NGC and ISO 6983 (public standards): G17
//! selects XY, G18 XZ (ZX), G19 YZ; G2 is clockwise and G3
//! counterclockwise as seen LOOKING FROM THE POSITIVE END of the axis
//! normal to the plane toward the origin. Clean-room: no vendor
//! material consulted.
//!
//! The convention, stated once for the whole codebase:
//!
//! - [`ArcMove::center`] holds the center's two in-plane coordinates in
//!   WORD order - `(X, Y)` for XY, `(X, Z)` for XZ, `(Y, Z)` for YZ -
//!   the pairs the I/J, I/K and J/K center words address. The third
//!   (normal) coordinate is not stored: an arc's normal coordinate may
//!   change between its endpoints, which is a helix along the plane's
//!   normal (Z for G17, Y for G18, X for G19), linear in the sweep.
//! - [`ArcMove::ccw`] is G3 (`true`) / G2 (`false`) by the RS-274 rule
//!   above, in EVERY plane. The math runs in a right-handed in-plane
//!   frame `(u, v)` whose cross product is the plane's +normal -
//!   [`ArcPlane::uvw`] - so "ccw" is the ordinary counterclockwise
//!   angle of `atan2(v, u)` in all three planes:
//!   XY `(u, v, w) = (x, y, z)`, XZ `(z, x, y)` (Z × X = +Y), YZ
//!   `(y, z, x)` (Y × Z = +X).
//! - G18's well-known handedness falls out: drawn with X to the right
//!   and Z up (the front view, seen from -Y), a G3 arc APPEARS
//!   clockwise, because the rule looks from +Y. Worked example: center
//!   (X0, Z0), from (X10, Z0) to (X0, Z10) - the short quarter is G2
//!   (in `(u, v) = (z, x)` it runs from 90° down to 0°), its midpoint
//!   is (X7.071, Z7.071); the same endpoints with G3 sweep 270° through
//!   (X0, Z-10) and then (X-10, Z0). (Seen from +Y the viewer's right
//!   is -X: the "CCW" of G3 runs +X -> -Z -> -X -> +Z.) G19, by
//!   contrast, reads naturally from
//!   +X with Y right and Z up: center (Y0, Z0), (Y10, Z0) -> (Y0, Z10)
//!   is the G3 quarter.
//!
//! A vertical-plane arc never turns past a Z extremum inside its sweep:
//! [`super::PathBuilder::push_arc_in`] splits it there, so every
//! XZ/YZ arc in a path is Z-monotone and its Z range is its endpoints',
//! the reading every endpoint-Z law (verify's level and band laws)
//! already takes of a helix.

use std::f64::consts::PI;

use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::moves::{arc_chords, ArcMove, Move};

/// The plane an arc turns in. `Xy` is the default and the only plane
/// any generator produced before M11.2; it states nothing new anywhere
/// (dump, stream, project file, NC).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ArcPlane {
    #[default]
    Xy,
    Xz,
    Yz,
}

impl ArcPlane {
    /// The RS-274 plane-select number: 17, 18 or 19.
    #[must_use]
    pub const fn g_code(self) -> u8 {
        match self {
            ArcPlane::Xy => 17,
            ArcPlane::Xz => 18,
            ArcPlane::Yz => 19,
        }
    }

    #[must_use]
    pub const fn is_xy(self) -> bool {
        matches!(self, ArcPlane::Xy)
    }

    /// The stream / file spelling: `XY`, `XZ`, `YZ`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            ArcPlane::Xy => "XY",
            ArcPlane::Xz => "XZ",
            ArcPlane::Yz => "YZ",
        }
    }

    /// The center-word letters, in [`ArcMove::center`]'s order.
    #[must_use]
    pub const fn center_letters(self) -> (char, char) {
        match self {
            ArcPlane::Xy => ('I', 'J'),
            ArcPlane::Xz => ('I', 'K'),
            ArcPlane::Yz => ('J', 'K'),
        }
    }

    /// The plane a center-word pair addresses (`I`+`J`, `I`+`K`,
    /// `J`+`K`); anything else names no plane.
    #[must_use]
    pub fn from_letters(a: char, b: char) -> Option<Self> {
        match (a, b) {
            ('I', 'J') => Some(ArcPlane::Xy),
            ('I', 'K') => Some(ArcPlane::Xz),
            ('J', 'K') => Some(ArcPlane::Yz),
            _ => None,
        }
    }

    /// The right-handed in-plane frame: `(u, v, w)` with `u × v` the
    /// plane's +normal and `w` the normal coordinate. Pure
    /// permutation - no arithmetic, so the XY case is the identity
    /// bit for bit.
    #[must_use]
    pub fn uvw(self, p: Point3) -> (f64, f64, f64) {
        match self {
            ArcPlane::Xy => (p.x, p.y, p.z),
            ArcPlane::Xz => (p.z, p.x, p.y),
            ArcPlane::Yz => (p.y, p.z, p.x),
        }
    }

    /// The inverse of [`uvw`](Self::uvw).
    #[must_use]
    pub fn point(self, u: f64, v: f64, w: f64) -> Point3 {
        match self {
            ArcPlane::Xy => Point3::new(u, v, w),
            ArcPlane::Xz => Point3::new(v, w, u),
            ArcPlane::Yz => Point3::new(w, u, v),
        }
    }

    /// A word-order center as `(u, v)`.
    #[must_use]
    pub fn center_uv(self, c: Point2) -> (f64, f64) {
        match self {
            ArcPlane::Xy | ArcPlane::Yz => (c.x, c.y),
            ArcPlane::Xz => (c.y, c.x),
        }
    }

    /// A 3-D point's word-order in-plane pair (what a center stores).
    #[must_use]
    pub fn words(self, p: Point3) -> Point2 {
        match self {
            ArcPlane::Xy => Point2::new(p.x, p.y),
            ArcPlane::Xz => Point2::new(p.x, p.z),
            ArcPlane::Yz => Point2::new(p.y, p.z),
        }
    }
}

/// An arc's in-plane geometry, read once: start angle, radius, sweep
/// (in the travel direction, `(0, 2pi]`), plus the uv center and the
/// endpoints' normal coordinates. None for the degenerate arc (zero
/// radius, non-finite, zero sweep) - every caller falls back to the
/// straight move then, `arc_chords`' law.
pub(crate) struct Sweep {
    pub cu: f64,
    pub cv: f64,
    pub r: f64,
    pub a0: f64,
    pub d: f64,
    pub w0: f64,
    pub w1: f64,
}

pub(crate) fn sweep(arc: &ArcMove, from: Point3, to: Point3) -> Option<Sweep> {
    let pl = arc.plane;
    let (cu, cv) = pl.center_uv(arc.center);
    let (u0, v0, w0) = pl.uvw(from);
    let (u1, v1, w1) = pl.uvw(to);
    let ra = ((u0 - cu).powi(2) + (v0 - cv).powi(2)).sqrt();
    let rb = ((u1 - cu).powi(2) + (v1 - cv).powi(2)).sqrt();
    let r = ra.max(rb);
    let a0 = (v0 - cv).atan2(u0 - cu);
    let a1 = (v1 - cv).atan2(u1 - cu);
    let mut d = if arc.ccw { a1 - a0 } else { a0 - a1 };
    d = d.rem_euclid(2.0 * PI);
    if r.is_finite() && d.is_finite() && r > f64::EPSILON && d > 0.0 {
        Some(Sweep { cu, cv, r, a0, d, w0, w1 })
    } else {
        None
    }
}

impl ArcMove {
    /// An XY (G17) arc - the pre-M11.2 shape. The center is taken as
    /// given (struct-literal semantics); [`Move::with_arc`] snaps.
    #[must_use]
    pub const fn xy(center: Point2, ccw: bool) -> Self {
        ArcMove { center, ccw, plane: ArcPlane::Xy }
    }

    /// An arc in `plane`, center in that plane's word order.
    #[must_use]
    pub const fn in_plane(center: Point2, ccw: bool, plane: ArcPlane) -> Self {
        ArcMove { center, ccw, plane }
    }

    /// `(radius, sweep)` in the arc's own plane; `(0.0, 0.0)` for a
    /// degenerate arc. The XY case is the timeline's historical
    /// `arc_sweep` law, float for float.
    #[must_use]
    pub fn radius_sweep(&self, from: Point3, to: Point3) -> (f64, f64) {
        sweep(self, from, to).map_or((0.0, 0.0), |s| (s.r, s.d))
    }

    /// The point at sweep fraction `f` (0 = `from`, 1 = `to`), the
    /// normal coordinate linear in the sweep (the helix law). None
    /// for a degenerate arc.
    #[must_use]
    pub fn point_at(&self, from: Point3, to: Point3, f: f64) -> Option<Point3> {
        let s = sweep(self, from, to)?;
        let th = s.a0 + if self.ccw { s.d * f } else { -s.d * f };
        Some(self.plane.point(
            s.cu + s.r * th.cos(),
            s.cv + s.r * th.sin(),
            s.w0 + (s.w1 - s.w0) * f,
        ))
    }

    /// Exact path length: in-plane arc length over the normal rise
    /// (a helix's), or `None` when degenerate (the caller measures
    /// the straight chord).
    #[must_use]
    pub fn length(&self, from: Point3, to: Point3) -> Option<f64> {
        let s = sweep(self, from, to)?;
        let a = s.r * s.d;
        let dw = s.w1 - s.w0;
        Some((a * a + dw * dw).sqrt())
    }
}

/// A vertical-plane arc's 3-D chords at sagitta `tol`, endpoints
/// exact: the in-plane tessellation is `arc_chords`' (one sagitta law
/// for the codebase), the normal coordinate linear by chord index
/// (equal angle steps, so linear in the sweep).
pub(crate) fn vertical_segments(m: &Move, arc: ArcMove, tol: f64) -> Vec<(Point3, Point3)> {
    let pl = arc.plane;
    let (cu, cv) = pl.center_uv(arc.center);
    let (u0, v0, w0) = pl.uvw(m.from);
    let (u1, v1, w1) = pl.uvw(m.to);
    let cs = arc_chords(
        Point2::new(u0, v0),
        Point2::new(u1, v1),
        ArcMove::xy(Point2::new(cu, cv), arc.ccw),
        tol,
    );
    let n = cs.len().max(1) as f64;
    let last = cs.len().saturating_sub(1);
    cs.into_iter()
        .enumerate()
        .map(|(j, (a, b))| {
            let pa = if j == 0 {
                m.from
            } else {
                pl.point(a.x, a.y, w0 + (w1 - w0) * (j as f64 / n))
            };
            let pb = if j == last {
                m.to
            } else {
                pl.point(b.x, b.y, w0 + (w1 - w0) * ((j + 1) as f64 / n))
            };
            (pa, pb)
        })
        .collect()
}

/// The interior sweep fractions at which a vertical arc reaches a Z
/// extremum (top or bottom of its circle), ascending; empty for XY
/// arcs and for arcs whose extrema sit at (or past) their endpoints.
/// The builder splits there - the Z-monotone law of the module header.
pub(crate) fn z_extrema(arc: &ArcMove, from: Point3, to: Point3) -> Vec<f64> {
    // Z is u in the XZ frame (z, x, y) and v in the YZ frame (y, z, x):
    // the circle's top/bottom angles.
    let angles = match arc.plane {
        ArcPlane::Xy => return Vec::new(),
        ArcPlane::Xz => [0.0, PI],
        ArcPlane::Yz => [0.5 * PI, 1.5 * PI],
    };
    let Some(s) = sweep(arc, from, to) else {
        return Vec::new();
    };
    // A relative margin keeps an extremum that IS an endpoint (the
    // tangent bottom of a lead or entry arc) from splitting off a
    // zero-length piece.
    let eps = 1e-9;
    let mut fs: Vec<f64> = angles
        .iter()
        .filter_map(|&te| {
            let de = if arc.ccw { te - s.a0 } else { s.a0 - te };
            let f = de.rem_euclid(2.0 * PI) / s.d;
            (f > eps && f < 1.0 - eps).then_some(f)
        })
        .collect();
    fs.sort_by(f64::total_cmp);
    fs
}
