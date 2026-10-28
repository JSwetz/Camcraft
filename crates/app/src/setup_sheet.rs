//! Setup sheets: printable setup documentation generated from CAM data
//! (M3.2 stage D).
//!
//! Provenance: the setup-sheet convention - one page per setup carrying
//! the stock, the work offsets, the fixtures, the program-ordered
//! operation table with its resolved depths, and the tool list with its
//! times - is standard machine-shop practice, and the sheet's shape was
//! specced from the SolveCraft setup-sheet reference images cited in
//! ROADMAP.md's M3.2 section. BEHAVIOR ONLY: no vendor text, template,
//! or layout file was copied; every string, column and alignment here is
//! our own.
//!
//! ## The laws here
//!
//! - **Determinism is the law.** The same document renders byte-identical
//!   text, every call. Nothing is iterated in entity order (everything
//!   comes from name-sorted queries or [`crate::setup::operation_order`]),
//!   every float goes through [`mm`], and there is deliberately **no
//!   timestamp** - a printed date would break the law, so the sheet says
//!   "resolved at render time" instead.
//! - **Resolved numbers, expressed references.** The depth columns print
//!   world Zs from [`crate::operation::resolve_height`] - the exact
//!   machinery the generation stamp digests through, so the sheet and a
//!   regenerate can never disagree. A height that will not resolve
//!   renders `-` plus an annotated reason line (via the public
//!   `crate::operation::missing_source`), never a guessed number (Prime
//!   Directive: visibly refused, never silently wrong).
//! - **Membership by the setup law.** The table lists exactly the
//!   operations [`crate::setup::effective`] resolves into this setup -
//!   including implicit members whose declared setup is `None` - in the
//!   one program order the post walks.
//! - **The sim is not free.** [`simulate_verdict`] is the only path that
//!   runs the dexel program simulation (a grid over the stock, not
//!   cheap); [`render`] never calls it.
//! - **No panics.** Every read is an `Option`/`Result` from the public
//!   model/setup/operation APIs; a half-built document degrades to
//!   "(none)" lines, and only a declared setup name that does not exist
//!   is an error.

use flecs_ecs::prelude::World;

use crate::cl::CycleKind;
use crate::drilling::{self, CycleChoice};
use crate::fixture;
use crate::model;
use crate::operation::{self, Height, OpParams, SrcRef};
use crate::schema::WCS_SLOTS;
use crate::setup;
use crate::stock::StockShape;

/// Format one millimeter number for the sheet: fixed three decimals, no
/// trailing-zero trimming (fixed width keeps the columns honest, and
/// trimming would make alignment depend on values - determinism is the
/// law). A non-finite value prints `n/a`: the resolve machinery refuses
/// NaN/inf heights upstream, so if one ever reached the sheet the `n/a`
/// is the visible refusal, never a silent `NaN`.
#[must_use]
pub fn mm(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.3}")
    } else {
        "n/a".to_string()
    }
}

/// Format one time (minutes) for the sheet's totals: two decimals. Same
/// non-finite refusal as [`mm`].
#[must_use]
pub fn minutes(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.2}")
    } else {
        "n/a".to_string()
    }
}

/// The plain-text setup sheet for one setup context.
///
/// `setup` is a DECLARED setup: `None` means "the first setup by name",
/// which in a document without setups is the legacy implicit document
/// (the implicit-setup law) and titles the sheet `Implicit`. The only
/// error this returns is a declared name that wears no setup - a
/// half-built document renders with "(none)" sections, never an error.
pub fn render(world: &World, setup_name: Option<&str>) -> Result<String, String> {
    if let Some(s) = setup_name
        && !setup::exists(world, s)
    {
        return Err(format!("no setup named '{s}' in this document"));
    }
    // The resolved context every lookup below stands in for. `None`
    // stays `None` - the setup resolvers read that as the first setup
    // (or legacy globals), exactly what the sheet must print.
    let target = setup::effective(world, setup_name);
    let ctx = target.as_deref();
    let title = target.clone().unwrap_or_else(|| "Implicit".to_string());

    let mut out = String::new();
    out.push_str(&format!("SETUP SHEET - {title}\n"));
    out.push_str("LAW: every number is resolved from the live document at render\n");
    out.push_str("     time (no timestamps - the same document renders byte-identical).\n");
    out.push_str("     Operations follow the program law: setup rank, then name.\n");
    out.push_str("PAGE 1 OF 1 (single-page sheet)\n");
    out.push('\n');

    render_stock(world, ctx, &mut out);
    render_wcs(world, ctx, &mut out);
    render_datum(world, ctx, &mut out);
    render_fixtures(world, ctx, &mut out);
    render_operations(world, target.as_ref(), &mut out);
    render_tool_list(world, ctx, &mut out);

    out.push('\n');
    out.push_str("END OF SHEET\n");
    Ok(out)
}

