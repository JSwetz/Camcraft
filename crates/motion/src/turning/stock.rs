//! Issue #130: the revolved stock envelope the turning kernels approach from - the bar's outer radius about the part's axis, read by the app layer as the meridian rapid law reads it; the shop rule that a lathe tool approaches and retracts clear of the bar's OD and roughs from it down (machine-tool handbooks); our own design, no vendor material consulted.

/// The setup stock as a turning kernel reads it: the outer radius the
/// bar reaches about the part's own fitted axis, and the radial
/// clearance every approach and retract keeps over it. `None` where a
/// kernel takes an `Option<StockEnv>` is "no stock readable": the
/// kernel keeps its part-based approach.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StockEnv {
    /// The stock's widest radius about the part's axis (mm).
    pub r_out: f64,
    /// The radial gap kept over `r_out` by every rapid (mm).
    pub clearance: f64,
}

/// How far the bar must stand past the part's widest radius to count
/// as wider (mm): a bar-sized stock's tessellation reads the part's own
/// radius to the float, and keeps the part-based program.
const WIDER: f64 = 1e-6;

impl StockEnv {
    /// The envelope, if its numbers are sizes and the bar stands wider
    /// than the part's widest radius `part_r` - a non-finite reading,
    /// or a bar the part fills, is the caller's part-based law.
    pub(crate) fn usable(s: Option<StockEnv>, part_r: f64) -> Option<StockEnv> {
        s.filter(|e| {
            e.r_out.is_finite() && e.clearance.is_finite() && e.clearance >= 0.0 && e.r_out > part_r + WIDER
        })
    }

    /// The rapid radius for a tool whose moves stand `reach` inside its
    /// own outermost steel (an insert's nose radius; 0 for a blade's
    /// tip): never less than the part-based `part_clear`, and clear of
    /// a bar wider than the part by the clearance.
    pub(crate) fn rapid_radius(s: Option<StockEnv>, part_r: f64, part_clear: f64, reach: f64) -> f64 {
        match Self::usable(s, part_r) {
            Some(e) => part_clear.max(e.r_out + reach.max(0.0) + e.clearance),
            None => part_clear,
        }
    }

    /// The radius a roughing ladder starts eating from: the bar's OD
    /// when it stands wider than the part.
    pub(crate) fn top(s: Option<StockEnv>, part_r: f64, part_top: f64) -> f64 {
        match Self::usable(s, part_r) {
            Some(e) => part_top.max(e.r_out),
            None => part_top,
        }
    }
}
