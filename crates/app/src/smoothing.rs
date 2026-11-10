//! Per-operation smoothing (#616): what one operation asks of the
//! control's contour smoothing, dialect-neutral - a deviation in mm and
//! whether it finishes or roughs. The post writes the words.
//! Provenance: our own design from the request on #616 (finishing asks
//! maximum accuracy, roughing's aggressiveness follows the stock left);
//! the control words are each dialect's own public documentation, in
//! the posts. No vendor CAM material consulted.
//!
//! ## The law here
//!
//! - **Finish** - an op that states a contour `tolerance` and leaves no
//!   stock: the control may deviate by HALF that tolerance
//!   ([`FINISH_SHARE`]). The path is already generated and verified at
//!   the op's tolerance; the control's deviation stacks on top, so
//!   asking the whole tolerance again would let the part sit ~2x tol
//!   from the verified path (review of #635). Half keeps the stack at
//!   1.5x tol - stated here and in the decision record.
//! - **Rough** - an op that leaves stock: a fifth of the stock left
//!   ([`ROUGH_SHARE`]), capped at [`ROUGH_MAX_MM`], and NEVER more than
//!   that fifth - a thin allowance gets a tight ask, so a rounded corner
//!   never bites into what the finish pass is left to remove.
//! - **Nothing** - drilling, tapping, probing, threading, facing, the
//!   lathe kinds and anything else: `None`, the stream says nothing and
//!   the post writes nothing for that op.

use crate::operation::OpParams;
use crate::post::Smoothing;

/// The share of a finish op's tolerance the control may deviate by.
pub const FINISH_SHARE: f64 = 0.5;
/// The loosest rough deviation, mm: a quarter millimetre is already the
/// most a roughing corner should round over, whatever the stock left.
pub const ROUGH_MAX_MM: f64 = 0.25;
/// The share of the stock left a roughing path may deviate by.
pub const ROUGH_SHARE: f64 = 0.2;
/// A finish op that states no usable tolerance asks this, mm.
pub const FINISH_FLOOR_MM: f64 = 0.01;

/// The rough deviation for a stock-to-leave, mm: a fifth of it, capped -
/// never more than the fifth (no floor: a thin allowance asks tight).
/// `None` for a non-finite or non-positive allowance.
#[must_use]
pub fn rough_tol(stock_to_leave: f64) -> Option<f64> {
    (stock_to_leave.is_finite() && stock_to_leave > 0.0).then(|| (stock_to_leave * ROUGH_SHARE).min(ROUGH_MAX_MM))
}

fn finish(tol: f64) -> Smoothing {
    let tol = if tol.is_finite() && tol > 0.0 { tol } else { FINISH_FLOOR_MM };
    Smoothing { tol_mm: tol * FINISH_SHARE, finish: true }
}

fn by_leave(leave: f64, tol: f64) -> Smoothing {
    match rough_tol(leave) {
        Some(tol_mm) => Smoothing { tol_mm, finish: false },
        None => finish(tol),
    }
}

/// One operation's smoothing ask: see the module law.
#[must_use]
pub fn of(params: &OpParams) -> Option<Smoothing> {
    match params {
        OpParams::Contour { stock_to_leave, tolerance, .. } => Some(by_leave(*stock_to_leave, *tolerance)),
        OpParams::Chamfer2D { tolerance, .. }
        | OpParams::Finish3D { tolerance, .. }
        | OpParams::Contour5D { tolerance, .. }
        | OpParams::Parallel5D { tolerance, .. }
        | OpParams::Swarf { tolerance, .. }
        | OpParams::AdvancedSwarf { tolerance, .. }
        | OpParams::RotaryContour { tolerance, .. } => Some(finish(*tolerance)),
        OpParams::Pocket { stock_to_leave, .. }
        | OpParams::Adaptive2D { stock_to_leave, .. }
        | OpParams::Offset2D { stock_to_leave, .. }
        | OpParams::Slot { stock_to_leave, .. }
        | OpParams::Circular { stock_to_leave, .. }
        | OpParams::Bore { stock_to_leave, .. }
        | OpParams::Pocket3D { stock_to_leave, .. }
        | OpParams::RotaryOffset { stock_to_leave, .. }
        | OpParams::MultiAxisRough { stock_to_leave, .. } => Some(by_leave(*stock_to_leave, FINISH_FLOOR_MM)),
        _ => None,
    }
}
