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
//! machine origin would invent the very number the check needs.

use serde::{Deserialize, Serialize};

use flecs_ecs::core::WorldGet;
use flecs_ecs::prelude::World;

use crate::geometry::Point3;
use crate::model;
use crate::posting;
use crate::schema::MachineDef;

/// The document's machine, read from the flecs singleton: (the path it
/// was loaded from, the definition). `("", None)` until a `LoadMachine`
/// command or a v19 project file says otherwise - and a document with
/// no machine checks nothing, which is the unstated-limit law wearing
/// a document-sized hat.
#[must_use]
pub fn of(world: &World) -> (String, Option<Machine>) {
    world
        .try_get::<&MachineDef>(|d| (d.path.clone(), d.machine.clone()))
        .unwrap_or_default()
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

/// One prismatic axis as the machine file describes it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AxisLimits {
    pub name: String,
    /// G1 ceiling, mm/min. The feed a program asks for is checked
    /// against the SLOWEST moving axis of a coordinated move - an
    /// interpolation has to feed every axis it drives.
    pub feed: Option<f64>,
    /// G0 rate, mm/min. Programs never ask for a rapid rate, so this
    /// is carried for the setup sheet (stage E), not for a check.
    pub rapid: Option<f64>,
    /// Stroke in program coordinates, [min, max]. `None` = the file
    /// does not say (which is every stock vendor file - see the
    /// module's travel note).
    pub travel: Option<(f64, f64)>,
}

/// A machine, as parsed from a `.mch` file. The identity strings are
/// display data; every limit is optional by the unstated-limits law.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Machine {
    pub model: String,
    pub vendor: String,
    pub spindle_min: Option<f64>,
    pub spindle_max: Option<f64>,
    /// In X, Y, Z order (the file's `coordinate` index decides);
    /// empty when the file names no linear axes.
    pub axes: Vec<AxisLimits>,
    pub max_tool_length: Option<f64>,
    pub max_tool_diameter: Option<f64>,
    /// The spindle BODY (M3.2 stage B): diameter, and how far the
    /// housing stands proud above the tool holder, mm. Not the speed
    /// range above - the geometry the rotating house occupies, for the
    /// collision band that runs past the holder. Unstated by the
    /// unstated-limit law (files that name nothing check nothing);
    /// when both are stated the band is a cylinder of `spindle_d`
    /// rising `spindle_len` above the holder top.
    pub spindle_d: Option<f64>,
    pub spindle_len: Option<f64>,
}

/// What a machine is, inferred from its axis inventory (M3.2 stage E -
/// the workspaces-by-kind law). Provenance: general machine-tool
/// practice - a mill moves a cutter in X, Y and Z; a lathe turns the
/// part and moves a tool in X and Z only; a mill-turn does both, which
/// an ordinary file reveals as a Y axis with a rotary (A/B/C) among
/// its axes. The inference is deliberately the minimal honest one:
/// anything the axis list cannot decide - an unnamed file, a file with
/// no X/Z pair - reads [`Kind::Mill`], because a mill is what every
/// assumption in this codebase currently is, and an invented lathe
/// workspace would be the silent wrong answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Mill,
    Lathe,
    MillTurn,
}

impl Kind {
    /// The word the pages and tips use for the kind.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Kind::Mill => "milling",
            Kind::Lathe => "turning",
            Kind::MillTurn => "mill-turning",
        }
    }
}

/// The kind a machine definition implies, by the axis-inventory law
/// above. Rotary axes alone do not make a mill-turn: a turning center
/// with a C axis still has no Y, and the file says "lathe".
#[must_use]
pub fn kind(machine: Option<&Machine>) -> Kind {
    let Some(m) = machine else {
        return Kind::Mill; // the unstated machine is today's document
    };
    let has = |n: &str| m.axes.iter().any(|a| a.name.eq_ignore_ascii_case(n));
    let rotary = ["A", "B", "C"].iter().any(|r| has(r));
    match (has("Y"), has("X"), has("Z"), rotary) {
        (true, _, _, true) => Kind::MillTurn,
        (true, _, _, false) => Kind::Mill,
        (false, true, true, _) => Kind::Lathe,
        _ => Kind::Mill,
    }
}

/// The document's machine kind, read off the singleton without the
/// clone `of` does - the per-frame accessor for ribbon gating.
#[must_use]
pub fn kind_of(world: &World) -> Kind {
    world
        .try_get::<&MachineDef>(|d| kind(d.machine.as_ref()))
        .unwrap_or(Kind::Mill)
}

