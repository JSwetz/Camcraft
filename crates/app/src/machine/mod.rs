//! Machine definitions and program limits (M2.3 stage C).
//!
//! Provenance: the `.mch` file layout is the machine-definition JSON
//! format reused by user instruction (our reader, our field semantics,
//! nothing copied from any vendor's parser or documentation beyond the
//! observable shape of a machine file); the limit-checking rules are
//! general machine-tool practice - a spindle has a speed range, an
//! axis has a feed ceiling and a stroke, and a tool table has a
//! length and diameter the magazine accepts.
//!
//! The design law: an UNSTATED limit checks nothing. Vendor machine
//! files carry zeros for speeds nobody configured, and a zero rapid
//! speed does not mean the machine cannot rapids - it means the file
//! doesn't say. Every number here is therefore an `Option`, and `0`
//! reads as `None`. A program judged against a sparse machine says
//! exactly what the machine file says, no more - the silent wrong
//! answer in its friendliest form would be a green checkmark earned
//! by inventing a ceiling.
//!
//! Travel is the same law with a sharper edge: standard machine files
//! carry no stroke numbers at all, so the check is live only when our
//! own files add a `"travel": {"min": .., "max": ..}` object to a
//! linear part. Those numbers are read in PROGRAM coordinates through
//! the active WCS - the file cannot know a work offset, and guessing a
//! machine origin would invent the very number the check needs. So a
//! stroke L is stated `-L..L` (the origin may sit anywhere along it);
//! a machine-frame `-500..0` refused every retract (#457).
//!
//! M3.3 stage C widened the readable grammar, all of it optional -
//! files written before stage C parse exactly as they always did:
//! `general.kind` declares what the machine IS (mill / lathe /
//! mill-turn; unstated means the axis inventory decides, stage E's
//! older law); `spindle.power` (kW), `tooling.number_of_tools` (the
//! changer's slots), and `tool_station.coolants` (names, with their
//! M-codes where the file states them) carry the full numbers; and
//! `"type": "rotary"` parts join the axis table with degrees for
//! travel and a `"resolution"` key for positioning steps. The mirror
//! to the post grammar (posting) now speaks coolants too, so a
//! mounted machine's coolant words are its own. M5.3 stage C added
//! the spindle INVENTORY - heads stating a spindle, counted
//! whole-tree - which is what makes a program's part transfer a
//! checkable claim instead of a hope.

//!
//! M4.1 R0 split (docs/M4.1-CRATE-SPLIT.md): the plain types live in
//! [`def`], the `.mch` reader in [`parse`]; this module keeps what
//! touches the world - the singleton glue and the program checker.

// M4.1 R3: the plain types and the `.mch` reader live in camcraft-post
// (crates/post/src/machine/); this module keeps only what touches the
// world - the singleton glue and the program checker.
pub use camcraft_post::machine::*;

// #287: the Machines pane's catalogue, property sheet and validator.
pub mod library;
mod sheet;
pub mod validate;
// #383: the machine editor - property grid, draft, user folder and save.
pub mod draft;
pub mod fields;
pub mod store;

use flecs_ecs::prelude::World;

use crate::geometry::Point3;
use crate::model;
use crate::posting;
use crate::schema::MachineDef;

/// The MOUNTED machine: the active setup's (#343 - its own, else the
/// document's), as (the path it was loaded from, the definition).
/// `("", None)` until a `LoadMachine` command or a project file says
/// otherwise - and a document with no machine checks nothing, which is
/// the unstated-limit law wearing a document-sized hat. The document's
/// stored machine alone is [`document_of`].
#[must_use]
pub fn of(world: &World) -> (String, Option<Machine>) {
    crate::setup_mount::machine_of(world, crate::setup::active(world).as_deref())
}

/// The document's machine as stored - what every setup that owns no
/// machine follows (#343). For `LoadMachine`, the save, and the
/// document's machine section.
#[must_use]
pub fn document_of(world: &World) -> (String, Option<Machine>) {
    crate::setup_mount::document_machine(world)
}

/// The machine a declared setup context mounts (#343).
#[must_use]
pub fn of_setup(world: &World, declared: Option<&str>) -> (String, Option<Machine>) {
    crate::setup_mount::machine_of(world, declared)
}

