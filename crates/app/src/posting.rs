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
use crate::schema::{PostRef, WCS_SLOTS};
use flecs_ecs::prelude::*;
use serde_json::Value;
use std::path::Path;

/// The post scripts that ship with the app (name, text), embedded so
/// an install never depends on a loose path. `PostRef::Builtin` means
/// the first entry (the default document's post, the meaning of a
/// project with no post key); a named built-in addresses an entry by
/// name, which survives registry growth the way an index never could.
/// Entry 0 is Fanuc and stays Fanuc - the goldens and every pre-stage-B
/// file say so.
pub const BUILTINS: &[(&str, &str)] = &[
    ("Fanuc", include_str!("../../../assets/posts/fanuc.cps")),
    ("Haas", include_str!("../../../assets/posts/haas.cps")),
    ("Siemens 840D", include_str!("../../../assets/posts/sinumerik.cps")),
    ("Heidenhain", include_str!("../../../assets/posts/heidenhain.cps")),
    ("Mazak", include_str!("../../../assets/posts/mazak.cps")),
];

/// The default machine as text: a plain 3-axis VMC with flood coolant.
pub const DEFAULT_MCH: &str = include_str!("../../../assets/machines/3axis-vmc.mch");

/// The built-in machine LIBRARY (M3.3 stage C), checker grammar - the
/// files the Setup pane's machine group lists by name, the way
/// `BUILTINS` lists posts. Same law as the post registry: addressed
/// by name (a Setup that names "Mill-turn" still names it after the
/// library grows), and entry 0 stays the plain VMC, because the
/// first-shipped default is what old sessions remember. Unlike posts,
/// a machine choice is not stored as a reference - the document
/// mounts the parsed machine itself - so the name here is a picker
/// label and an install-time default, never a file's identity.
pub const MACHINES: &[(&str, &str)] = &[
    ("VMC 3-axis", include_str!("../../../assets/machines/vmc.mch")),
    ("Tilt-head 5-axis", include_str!("../../../assets/machines/tilthead.mch")),
    ("Mill-turn", include_str!("../../../assets/machines/millturn.mch")),
    ("Lathe", include_str!("../../../assets/machines/lathe.mch")),
];

/// The mount path a built-in library entry records (stage C): the
/// pane's combo and the `LoadMachine::builtin` command say the same
/// string, so "loaded from" never quotes a file that does not exist.
#[must_use]
pub fn builtin_path(name: &str) -> String {
    format!("built-in: {name}")
}

/// The default machine: a plain 3-axis VMC with flood coolant.
pub fn default_machine() -> Value {
    static M: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    M.get_or_init(|| serde_json::from_str(DEFAULT_MCH).unwrap_or(Value::Null)).clone()
}

/// The machine this document posts to (M3.3 A): the mounted `.mch`
/// restated in the static post grammar, or the default VMC where the
/// document says no machine. The same fact the limit gate judges, now
/// also shaping the script's `machine` global and the sim's traverse
/// clock - post, gate and timeline cannot disagree about the machine.
#[must_use]
pub fn machine_value(world: &World) -> Value {
    match crate::machine::of(world).1 {
        Some(m) => post::machine::mirror(&m),
        None => default_machine(),
    }
}

/// The post this document runs, from the `PostRef` singleton. Setup
/// furniture moved through commands or a document load, never ad hoc
/// - the `machine::set` law wearing the post's own clothes.
#[must_use]
pub fn post_of(world: &World) -> PostRef {
    world.try_get::<&PostRef>(|p| p.clone()).unwrap_or_default()
}

/// Write the singleton. For the `SelectPost` command and the project
/// reader only.
pub fn post_set(world: &World, next: &PostRef) {
    world.set(next.clone());
}