/// The number a machine field reads as, with the unstated-limit law:
/// absent, null, non-numeric, or exactly zero reads `None`; a
/// negative or NaN reads `Some` only for travel bounds (where a
/// negative stroke limit is the point), and `None` for rates (where
/// it is a broken file, checked at the point of use).
fn rate(v: Option<&serde_json::Value>) -> Option<f64> {
    let x = v?.as_f64()?;
    if x.is_finite() && x > 0.0 {
        Some(x)
    } else {
        None
    }
}

/// Length-unit factor: a file declaring inches multiplies every
/// distance - and every distance-per-minute - by 25.4 on the way in,
/// so the whole checker stays mm-only.
fn mm_factor(units: Option<&serde_json::Value>) -> Option<f64> {
    match units?.get("length")?.as_str()? {
        "inch" | "inches" => Some(25.4),
        _ => Some(1.0),
    }
}

/// Walk a linear-part list collecting every `"type": "linear"` part
/// by name, depth-first (the standard shape nests Y inside X inside
/// Z - the chain is the kinematics, but the limit table is flat).
fn collect_linear(parts: Option<&serde_json::Value>, out: &mut Vec<(String, serde_json::Value)>) {
    let Some(arr) = parts.and_then(|p| p.as_array()) else { return };
    for p in arr {
        let is_linear = p.get("type").and_then(|t| t.as_str()) == Some("linear");
        let name = p
            .get("name")
            .and_then(|n| n.as_str())
            .or_else(|| p.get("id").and_then(|n| n.as_str()))
            .unwrap_or("")
            .to_owned();
        if is_linear && !name.is_empty() {
            out.push((name, p.clone()));
        }
        collect_linear(p.get("parts"), out);
    }
}

/// First part anywhere in the tree with the given `type`, depth-first.
fn find_type<'a>(parts: &'a serde_json::Value, ty: &str) -> Option<&'a serde_json::Value> {
    let arr = parts.as_array()?;
    for p in arr {
        if p.get("type").and_then(|t| t.as_str()) == Some(ty) {
            return Some(p);
        }
        if let Some(hit) = find_type(p.get("parts")?, ty) {
            return Some(hit);
        }
    }
    None
}

/// Parse a `.mch` machine file. Malformed JSON refuses; a well-formed
/// file that describes nothing answers with an all-`None` Machine -
/// an empty machine checks nothing, which is the truth rather than a
/// silently stricter one.
pub fn parse_mch(text: &str) -> Result<Machine, String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("machine file: {e}"))?;
    if !v.is_object() {
        return Err("machine file: expected a machine object".to_owned());
    }

    let general = v.pointer("/general");
    let mut m = Machine {
        model: text_of(general.and_then(|g| g.get("model"))),
        vendor: text_of(general.and_then(|g| g.get("vendor"))),
        ..Default::default()
    };

    let kine = v.pointer("/kinematics/default/parts");
    let f = mm_factor(v.pointer("/kinematics/default/units")).unwrap_or(1.0);

    if let Some(parts) = kine {
        if let Some(head) = find_type(parts, "head") {
            let sp = head.get("spindle");
            m.spindle_min = rate(sp.and_then(|s| s.get("min_speed")));
            m.spindle_max = rate(sp.and_then(|s| s.get("max_speed")));
            let ts = head.get("tool_station");
            m.max_tool_length = scale(rate(ts.and_then(|t| t.get("max_tool_length"))), f);
            m.max_tool_diameter = scale(rate(ts.and_then(|t| t.get("max_tool_diameter"))), f);
            m.spindle_d = scale(rate(sp.and_then(|s| s.get("body_diameter"))), f);
            m.spindle_len = scale(rate(sp.and_then(|s| s.get("body_length"))), f);
        }
        let mut linear: Vec<(String, serde_json::Value)> = Vec::new();
        collect_linear(Some(parts), &mut linear);

        // Controller speeds, keyed by axis name; the kinematics list
        // decides WHICH axes exist, the controller says how fast they
        // go, and a `"travel"` object (our extension) says how far.
        let ctrl = v.pointer("/controller/synced_configuration/parts");
        let mut axes: Vec<AxisLimits> = Vec::new();
        for (name, part) in &linear {
            let c = ctrl.and_then(|c| c.get(name.as_str()));
            let travel = part.get("travel").and_then(|t| {
                let lo = t.get("min")?.as_f64()?;
                let hi = t.get("max")?.as_f64()?;
                if lo.is_finite() && hi.is_finite() && hi > lo {
                    Some((lo * f, hi * f))
                } else {
                    None
                }
            });
            axes.push(AxisLimits {
                name: name.clone(),
                feed: scale(rate(c.and_then(|c| c.get("max_normal_speed"))), f),
                rapid: scale(rate(c.and_then(|c| c.get("max_rapid_speed"))), f),
                travel,
            });
        }
        axes.sort_by_key(|a| ctrl_coord(ctrl, &a.name));
        m.axes = axes;
    }
    Ok(m)
}