/// Write the singleton. For the `LoadMachine` command and the project
/// reader only - the same law `units::set` keeps: setup state moves
/// through commands or a document load, never ad hoc.
pub fn set(world: &World, path: &str, machine: Option<&Machine>) {
    world.set(MachineDef {
        path: path.to_owned(),
        machine: machine.cloned(),
    });
}

/// The mounted machine's kind (the active setup's, #343) - the
/// per-frame accessor for ribbon gating and the new-op door (a new op
/// joins the active setup). An op's own laws read [`kind_of_setup`].
#[must_use]
pub fn kind_of(world: &World) -> Kind {
    kind_of_setup(world, crate::setup::active(world).as_deref())
}

pub use crate::setup_mount::kind_of_setup;

/// What a program asks that the machine cannot give. Located like the
/// deviation verdict - every line names its op - because a limit
/// violation without a WHERE is trivia, not a warning.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MachineCheck {
    pub problems: Vec<String>,
    /// How many more the reporter withheld (the walk caps its list;
    /// the count never lies).
    pub more: usize,
}

impl MachineCheck {
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.problems.is_empty()
    }
}

const REPORT_CAP: usize = 40;

/// Check the WHOLE program - the same op walk the sim and the post
/// share, so the gate that stage D wires sees one machine judgment
/// over exactly what would post. Never re-plans: cached moves only.
pub fn check_document(world: &World, machine: &Machine) -> Result<MachineCheck, String> {
    check_ops(world, machine, &posting::program_ops(world)?)
}

