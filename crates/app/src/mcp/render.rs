//! `render_stock` (#182): the program's simulated stock as a PNG image
//! block plus a numeric summary (rest and gouge volume, the bounds of
//! what stands and of the rest), so a headless agent can see what the
//! program leaves. The picture is the column verdict's own sheet
//! (`stock_sim::simulate_document`, the post gate's sim) painted by its
//! deviation classes through the CPU rasteriser (`crate::raster`).
//!
//! Provenance: the MCP image content block (`type: image`, base64
//! `data`, `mimeType`) from the public MCP specification (2025-06-18,
//! "Tool Result"); base64 per RFC 4648 section 4. Our own code.

use super::tools::opt_str;
use crate::geometry::{Aabb, Point3};
use crate::raster::{Camera, Canvas, ViewDir, MAX_EDGE, MIN_EDGE};
use crate::script::{fnum, Env};
use crate::stock_sim::{ProgramSim, SimSurface};

use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// The reserved key a handler's answer carries its PNG under; `tool_ok`
/// lifts it out of the JSON into an image content block.
pub(super) const PNG_KEY: &str = "_png_base64";

const BACKGROUND: [u8; 3] = [236, 238, 241];
/// Paints by deviation class: clean, rest, gouge, unjudged.
const CLASS_PAINT: [[u8; 3]; 4] = [[150, 170, 196], [235, 170, 50], [220, 45, 45], [165, 150, 200]];
const RAPID: [u8; 3] = [210, 40, 40];
const FEED: [u8; 3] = [20, 110, 230];
const PLUNGE: [u8; 3] = [230, 160, 0];

pub(super) fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "width": { "type": "integer", "minimum": MIN_EDGE, "maximum": MAX_EDGE, "description": "Image width in pixels (default 640)." },
            "height": { "type": "integer", "minimum": MIN_EDGE, "maximum": MAX_EDGE, "description": "Image height in pixels (default 480)." },
            "view": { "type": "string", "enum": ["iso", "top"], "description": "Camera: isometric from front-right-above (default) or top-down." },
            "toolpath": { "type": "boolean", "description": "Overlay every operation's moves: feed blue, plunge amber, rapid red (default false)." }
        },
        "required": [],
        "additionalProperties": false,
    })
}

fn edge(a: &Map<String, Value>, key: &str, default: u32) -> Result<u32, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| (MIN_EDGE..=MAX_EDGE).contains(n))
            .ok_or_else(|| format!("\"{key}\" must be an integer from {MIN_EDGE} to {MAX_EDGE}, got {v}")),
    }
}

/// Render the program's simulated stock. Refusals: the sim's own (no
/// generated program, a posed or rotary program the column verdict does
/// not judge, no stock), and a sim that leaves nothing to draw.
pub(super) fn render_stock(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let width = edge(a, "width", 640)?;
    let height = edge(a, "height", 480)?;
    let view = match opt_str(a, "view")? {
        None | Some("iso") => ViewDir::Iso,
        Some("top") => ViewDir::Top,
        Some(other) => return Err(format!("\"view\" must be \"iso\" or \"top\", got \"{other}\"")),
    };
    let toolpath = match a.get("toolpath") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(other) => return Err(format!("\"toolpath\" must be a boolean, got {other}")),
    };
    let w = &env.world;
    let sim = crate::stock_sim::simulate_document(w).map_err(|e| format!("no stock simulation to render: {e}"))?;
    let floor = crate::model::stock_world_aabb_for(w, crate::setup::active(w).as_deref()).map(|b| b.min.z);
    let tris = scene(&sim.surface, floor, sim.dev.cell);
    if tris.is_empty() {
        return Err("the simulated stock has no surface to render (no material stands above the stock floor)".into());
    }
    let moves = if toolpath { overlay_moves(w)? } else { Vec::new() };

    let pts = tris.iter().flat_map(|(t, _)| t.iter().copied()).chain(moves.iter().flat_map(|(a, b, _)| [*a, *b]));
    let cam = Camera::fit(view, pts, width, height).ok_or("the simulated stock has no finite extent to frame")?;
    let mut canvas = Canvas::new(cam, BACKGROUND);
    for (t, cls) in &tris {
        canvas.triangle(t[0], t[1], t[2], paint(*cls));
    }
    for (a, b, c) in &moves {
        canvas.overlay_line(*a, *b, *c);
    }
    let png = canvas.png()?;
    let mut out = summary(&sim, &tris);
    out["image"] = json!({ "width": width, "height": height, "view": if view == ViewDir::Top { "top" } else { "iso" },
        "toolpath": toolpath, "stock_pixels": canvas.covered(), "png_bytes": png.len() });
    out[PNG_KEY] = json!(base64(&png));
    Ok(out)
}

