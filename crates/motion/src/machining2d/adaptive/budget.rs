//! The growth rings' planning budget (issue #13): the ring spiral
//! (`grow`) re-walks the whole cleared outline every revolution, so its
//! work grows with the square of how far the fronts must travel - a long
//! narrow channel (the band an Outside containment leaves round a part)
//! takes hundreds of revolutions, each over a ring hundreds of mm long,
//! and planned for minutes. Two laws keep it bounded and named:
//!
//! 1. **Up front**, the least work the spiral can need: the channel's
//!    length `L = area / (2 c)` (its center area over its width, `c` the
//!    clearance at the entry), its fronts each travelling `L / 2` at no
//!    more than the step cap a revolution, over rings averaging `L`
//!    round: `L^2 / (2 cap)`. Past [`LEAST_WORK_MAX`] tool radii the
//!    area is refused at once, by name.
//! 2. **While growing**, the length of every revolution tried is summed;
//!    past [`WORK_MAX`] tool radii the plan stops and is refused, whatever
//!    the shape (the Prime Directive's "never hang").
//!
//! The refused area is cut by the Offset rings, named (`pocket_hsm`).
//! Ours: the bound is plain arithmetic on the ring family's geometry;
//! the limits are calibrated on our own fixtures (the 60 x 14 slot needs
//! ~110 r up front and ~5.8 k r grown; the user's 14 mm band round a
//! 60 x 50 part ~3.4 k r up front). No vendor material consulted.

/// The least growth work (tool radii of revolution length) an area may
/// need before it is refused up front.
pub(super) const LEAST_WORK_MAX: f64 = 2_000.0;

/// The growth work (tool radii of revolutions tried, every margin
/// attempt) a plan may spend before it is refused.
pub(super) const WORK_MAX: f64 = 30_000.0;

/// The channel an area reads as: its length (mm) from its center
/// `area` and the clearance `clear` at the entry.
pub(super) fn channel_len(area: f64, clear: f64) -> f64 {
    if clear > 1e-9 { area / (2.0 * clear) } else { f64::INFINITY }
}

/// The least growth work (tool radii) for a channel `len` long, fronts
/// stepping at most `cap` mm a revolution, for a tool of radius `r`;
/// with the least revolutions it takes.
pub(super) fn least_work(len: f64, cap: f64, r: f64) -> (f64, f64) {
    let revs = 0.5 * len / cap.max(1e-9);
    (revs * len / r.max(1e-9), revs)
}

/// The refusal for a channel `width` mm wide (cutter reach included)
/// and `len` long that would need `revs` revolutions at least.
pub(super) fn too_long(width: f64, len: f64, revs: f64) -> String {
    format!(
        "is refused: the area is a channel {width:.1} mm wide and ~{len:.0} mm long (the band round a part?) - one adaptive spiral would need at least {revs:.0} revolutions along it, each round a ring up to ~{:.0} mm long: too narrow and long to plan", 2.0 * len
    )
}

/// The refusal once the growth work ran past [`WORK_MAX`].
pub(super) fn over_work(revs: usize) -> String {
    format!("is refused: growing the cleared area ran past its planning budget after {revs} revolutions")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_band_needs_far_more_work_than_a_short_slot() {
        // The 60 x 14 slot (center 50 x 4) and the 14 mm band round a
        // 60 x 50 part (center ~275 x 4), D10 at the default load.
        let cap = 1.5 * 2.0 * 5.0 * crate::load::DEFAULT_OPTIMAL_LOAD;
        let (slot, _) = least_work(channel_len(200.0, 2.0), cap, 5.0);
        let (band, revs) = least_work(channel_len(1100.0, 2.0), cap, 5.0);
        assert!(slot < 0.1 * LEAST_WORK_MAX && band > 1.5 * LEAST_WORK_MAX, "slot {slot}, band {band}");
        assert!(too_long(14.0, 275.0, revs).contains("14.0 mm wide"));
        assert_eq!(channel_len(10.0, 0.0), f64::INFINITY);
    }
}