/// [`check_document`] over a slice of the program (#388): the ops one
/// machine runs, when setups mount different machines.
pub fn check_ops(world: &World, machine: &Machine, program: &[(String, flecs_ecs::prelude::Entity)]) -> Result<MachineCheck, String> {
    let ops = program.to_vec();
    let mut out = MachineCheck::default();

    // The changer (M3.3 C): a program whose ops draw on more distinct
    // tools than the drum holds physically cannot run, however each
    // tool passes on its own. Counted by name, the same identity the
    // ops and the library agree on.
    if let Some(slots) = machine.tool_count {
        let mut names: Vec<String> = Vec::new();
        for (_, e) in &ops {
            if let Some(op) = model::operation_of(world, e)
                && !names.contains(&op.tool)
            {
                names.push(op.tool.clone());
            }
        }
        if names.len() > slots {
            say(
                &mut out,
                format!("program calls {} tools, the changer holds {slots}", names.len()),
            );
        }
    }

    for (name, e) in &ops {
        let op = model::operation_of(world, e).ok_or_else(|| format!("{name}: lost its operation"))?;
        let tool_entity = model::find_by_name(world, &op.tool)
            .ok_or_else(|| format!("{name}: tool {} is not in the library", op.tool))?;
        let tool = model::tool_of(world, &tool_entity)
            .ok_or_else(|| format!("{name}: tool {} is not in the library", op.tool))?;
        // #368: a modelled body is the holder the magazine measures.
        let tool = crate::tool_body::judged(world, &tool_entity, tool)
            .map_err(|e| format!("{name}: tool {}: {e}", op.tool))?
            .0;
        let cache = model::toolpath_of(world, e).ok_or_else(|| format!("{name}: lost its toolpath"))?;
        // Travel numbers in the file are PROGRAM coordinates; the
        // cached moves are world millimeters. This is the post's own
        // origin law (one copy, in posting), so a G54 slide shifts
        // what passes exactly as it shifts the G-code.
        let (_, origin) = posting::wcs_context(world, op.setup.as_deref());

        // Spindle: the tool's suggested speed is what the op runs -
        // today every op posts its tool's rpm, so the tool IS the
        // program's spindle word.
        if tool.rpm > 0.0 {
            if let Some(hi) = machine.spindle_max
                && tool.rpm > hi
            {
                say(&mut out, format!("{name}: tool {} runs {:.0} rpm, above the machine's {hi:.0}", op.tool, tool.rpm));
            }
            if let Some(lo) = machine.spindle_min
                && tool.rpm < lo
            {
                say(&mut out, format!("{name}: tool {} runs {:.0} rpm, below the machine's {lo:.0}", op.tool, tool.rpm));
            }
        }
        // Spindle power (M13.2): the estimate the cutting data makes
        // of the op's own engagement against the stated motor. Every
        // input must be stated - machine power, document material,
        // tool flutes, an engagement the op's numbers fix - or no
        // claim is made (the unstated-limit law, `crate::cutting`).
        if let Some(limit) = machine.spindle_power
            && let Some(mat) = crate::cutting::material_of(world)
            && let Some(p) = crate::cutting::power_of(&op.params, &tool, mat)
            && p.kw > limit
        {
            say(
                &mut out,
                format!(
                    "{name}: spindle power estimated {:.2} kW ({:.1} cm3/min of {} at kc {:.0} N/mm2), above the machine's {limit:.2} kW",
                    p.kw,
                    p.mrr / 1000.0,
                    mat.label(),
                    p.kc
                ),
            );
        }
        // The tool table: long or fat cutters the magazine refuses.
        // "Tool length" is the whole protrusion from the spindle nose
        // to the tip - what the magazine's clearance measures: cutter,
        // shank, and the holder the setup assumes above them.
        if let Some(l) = machine.max_tool_length {
            let len = tool.flute_len + tool.shank_len + tool.holder_len;
            if len > l {
                say(&mut out, format!("{name}: tool {} is {len:.1} mm long, the machine takes {l:.1}", op.tool));
            }
        }
        if let Some(diam) = machine.max_tool_diameter
            && tool.diameter > diam
        {
            say(&mut out, format!("{name}: tool {} is {:.1} mm across, the machine takes {diam:.1}", op.tool, tool.diameter));
        }
        // The handoff step (M5.3 C): a parting op that asks for its
        // part to move to a second spindle wants steel the machine
        // may not have. Stated spindles check, unstated check
        // nothing - the same floor the tilt refusal stands on, and
        // the number a `PART TRANSFER 1 2` comment would have the
        // operator reach for a hand that isn't there.
        if let crate::operation::OpParams::Part { bar_transfer: true, .. } = &op.params
            && let Some(n) = machine.spindles
            && n < 2
        {
            say(
                &mut out,
                format!(
                    "{name}: parting hands the part to a second spindle; {} states {n} spindle(s)",
                    machine.model
                ),
            );
        }

        for (i, mv) in cache.path.moves.iter().enumerate() {
            // Pose (M5.1 stage A): a move that leans the tool asks
            // the machine to hold an angle, and the machine answers
            // with its rotaries - by name, never silently. Absence
            // is the ordinary case (prismatic), checked exact so no
            // float dust enters this branch from a normal path.
            // A C-axis live-tool move (issue #33) states the spindle's
            // angle; its pose is the fixed station's, so the letter it
            // asks for is C alone - read here, never re-derived from
            // the part-frame axis (whose A/B the machine does not have).
            if let Some(c) = mv.rotary {
                if let Some(msg) = c_letter_problem(machine, i, c) {
                    say(&mut out, format!("{name}: {msg}"));
                }
            } else if let Some(a) = mv.axis
                && !crate::cl::is_vertical(&a)
            {
                if !can_tilt(machine) {
                    say(
                        &mut out,
                        format!(
                            "{name}: move {i} tilts the tool; {} states no tilting rotary",
                            machine.model
                        ),
                    );
                }
                let [da, db, dc] = crate::cl::axis_to_angles(a);
                for (letter, ang) in [('A', da), ('B', db), ('C', dc)] {
                    if ang.abs() <= 1e-9 {
                        continue; // that letter holds nothing to check
                    }
                    // The letter names the rotary that must hold the
                    // angle. A machine whose tilt lives under another
                    // letter (an A trunnion asked for a B turn) is
                    // refused by name too - the in-app check is the
                    // safe floor; the machine's own post knows how to
                    // re-express a pose, this gate never does IK.
                    match machine
                        .axes
                        .iter()
                        .find(|x| x.rotary && x.name.eq_ignore_ascii_case(&letter.to_string()))
                    {
                        None => say(
                            &mut out,
                            format!(
                                "{name}: move {i} commands {letter} {ang:.3} deg; {} states no {letter} axis",
                                machine.model
                            ),
                        ),
                        Some(ax) => {
                            if let Some((lo, hi)) = ax.travel
                                && (ang < lo - 1e-9 || ang > hi + 1e-9)
                            {
                                say(
                                    &mut out,
                                    format!(
                                        "{name}: move {i} commands {letter} {ang:.3} deg, the travel is {lo:.3}..{hi:.3}"
                                    ),
                                );
                            }
                        }
                    }
                }
            }
            // Feed: only stated feeds check against stated ceilings.
            if let Some(feed) = mv.feed {
                let d = [
                    mv.to.x - mv.from.x,
                    mv.to.y - mv.from.y,
                    mv.to.z - mv.from.z,
                ];
                if let Some(ceil) = machine.feed_ceiling(d)
                    && feed > ceil
                {
                    say(&mut out, format!(
                        "{name}: move {i} feeds {feed:.0} mm/min, {} tops out at {ceil:.0}",
                        slowest_axis(&machine.axes, ceil)
                    ));
                }
            }
            // Travel: endpoints and - for arcs - the quarter points
            // the sweep can bulge past. A straight move's extremes
            // ARE its endpoints. The stroke is the machine's own; an
            // inverted axis (#373) reads program `v` as `-v`, so the
            // band is judged in the machine's coordinates.
            for p in move_extremes(mv) {
                for (k, w) in [p.x, p.y, p.z].iter().enumerate() {
                    let Some(ax) = machine.axis(k) else { continue };
                    let Some((lo, hi)) = ax.travel else { continue };
                    let v = (w - origin[k]) * ax.direction.sign();
                    if v < lo || v > hi {
                        let inv = if ax.direction.is_standard() { "" } else { " (inverted axis)" };
                        say(&mut out, format!(
                            "{name}: move {i} reaches {v:.3} in {}{inv}, the stroke is {lo:.3}..{hi:.3}",
                            ax.name
                        ));
                    }
                }
            }
        }
    }
    Ok(out)
}

