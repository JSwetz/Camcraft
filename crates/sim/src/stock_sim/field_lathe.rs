//! The turning insert's swept solid on the field (M13.1 owed, #19):
//! the revolved stadium about a STATED lathe axis. Split from
//! `field_cut.rs` by the 400-line law.
//!
//! Provenance: the solid of revolution of a half-plane stadium (the
//! nose circle swept along its centre chord, the M5.2 E nose-centre
//! envelope) - plain plane geometry: the nearest point of a solid of
//! revolution to a query lies in the query's own meridian, so the
//! 2-D stadium distance in (radius, station) IS the 3-D distance.
//! Point-to-line projection for the meridian coordinates. Nothing
//! from vendor binaries.
//!
//! ## The laws here
//!
//! - **The axis is data, not a convention.** The turning kernels write
//!   world points in the plane of their fitted axis (`turning::Frame`):
//!   the axis stands wherever the part does. The sweep reads each
//!   endpoint's (radius, station) about that line; `LatheAxis::Z` (the
//!   world +Z through the origin) is the case the sim's own suite and
//!   `Stock::simulate_lathe` keep.
//! - **A chord keeps its side.** The second endpoint's radius is
//!   signed against the first's radial direction, so a parting chord
//!   that runs past the centre is the meridian segment it is (7 to -1,
//!   not 7 to 1); the distance reads both signs of the query's radius,
//!   so the kerf disc is exact either way.
//! - **A chord lies in one meridian, or is refused.** The flat
//!   (radius, station) segment is the swept solid only when both ends
//!   share a plane through the axis. A chord skewed to the axis sweeps
//!   a hyperboloid whose waist is nearer the axis than either end: the
//!   flat reading would show LESS removed than was cut, so such a
//!   chord refuses by name ([`LatheAxis::chord_in_meridian`]). The test
//!   is the out-of-plane offset `|ra x rb| / max(|ra|, |rb|)` against
//!   [`MERIDIAN_TOL`], so a chord through or near the axis (parting
//!   past centre) passes - its radial vectors are (anti)parallel or
//!   vanish.
//! - **Total input.** A non-finite or zero axis refuses by name before
//!   a cell is written (`LatheAxis::new`).

use super::field::cell_of;
use super::field_solid::dist_pt_seg2;
use super::kernel::StockError;

use crate::geometry::Point3;

/// How far (mm) a chord's second end may stand off its first end's
/// meridian plane before the flat meridian reading is refused - far
/// below any cell, far above the kernels' float noise.
pub const MERIDIAN_TOL: f64 = 1e-6;

/// The lathe's spindle line: a point on it and its unit direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LatheAxis {
    origin: Point3,
    dir: [f64; 3],
}

impl LatheAxis {
    /// World +Z through the origin - the convention `simulate_lathe`
    /// and the sim's parity suite stand on.
    pub const Z: LatheAxis = LatheAxis { origin: Point3::ZERO, dir: [0.0, 0.0, 1.0] };

    /// The axis through `origin` along `dir` (normalized), refused by
    /// name when either is not a finite line.
    pub fn new(origin: Point3, dir: [f64; 3]) -> Result<Self, StockError> {
        let n = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        if !origin.is_finite() || !n.is_finite() || n <= 0.0 {
            return Err(StockError::Invalid(format!(
                "lathe axis through {origin:?} along {dir:?} is not a finite line - the sim refuses instead of guessing a spindle"
            )));
        }
        Ok(LatheAxis { origin, dir: [dir[0] / n, dir[1] / n, dir[2] / n] })
    }

    /// The point on the axis at `station`.
    #[must_use]
    pub fn origin(&self) -> Point3 {
        self.origin
    }

    /// The unit direction.
    #[must_use]
    pub fn dir(&self) -> [f64; 3] {
        self.dir
    }

    /// `q` split about the axis: (radial vector, station).
    fn split(&self, q: Point3) -> ([f64; 3], f64) {
        let w = [q.x - self.origin.x, q.y - self.origin.y, q.z - self.origin.z];
        let d = self.dir;
        let s = w[0] * d[0] + w[1] * d[1] + w[2] * d[2];
        ([w[0] - s * d[0], w[1] - s * d[1], w[2] - s * d[2]], s)
    }

