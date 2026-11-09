//! Probing in the intermediate stream (#108): the `TOUCH` record (one
//! probing move toward a target that stops on contact), the `GUARD`
//! record (#174: a positioning feed of a probe path that stops on an
//! UNEXPECTED contact and alarms) and the `FOUND` record (what the
//! measured trips find, and what the program does with it).
//! Provenance: NIST RS274NGC (NISTIR 6556) G38.2 probe semantics - the
//! trip point lands in the probe parameters - the LinuxCNC G-code
//! reference's G38.3 (stop on contact, no error without one) and
//! touch-probe practice (a centre is the mean of opposite touches;
//! positioning moves are protected); our own record names and text
//! form, specced beside `docs/CNC-INTERMEDIATE.md`.

use serde_json::{json, Value};
use std::collections::BTreeMap;

use crate::cl::Toolpath;

/// One probing move: probe toward `target` at `feed`, the nominal trip
/// at `expect`. `measure` is `Some(k)` when the trip is measurement k.
#[derive(Clone, Debug, PartialEq)]
pub struct TouchRec {
    pub target: [f64; 3],
    pub expect: [f64; 3],
    pub feed: f64,
    pub measure: Option<u32>,
}

/// The move straight back off a trip (#108): an INCREMENTAL `delta`
/// from wherever the stylus tripped, so a part found nearer than the
/// model says is never driven into by an absolute back-off. `expect`
/// is the nominal end (the chain's), `feed` the move's.
#[derive(Clone, Debug, PartialEq)]
pub struct RetreatRec {
    pub delta: [f64; 3],
    pub expect: [f64; 3],
    pub feed: f64,
}

impl RetreatRec {
    pub fn props(&self) -> BTreeMap<String, Value> {
        let mut m = BTreeMap::new();
        m.insert("command".into(), json!("ProbeRetreat"));
        m.insert("DeltaX".into(), json!(self.delta[0]));
        m.insert("DeltaY".into(), json!(self.delta[1]));
        m.insert("DeltaZ".into(), json!(self.delta[2]));
        m.insert("Feed".into(), json!(self.feed));
        m
    }
}

pub(super) fn write_retreat(r: &RetreatRec) -> String {
    format!(
        "RETREAT {} {} {} EXPECT {} {} {} F {}
",
        q(r.delta[0]),
        q(r.delta[1]),
        q(r.delta[2]),
        q(r.expect[0]),
        q(r.expect[1]),
        q(r.expect[2]),
        q(r.feed)
    )
}

/// `RETREAT dx dy dz EXPECT x y z F feed` (lengths in microns).
pub(super) fn parse_retreat<'a>(it: &mut impl Iterator<Item = &'a str>) -> Result<RetreatRec, String> {
    let delta = [um(it.next(), "RETREAT X")?, um(it.next(), "RETREAT Y")?, um(it.next(), "RETREAT Z")?];
    if it.next() != Some("EXPECT") {
        return Err("RETREAT needs EXPECT after its delta".into());
    }
    let expect = [um(it.next(), "EXPECT X")?, um(it.next(), "EXPECT Y")?, um(it.next(), "EXPECT Z")?];
    if it.next() != Some("F") {
        return Err("RETREAT needs a feed (F)".into());
    }
    let feed = um(it.next(), "RETREAT feed")?;
    if feed <= 0.0 {
        return Err("a RETREAT feed must be positive".into());
    }
    Ok(RetreatRec { delta, expect, feed })
}

/// A protected positioning move (#174): a straight feed of a probe
/// path that is not a touch or its back-off - the descent to a touch's
/// start, the travel between touches, the rise - posted as a
/// stop-on-contact move that ALARMS when the stylus trips on the way,
/// so a part or clamp that is not where the model says stops the
/// machine instead of breaking the stylus. `to` is the move's end
/// (world mm), `feed` its rate.
#[derive(Clone, Debug, PartialEq)]
pub struct GuardRec {
    pub to: [f64; 3],
    pub feed: f64,
}

impl GuardRec {
    pub fn props(&self) -> BTreeMap<String, Value> {
        let mut m = BTreeMap::new();
        m.insert("command".into(), json!("ProbeGuard"));
        m.insert("X".into(), json!(self.to[0]));
        m.insert("Y".into(), json!(self.to[1]));
        m.insert("Z".into(), json!(self.to[2]));
        m.insert("Feed".into(), json!(self.feed));
        m
    }
}

/// The positioning law of a probe path (#174): every motion of a path
/// that carries touches, other than the touches, their back-offs and
/// the rapids, is a protected move. `Ok(None)` for a rapid (the safe
/// plane's travel stays a rapid); `Err` names a move that cannot be
/// protected - an arc, or a feed with no rate - so it is refused rather
/// than posted as a plain feed.
pub fn guard_of(m: &crate::cl::Move) -> Result<Option<GuardRec>, String> {
    if m.kind == crate::cl::MoveKind::Rapid {
        return Ok(None);
    }
    if m.arc.is_some() {
        return Err("probing: an arc in a probe path cannot be a protected (stop-on-contact) move - refused".into());
    }
    let feed = m
        .feed
        .filter(|f| f.is_finite() && *f > 0.0)
        .ok_or_else(|| "probing: a probe path positioning feed states no feed rate - refused".to_string())?;
    Ok(Some(GuardRec { to: [m.to.x, m.to.y, m.to.z], feed }))
}