/// The flip chart: one entry per named setup in program order, or the
/// single `Implicit` entry of a setup-less document. Entries are
/// `(title, sheet)`; a setup whose sheet cannot render (it exists by
/// construction here, so it can only fail defensively) is skipped, never
/// panicked on.
#[must_use]
pub fn render_all(world: &World) -> Vec<(String, String)> {
    let names = setup::names(world);
    if names.is_empty() {
        return vec![(
            "Implicit".to_string(),
            render(world, None).unwrap_or_else(|e| format!("(render failed: {e})")),
        )];
    }
    names
        .into_iter()
        .filter_map(|n| render(world, Some(&n)).ok().map(|sheet| (n, sheet)))
        .collect()
}

/// The "program simulation" verdict line a sheet builder may opt into.
/// NOT called by [`render`]: the dexel grid over the stock is real
/// work, and a sheet builder should opt in explicitly. Deterministic
/// like everything else here - the sim reads only the document.
#[must_use]
pub fn simulate_verdict(world: &World) -> String {
    match crate::stock_sim::simulate_document(world) {
        Ok(sim) if sim.dev.is_clean() => "SIM: clean - within deviation tolerance".to_string(),
        Ok(sim) => format!(
            "SIM: {} rest cell(s) (max {} mm), {} gouge cell(s) (max {} mm)",
            sim.dev.rest_cells,
            mm(sim.dev.max_rest),
            sim.dev.gouge_cells,
            mm(sim.dev.max_gouge),
        ),
        Err(e) => format!("SIM: not judged - {e}"),
    }
}

// ------------------------------------------------------------------ sections