    /// `Ok` when the chord `a -> b` lies in one plane through the axis
    /// (within [`MERIDIAN_TOL`]); the refusal by name when it is skewed
    /// and its revolved sweep is not the flat meridian segment.
    pub fn chord_in_meridian(&self, a: Point3, b: Point3) -> Result<(), StockError> {
        let (ra, _) = self.split(a);
        let (rb, _) = self.split(b);
        let len = |v: [f64; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        let x = [ra[1] * rb[2] - ra[2] * rb[1], ra[2] * rb[0] - ra[0] * rb[2], ra[0] * rb[1] - ra[1] * rb[0]];
        let reach = len(ra).max(len(rb));
        let off = if reach > 0.0 { len(x) / reach } else { 0.0 };
        if off.is_finite() && off <= MERIDIAN_TOL {
            return Ok(());
        }
        Err(StockError::Invalid(format!(
            "lathe chord {a:?} -> {b:?} is skewed {off:.3e} mm off one meridian of the spindle - its revolved sweep is not the flat (radius, station) segment, so the sim refuses instead of showing less removed than was cut"
        )))
    }

    /// `q` in its own meridian: (radius >= 0, station).
    #[must_use]
    pub fn meridian(&self, q: Point3) -> (f64, f64) {
        let (r, s) = self.split(q);
        ((r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt(), s)
    }
}

/// One chord of a turning move, read in the meridian half-plane.
pub(crate) struct LatheSweep {
    r0: f64,
    z0: f64,
    r1: f64,
    z1: f64,
    nose: f64,
    axis: LatheAxis,
}

impl LatheSweep {
    /// The chord `a -> b` about `axis`: `b`'s radius signed against
    /// `a`'s radial direction (the side law above); a chord skewed
    /// off one meridian refuses by name (the meridian law above).
    pub(crate) fn of(axis: LatheAxis, a: Point3, b: Point3, nose: f64) -> Result<Self, StockError> {
        axis.chord_in_meridian(a, b)?;
        let (ra, z0) = axis.split(a);
        let (rb, z1) = axis.split(b);
        let len = |v: [f64; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        let (r0, mut r1) = (len(ra), len(rb));
        if ra[0] * rb[0] + ra[1] * rb[1] + ra[2] * rb[2] < 0.0 {
            r1 = -r1;
        }
        Ok(LatheSweep { r0, z0, r1, z1, nose, axis })
    }

    /// The nose radius: the solid's dilation of its chord.
    pub(crate) fn nose(&self) -> f64 {
        self.nose
    }

    /// The chord's first generator at the axis frame's own radial
    /// side - one point of the solid, never its extent.
    pub(crate) fn path(&self) -> (Point3, Point3) {
        let o = self.axis.origin;
        let d = self.axis.dir;
        let at = |s: f64| Point3::new(o.x + d[0] * s, o.y + d[1] * s, o.z + d[2] * s);
        (at(self.z0), at(self.z1))
    }

    /// Signed distance to the revolved stadium (negative inside):
    /// the query's meridian, both signs of its radius, exact.
    pub(crate) fn dist(&self, q: Point3) -> f64 {
        let (rho, s) = self.axis.meridian(q);
        dist_pt_seg2(rho, s, self.r0, self.z0, self.r1, self.z1)
            .min(dist_pt_seg2(-rho, s, self.r0, self.z0, self.r1, self.z1))
            - self.nose
    }

    /// The lattice box the solid can reach: the axis segment over the
    /// station span, dilated by the outer radius in each world axis
    /// as far as a disc normal to the axis reaches there
    /// (`rr * sqrt(1 - d_k^2)`), every reach padded by the nose, the
    /// cap and a cell. On `LatheAxis::Z` it is the annulus box the
    /// sweep has always had.
    pub(crate) fn window(&self, cell: f64, cap: f64) -> [(i32, i32); 3] {
        let pad = self.nose + cap + cell;
        let rr = self.r0.abs().max(self.r1.abs()) + pad;
        let (sl, sh) = (self.z0.min(self.z1) - pad, self.z0.max(self.z1) + pad);
        let o = [self.axis.origin.x, self.axis.origin.y, self.axis.origin.z];
        let d = self.axis.dir;
        let span = |k: usize| {
            let (a, b) = (o[k] + d[k] * sl, o[k] + d[k] * sh);
            let ext = rr * (1.0 - d[k] * d[k]).max(0.0).sqrt();
            (cell_of(a.min(b) - ext, cell), cell_of(a.max(b) + ext, cell))
        };
        [span(0), span(1), span(2)]
    }
}
