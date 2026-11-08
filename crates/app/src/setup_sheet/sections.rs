//! Setup-sheet sections: stock, WCS offsets, datum, fixtures (M3.2 stage
//! D). Provenance: our own text; machine-shop setup-sheet convention,
//! behavior only - see the module doc in `setup_sheet/mod.rs`.

use flecs_ecs::prelude::World;

use super::mm;
use crate::fixture;
use crate::model;
use crate::schema::WCS_SLOTS;
use crate::setup;
use crate::stock::StockShape;

pub(super) fn render_stock(world: &World, ctx: Option<&str>, out: &mut String) {
    out.push_str("STOCK\n");
    let name = setup::stock_entity_name(world, ctx);
    let Some(entity) = setup::stock_entity(world, ctx) else {
        out.push_str(&format!("  {name}: (none)\n\n"));
        return;
    };
    out.push_str(&format!("  entity   {name}\n"));
    match model::stock_def_of(world, &entity) {
        Some(StockShape::Box { dx, dy, dz }) => {
            out.push_str(&format!(
                "  box      X {}  Y {}  Z {}\n",
                mm(dx),
                mm(dy),
                mm(dz)
            ));
        }
        Some(StockShape::Cylinder { diameter, height }) => {
            out.push_str(&format!(
                "  cylinder DIA {}  HEIGHT {}\n",
                mm(diameter),
                mm(height)
            ));
        }
        Some(StockShape::FromModel { source }) => {
            let facets = model::facets_of(world, &entity).map_or(0, |t| t.len());
            out.push_str(&format!(
                "  from model '{source}' - {facets} facet(s)\n"
            ));
            if let Some(b) = model::entity_world_aabb(world, &entity) {
                out.push_str(&format!(
                    "  world box  min ({}, {}, {})  max ({}, {}, {})\n",
                    mm(b.min.x),
                    mm(b.min.y),
                    mm(b.min.z),
                    mm(b.max.x),
                    mm(b.max.y),
                    mm(b.max.z),
                ));
            }
        }
        None => out.push_str("  (stock entity with no shape definition)\n"),
    }
    let loc = model::transform_of(world, &entity).unwrap_or_default();
    out.push_str(&format!(
        "  placed at  X {}  Y {}  Z {}\n\n",
        mm(loc.pos.x),
        mm(loc.pos.y),
        mm(loc.pos.z)
    ));
}

pub(super) fn render_wcs(world: &World, ctx: Option<&str>, out: &mut String) {
    out.push_str("WCS OFFSETS\n");
    let mut any = false;
    for (slot, word) in WCS_SLOTS.iter().enumerate() {
        if let Some(entity) = setup::wcs_entity(world, ctx, slot) {
            any = true;
            let loc = model::transform_of(world, &entity).unwrap_or_default();
            let pos = loc.pos;
            let entity_name = model::name_of(world, &entity).unwrap_or_default();
            out.push_str(&format!(
                "  {word}  X {}  Y {}  Z {}   (entity {entity_name})\n",
                mm(pos.x),
                mm(pos.y),
                mm(pos.z)
            ));
            // #327: the rotation the translation line cannot show.
            match crate::posting::offset_frame(&loc) {
                crate::posting::OffsetFrame::Square => {}
                crate::posting::OffsetFrame::Rotated { deg, axis } => out.push_str(&format!(
                    "       rotated {deg:.3} deg about {axis} - the post refuses a rotated offset\n"
                )),
                crate::posting::OffsetFrame::NonFinite => out.push_str(
                    "       placement is not finite - the post refuses this offset\n",
                ),
            }
        }
    }
    if !any {
        out.push_str("  (none placed)\n");
    }
    // #196: the word the program runs in - the setup's pick, or the
    // per-operation law when it picked none.
    match ctx.and_then(|s| setup::work_offset(world, s)) {
        Some(slot) => out.push_str(&format!(
            "  program offset: {} (picked for this setup)\n",
            WCS_SLOTS.get(slot).copied().unwrap_or("G54")
        )),
        None => out.push_str(
            "  program offset: per operation - its height references, else G54 (not picked)\n",
        ),
    }
    out.push('\n');
}

pub(super) fn render_datum(world: &World, ctx: Option<&str>, out: &mut String) {
    if let Some(from) = ctx.and_then(|s| setup::def_of(world, s))
        .map(|d| d.datum_from)
        .filter(|s| !s.is_empty())
    {
        out.push_str("DATUM\n");
        out.push_str(&format!(
            "  transferred from '{from}' - its stock and placed offsets were cloned\n\n"
        ));
    }
}

pub(super) fn render_fixtures(world: &World, ctx: Option<&str>, out: &mut String) {
    out.push_str("FIXTURES\n");
    let lines = fixture_lines(world, ctx);
    if lines.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for l in lines {
            out.push_str(&format!("  {l}\n"));
        }
    }
    out.push('\n');
}

/// One line per fixture of the setup context, name-sorted by
/// `fixture::tagged` (its own determinism law). A fixture is now a solid
/// the user loaded, so the sheet prints its placement and the footprint
/// its geometry occupies in the world (the transformed AABB) - there
/// are no defining parameters to name any more, and the machine reads
/// the sheet against real hardware it can measure. The derived machine
/// table, if the loaded `.mch` states one, is a final line.
///
/// Every line ends with the workholding law the verifier judged the
/// steel by (M13.8): [`FIXED_WORDING`] for a fixture declared fixed -
/// it stays in the machine frame while a rotary turns the part - and
/// [`TURNING_WORDING`] for the default that rides the rotary with it.
/// The machine table is always fixed: a table never turns. Printed so
/// the operator reads exactly the assumption the collision verdict
/// stood on, never a guess at it.
fn fixture_lines(world: &World, setup: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = fixture::tagged(world, setup)
        .into_iter()
        .filter_map(|(name, entity)| {
            let loc = model::transform_of(world, &entity)?;
            let box_ = model::entity_world_aabb(world, &entity)?;
            let fixed = fixture::def_of(world, &entity).is_some_and(|d| d.fixed);
            Some(format!(
                "{name}  at X {}  Y {}  Z {}  footprint {} x {}  {}",
                mm(loc.pos.x),
                mm(loc.pos.y),
                mm(loc.pos.z),
                mm(box_.max.x - box_.min.x),
                mm(box_.max.y - box_.min.y),
                frame_wording(fixed),
            ))
        })
        .collect();
    if let Some(t) = fixture::table_box(world) {
        out.push(format!(
            "{}  top Z {}  span {} x {}  {}",
            fixture::MACHINE_TABLE,
            mm(t.max.z),
            mm(t.max.x - t.min.x),
            mm(t.max.y - t.min.y),
            frame_wording(true),
        ));
    }
    out
}

/// The sheet's words for a fixture judged in the machine frame.
pub const FIXED_WORDING: &str = "fixed (machine frame)";

/// The sheet's words for a fixture judged in the part's turning frame.
pub const TURNING_WORDING: &str = "turns with the part";

/// The trailing column of a fixture line: which frame the verifier
/// judged the steel in, in the operator's words.
fn frame_wording(fixed: bool) -> &'static str {
    if fixed { FIXED_WORDING } else { TURNING_WORDING }
}