/// The document's post for display: built-in name, or the file path
/// as chosen. Never touches disk - a pane may draw this every frame,
/// and it names exactly what `script_source` resolves. One truth,
/// read two ways.
#[must_use]
pub fn post_label(world: &World) -> String {
    match post_of(world) {
        PostRef::Builtin => match BUILTINS.first() {
            Some((name, _)) => format!("{name} (built-in)"),
            None => "built-in".to_owned(),
        },
        PostRef::Named(name) => match BUILTINS.iter().find(|(n, _)| *n == name) {
            Some((n, _)) => format!("{n} (built-in)"),
            // Unresolvable at display time: name the stranger. A
            // removed registry entry must show as itself, not as the
            // default it will never post as (script_source refuses).
            None => format!("{name} (built-in, missing)"),
        },
        PostRef::File(path) => path,
    }
}

/// The script source this document posts with: (label, text). A
/// `File` post whose text cannot be read, or a `Named` post missing
/// from the registry, is a refusal naming what is gone - never a
/// silent run with the default instead (a substituted post is exactly
/// the silent wrong answer this whole codebase exists to prevent; the
/// Prime Directive).
pub fn script_source(world: &World) -> Result<(String, String), String> {
    match post_of(world) {
        PostRef::Builtin => BUILTINS
            .first()
            .map(|(name, cps)| (format!("{name} (built-in)"), (*cps).to_owned()))
            .ok_or_else(|| "the built-in post registry is empty".to_owned()),
        PostRef::Named(name) => BUILTINS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(n, cps)| (format!("{n} (built-in)"), (*cps).to_owned()))
            .ok_or_else(|| format!("built-in post {name} is not in the registry")),
        PostRef::File(path) => {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("post script {path}: {e}"))?;
            Ok((path, text))
        }
    }
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
    /// Block-by-block trace of this very run (M3.3 stage D): which
    /// toolpath record wrote which program lines, in the engine's own
    /// words. The viewer's trace pane rides the same run that wrote
    /// the file - one source of truth, never a re-post.
    pub blocks: Vec<engine::TraceBlock>,
}

/// Post every operation of the document, in tree order, to one NC
/// file. Every refusal names its reason; nothing partial is ever
/// written. Cached generation warnings ride the stream as `WARN`
/// records, so the engine's own warning count is the gate.
pub fn post_setup(world: &World, out_path: &Path) -> Result<Posted, String> {
    let (_label, cps) = script_source(world)?;
    post_script(world, out_path, &cps)
}

/// The gate's post at a given script (M3.3 A): the stream and the
/// verification are the document's own; only the translation is
/// parameterized - what a caller holding a script text runs, and
/// what the tests use to exercise the script seam directly.
pub fn post_script(world: &World, out_path: &Path, cps: &str) -> Result<Posted, String> {
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
    let out = engine::run(cps, &machine_value(world), &stream)
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
        blocks: out.blocks,
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
    // THE order law by call (M3.2 A): setup rank, then name - the same
    // walk re-machining, the sim and the machine-limit check make.
    let ops: Vec<(String, Entity)> = crate::setup::operation_order(world);
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
/// limit the program outruns; and a PROBE ask the program's own cuts
/// cannot measure to (M5.3 D): the part is what the program's blades
/// cut together, so the ask is judged on that composition, at the
/// post - the law and the lines in `operation::probe_problems`. What
/// does not: REST material on a program that asked no probe (a
/// roughing program posts with its promise of a later finish op -
/// Deviate shows the shortfall, the gate respects the intent; an
/// authored ask is what turns that promise into a fact), and a sim
/// that cannot run at all (no stock, rotated stock, a revolved path's
/// own frame - nothing judged here is claimed).
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

    // The workholding clause (M3.2 B, live-gate law): the same fixture
    // bands and rapid laws generation reports early, re-run HERE against
    // the paths as cached and the steel as it stands now. A vise slid
    // under a programmed tool after generation - or a machine file that
    // just grew a table the paths were never checked against - blocks
    // the post without staling a single stamp (the M2.3 machine-limits
    // precedent: what the room is made of is verified now, never baked
    // into a digest).
    problems.extend(fixture_problems(world));

    // The in-process measurement ask (M5.3 D): a Part op's probe
    // budget is a claim about the FINISHED part, and the finished
    // part is the program's blades composed - so it is judged here,
    // against every blade leg's cached path, not at any single
    // operation's generation (a rough's left floor is a fact about
    // what came after it). The law and lines live in
    // `operation::probe_problems`.
    problems.extend(crate::operation::probe_problems(world));
    problems
}

