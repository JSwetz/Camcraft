//! Law (d), lateral rapids under the clearance plane over rest material - the shop practice of traversing only above the stock (machine-tool handbooks) on our own conservative rest model (split from `engine.rs` by M13.7; structure only).

use super::laws::xy;
use super::rest::Rest;
use super::types::{Mode, VerifyContext, Warning};

use crate::cl::{MoveKind, Toolpath};
use crate::geo2d::Point2;

/// (d) Rapids: lateral travel under the clearance plane inside stock.
/// Mode::Revolve stands this law down (M5.2 D): the XY column is the
/// mill's geometry - a turning insert is a POINT in XY and its whole
/// form lives in the meridian plane. The pull-out from a thread's
/// last V slides along flanks that coincide with the cut's own
/// surfaces, yet the column above the tip circle is solid core
/// forever - so this law convicts every full-depth threading retract
/// for a collision the meridian scan knows cannot happen. The
/// turning rapids' safety promise is the kernel's own: radial exits
/// from cut stations and traverses at rho_clear, asserted by each
/// family's tests. Fixture steel (d2) still stands - it is a box.
/// Mode::Radial stands down on the same reading (M5.2 F2): the
/// bore is a tunnel a height field cannot see, so the retract
/// sliding out along its own kerf reads as lateral travel under
/// solid stock; the exit-the-hole-along-its-axis and outside-
/// stock_r traverse promises are the kernel's, asserted in
/// drilling's cross tests.
pub(super) fn rapids_in_stock(
    path: &Toolpath,
    cx: &VerifyContext,
    rest: &Rest,
    warns: &mut Vec<Warning>,
) {
    if !matches!(cx.mode, Mode::Revolve | Mode::Radial | Mode::Rotary) {
        for (i, m) in path.moves.iter().enumerate() {
            if m.kind != MoveKind::Rapid {
                continue;
            }
            let (a, b) = (xy(&m.from), xy(&m.to));
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            if dx * dx + dy * dy <= cx.z_tol * cx.z_tol {
                continue; // vertical retract through the tool's own kerf
            }
            let low = m.from.z.min(m.to.z);
            if low >= cx.clearance_z - cx.z_tol {
                continue;
            }
            let mid = Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
            // The overhang backstop (M13.7 B, [`super::hidden`]): the
            // tool disc swept through stock hidden under a ledge, which
            // no column surface shows - asked only when the column law
            // is silent, so it adds convictions, never removes one.
            let bad = [a, mid, b]
                .into_iter()
                .any(|q| rest.in_stock(q) && low < rest.surface(q))
                || rest.hidden_along(a, b, low, cx.tool.radius - cx.gouge_tol);
            if bad {
                warns.push(Warning::RapidInStock { at: i });
            }
        }
    }
}
