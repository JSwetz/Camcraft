//! Workholding in a carried setup (#321): a setup entered with the body
//! the earlier setups machined (#317) holds THAT body, not its declared
//! block - so its fixtures are read against the carried stock. A fixture
//! standing inside the material the program really carries in (a vise
//! jaw modelled into a wall setup 1 left standing, a clamp on a face that
//! is still there) is workholding that cannot be where it is modelled:
//! every fixture verdict of that setup judged steel the machine will not
//! hold there, and the post is refused by name.
//!
//! Provenance: our own design per #308's research decision (the carried
//! body is the incoming stock, read through each setup's transform);
//! plain re-clamp practice from machine-tool handbooks. No vendor
//! material consulted.
//!
//! ## The laws here
//!
//! - **The carried body, not the declared block.** Only the body the
//!   setup is ENTERED with is read (`rest::entered_body`, the carried-body
//!   cache's entry): a clamp seated in a pocket setup 1 cut, inside the
//!   declared block but clear of the real material, is no conflict.
//! - **Penetration, not contact.** A column counts when its centre lies
//!   strictly inside the fixture's footprint and its material
//!   (`floor..top`) overlaps the fixture's Z span by more than
//!   [`FIXTURE_HIT_TOL`] - jaws gripping a side face and a part seated on
//!   a plate are contact by design. The column sampling resolves to the
//!   carried grid's pitch: an overlap narrower than half a cell beside a
//!   face can pass, and is stated so in the decision record.
//! - **Live, like every workholding law.** Read at the post gate against
//!   the steel as it stands now, never baked into a stamp (`fixture`'s
//!   live-gate law). The machine table is not read: the table is what the
//!   setup's stock stands on, not steel placed against it.
//! - **Unreadable is not clear.** A carry the gate cannot read leaves the
//!   ops of that setup refused by their own generation (#318/#319); this
//!   clause adds nothing for it, it never reads the declared block instead.

use flecs_ecs::prelude::World;

use crate::fixture::{FIXTURE_HIT_TOL, FixtureBox};
use crate::model;
use crate::stock_sim::Body;

/// The phrase every carried-workholding refusal carries.
pub const FIXTURE_IN_CARRIED: &str = "stands inside the stock carried from";

/// How deep `b` stands in `body`'s material: the largest Z overlap of a
/// column whose centre lies strictly inside the box's footprint, or
/// `None` when no column overlaps beyond [`FIXTURE_HIT_TOL`].
#[must_use]
pub fn penetration(body: &Body, b: &FixtureBox) -> Option<f64> {
    let dex = &body.dex;
    let (bx0, bx1) = (b.min.x.min(b.max.x), b.max.x.max(b.min.x));
    let (by0, by1) = (b.min.y.min(b.max.y), b.max.y.max(b.min.y));
    let (bz0, bz1) = (b.min.z.min(b.max.z), b.max.z.max(b.min.z));
    if !(bx0.is_finite() && bx1.is_finite() && by0.is_finite() && by1.is_finite() && bz0.is_finite() && bz1.is_finite())
        || !(dex.cell.is_finite() && dex.cell > 0.0)
    {
        return None;
    }
    // The cell index span the footprint covers, clamped to the grid
    // (float -> index through a clamp, never an unchecked cast).
    let span = |lo: f64, hi: f64, o: f64, n: usize| -> (usize, usize) {
        let top = n as f64;
        let i0 = ((lo - o) / dex.cell - 1.0).floor().clamp(0.0, top);
        let i1 = ((hi - o) / dex.cell + 1.0).ceil().clamp(0.0, top);
        (i0 as usize, i1 as usize)
    };
    let (ix0, ix1) = span(bx0, bx1, dex.x0, dex.nx);
    let (iy0, iy1) = span(by0, by1, dex.y0, dex.ny);
    let mut worst: Option<f64> = None;
    for iy in iy0..iy1 {
        for ix in ix0..ix1 {
            let c = dex.center(ix, iy);
            let inside = c.x > bx0 + FIXTURE_HIT_TOL
                && c.x < bx1 - FIXTURE_HIT_TOL
                && c.y > by0 + FIXTURE_HIT_TOL
                && c.y < by1 - FIXTURE_HIT_TOL;
            let k = ix + iy * dex.nx;
            if !inside || body.empty_at(k) {
                continue;
            }
            let depth = dex.top_at(k).min(bz1) - body.floor_at(k).max(bz0);
            if depth.is_finite() && depth > FIXTURE_HIT_TOL {
                worst = Some(worst.map_or(depth, |w: f64| w.max(depth)));
            }
        }
    }
    worst
}

/// The gate's carried-workholding clause: per setup entered with a
/// carried body, one problem per tagged fixture standing in it. The
/// setup's first op in program order reads the chain (every op of a
/// setup is entered with the same body).
#[must_use]
pub fn carried_fixture_problems(world: &World) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen: Vec<Option<String>> = Vec::new();
    for (name, e) in crate::setup::operation_order(world) {
        let Some(op) = model::operation_of(world, &e) else { continue };
        let setup = crate::setup::effective(world, op.setup.as_deref());
        if seen.contains(&setup) {
            continue;
        }
        seen.push(setup.clone());
        let fixtures = crate::fixture::tagged(world, setup.as_deref());
        if fixtures.is_empty() {
            continue;
        }
        let tool_r = model::find_by_name(world, &op.tool)
            .and_then(|t| model::tool_of(world, &t))
            .map_or(0.0, |t| t.diameter * 0.5);
        // A refused carry is refused by every op's own generation.
        let Ok(Some((origin, body))) = crate::rest::entered_body(world, &name, setup.as_deref(), tool_r) else {
            continue;
        };
        let to = setup.as_deref().unwrap_or_default();
        for (fname, fe) in fixtures {
            let Some(b) = model::entity_world_aabb(world, &fe) else { continue };
            if let Some(d) = penetration(&body, &FixtureBox { min: b.min, max: b.max }) {
                out.push(format!(
                    "setup '{to}': fixture '{fname}' {FIXTURE_IN_CARRIED} setup '{origin}' ({d:.2} mm deep) - \
                     the part that setup leaves cannot be held as modelled; move the fixture or re-check the hold"
                ));
            }
        }
    }
    out
}