fn paint(cls: f32) -> [u8; 3] {
    // Classes are 0..=3 exactly (`color_classes`); anything else reads
    // as unjudged, never as clean.
    let k = if cls == 0.0 { 0 } else if cls == 1.0 { 1 } else if cls == 2.0 { 2 } else { 3 };
    CLASS_PAINT[k]
}

fn bounds(pts: impl Iterator<Item = Point3>) -> Option<Value> {
    let mut b: Option<Aabb> = None;
    for p in pts.filter(|p| p.is_finite()) {
        b = Some(match b {
            None => Aabb { min: p, max: p },
            Some(o) => Aabb {
                min: Point3::new(o.min.x.min(p.x), o.min.y.min(p.y), o.min.z.min(p.z)),
                max: Point3::new(o.max.x.max(p.x), o.max.y.max(p.y), o.max.z.max(p.z)),
            },
        });
    }
    b.map(|b| json!({ "min": [fnum(b.min.x), fnum(b.min.y), fnum(b.min.z)], "max": [fnum(b.max.x), fnum(b.max.y), fnum(b.max.z)] }))
}

fn at(p: Option<Point3>) -> Value {
    p.map_or(Value::Null, |p| json!([fnum(p.x), fnum(p.y), fnum(p.z)]))
}

/// The numbers beside the picture: the deviation verdict's aggregates,
/// what stands (the drawn solid's bounds, skirt to the floor included)
/// and where the rest is seen from above (`top_bounds`: the sheet over
/// rest columns - the rest itself runs down from there by up to `max`).
fn summary(sim: &ProgramSim, tris: &[([Point3; 3], f32)]) -> Value {
    let s = &sim.surface;
    let d = &sim.dev;
    let rest_pts = s.tris.iter().filter(|t| class_of(s, t) == 1.0).flat_map(|t| t.iter().filter_map(|&i| s.verts.get(i as usize).copied()));
    json!({
        "clean": d.is_clean(),
        "cell": fnum(d.cell),
        "tolerance": fnum(d.tol),
        "judged_cells": d.dev.iter().filter(|v| v.is_finite()).count(),
        "rest": { "cells": d.rest_cells, "volume": fnum(d.rest_volume), "max": fnum(d.max_rest), "max_at": at(d.max_rest_at), "top_bounds": bounds(rest_pts) },
        "gouge": { "cells": d.gouge_cells, "volume": fnum(d.gouge_volume), "max": fnum(d.max_gouge), "max_at": at(d.max_gouge_at) },
        "stock_bounds": bounds(tris.iter().flat_map(|(t, _)| t.iter().copied())),
        "legend": { "clean": "blue-grey", "rest": "amber", "gouge": "red", "unjudged": "lilac" },
    })
}

fn class_of(s: &SimSurface, t: &[u32; 3]) -> f32 {
    s.vcls.get(t[0] as usize).copied().unwrap_or(3.0)
}

