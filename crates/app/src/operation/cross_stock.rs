//! The cross drill stands off the RAW stock (issue #77): a cross drill
//! run before the bar is turned to size meets the setup's stock, not
//! the finished part, so every standoff the radial kernel measures -
//! the hop and clearance radii, the chord law between stations, the
//! feed-entry point above each mouth, the end-face detour - is taken
//! about the larger of the two. A stock wider than the part moves each
//! hole's mouth out to the raw surface along its own axis (the floor
//! stays where it was, so the drill feeds through the uncut skin), and
//! the finished chain is then proven clear of that stock rapid by
//! rapid - refused by name if any one is not, never posted. The
//! verify crate's rapid-in-stock law stands down in `Mode::Radial` (a
//! bore is a tunnel its height field cannot see), so this proof is the
//! only one the radial rapids get.
//!
//! Provenance: traversing only outside the uncut material is the
//! machine-tool handbooks' shop practice; the stock of a lathe job is
//! the bar as clamped (RS-274 / ISO 6983 radial measurement from the
//! spindle centreline). Closest approach of a segment to a line is
//! textbook geometry. Our own design notes; no vendor material
//! consulted.

use crate::cl::{MoveKind, Toolpath};
use crate::drilling::CrossHole;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

/// What the cross drill stands off, measured about world Z through the
/// spindle origin `(ox, oy)` (already the kernel's frame).
pub(super) struct Standoff {
    /// The radius the kernel's traverse law stands off.
    pub r: f64,
    /// The Z top the end-face detour swings above.
    pub z_max: Option<f64>,
    /// The raw stock's radius when it is wider than the part: the
    /// mouths move out to it. `None` keeps the part-only chain.
    pub raw: Option<f64>,
}

fn reach(facets: &[Triangle], ox: f64, oy: f64) -> (f64, Option<f64>) {
    let verts = || facets.iter().flat_map(|t| [t.a, t.b, t.c]).filter(|v| v.is_finite());
    let r = verts().map(|v| (v.x - ox).hypot(v.y - oy)).fold(0.0_f64, f64::max);
    (r, verts().map(|v| v.z).reduce(f64::max))
}

/// Measure the part and, when known, the setup's stock about the
/// spindle. A stock's vertex hull bounds it, so the max vertex radius
/// is a box's circumscribed radius and a cylinder's own. An absent
/// stock (none declared, or the op's source IS the stock) measures the
/// part - today's reading, said so in the doc comment, not guessed.
/// The stock counts only when it reaches past the part, so a stock
/// equal to the part keeps the part-only chain bit for bit.
pub(super) fn standoff(part: &[Triangle], stock: Option<&[Triangle]>, ox: f64, oy: f64) -> Standoff {
    let (part_r, part_z) = reach(part, ox, oy);
    let Some((stock_r, stock_z)) = stock.filter(|s| !s.is_empty()).map(|s| reach(s, ox, oy)) else {
        return Standoff { r: part_r, z_max: part_z, raw: None };
    };
    let raw = (stock_r > part_r + 1e-9).then_some(stock_r);
    let z_max = match (part_z, stock_z) {
        (Some(a), Some(b)) if b > a + 1e-9 => Some(b),
        (a, b) => a.or(b),
    };
    Standoff { r: raw.unwrap_or(part_r), z_max, raw }
}

/// Each hole's mouth moved out along its own axis to the raw stock's
/// radius `raw` (the floor unchanged: `floor_d` deepens by the shift).
/// A hole whose line misses the stock radius, or whose mouth already
/// stands outside it, is untouched. `holes` are in the spindle frame.
pub(super) fn to_raw_mouths(holes: &[CrossHole], raw: f64) -> Vec<CrossHole> {
    holes
        .iter()
        .map(|h| {
            // Axis-aligned XY plunges (the kernel refuses anything
            // else): the plunge coordinate and the transverse offset.
            let (c, t) = if h.dir.x.abs() >= h.dir.y.abs() {
                (h.origin.x, h.origin.y)
            } else {
                (h.origin.y, h.origin.x)
            };
            let surf = (raw * raw - t * t).max(0.0).sqrt();
            let shift = surf - c.abs();
            if !(shift.is_finite() && shift > 0.0) {
                return *h;
            }
            CrossHole {
                origin: Point3::new(
                    h.origin.x - h.dir.x * shift,
                    h.origin.y - h.dir.y * shift,
                    h.origin.z - h.dir.z * shift,
                ),
                floor_d: h.floor_d - shift,
                ..*h
            }
        })
        .collect()
}

fn approach(a: Point3, b: Point3) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let ll = dx * dx + dy * dy;
    let t = if ll <= 0.0 { 0.0 } else { (-(a.x * dx + a.y * dy) / ll).clamp(0.0, 1.0) };
    (a.x + t * dx).hypot(a.y + t * dy)
}

/// On the line through `p` along unit `dir`, within `tol`?
fn on_line(q: Point3, p: Point3, dir: [f64; 3]) -> bool {
    let w = [q.x - p.x, q.y - p.y, q.z - p.z];
    let s = w[0] * dir[0] + w[1] * dir[1] + w[2] * dir[2];
    let perp = [w[0] - s * dir[0], w[1] - s * dir[1], w[2] - s * dir[2]];
    perp.iter().map(|c| c * c).sum::<f64>() <= 1e-12
}

/// Every rapid of the kernel's chain (spindle frame) proven clear of
/// the stock it stood off: a retract up the line its own cut just
/// drilled is the kerf; a rapid along its pose axis (the approach to a
/// feed-entry point) stays outside the raw stock when there is one;
/// a swing above the stock's Z top (plus the tool's rim) is the end-
/// face detour; every other rapid keeps the tool's rim (`d / 2`)
/// outside `st.r`. Anything else is a rapid through material, refused
/// by name.
pub(super) fn rapids_clear(t: &Toolpath, st: &Standoff, d: f64) -> Result<(), GenError> {
    let tol = 1e-6;
    let rim = 0.5 * d;
    let mut kerf: Option<(Point3, [f64; 3])> = None;
    for (i, m) in t.moves.iter().enumerate() {
        let axis = m.axis.unwrap_or([0.0, 0.0, 1.0]);
        if m.kind != MoveKind::Rapid {
            kerf = Some((m.to, axis));
            continue;
        }
        if kerf.is_some_and(|(p, a)| on_line(m.from, p, a) && on_line(m.to, p, a)) {
            continue;
        }
        kerf = None;
        let along_pose = on_line(m.to, m.from, axis);
        let (need, what) = if along_pose {
            (st.raw.unwrap_or(0.0), "radius")
        } else if st.z_max.is_some_and(|z| m.from.z.min(m.to.z) >= z + rim - tol) {
            continue;
        } else {
            (st.r + rim, "radius plus the tool's rim")
        };
        let got = approach(m.from, m.to);
        if got < need - tol {
            return Err(GenError::Invalid(format!(
                "cross-drill rapid {i} passes {got:.3} mm from the spindle, inside the stock's \
                 {what} ({need:.3} mm) - a rapid through material; raise the clearance or \
                 retract height, or turn the bar to size first"
            )));
        }
    }
    Ok(())
}