pub(super) fn write_guard(g: &GuardRec) -> String {
    format!("GUARD {} {} {} F {}\n", q(g.to[0]), q(g.to[1]), q(g.to[2]), q(g.feed))
}

/// `GUARD x y z F feed` (lengths in microns).
pub(super) fn parse_guard<'a>(it: &mut impl Iterator<Item = &'a str>) -> Result<GuardRec, String> {
    let to = [um(it.next(), "GUARD X")?, um(it.next(), "GUARD Y")?, um(it.next(), "GUARD Z")?];
    if it.next() != Some("F") {
        return Err("GUARD needs a feed (F)".into());
    }
    let feed = um(it.next(), "GUARD feed")?;
    if feed <= 0.0 {
        return Err("a GUARD feed must be positive".into());
    }
    Ok(GuardRec { to, feed })
}

/// One found axis: the mean of the listed measurements' trips on
/// `axis` (0 X, 1 Y, 2 Z), expected at `expected` (world mm).
#[derive(Clone, Debug, PartialEq)]
pub struct FoundAxisRec {
    pub axis: usize,
    pub expected: f64,
    pub measures: Vec<u32>,
}

/// The probing result: set work offset `set_wcs` (0..=5, G54..G59) by
/// the deviation, or (None) report it and stop for the operator; in
/// both cases a deviation past `tol_mm` stops the program.
#[derive(Clone, Debug, PartialEq)]
pub struct FoundRec {
    pub set_wcs: Option<u8>,
    pub tol_mm: f64,
    pub axes: Vec<FoundAxisRec>,
}

/// What the op asks of its measurements (the dump's context).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbeResult {
    pub set_wcs: Option<u8>,
    pub tol_mm: f64,
}

pub(super) const AXES: [&str; 3] = ["X", "Y", "Z"];

impl TouchRec {
    pub fn props(&self) -> BTreeMap<String, Value> {
        let mut m = BTreeMap::new();
        m.insert("command".into(), json!("ProbeMove"));
        m.insert("X".into(), json!(self.target[0]));
        m.insert("Y".into(), json!(self.target[1]));
        m.insert("Z".into(), json!(self.target[2]));
        m.insert("ExpectX".into(), json!(self.expect[0]));
        m.insert("ExpectY".into(), json!(self.expect[1]));
        m.insert("ExpectZ".into(), json!(self.expect[2]));
        m.insert("Feed".into(), json!(self.feed));
        m.insert("Measure".into(), self.measure.map_or(json!(-1), |k| json!(k)));
        m
    }
}

impl FoundRec {
    pub fn props(&self) -> BTreeMap<String, Value> {
        let mut m = BTreeMap::new();
        m.insert("command".into(), json!("ProbeResult"));
        m.insert("SetWorkOffset".into(), self.set_wcs.map_or(json!(-1), |s| json!(s)));
        m.insert("Tolerance".into(), json!(self.tol_mm));
        let axes: Vec<Value> = self
            .axes
            .iter()
            .map(|a| json!({"Axis": AXES.get(a.axis).copied().unwrap_or("X"), "Expected": a.expected, "Measures": a.measures}))
            .collect();
        m.insert("Axes".into(), Value::Array(axes));
        m
    }
}

/// The axes a path's measuring touches find, read off the touches: a
/// measurement counts on the axis it travels along, and the expected
/// value is the mean of the nominal trips (opposite touches average to
/// the centre). Touches that do not sit on their move count nothing -
/// the dump refuses them by name.
#[must_use]
pub fn found_axes(t: &Toolpath) -> Vec<FoundAxisRec> {
    let mut out: Vec<FoundAxisRec> = Vec::new();
    for p in t.probes.iter().filter(|p| p.is_sane(&t.moves)) {
        let (Some(k), Some(d), Some(m)) = (p.measure, p.direction(&t.moves), t.moves.get(p.index)) else {
            continue;
        };
        let c = [d.x.abs(), d.y.abs(), d.z.abs()];
        let axis = if c[0] >= c[1] && c[0] >= c[2] {
            0
        } else if c[1] >= c[2] {
            1
        } else {
            2
        };
        let v = [m.to.x, m.to.y, m.to.z][axis];
        match out.iter_mut().find(|a| a.axis == axis) {
            Some(a) => {
                let n = a.measures.len() as f64;
                a.expected = (a.expected * n + v) / (n + 1.0);
                a.measures.push(k);
            }
            None => out.push(FoundAxisRec { axis, expected: v, measures: vec![k] }),
        }
    }
    out.sort_by_key(|a| a.axis);
    out
}

