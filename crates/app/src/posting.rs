//! In-app posting: the live world to NC text, through the CPS
//! architecture of [`crate::post`].
//!
//! Provenance: our own design; the three-file role split (script,
//! machine, intermediate) per `docs/CNC-INTERMEDIATE.md` and
//! ROADMAP M1.7, specced from Autodesk's published post-processor
//! documentation, behavior only, per the Clean-Room Protocol.
//!
//! The law here is the validation gate: a program is written only
//! when every operation is generated, fresh, and warning-free, AND
//! the program-level verification passes - no gouge in the material
//! sim, no limit violated on the mounted machine (M2.3 D; or nothing
//! is written at all - the Prime Directive's blocking rule). Context
//! resolution mirrors generation exactly - the same `resolve_params`,
//! the same stock box, the tool library's own rank for `T..`/`H..` -
//! so the posted numbers are the machined ones.

use crate::model;
use crate::operation::{self, Height, HeightReference, OpStatus, OpParams};
use crate::post::{self, cnc, engine, Dump};
use crate::schema::WCS_SLOTS;
use flecs_ecs::prelude::*;
use serde_json::Value;
use std::path::Path;

/// The post script product-shipped with the app: Fanuc, our first
/// `.cps` (embedded so an install never depends on a loose path).
pub const FANUC_CPS: &str = include_str!("../../../assets/posts/fanuc.cps");

/// The default machine: a plain 3-axis VMC with flood coolant.
pub fn default_machine() -> Value {
    static M: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    M.get_or_init(|| {
        serde_json::from_str(include_str!("../../../assets/machines/3axis-vmc.mch"))
            .unwrap_or(Value::Null)
    })
    .clone()
}

/// What a successful post says back.
#[derive(Debug)]
pub struct Posted {
    pub ops: usize,
    pub lines: usize,
    /// The exact program that was written - the M1.7d viewer shows the
    /// disk's content without re-reading it (one source of truth: text
    /// and positions come from the same engine run).
    pub program: String,
    /// One entry per program line: the world position of the move that
    /// line writes, or `None` for header, tool and section lines.
    pub line_pos: Vec<Option<[f64; 3]>>,
}

/// Post every operation of the document, in tree order, to one NC
/// file. Every refusal names its reason; nothing partial is ever
/// written. Cached generation warnings ride the stream as `WARN`
/// records, so the engine's own warning count is the gate.
pub fn post_setup(world: &World, out_path: &Path) -> Result<Posted, String> {
    let records = stream_records(world)?;
    // The verification gate (M2.3 D): the same two program-level
    // judgments the Deviate cell shows, made binding. Evidence is
    // computed here and now - never a cached verdict carried in from
    // the viewer (the PostView law: a verdict about an older world
    // must not release this one's metal).
    let problems = verification_gate(world);
    if !problems.is_empty() {
        return Err(format!("post blocked - {}", problems.join("; ")));
    }
    let stream = cnc::serialize(&records);
    let out = engine::run(FANUC_CPS, &default_machine(), &stream)
        .map_err(|e| format!("post engine: {e}"))?;
    if !out.warnings.is_empty() {
        return Err(format!(
            "post blocked - {} warning(s): {}",
            out.warnings.len(),
            out.warnings.join("; ")
        ));
    }
    std::fs::write(out_path, out.program.as_bytes()).map_err(|e| {
        let _ = std::fs::remove_file(out_path); // a half-written program is the silent wrong answer
        format!("cannot write {}: {e}", out_path.display())
    })?;
    let ops = records
        .iter()
        .filter(|r| matches!(r, cnc::Record::Section { begin: true }))
        .count();
    let lines = out.program.lines().count();
    let line_pos = viewer_map(&records, &out);
    Ok(Posted {
        ops,
        lines,
        program: out.program,
        line_pos,
    })
}