/// The sheet's triangles, clipped to the stock's floor, plus a skirt.
/// The column grid reaches below the block (a semi-infinite column), so
/// a column cut to or through the floor is air here: its top is dropped
/// and walls stop at the floor. The sheet stores column tops and the
/// steps between them, never the block's outer sides, so a horizontal
/// edge no other facet shares (a column beside air) is walled down to
/// the floor - a picture of a solid, not a floating lid. Shared edges
/// are matched on a lattice a hundredth of a cell fine, so float noise
/// in neighbouring corners still pairs. Without a stock box to read
/// (never expected - the sim needs one) the floor sits a cell under
/// the lowest top.
fn scene(s: &SimSurface, floor: Option<f64>, cell: f64) -> Vec<([Point3; 3], f32)> {
    let mut out: Vec<([Point3; 3], f32)> = Vec::with_capacity(s.tris.len() * 2);
    let vert = |i: u32| s.verts.get(i as usize).copied();
    let q = if cell.is_finite() && cell > 0.0 { cell * 0.01 } else { 1e-6 };
    let z_min = s.verts.iter().map(|p| p.z).filter(|z| z.is_finite()).fold(f64::INFINITY, f64::min);
    let Some(floor) = floor.filter(|f| f.is_finite()).or((z_min - q * 100.0).is_finite().then_some(z_min - q * 100.0))
    else {
        return out;
    };
    let key = |p: Point3| ((p.x / q).round() as i64, (p.y / q).round() as i64, (p.z / q).round() as i64);
    let ekey = |a: Point3, b: Point3| {
        let (ka, kb) = (key(a), key(b));
        if ka <= kb { (ka, kb) } else { (kb, ka) }
    };
    let clip = |p: Point3| Point3::new(p.x, p.y, p.z.max(floor));
    let mut uses: HashMap<_, u32> = HashMap::new();
    let mut flat: Vec<(Point3, Point3, f32)> = Vec::new();
    for t in &s.tris {
        let (Some(a), Some(b), Some(c)) = (vert(t[0]), vert(t[1]), vert(t[2])) else { continue };
        if a.z <= floor && b.z <= floor && c.z <= floor {
            continue; // cut to the floor or through it: air
        }
        let (a, b, c) = (clip(a), clip(b), clip(c));
        let cls = class_of(s, t);
        out.push(([a, b, c], cls));
        for (p, r) in [(a, b), (b, c), (c, a)] {
            *uses.entry(ekey(p, r)).or_insert(0) += 1;
        }
        if a.z == b.z && b.z == c.z {
            flat.extend([(a, b, cls), (b, c, cls), (c, a, cls)]);
        }
    }
    // The order of `flat` is the sheet's own: the skirt is as
    // deterministic as the sheet (the map is only counted, never walked).
    for (a, b, cls) in flat {
        if uses.get(&ekey(a, b)).copied() != Some(1) {
            continue;
        }
        let (fa, fb) = (Point3::new(a.x, a.y, floor), Point3::new(b.x, b.y, floor));
        out.push(([a, b, fb], cls));
        out.push(([a, fb, fa], cls));
    }
    out
}

/// Every generated operation's moves, in program order, as segments
/// coloured by kind. Arcs are walked as their XY chords with Z run
/// linearly across them (a display, not the post's arc).
fn overlay_moves(w: &flecs_ecs::prelude::World) -> Result<Vec<(Point3, Point3, [u8; 3])>, String> {
    use crate::cl::MoveKind;
    let planned = crate::stock_sim::plan_program(w).map_err(|e| format!("no toolpath to overlay: {e}"))?;
    let mut segs = Vec::new();
    for p in &planned {
        for m in &p.moves {
            let c = match m.kind {
                MoveKind::Rapid => RAPID,
                MoveKind::Feed => FEED,
                MoveKind::Plunge => PLUNGE,
            };
            if m.arc.is_none() {
                segs.push((m.from, m.to, c));
                continue;
            }
            let chords = m.chords(0.05);
            let n = chords.len().max(1) as f64;
            for (i, (u, v)) in chords.iter().enumerate() {
                let z = |k: f64| m.from.z + (m.to.z - m.from.z) * (k / n);
                segs.push((Point3::new(u.x, u.y, z(i as f64)), Point3::new(v.x, v.y, z(i as f64 + 1.0)), c));
            }
        }
    }
    Ok(segs)
}

/// Standard base64 (RFC 4648 section 4) with padding.
pub(super) fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for ch in bytes.chunks(3) {
        let b = [ch.first().copied().unwrap_or(0), ch.get(1).copied().unwrap_or(0), ch.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let sym = |shift: u32| char::from(A[((n >> shift) & 63) as usize]);
        s.push(sym(18));
        s.push(sym(12));
        s.push(if ch.len() > 1 { sym(6) } else { '=' });
        s.push(if ch.len() > 2 { sym(0) } else { '=' });
    }
    s
}