/// µm integer, the stream's length law.
fn q(v: f64) -> String {
    if !v.is_finite() {
        return "nan".into();
    }
    ((v * 1000.0).round() as i64).to_string()
}

pub(super) fn write_touch(t: &TouchRec) -> String {
    format!(
        "TOUCH {} {} {} EXPECT {} {} {} F {} M {}\n",
        q(t.target[0]),
        q(t.target[1]),
        q(t.target[2]),
        q(t.expect[0]),
        q(t.expect[1]),
        q(t.expect[2]),
        q(t.feed),
        t.measure.map_or_else(|| "-".to_string(), |k| k.to_string())
    )
}

pub(super) fn write_found(f: &FoundRec) -> String {
    let mut s = match f.set_wcs {
        Some(w) => format!("FOUND SET {w}"),
        None => "FOUND CONFIRM".to_string(),
    };
    s.push_str(&format!(" TOL {}", q(f.tol_mm)));
    for a in &f.axes {
        s.push_str(&format!(" AXIS {} {}", AXES.get(a.axis).copied().unwrap_or("X"), q(a.expected)));
        for k in &a.measures {
            s.push_str(&format!(" {k}"));
        }
    }
    s.push('\n');
    s
}

fn um(tok: Option<&str>, what: &str) -> Result<f64, String> {
    let t = tok.ok_or_else(|| format!("{what} is missing"))?;
    let v: f64 = t.parse().map_err(|_| format!("{what} {t:?} is not a number"))?;
    if !v.is_finite() {
        return Err(format!("{what} {t:?} is not finite"));
    }
    Ok(v / 1000.0)
}

/// `TOUCH x y z EXPECT x y z F feed M k|-` (lengths in microns).
pub(super) fn parse_touch<'a>(it: &mut impl Iterator<Item = &'a str>) -> Result<TouchRec, String> {
    let target = [um(it.next(), "TOUCH X")?, um(it.next(), "TOUCH Y")?, um(it.next(), "TOUCH Z")?];
    if it.next() != Some("EXPECT") {
        return Err("TOUCH needs EXPECT after its target".into());
    }
    let expect = [um(it.next(), "EXPECT X")?, um(it.next(), "EXPECT Y")?, um(it.next(), "EXPECT Z")?];
    if it.next() != Some("F") {
        return Err("TOUCH needs a feed (F)".into());
    }
    let feed = um(it.next(), "TOUCH feed")?;
    if feed <= 0.0 {
        return Err("a TOUCH feed must be positive".into());
    }
    if it.next() != Some("M") {
        return Err("TOUCH needs its measure word (M k or M -)".into());
    }
    let measure = match it.next() {
        Some("-") => None,
        Some(k) => Some(k.parse::<u32>().map_err(|_| format!("TOUCH measure {k:?}"))?),
        None => return Err("TOUCH measure is missing".into()),
    };
    Ok(TouchRec { target, expect, feed, measure })
}

/// `FOUND SET n|CONFIRM TOL tol (AXIS X|Y|Z expected k...)*`.
pub(super) fn parse_found<'a>(it: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>) -> Result<FoundRec, String> {
    let set_wcs = match it.next() {
        Some("SET") => {
            let w = it.next().ok_or("FOUND SET needs a work offset slot")?;
            let n = w.parse::<u8>().map_err(|_| format!("FOUND SET slot {w:?}"))?;
            if n > 5 {
                return Err(format!("FOUND SET slot {n} is not one of G54..G59"));
            }
            Some(n)
        }
        Some("CONFIRM") => None,
        other => return Err(format!("FOUND needs SET or CONFIRM, found {other:?}")),
    };
    if it.next() != Some("TOL") {
        return Err("FOUND needs a tolerance (TOL)".into());
    }
    let tol_mm = um(it.next(), "FOUND tolerance")?;
    if tol_mm <= 0.0 {
        return Err("a FOUND tolerance must be positive".into());
    }
    let mut axes = Vec::new();
    while let Some(w) = it.next() {
        if w != "AXIS" {
            return Err(format!("FOUND expected AXIS, found {w:?}"));
        }
        let name = it.next().ok_or("FOUND AXIS needs a letter")?;
        let axis = AXES.iter().position(|a| *a == name).ok_or_else(|| format!("FOUND AXIS {name:?}"))?;
        let expected = um(it.next(), "FOUND expected")?;
        let mut measures = Vec::new();
        while let Some(k) = it.peek().and_then(|t| t.parse::<u32>().ok()) {
            measures.push(k);
            it.next();
        }
        if measures.is_empty() {
            return Err(format!("FOUND AXIS {name} names no measurement"));
        }
        axes.push(FoundAxisRec { axis, expected, measures });
    }
    if axes.is_empty() {
        return Err("FOUND finds no axis".into());
    }
    Ok(FoundRec { set_wcs, tol_mm, axes })
}