fn text_of(v: Option<&serde_json::Value>) -> String {
    v.and_then(|x| x.as_str()).unwrap_or("unknown").to_owned()
}

fn scale(v: Option<f64>, f: f64) -> Option<f64> {
    v.map(|x| x * f)
}

fn ctrl_coord(ctrl: Option<&serde_json::Value>, name: &str) -> i64 {
    ctrl.and_then(|c| c.get(name))
        .and_then(|a| a.get("coordinate"))
        .and_then(|x| x.as_i64())
        .unwrap_or(i64::MAX)
}

impl Machine {
    /// Is every stated limit statable: finite rates, and strokes that
    /// run somewhere. The project reader gates on this BEFORE writing
    /// the world - a hand-edited machine that means nothing is a
    /// corrupt file, refused, never a checker that fails backwards.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        let rate_ok = |r: Option<f64>| r.is_none_or(|x| x.is_finite() && x > 0.0);
        rate_ok(self.spindle_min)
            && rate_ok(self.spindle_max)
            && rate_ok(self.max_tool_length)
            && rate_ok(self.max_tool_diameter)
            && rate_ok(self.spindle_d)
            && rate_ok(self.spindle_len)
            && self.axes.iter().all(|a| {
                rate_ok(a.feed) && rate_ok(a.rapid) && a.travel.is_none_or(|(lo, hi)| lo.is_finite() && hi.is_finite() && hi > lo)
            })
            && match (self.spindle_min, self.spindle_max) {
                (Some(lo), Some(hi)) => hi >= lo,
                _ => true,
            }
    }

    /// The one-line identity for the Setup pane and the stage E sheet.
    #[must_use]
    pub fn summary(&self) -> String {
        let n = self.axes.iter().filter(|a| a.travel.is_some()).count();
        format!(
            "{} {} - spindle {}-{} rpm, {} axes ({} with a known stroke)",
            self.vendor,
            self.model,
            self.spindle_min.map_or_else(|| "?".into(), |x| format!("{x:.0}")),
            self.spindle_max.map_or_else(|| "?".into(), |x| format!("{x:.0}")),
            self.axes.len(),
            n,
        )
    }

    /// The limits of the coordinate `k` (0=X, 1=Y, 2=Z), found BY NAME.
    /// Positional indexing would silently check the wrong stroke when a
    /// file names a subset of axes or leads with a U - the table is
    /// ordered for display, names are what make a limit mean something.
    fn axis(&self, k: usize) -> Option<&AxisLimits> {
        let n = ["X", "Y", "Z"].get(k)?.to_ascii_uppercase();
        self.axes
            .iter()
            .find(|a| a.name.to_ascii_uppercase() == n)
    }

    /// The feed ceiling of a coordinated move: the slowest stated
    /// ceiling among the axes the move actually drives. `None` when
    /// no driven axis states one - nothing to violate.
    fn feed_ceiling(&self, d: [f64; 3]) -> Option<f64> {
        let mut best: Option<f64> = None;
        for (k, moved) in d.map(|x| x.abs() > 0.0).iter().enumerate() {
            if !*moved {
                continue;
            }
            if let Some(f) = self.axis(k).and_then(|a| a.feed) {
                best = Some(best.map_or(f, |b: f64| b.min(f)));
            }
        }
        best
    }
}

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
    let ops = posting::program_ops(world)?;
    let mut out = MachineCheck::default();

    for (name, e) in &ops {
        let op = model::operation_of(world, e).ok_or_else(|| format!("{name}: lost its operation"))?;
        let tool = model::find_by_name(world, &op.tool)
            .and_then(|t| model::tool_of(world, &t))
            .ok_or_else(|| format!("{name}: tool {} is not in the library", op.tool))?;
        let cache = model::toolpath_of(world, e).ok_or_else(|| format!("{name}: lost its toolpath"))?;
        // Travel numbers in the file are PROGRAM coordinates; the
        // cached moves are world millimeters. This is the post's own
        // origin law (one copy, in posting), so a G54 slide shifts
        // what passes exactly as it shifts the G-code.
        let (_, origin) = posting::wcs_context(world, op.setup.as_deref(), &op.params);

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

        for (i, mv) in cache.path.moves.iter().enumerate() {
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
            // ARE its endpoints.
            for p in move_extremes(mv) {
                for (k, w) in [p.x, p.y, p.z].iter().enumerate() {
                    let v = w - origin[k];
                    if let Some((lo, hi)) = machine.axis(k).and_then(|a| a.travel)
                        && (v < lo || v > hi)
                    {
                        say(&mut out, format!(
                            "{name}: move {i} reaches {v:.3} in {}, the stroke is {lo:.3}..{hi:.3}",
                            machine.axis(k).map_or("?", |a| a.name.as_str())
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