fn render_stock(world: &World, ctx: Option<&str>, out: &mut String) {
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

fn render_wcs(world: &World, ctx: Option<&str>, out: &mut String) {
    out.push_str("WCS OFFSETS\n");
    let mut any = false;
    for (slot, word) in WCS_SLOTS.iter().enumerate() {
        if let Some(entity) = setup::wcs_entity(world, ctx, slot) {
            any = true;
            let pos = model::transform_of(world, &entity)
                .unwrap_or_default()
                .pos;
            let entity_name = model::name_of(world, &entity).unwrap_or_default();
            out.push_str(&format!(
                "  {word}  X {}  Y {}  Z {}   (entity {entity_name})\n",
                mm(pos.x),
                mm(pos.y),
                mm(pos.z)
            ));
        }
    }
    if !any {
        out.push_str("  (none placed)\n");
    }
    out.push('\n');
}

fn render_datum(world: &World, ctx: Option<&str>, out: &mut String) {
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

fn render_fixtures(world: &World, ctx: Option<&str>, out: &mut String) {
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
fn fixture_lines(world: &World, setup: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = fixture::tagged(world, setup)
        .into_iter()
        .filter_map(|(name, entity)| {
            let loc = model::transform_of(world, &entity)?;
            let box_ = model::entity_world_aabb(world, &entity)?;
            Some(format!(
                "{name}  at X {}  Y {}  Z {}  footprint {} x {}",
                mm(loc.pos.x),
                mm(loc.pos.y),
                mm(loc.pos.z),
                mm(box_.max.x - box_.min.x),
                mm(box_.max.y - box_.min.y),
            ))
        })
        .collect();
    if let Some(t) = fixture::table_box(world) {
        out.push(format!(
            "{}  top Z {}  span {} x {}",
            fixture::MACHINE_TABLE,
            mm(t.max.z),
            mm(t.max.x - t.min.x),
            mm(t.max.y - t.min.y),
        ));
    }
    out
}

fn render_operations(world: &World, target: Option<&String>, out: &mut String) {
    out.push_str("OPERATIONS (program order)\n");
    out.push_str(&format!(
        "  {:>2}  {:<12} {:<10} {:<14} {:<12} {:>8} {:>8} {:>8} {:>8}\n",
        "#", "NAME", "TYPE", "TOOL", "SOURCE", "CLR", "RET", "TOP", "BOT"
    ));
    let mut count = 0usize;
    for (name, entity) in setup::operation_order(world) {
        let Some(op) = model::operation_of(world, &entity) else {
            continue;
        };
        // Membership by the resolution law, so implicit members (declared
        // `None`) ride in exactly where the program puts them.
        let in_this = match (setup::effective(world, op.setup.as_deref()), target) {
            (Some(s), Some(t)) => s == *t,
            (None, None) => true,
            _ => false,
        };
        if !in_this {
            continue;
        }
        count += 1;
        let src = src_label(&op.src);
        let cols = height_fields(&op.params);
        let mut cells: [String; 4] = std::array::from_fn(|_| String::new());
        let mut notes: Vec<String> = Vec::new();
        for (i, (col, field, expr)) in cols.iter().enumerate() {
            match expr {
                Some(h) => match operation::resolve_height(world, op.setup.as_deref(), *h) {
                    Some(z) => cells[i] = mm(z),
                    None => {
                        cells[i] = "-".to_string();
                        let why = operation::missing_source(
                            world,
                            op.setup.as_deref(),
                            h.reference,
                        )
                        .unwrap_or("the height does not resolve");
                        notes.push(format!(
                            "      ! {col} ({field}): From {} {} - {why}",
                            h.reference,
                            signed(h.offset)
                        ));
                    }
                },
                None => cells[i] = "-".to_string(),
            }
        }
        out.push_str(&format!(
            "  {count:>2}  {:<12} {:<10} {:<14} {:<12} {:>8} {:>8} {:>8} {:>8}\n",
            name,
            op.params.kind(),
            op.tool,
            src,
            cells[0],
            cells[1],
            cells[2],
            cells[3],
        ));
        for n in notes {
            out.push_str(&n);
            out.push('\n');
        }
        render_cycles(world, op.setup.as_deref(), &op.tool, &op.params, out);
    }
    if count == 0 {
        out.push_str("  (no operations in this setup)\n");
    }
    out.push('\n');
}

/// One line per hole of a Drill operation: the cycle kind, the XY, and
/// the bottom that hole machines to (the params override when set, the
/// hole's own floor otherwise - the same rule `drilling` cuts by).
/// Silent for every other operation kind - the Drill arm lives in the
/// params, not at the call site.
fn render_cycles(
    world: &World,
    op_setup: Option<&str>,
    tool_name: &str,
    params: &OpParams,
    out: &mut String,
) {
    let OpParams::Drill {
        top_z,
        bottom_z,
        cycle,
        holes,
        ..
    } = params
    else {
        return;
    };
    if holes.is_empty() {
        out.push_str("      (no holes)\n");
        return;
    }
    let r_z = operation::resolve_height(world, op_setup, *top_z);
    let override_z = (*bottom_z).and_then(|b| operation::resolve_height(world, op_setup, b));
    let tool = model::find_by_name(world, tool_name).and_then(|e| model::tool_of(world, &e));
    for (i, h) in holes.iter().enumerate() {
        let bottom = override_z.unwrap_or(h.floor_z);
        let kind = match cycle {
            CycleChoice::Auto => match (&tool, r_z) {
                (Some(t), Some(r)) if (r - bottom).is_finite() && t.diameter.is_finite() => {
                    kind_label(drilling::auto_kind(t.kind, r - bottom, t.diameter))
                }
                _ => "auto".to_string(),
            },
            other => choice_label(*other).to_lowercase(),
        };
        let thru = if h.through { "  thru" } else { "" };
        out.push_str(&format!(
            "      hole {:>2}  {:<10} X {}  Y {}  bottom {}{thru}\n",
            i + 1,
            kind,
            mm(h.xy.x),
            mm(h.xy.y),
            mm(bottom),
        ));
    }
}

fn render_tool_list(world: &World, ctx: Option<&str>, out: &mut String) {
    out.push_str("TOOL LIST\n");
    out.push_str(&format!(
        "  {:<14} {:<30} {:>10} {:>10} {:>7} {:>5}\n",
        "TOOL", "OPERATIONS", "CUT mm", "RAPID mm", "MIN", "CHG"
    ));
    let rows = crate::tool_list::for_setup(world, ctx);
    if rows.is_empty() {
        out.push_str("  (no tools in this setup)\n\n");
        return;
    }
    let mut cut = 0.0f64;
    let mut rapid = 0.0f64;
    for row in &rows {
        cut += row.cut_mm;
        rapid += row.rapid_mm;
        out.push_str(&format!(
            "  {:<14} {:<30} {:>10} {:>10} {:>7} {:>5}\n",
            row.tool,
            row.ops.join(", "),
            mm(row.cut_mm),
            mm(row.rapid_mm),
            minutes(row.minutes),
            row.changes,
        ));
        match model::find_by_name(world, &row.tool).and_then(|e| model::tool_of(world, &e)) {
            Some(t) => {
                let mut geo = format!(
                    "      D {}  {}  FLUTE {}",
                    mm(t.diameter),
                    t.kind,
                    mm(t.flute_len)
                );
                if t.tip_angle != 0.0 {
                    geo.push_str(&format!("  TIP {}", mm(t.tip_angle)));
                }
                if t.pitch != 0.0 {
                    geo.push_str(&format!("  PITCH {}", mm(t.pitch)));
                }
                out.push_str(&geo);
                out.push('\n');
            }
            None => out.push_str(&format!("      (tool '{}' is no longer in the document)\n", row.tool)),
        }
    }
    // The total time speaks with tool_list's own voice (setup_minutes:
    // the sum of the rows, tool-change allowance included), so the
    // footer and the stage-C report can never disagree.
    out.push_str(&format!(
        "  TOTAL: {} tool(s), cut {} mm, rapid {} mm, {}\n\n",
        rows.len(),
        mm(cut),
        mm(rapid),
        minutes(crate::tool_list::setup_minutes(world, ctx)),
    ));
}

// ------------------------------------------------------------------- helpers

/// The operation's geometry source, labeled.
fn src_label(src: &SrcRef) -> String {
    match src {
        SrcRef::Stock => "stock".to_string(),
        SrcRef::Entity(name) => format!("model '{name}'"),
        SrcRef::Selection(items) => format!("selection x{}", items.len()),
    }
}

/// The four depth columns as `(column, field name, height expression)`,
/// in table order. The expression is what `resolve_params`/`stamp_with`
/// feed the same world questions - this only lays them out per kind, so
/// the sheet reads the machinery, never a copy of its rules.
fn height_fields(params: &OpParams) -> [(&'static str, &'static str, Option<Height>); 4] {
    match params {
        OpParams::Face {
            clearance_z,
            cut_z,
            top_z,
            ..
        } => [
            ("CLR", "Clearance Z", Some(*clearance_z)),
            ("RET", "Retract Z", None),
            ("TOP", "Top Z", *top_z),
            ("BOT", "Cut Z", Some(*cut_z)),
        ],
        OpParams::Contour {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        } => [
            ("CLR", "Clearance Z", Some(*clearance_z)),
            ("RET", "Retract Z", Some(*retract_z)),
            ("TOP", "Top of Part Z", Some(*top_z)),
            ("BOT", "Bottom of Part Z", Some(*bottom_z)),
        ],
        OpParams::Pocket {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::Slot {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::Trace {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::Pocket3D {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::Finish3D {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        // The 5-axis twins ride the same four rows as the finish -
        // the optional hop included (M5.1 stage C).
        | OpParams::Contour5D {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::Parallel5D {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        // The swarf family's rows are the same four (M5.1 stage D).
        | OpParams::Swarf {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::AdvancedSwarf {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        // The rotary twins' too - the world planes the generator
        // reads into axial stations (M5.1 stage E).
        | OpParams::RotaryContour {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::RotaryParallel {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        // The stamped ladder states the pocket family's four rows,
        // the optional hop among them (M5.1 stage E3), and so does
        // flow (M5.1 I).
        | OpParams::MultiAxisRough {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::Flow5D {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        // The drive-curve contour states the same four rows; its
        // TOP/BOT are the window the picked chain must stand in, and
        // the Geometry column already spells the chain by its picks
        // (M5.1 J).
        | OpParams::MultiAxisContour {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        } => [
            ("CLR", "Clearance Z", Some(*clearance_z)),
            ("RET", "Retract Z", *retract_z),
            ("TOP", "Top of Part Z", Some(*top_z)),
            ("BOT", "Bottom of Part Z", Some(*bottom_z)),
        ],
        // The rough ladder states the sheet with no RET row - the
        // kernel's hop plane is a law, not a height (M5.1 stage E2).
        OpParams::RotaryOffset {
            clearance_z,
            top_z,
            bottom_z,
            ..
        } => [
            ("CLR", "Clearance Z", Some(*clearance_z)),
            ("RET", "Retract Z", None),
            ("TOP", "Top of Part Z", Some(*top_z)),
            ("BOT", "Bottom of Part Z", Some(*bottom_z)),
        ],
        // The facing ladder states the sheet the wrap's way - three
        // rows, no RET: the station rapids are one infeed off the
        // profile, a law and not a height. Its bottom names itself
        // for the axis: every station steps TOWARD it (M5.2 A).
        OpParams::Facing {
            clearance_z,
            top_z,
            bottom_z,
            ..
        } => [
            ("CLR", "Clearance Z", Some(*clearance_z)),
            ("RET", "Retract Z", None),
            ("TOP", "Top of Part Z", Some(*top_z)),
            ("BOT", "Facing Plane Z", Some(*bottom_z)),
        ],
        // The profile kinds state the same three rows with the plain
        // names: their bottom is the low end of the station window,
        // not a plane the ladder faces toward (M5.2 B).
        OpParams::ProfileRough {
            clearance_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::ProfileFinish {
            clearance_z,
            top_z,
            bottom_z,
            ..
        }
        // The blade windows state the same four rows (M5.2 C)...
        | OpParams::GrooveRough {
            clearance_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::GrooveFinish {
            clearance_z,
            top_z,
            bottom_z,
            ..
        }
        // Threading states the same four rows - the window of turns
        // by its plain names (M5.2 D).
        | OpParams::Threading {
            clearance_z,
            top_z,
            bottom_z,
            ..
        } => [
            ("CLR", "Clearance Z", Some(*clearance_z)),
            ("RET", "Retract Z", None),
            ("TOP", "Top of Part Z", Some(*top_z)),
            ("BOT", "Bottom of Part Z", Some(*bottom_z)),
        ],
        // ...and parting's bottom names the plane the blade severs at,
        // facing's naming law read as a cutoff (M5.2 C).
        OpParams::Part {
            clearance_z,
            top_z,
            bottom_z,
            ..
        } => [
            ("CLR", "Clearance Z", Some(*clearance_z)),
            ("RET", "Retract Z", None),
            ("TOP", "Top of Part Z", Some(*top_z)),
            ("BOT", "Parting Plane Z", Some(*bottom_z)),
        ],
        OpParams::Drill {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        } => [
            ("CLR", "Clearance Z", Some(*clearance_z)),
            ("RET", "Retract Z", *retract_z),
            ("TOP", "R Plane (Top of Part Z)", Some(*top_z)),
            ("BOT", "Bottom Z", *bottom_z),
        ],
    }
}

/// A height offset with its sign printed (the reference Display words the
/// source; the signed number words the push).
fn signed(v: f64) -> String {
    if v >= 0.0 {
        format!("+{}", mm(v))
    } else {
        mm(v)
    }
}

fn kind_label(k: CycleKind) -> String {
    let s = match k {
        CycleKind::Spot => "spot",
        CycleKind::Drill => "drill",
        CycleKind::Peck => "peck",
        CycleKind::Deep => "deep-peck",
        CycleKind::Tap => "tap",
        CycleKind::Bore => "bore",
        CycleKind::BackBore => "back-bore",
    };
    s.to_string()
}

fn choice_label(c: CycleChoice) -> &'static str {
    match c {
        CycleChoice::Auto => "Auto",
        CycleChoice::Spot => "Spot",
        CycleChoice::Drill => "Drill",
        CycleChoice::Peck => "Peck",
        CycleChoice::Tap => "Tap",
        CycleChoice::Bore => "Bore",
        CycleChoice::BackBore => "BackBore",
    }
}
