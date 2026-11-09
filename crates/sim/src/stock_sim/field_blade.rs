//! The turning tool's meridian footprint (#21, M13.1 owed): which
//! convex profile a turning move sweeps in its (radius, station)
//! half-plane - the nose disc, a grooving/parting blade's rectangle,
//! a threading insert's truncated V, or a form gauge's crown - and
//! the exact signed distance to its sweep along a chord.
//!
//! Provenance: the blade footprints are machine-tool handbook
//! turning-chapter material (a grooving/parting insert is a full-width
//! rectangle of its ground width, optionally corner-rounded; a
//! single-point thread insert is the V of its included angle cut off
//! at the crest width; a form gauge's edge is its ground arc). The
//! geometry is the published convex-geometry catalogue: the Minkowski
//! sum of a convex polygon and a segment is the convex hull of the
//! polygon at both ends (Andrew's monotone chain builds it), a
//! rounded polygon is a polygon dilated by a disc, and the signed
//! distance to a convex polygon is the nearest edge's distance,
//! negated inside. Nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **One reading with the rapid law.** The footprint stands where
//!   #103's meridian law (`turning_rapids::Meridian`) says it does: a
//!   blade's move carries its TIP at the centre of its band, the band
//!   spans `width / 2` either side along the axis, and the body stands
//!   OUTWARD (larger radius) of the tip; a thread's V widens by
//!   `tan(angle / 2)` per mm outward until it is the band wide (its
//!   crest), and stays the band wide above it; a form gauge's crown
//!   rises by the sagitta `R - sqrt(R^2 - d^2)` off its centre.
//! - **The body is finite and stated.** The blade's outward reach is
//!   its `height` (the tool's cutting depth): the field is a bounded
//!   lattice, so an infinite body would be an infinite window.
//! - **Exact where it can be, inscribed where it cannot.** The
//!   rectangle and the V are polygons and their sweep's distance is
//!   closed form - no sampling, no residual. The crown is the one
//!   curve: it is chorded with its vertices ON the arc, so the polygon
//!   lies inside the true crown (the field's never-over-claim side) by
//!   at most [`CROWN_SAG_TOL`], far below any cell.
//! - **Total input.** A non-finite or non-positive width, height,
//!   radius or angle, a corner wider than half the blade, a V not
//!   strictly between 0 and 180 degrees, or a band wider than the
//!   crown's own diameter refuses by name before a cell is written.

use super::kernel::StockError;

/// How far (mm) the chorded crown may stand inside the true arc.
pub const CROWN_SAG_TOL: f64 = 1e-4;

/// Ceiling on the crown's chords, whatever the tolerance asks.
const MAX_CROWN_CHORDS: usize = 256;

/// The meridian footprint a turning tool cuts with. Local frame:
/// `u` outward (larger radius) from the move's point, `v` along the
/// axis from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LatheProfile {
    /// A turning insert or boring bar: the nose disc of `radius`
    /// centred on the move (the M5.2 E nose-centre convention).
    Nose { radius: f64 },
    /// A grooving/parting blade: `width` along the axis centred on
    /// the move's tip, `height` outward, its corners rounded by
    /// `corner` (0 for a sharp blade).
    Blade { width: f64, corner: f64, height: f64 },
    /// A threading insert: the V of included `angle` (degrees) with
    /// its apex at the move's tip, cut off at the crest where it is
    /// `width` wide, the band `width` wide above it up to `height`.
    Thread { width: f64, angle: f64, height: f64 },
    /// A form gauge: the crown arc of radius `crown` with its lowest
    /// point at the move's tip, ground `width` wide, `height` outward.
    Form { width: f64, crown: f64, height: f64 },
}

/// A profile as the sweep reads it: a convex polygon (counter-
/// clockwise in (u, v), possibly a single point) and the disc it is
/// dilated by.
pub(crate) struct Footprint {
    pub(crate) poly: Vec<[f64; 2]>,
    pub(crate) round: f64,
}

fn positive(what: &str, v: f64) -> Result<(), StockError> {
    if v.is_finite() && v > 0.0 {
        Ok(())
    } else {
        Err(StockError::Invalid(format!("lathe profile: {what} {v} is not a positive finite number")))
    }
}

impl LatheProfile {
    /// `Ok` when the profile is a footprint the field can sweep; the
    /// refusal by name otherwise (the same gate the sweep applies,
    /// for a caller that wants it said before any cell is cut).
    pub fn validate(&self) -> Result<(), StockError> {
        self.footprint().map(|_| ())
    }

    /// The footprint swept along the meridian chord `(r0, z0) -> (r1,
    /// z1)` as (convex hull in (radius, station), the disc it is
    /// dilated by) - the very solid the field cuts, for a caller that
    /// credits a move's cut outside the field (#369's holder law).
    pub fn swept_outline(&self, r0: f64, z0: f64, r1: f64, z1: f64) -> Result<(Vec<[f64; 2]>, f64), StockError> {
        let f = self.footprint()?;
        Ok((f.swept(r0, z0, r1, z1), f.round))
    }