/// Program line -> world position, by the stream's own laws: the dump
/// emits exactly one Move record per path move in chain order
/// ([`post::dump`]), so the k-th Move record is the k-th global move -
/// the same counter that seats cycles' `first_move` stream-global
/// above. Move coordinates are pre-origin-shift world millimeters (the
/// script applies the origin, M1.7b's law) - the very numbers the
/// viewport draws, so a highlighted line and a highlighted move cannot
/// disagree.
fn viewer_map(records: &[cnc::Record], out: &engine::PostOutput) -> Vec<Option<[f64; 3]>> {
    let moves: Vec<[f64; 3]> = records
        .iter()
        .filter_map(|r| match r {
            cnc::Record::Move(m) => Some([m.x, m.y, m.z]),
            _ => None,
        })
        .collect();
    let mut line_pos = vec![None; out.program.lines().count()];
    for info in &out.line_map {
        let Some(rec) = info.record.filter(|i| *i < records.len()) else {
            continue; // header lines carry no record; a broken map must not panic
        };
        let pos = match &records[rec] {
            cnc::Record::Move(m) => Some([m.x, m.y, m.z]),
            cnc::Record::Cycle(c) => moves.get(c.first_move).copied(),
            _ => None,
        };
        if let Some(slot) = line_pos.get_mut(info.line) {
            *slot = pos;
        }
    }
    line_pos
}

/// The program in posting order: every operation entity, document
/// order, validated the way posting validates - all generated, all
/// fresh, or a refusal naming the op. `stream_records` walks on it;
/// the M2.3 program sim walks on it too, so *what the sim judges is
/// what would post* holds by construction, not by two copies of the
/// law agreeing (stage D makes the agreement load-bearing).
pub fn program_ops(world: &World) -> Result<Vec<(String, Entity)>, String> {
    let ops: Vec<(String, Entity)> = model::named_entities(world)
        .into_iter()
        .filter(|(_, e)| model::is_operation(world, e))
        .collect();
    if ops.is_empty() {
        return Err("no operations to post".into());
    }

    // Validate early: every operation must be generated and fresh
    // before a single record is built (a half-posted program is the
    // silent wrong answer this file exists to prevent).
    for (name, e) in &ops {
        match operation::status_of(world, e) {
            OpStatus::Never => {
                return Err(format!("{name}: no toolpath yet - generate it first"))
            }
            OpStatus::Stale => {
                return Err(format!(
                    "{name}: inputs moved since generation - regenerate before posting"
                ))
            }
            OpStatus::Ok => {}
        }
    }
    Ok(ops)
}

/// The program-level verification, made binding (M2.3 D): the two
/// judgments the Deviate cell informs about, read again here as the
/// release gate over exactly the op list `program_ops` just blessed -
/// the shared-gate law, so the verdict and the gate can never disagree
/// about WHICH program they judged. Every line already names its op;
/// a refusal that cannot say where is trivia, not a warning.
///
/// What blocks: a GOUGE (the program cuts below the model's final
/// geometry - the Prime Directive's "never", located); a stated machine
/// limit the program outruns. What does not: REST material (a roughing
/// program posts with its promise of a later finish op - Deviate shows
/// the shortfall, the gate respects the intent), and a sim that cannot
/// run at all (no stock, rotated stock - nothing was judged, so nothing
/// is claimed; the per-op WARN gate is still the net under it, and the
/// Deviate cell states the reason on demand).
#[must_use]
pub fn verification_gate(world: &World) -> Vec<String> {
    let mut problems = Vec::new();

    match crate::stock_sim::simulate_document(world) {
        Ok(sim) if sim.dev.gouge_cells > 0 => {
            let at = sim.dev.max_gouge_at.map_or_else(
                || "?".to_owned(),
                |q| format!(" at ({:.2}, {:.2}, z {:.2})", q.x, q.y, q.z),
            );
            problems.push(format!(
                "gouge: program cuts below the model in {} cell(s), worst {:.2} mm{}",
                sim.dev.gouge_cells, sim.dev.max_gouge, at
            ));
        }
        Ok(_) | Err(_) => {}
    }

    if let Some(m) = crate::machine::of(world).1 {
        match crate::machine::check_document(world, &m) {
            Ok(c) if !c.is_ok() => {
                let more = if c.more > 0 { format!(" (+{} more)", c.more) } else { String::new() };
                problems.push(format!(
                    "machine {}: {} limit problem(s): {}{more}",
                    m.model,
                    c.problems.len(),
                    c.problems.join("; ")
                ));
            }
            Ok(_) => {}
            // Unreachable in one thread - program_ops just walked the
            // same ops - but silence is the forbidden arm, not this one.
            Err(err) => problems.push(format!("machine: {err}")),
        }
    }
    problems
}