/// The gate's fixture walk: one problem string per operation with a
/// live collision, naming the op, the move, and the struck element.
/// Ops that lost their tool or path mid-session are skipped here -
/// `stream_records` refuses those by name before this ever releases
/// metal; the gate's job is the steel, not the bookkeeping.
fn fixture_problems(world: &World) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(ops) = program_ops(world) else {
        return out;
    };
    let spindle = crate::machine::of(world)
        .1
        .and_then(|m| m.spindle_d.zip(m.spindle_len))
        .map(|(d, l)| (d * 0.5, l));
    for (name, e) in ops {
        let Some(op) = model::operation_of(world, &e) else {
            continue;
        };
        let Some(cache) = model::toolpath_of(world, &e) else {
            continue;
        };
        let Some(tool) = model::find_by_name(world, &op.tool)
            .and_then(|t| model::tool_of(world, &t))
        else {
            continue;
        };
        // `boxes_for` already folds in the machine-derived table, so an
        // empty list means no steel this op could strike at all (the
        // spindle band is tested against that list, never on its own).
        let fixtures = crate::fixture::boxes_for(world, op.setup.as_deref());
        if fixtures.is_empty() {
            continue;
        }
        let geom = tool.geom();
        let mut hits = crate::verify::bands_vs_fixtures(
            &cache.path,
            &geom,
            &fixtures,
            spindle,
            operation::GOUGE_TOL,
        );
        hits.extend(crate::verify::rapids_vs_fixtures(
            &cache.path,
            &geom,
            &fixtures,
        ));
        for hit in hits {
            out.push(format!("{name}: {hit}"));
        }
    }
    out
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
        let setup = op.setup.as_deref();
        let hs = operation::resolve_params(world, setup, &op.params)
            .map_err(|err| format!("{name}: {err}"))?;
        let tool_number = tool_ranks
            .iter()
            .position(|n| *n == op.tool)
            .map_or(0, |i| i as u32 + 1);
        let (wcs, origin) = wcs_context(world, setup, &op.params);
        // The stock box is THIS operation's setup context (M3.2 A): a
        // later setup's block is where and how tall its own stock says.
        let boxz = model::stock_world_aabb_for(world, setup);
        let (stock, stock_top_z) = match &boxz {
            Some(a) => ([a.min.x, a.max.x, a.min.y, a.max.y], a.max.z),
            None => ([0.0; 4], 0.0),
        };
        // The feeder signals ride the section (M5.2 G.2): only a
        // Part operation can carry them, and a load with an advance
        // of 0 is still a load - the ACT, not the number. The
        // handoff step joins the family (M5.3 C) - same extraction,
        // same law: a signal, no geometry, the gate says whether the
        // machine states the second spindle it names. The
        // measurement ask joins last (M5.3 D) with its own shape:
        // the NUMBER is the switch - an authored budget above 0
        // asks the probe, 0 is silence, so no second flag exists to
        // disagree with it.
        let (part_load, part_unload, part_transfer, probe_tol) = match &op.params {
            OpParams::Part {
                bar_load,
                bar_advance,
                bar_unload,
                bar_transfer,
                inspect_tol,
                ..
            } => (
                bar_load.then_some(*bar_advance),
                *bar_unload,
                *bar_transfer,
                (*inspect_tol > 0.0).then_some(*inspect_tol),
            ),
            _ => (None, false, false, None),
        };
        let d = Dump {
            op_name: name,
            part_load,
            part_unload,
            part_transfer,
            probe_tol,
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
///
/// The G-code WORD is always `WCS_SLOTS[slot]` - the machine's register
/// names do not take setup suffixes. The ENTITY the origin is measured
/// from follows the setup (M3.2 A): `Setup2/G54` is the same machine
/// offset word, re-placed for that setup's clamping.
pub(crate) fn wcs_context<'a>(
    world: &World,
    setup: Option<&str>,
    params: &'a OpParams,
) -> (&'a str, [f64; 3]) {
    let slot = params
        .heights()
        .into_iter()
        .find_map(|h| match h.reference {
            HeightReference::WorkOffset(i) if i < WCS_SLOTS.len() => Some(i),
            _ => None,
        })
        .unwrap_or(0);
    let name = WCS_SLOTS[slot];
    let origin = crate::setup::wcs_entity(world, setup, slot)
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
            // The twins reference the pocket's four heights (M2.4 A).
            | OpParams::Adaptive2D {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            | OpParams::Offset2D {
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
            // The engrave twin states trace's four verbatim, and the
            // circular ladder is the pocket family's shape - vault or
            // flat floors, both read top and bottom (M2.4 B).
            | OpParams::Engrave {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            | OpParams::Circular {
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
            // The 5-axis twins carry the finish's height shape one for
            // one (M5.1 stage C) - the optional hop included.
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
            // ...and the swarf family's (M5.1 stage D).
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
            // The rotary twins' shape is the same four world planes;
            // the generator reads them into axial stations itself
            // (M5.1 stage E), so the post sees the finish's law.
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
            // The stamped ladder's heights are the pocket family's
            // four, hop included (M5.1 stage E3), and flow states
            // the same four (M5.1 I).
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
            // The drive-curve contour states the same four (M5.1 J).
            | OpParams::MultiAxisContour {
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
            // The chamfer ride resolves three: its floor is DERIVED
            // from width and angle, so there is no bottom plane to
            // reference - the top plane is the only height the ride
            // reads (M2.4 B, variant docs).
            OpParams::Chamfer2D {
                clearance_z,
                retract_z,
                top_z,
                ..
            } => {
                let mut v = vec![clearance_z, top_z];
                v.extend(retract_z.iter());
                v
            }
            // The wrap ladder's three - no hop plane exists to ask
            // about; the kernel's rapids ride the skin, not a height
            // (M5.1 stage E2).
            OpParams::RotaryOffset {
                clearance_z,
                top_z,
                bottom_z,
                ..
            }
            // The facing ladder states the same three: the station
            // rapids ride the profile, not a hop height (M5.2 A) -
            // and so do the profile rungs, whose rapids clear the
            // ride's reach (M5.2 B).
            | OpParams::Facing { clearance_z, top_z, bottom_z, .. }
            | OpParams::ProfileRough { clearance_z, top_z, bottom_z, .. }
            | OpParams::ProfileFinish { clearance_z, top_z, bottom_z, .. }
            // The blade kinds carry the same three heights - the
            // parting plane is one of them, spelled `bottom_z`
            // (M5.2 C).
            | OpParams::GrooveRough { clearance_z, top_z, bottom_z, .. }
            | OpParams::GrooveFinish { clearance_z, top_z, bottom_z, .. }
            // Threading carries the same three - the rapids plane
            // and the window of turns (M5.2 D) - and so do the
            // contour rides: the plane and the face's two stations
            // (M5.2 E).
            | OpParams::Part { clearance_z, top_z, bottom_z, .. }
            | OpParams::Threading { clearance_z, top_z, bottom_z, .. }
            | OpParams::Chamfering { clearance_z, top_z, bottom_z, .. }
            | OpParams::TaperTurning { clearance_z, top_z, bottom_z, .. }
            | OpParams::MultiPassTurning { clearance_z, top_z, bottom_z, .. }
            // The crown gauge rides the same trio (M5.2 E2), and so
            // does the boring bar (M5.2 F).
            | OpParams::FormTurning { clearance_z, top_z, bottom_z, .. }
            | OpParams::BoringTurning { clearance_z, top_z, bottom_z, .. } =>
                vec![clearance_z, top_z, bottom_z],
            // Bore and Thread reference the drill's four (M2.4 C) -
            // the same optional-floor, optional-hop shape.
            OpParams::Drill {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            | OpParams::Bore {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                ..
            }
            | OpParams::Thread {
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
