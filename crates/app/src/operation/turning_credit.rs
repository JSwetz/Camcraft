//! Issue #103: what a drill ahead removed, as the turned family's rapid law credits it - an axial pilot as a cylinder with its point's cone in the meridian plane (the twist-drill point geometry of machine-tool handbooks); our own design, no vendor material consulted.

use crate::cl::MoveKind;
use crate::rest::DrillAhead;

use super::turning_rapids::SignedMeridian;

/// A drill ahead's cut on the spindle axis, in the meridian plane:
/// stations `lo` (the point's tip) to `hi`, radius `r`, the point's
/// cone widening `1/cot` per unit above the tip (#89's drill atom).
pub(super) struct Pilot {
    pub(super) lo: f64,
    pub(super) hi: f64,
    pub(super) r: f64,
    pub(super) cot: Option<f64>,
}

impl Pilot {
    pub(super) fn removed(&self, s: f64, r: f64) -> bool {
        if s < self.lo || s > self.hi {
            return false;
        }
        let wide = match self.cot {
            Some(k) if k > 0.0 => ((s - self.lo) / k).min(self.r),
            _ => self.r,
        };
        r < wide
    }
}

/// The drills' cutting moves that run ON the spindle axis (an off-axis
/// hole on a lathe is not a cylinder about it, and credits nothing).
pub(super) fn pilots(drills: &[DrillAhead], sm: &SignedMeridian) -> Vec<Pilot> {
    const ON_AXIS: f64 = 1e-3;
    drills
        .iter()
        .filter(|d| d.radius.is_finite() && d.radius > 0.0)
        .flat_map(|d| {
            d.moves.iter().filter(|m| m.kind != MoveKind::Rapid && m.arc.is_none()).filter_map(move |m| {
                let (a, b) = (sm.sr(m.from), sm.sr(m.to));
                (a.1.abs() <= ON_AXIS && b.1.abs() <= ON_AXIS)
                    .then(|| Pilot { lo: a.0.min(b.0), hi: a.0.max(b.0), r: d.radius, cot: d.cone })
            })
        })
        .collect()
}