/// The stream we would feed the engine: every operation's records,
/// in document order, validated first. Split from the write so the
/// gate's evidence (WARN records, cycle anchors) is testable without
/// touching the filesystem or the script.
fn stream_records(world: &World) -> Result<Vec<cnc::Record>, String> {
    let ops = program_ops(world)?;

    // Tool library rank, the T../H.. law: 1-based position among the
    // named tools in document order.
    let tool_ranks: Vec<String> = model::named_entities(world)
        .into_iter()
        .filter(|(_, e)| model::is_tool(world, e))
        .map(|(n, _)| n)
        .collect();

    // The stock box is setup context, resolved once.
    let boxz = model::stock_world_aabb(world);
    let (stock, stock_top_z) = match &boxz {
        Some(a) => ([a.min.x, a.max.x, a.min.y, a.max.y], a.max.z),
        None => ([0.0; 4], 0.0),
    };

    let mut records: Vec<cnc::Record> = Vec::new();
    let mut moved = 0usize; // cycle AT anchors are stream-global
    for (name, e) in &ops {
        let op = model::operation_of(world, e)
            .ok_or_else(|| format!("{name}: lost its operation"))?;
        let cache = model::toolpath_of(world, e)
            .ok_or_else(|| format!("{name}: lost its toolpath"))?;
        let tool = model::find_by_name(world, &op.tool)
            .and_then(|t| model::tool_of(world, &t))
            .ok_or_else(|| format!("{name}: tool {} is not in the library", op.tool))?;
        let hs = operation::resolve_params(world, &op.params)
            .map_err(|err| format!("{name}: {err}"))?;
        let tool_number = tool_ranks
            .iter()
            .position(|n| *n == op.tool)
            .map_or(0, |i| i as u32 + 1);
        let (wcs, origin) = wcs_context(world, &op.params);
        let d = Dump {
            op_name: name,
            tool_number,
            tool_name: &op.tool,
            tool_diameter: tool.diameter,
            rpm: tool.rpm,
            clearance_z: hs.clearance,
            retract_z: hs.retract.unwrap_or(hs.clearance),
            top_z: stock_top_z, // the TopOfStock PARAM: the stock, not the cut
            wcs,
            origin,
            stock,
            stock_top_z,
            warnings: cache
                .warnings
                .iter()
                .map(|w| format!("{name}: {w}"))
                .collect(),
        };
        let mut recs = post::dump(&cache.path, &d);
        if moved > 0 {
            for r in &mut recs {
                if let cnc::Record::Cycle(c) = r {
                    c.first_move += moved;
                }
            }
        }
        moved += cache.path.moves.len();
        records.extend(recs);
    }
    Ok(records)
}

/// The work offset this operation posts to: the first WCS slot its
/// height grammar references, else the default `G54` - and that
/// slot's origin as the machine would measure it. `pub(crate)` so the
/// machine-limit travel check (M2.3 C) reads program coordinates by
/// exactly this law, not by a second copy of it.
pub(crate) fn wcs_context<'a>(world: &World, params: &'a OpParams) -> (&'a str, [f64; 3]) {
    let slot = params
        .heights()
        .into_iter()
        .find_map(|h| match h.reference {
            HeightReference::WorkOffset(i) if i < WCS_SLOTS.len() => Some(i),
            _ => None,
        })
        .unwrap_or(0);
    let name = WCS_SLOTS[slot];
    let origin = model::find_by_name(world, name)
        .and_then(|e| model::transform_of(world, &e))
        .map_or([0.0; 3], |t| [t.pos.x, t.pos.y, t.pos.z]);
    (name, origin)
}

/// Every height a parameter set references (the same walk
/// `resolve_params` makes, but of the references themselves).
pub trait HeightRefs {
    fn heights(&self) -> Vec<&Height>;
}

impl HeightRefs for OpParams {
    fn heights(&self) -> Vec<&Height> {
        match self {
            OpParams::Face {
                clearance_z,
                cut_z,
                top_z,
                ..
            } => {
                let mut v = vec![clearance_z, cut_z];
                v.extend(top_z.iter());
                v
            }
            OpParams::Contour {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            } => vec![clearance_z, retract_z, top_z, bottom_z],
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
            } => {
                let mut v = vec![clearance_z, top_z, bottom_z];
                v.extend(retract_z.iter());
                v
            }
            OpParams::Drill {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            } => {
                let mut v = vec![clearance_z, top_z];
                v.extend(retract_z.iter());
                v.extend(bottom_z.iter());
                v
            }
        }
    }
}
