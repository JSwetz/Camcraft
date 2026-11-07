//! Law (d), lateral rapids under the clearance plane over rest material - the shop practice of traversing only above the stock (machine-tool handbooks) on our own conservative rest model (split from `engine.rs` by M13.7; structure only).
//!
//! Issue #106: the column is read under the tool's whole footprint -
//! the disc of radius `r - gouge_tol` the mill law's other disc
//! readings use ([`Rest::hidden_along`], [`super::rapids_radial`]) -
//! never under its centre alone: a dive whose centre stands just off
//! the stock outline still meets the stock with its rim, and a dive
//! whose centre an earlier cut cleared can still land its far side on
//! uncut stock. Each sample is judged at the height the tool's own
//! tip stands at its radius - the tip plane plus the ball's, bull
//! corner's or point's rise there ([`Cutter::lift_at`]), so a ball
//! diving beside a wall whose rim is lifted clear of it is air (the
//! judged cutter: square when the context names no tip, which can
//! only over-report). Sampled: the centre, a ring at half the radius, and
//! [`RIM`] rim points, so a graze shallower than `r (1 - cos(pi/RIM))`
//! between two rim samples can pass - the same bound
//! [`super::rapids_radial`] states.

use super::laws::xy;
use super::midpose;
use super::probe::Cutter;
use super::rapids_posed::{self, PartIndex};
use super::rapids_radial;
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
/// Mode::Radial leaves the column on the same reading (M5.2 F2): the
/// bore is a tunnel a height field cannot see, so the retract
/// sliding out along its own kerf reads as lateral travel under
/// solid stock. It is judged instead by the solid reading of
/// [`super::rapids_radial`] (#79): the flute body against the stock
/// prism minus the kerf of the cuts before it - the kernel's
/// traverse promise is proved again at the post gate, never trusted.
///
/// A POSED rapid (held or stated pose off the upright) that the
/// column reading convicts is re-read against the solid before the
/// conviction stands ([`super::rapids_posed`]): the height field
/// cannot see the air under an overhang, and a swarf link retracting
/// down a contact normal under a tube's equator is air by the mesh.
/// Upright rapids keep the column verdict byte-identical.
pub(super) fn rapids_in_stock(
    path: &Toolpath,
    cx: &VerifyContext,
    rest: &Rest,
    cutter: &Cutter,
    warns: &mut Vec<Warning>,
) {
    if cx.mode == Mode::Radial {
        rapids_radial::radial_rapids(path, cx, rest, warns);
        return;
    }
    if !matches!(cx.mode, Mode::Revolve | Mode::Rotary) {
        let poses = midpose::poses(path);
        let mut part = PartIndex::new();
        for (i, m) in path.moves.iter().enumerate() {
            if m.kind != MoveKind::Rapid {
                continue;
            }
            let (a, b) = (xy(&m.from), xy(&m.to));
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let d = cx.tool.radius - cx.gouge_tol;
            if dx * dx + dy * dy <= cx.z_tol * cx.z_tol {
                // Issue #96: a vertical retract rides the tool's own
                // kerf, but a vertical rapid DOWN is judged by its
                // column - below the surface the cuts before it left
                // (a drill's R plane under unfaced stock) it is a
                // rapid in stock. A peck's re-entry stays above its
                // own earlier plunge, so it is acquitted by the cuts.
                let down = m.to.z < m.from.z - cx.z_tol;
                let under = |(q, rho): (Point2, f64)| {
                    rest.in_stock(q) && m.to.z + cutter.lift_at(rho) < rest.surface_before(q, i) - cx.z_tol
                };
                if down && footprint(a, d).any(under) {
                    warns.push(Warning::RapidInStock { at: i });
                }
                continue;
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
            // Cut order (M13.7): the rapid meets the stock as the cuts
            // BEFORE it left it - a later feed's clearing is no alibi.
            let bad = [a, mid, b]
                .into_iter()
                .flat_map(|c| footprint(c, d))
                .any(|(q, rho)| rest.in_stock(q) && low + cutter.lift_at(rho) < rest.surface_before(q, i))
                || rest.hidden_along(a, b, low, cx.tool.radius - cx.gouge_tol, i);
            if !bad {
                continue;
            }
            let pose = poses.get(i).copied().unwrap_or((m.axis, m.axis));
            if rapids_posed::leaning(pose) && !rapids_posed::in_rest(m, i, pose, cx, rest, cutter, &mut part) {
                continue; // the solid reading cleared what the column misread
            }
            warns.push(Warning::RapidInStock { at: i });
        }
    }
}

/// Rim samples of the footprint disc (#106) - [`super::rapids_radial`]'s count.
const RIM: usize = 16;

/// The tool footprint's sample points about `c`, each with its radius
/// from the axis: the centre, a ring of `RIM / 2` at half of `d`,
/// `RIM` on the rim. A `d` that is not a
/// finite positive radius (a tool no wider than the gouge tolerance)
/// is the centre alone - the column reading the law had before #106.
fn footprint(c: Point2, d: f64) -> impl Iterator<Item = (Point2, f64)> {
    let d = if d.is_finite() && d > 0.0 { d } else { 0.0 };
    let rings = if d > 0.0 { [(RIM / 2, 0.5 * d), (RIM, d)] } else { [(0, 0.0), (0, 0.0)] };
    std::iter::once((c, 0.0)).chain(rings.into_iter().flat_map(move |(n, rad)| {
        (0..n).map(move |j| {
            let t = std::f64::consts::TAU * j as f64 / n as f64;
            (Point2::new(c.x + rad * t.cos(), c.y + rad * t.sin()), rad)
        })
    }))
}