/// The arc's four XY quarter points plus both endpoints; a straight
/// move's two endpoints. (A quarter is outside the sweep only when
/// the arc goes the other way around it; checking all four can only
/// over-report a limit the post would then also cross, never hide a
/// violation - and over-reporting a REAL stroke is stage C's honest
/// direction of error.)
fn move_extremes(mv: &crate::cl::Move) -> Vec<Point3> {
    let mut v = vec![mv.from, mv.to];
    // XZ / YZ (M11.2 A): the same four quarters, in the arc's plane.
    if let Some(arc) = mv.arc.filter(|a| !a.plane.is_xy()) {
        let pl = arc.plane;
        let (cu, cv) = pl.center_uv(arc.center);
        let (u0, v0, _) = pl.uvw(mv.from);
        let (_, _, w1) = pl.uvw(mv.to);
        let r = ((u0 - cu).powi(2) + (v0 - cv).powi(2)).sqrt();
        v.push(pl.point(cu + r, cv, w1));
        v.push(pl.point(cu - r, cv, w1));
        v.push(pl.point(cu, cv + r, w1));
        v.push(pl.point(cu, cv - r, w1));
        return v;
    }
    if let Some(arc) = &mv.arc {
        let dx = mv.from.x - arc.center.x;
        let dy = mv.from.y - arc.center.y;
        let r = (dx * dx + dy * dy).sqrt();
        v.push(Point3::new(arc.center.x + r, arc.center.y, mv.to.z));
        v.push(Point3::new(arc.center.x - r, arc.center.y, mv.to.z));
        v.push(Point3::new(arc.center.x, arc.center.y + r, mv.to.z));
        v.push(Point3::new(arc.center.x, arc.center.y - r, mv.to.z));
    }
    v
}

fn slowest_axis(axes: &[AxisLimits], ceil: f64) -> String {
    axes.iter()
        .find(|a| a.feed == Some(ceil))
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "an axis".into())
}

fn say(out: &mut MachineCheck, s: String) {
    if out.problems.len() < REPORT_CAP {
        out.problems.push(s);
    } else {
        out.more += 1;
    }
}

/// The C letter gate (issue #33): a move stating spindle C angle `c`
/// needs a rotary named C on the machine, and a stated C travel must
/// hold the unwrapped angle. None when it is fine.
fn c_letter_problem(machine: &Machine, i: usize, c: f64) -> Option<String> {
    let Some(ax) = machine.axes.iter().find(|x| x.rotary && x.name.eq_ignore_ascii_case("C")) else {
        return Some(format!("move {i} turns the spindle as C {c:.3} deg; {} states no C axis", machine.model));
    };
    let (lo, hi) = ax.travel?;
    (c < lo - 1e-9 || c > hi + 1e-9)
        .then(|| format!("move {i} commands C {c:.3} deg, the travel is {lo:.3}..{hi:.3}"))
}
