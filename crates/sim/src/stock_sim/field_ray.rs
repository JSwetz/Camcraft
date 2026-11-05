//! The ray-entry companion of the cut layer (M7.1 C): where the
//! vertical ray through a claimed cell first MEETS the swept solid.
//! Split from `field_cut` by the 400-line law; the storage rides
//! `field`'s struct, the sweeps live in `field_cut`, the consumer of
//! the numbers is `march`'s verdict walk.
//!
//! Provenance: a sweep of a convex solid along a segment is convex
//! (Minkowski sums - the same law `field_cut`'s distance rests on),
//! so its intersection with any line is an interval and the ray's
//! first crossing is a 1-D convex root, found here by bisection on
//! the same conservative distance. Nothing from vendor binaries.
//!
//! ## The law here
//!
//! - **The cell's cut magnitude measures the WRONG direction for a
//!   flank.** A pocket rim column says "tool wall 0.12 away" while
//!   nothing was removed above the center at all; a cone flank column
//!   says "0.06 away" while its surface stands 0.08 up the ray. One
//!   number cannot serve both verdicts - so the field keeps a second
//!   one: the entry distance up the vertical ray, NaN where the ray
//!   never meets the solid. The march answers a column by its entry
//!   when one exists (the cut's own surface, conservative to a lower
//!   bound) and by the cell boundary otherwise (the mesh's surface,
//!   exact for every lattice-straddled plane).
//! - **Conservative means the removed side.** `phi_sweep >= true
//!   distance` (`field_cut`), so a probe that says INSIDE is a proof,
//!   and a bisection that reports the last proven-inside point can
//!   only sit at or below the true entry: the estimated surface
//!   under-runs, the verdict over-reports the tool's reach, and no
//!   gouge is ever hidden by this layer.
//! - **Merged by a min.** The surface is the lowest entry any move
//!   proved (removals union upward along a ray), just as the cut
//!   magnitude is a running max - both commutative, replay stays
//!   order-independent.

use super::field::SdfField;
use super::field_cut::Sweep;

use crate::geometry::Point3;

/// Bisection rounds on the convex root: the interval ends at
/// cell * 1e-12, far under any float the verdict compares with.
const ROOT_ITERS: u32 = 40;

impl SdfField {
    /// The entry layer's value at a cell: the distance from the
    /// cell's CENTER UP to the first point the vertical ray is
    /// proven inside the swept solid; NaN where no move's solid ever
    /// meets the ray above any center's cell.
    pub(crate) fn up_at(&self, key: [i32; 3]) -> f64 {
        match self.cut_ups.get(&super::field::brick_of(key)) {
            Some(b) => b[super::field::in_brick(key)],
            None => f64::NAN,
        }
    }

    /// Record one proven entry, keeping the LOWEST (the union law).
    pub(crate) fn up_record(&mut self, key: [i32; 3], u: f64) {
        let slot = &mut self
            .cut_ups
            .entry(super::field::brick_of(key))
            .or_insert_with(|| Box::new([f64::NAN; super::field::BRICK3]))
            [super::field::in_brick(key)];
        if slot.is_nan() || u < *slot {
            *slot = u;
        }
    }
}

/// The vertical ray's first proven-inside point above `q`, measured
/// as a distance up from `q.z`, within the cell that holds `q`
/// (anything higher is the walk's business, not this cell's law).
/// NaN when the ray stays clear of the solid for the whole cell.
pub(crate) fn vertical_entry(ax: [f64; 3], sw: &Sweep, q: Point3, cell: f64) -> f64 {
    let g = |t: f64| sw.dist(ax, Point3::new(q.x, q.y, q.z + t));
    if g(0.0) < 0.0 {
        return 0.0; // the solid already covers the center
    }
    // Convex minimum of g on [0, cell]: golden section, same floor
    // as `field_cut`'s sweep search.
    const PHI: f64 = 0.6180339887498949;
    let (mut lo, mut hi) = (0.0f64, cell);
    let mut c = hi - PHI * (hi - lo);
    let mut d = lo + PHI * (hi - lo);
    let (mut fc, mut fd) = (g(c), g(d));
    for _ in 0..32 {
        if fc < fd {
            hi = d;
            d = c;
            fd = fc;
            c = hi - PHI * (hi - lo);
            fc = g(c);
        } else {
            lo = c;
            c = d;
            fc = fd;
            d = lo + PHI * (hi - lo);
            fd = g(d);
        }
    }
    let t_min = if fc <= fd { c } else { d };
    if g(t_min) >= 0.0 {
        return f64::NAN; // the ray never meets the solid inside this cell
    }
    // Bisect the decreasing leg [0, t_min], keeping the INSIDE side
    // (the proven one) as the answer: a lower bound of the true root.
    let (mut inside, mut outside) = (t_min, 0.0f64);
    for _ in 0..ROOT_ITERS {
        let mid = (inside + outside) * 0.5;
        if g(mid) < 0.0 {
            inside = mid;
        } else {
            outside = mid;
        }
    }
    inside
}