    /// The validated footprint, refused by name when nonsense.
    pub(crate) fn footprint(&self) -> Result<Footprint, StockError> {
        match *self {
            LatheProfile::Nose { radius } => {
                positive("nose radius", radius)?;
                Ok(Footprint { poly: vec![[0.0, 0.0]], round: radius })
            }
            LatheProfile::Blade { width, corner, height } => {
                positive("blade width", width)?;
                positive("blade height", height)?;
                if !(corner.is_finite() && corner >= 0.0 && corner <= 0.5 * width.min(height)) {
                    return Err(StockError::Invalid(format!(
                        "lathe profile: blade corner {corner} must be within 0 and half the blade ({width} wide, {height} tall)"
                    )));
                }
                let (h, c) = (0.5 * width - corner, corner);
                Ok(Footprint { poly: vec![[c, -h], [height - c, -h], [height - c, h], [c, h]], round: c })
            }
            LatheProfile::Thread { width, angle, height } => {
                positive("thread crest width", width)?;
                positive("thread insert height", height)?;
                if !(angle.is_finite() && angle > 0.0 && angle < 180.0) {
                    return Err(StockError::Invalid(format!(
                        "lathe profile: thread angle {angle} is not a V strictly between 0 and 180 degrees"
                    )));
                }
                let h = 0.5 * width;
                let crest = h / (0.5 * angle.to_radians()).tan();
                if !crest.is_finite() {
                    return Err(StockError::Invalid(format!("lathe profile: thread V {angle} deg x {width} has no finite crest")));
                }
                let top = height.max(crest);
                Ok(Footprint { poly: vec![[0.0, 0.0], [crest, -h], [top, -h], [top, h], [crest, h]], round: 0.0 })
            }
            LatheProfile::Form { width, crown, height } => {
                positive("form band width", width)?;
                positive("form crown radius", crown)?;
                positive("form gauge height", height)?;
                let h = 0.5 * width;
                if h > crown {
                    return Err(StockError::Invalid(format!(
                        "lathe profile: a form band {width} wide is wider than its crown's diameter {}", 2.0 * crown
                    )));
                }
                // Chords of angle `step` stand at most R(1 - cos(step/2))
                // inside the arc: pick the count that keeps it under tol.
                let span = (h / crown).clamp(-1.0, 1.0).asin();
                let step_max = 2.0 * (1.0 - CROWN_SAG_TOL / crown).clamp(-1.0, 1.0).acos();
                let n = if step_max > 0.0 { ((2.0 * span / step_max).ceil() as usize).clamp(2, MAX_CROWN_CHORDS) } else { MAX_CROWN_CHORDS };
                let sag = |a: f64| crown - crown * a.cos();
                let top = height.max(sag(span));
                let mut poly = vec![[top, -h]];
                for k in 0..=n {
                    let a = -span + 2.0 * span * (k as f64 / n as f64);
                    poly.push([sag(a), crown * a.sin()]);
                }
                poly.push([top, h]);
                // (top,-h) -> arc from -h to +h -> (top, h) runs clockwise in
                // (u, v); the sweep's hull orders it, so no reversal needed.
                Ok(Footprint { poly, round: 0.0 })
            }
        }
    }
}

impl Footprint {
    /// The farthest point of the solid from the move's point.
    pub(crate) fn reach(&self) -> f64 {
        self.poly.iter().map(|p| p[0].hypot(p[1])).fold(0.0, f64::max) + self.round
    }

    /// The polygon swept along the meridian chord `(r0, z0) -> (r1,
    /// z1)`, in (radius, station): the hull of both end copies. A
    /// blade's outward is +radius on the chord's own side (`r0`'s
    /// sign - a parting chord past centre keeps its body there).
    pub(crate) fn swept(&self, r0: f64, z0: f64, r1: f64, z1: f64) -> Vec<[f64; 2]> {
        let mut pts = Vec::with_capacity(2 * self.poly.len());
        for (r, z) in [(r0, z0), (r1, z1)] {
            pts.extend(self.poly.iter().map(|p| [r + p[0], z + p[1]]));
        }
        hull(pts)
    }
}

/// Andrew's monotone chain: the convex hull, counter-clockwise, no
/// collinear points; 1 or 2 points for a degenerate set.
fn hull(mut pts: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let chain = |it: &mut dyn Iterator<Item = [f64; 2]>| {
        let mut c: Vec<[f64; 2]> = Vec::new();
        for p in it {
            while c.len() >= 2 && cross(c[c.len() - 2], c[c.len() - 1], p) <= 0.0 {
                c.pop();
            }
            c.push(p);
        }
        c.pop(); // the other chain starts where this one ends
        c
    };
    let mut out = chain(&mut pts.iter().copied());
    out.extend(chain(&mut pts.iter().rev().copied()));
    out
}

/// Signed distance from `(x, y)` to the convex polygon `hull`
/// (counter-clockwise; a point or a segment is a zero-area set, never
/// strictly inside): the nearest edge's distance, negated inside.
pub(crate) fn sd_convex(hull: &[[f64; 2]], x: f64, y: f64) -> f64 {
    let n = hull.len();
    if n == 0 {
        return f64::INFINITY;
    }
    if n == 1 {
        return (x - hull[0][0]).hypot(y - hull[0][1]);
    }
    let mut d = f64::INFINITY;
    let mut inside = n >= 3;
    for i in 0..n {
        let a = hull[i];
        let b = hull[(i + 1) % n];
        d = d.min(super::field_solid::dist_pt_seg2(x, y, a[0], a[1], b[0], b[1]));
        if (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]) < 0.0 {
            inside = false;
        }
    }
    if inside {
        -d
    } else {
        d
    }
}
