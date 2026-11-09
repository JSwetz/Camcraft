//! Tool-body editing and display reads (#371): what the tool editor and
//! the simulation need from a body without committing anything - the
//! file read, the six axis presets, the placement preview (the same
//! place-and-envelope `SetToolBody` runs), the tip/axis picks on the
//! model-frame mesh, and the body posed at a tool position for drawing.
//! Specced from plain linear algebra (Rodrigues' rotation formula, the
//! ray/triangle crossing `pick::soup_hit` already uses); our own design,
//! no vendor file or source consulted.
//!
//! Nothing here writes the world: picks and previews are ephemeral, and
//! the one writer stays `SetToolBody` (AGENTS.md: everything is a
//! command).

use std::path::Path;

use flecs_ecs::prelude::World;

use crate::geometry::{Point3, Triangle};
use crate::model;
use crate::schema::ToolBody;
use crate::tool_body;

/// The six model directions the editor offers as the tool axis.
pub const AXIS_CHOICES: [(&str, [f64; 3]); 6] = [
    ("+Z", [0.0, 0.0, 1.0]),
    ("-Z", [0.0, 0.0, -1.0]),
    ("+X", [1.0, 0.0, 0.0]),
    ("-X", [-1.0, 0.0, 0.0]),
    ("+Y", [0.0, 1.0, 0.0]),
    ("-Y", [0.0, -1.0, 0.0]),
];

/// The preset name of `axis`, if it is one of [`AXIS_CHOICES`] (within
/// float dust after normalizing).
#[must_use]
pub fn axis_label(axis: [f64; 3]) -> Option<&'static str> {
    let a = unit(axis)?;
    AXIS_CHOICES.iter().find(|(_, d)| dot(a, *d) > 1.0 - 1e-9).map(|(n, _)| *n)
}

/// Read a STEP / STL / OBJ for a tool body: its file name and its
/// model-frame facets. Refuses an unreadable or empty file by name.
pub fn load(path: &Path) -> Result<(String, Vec<Triangle>), String> {
    let mesh = crate::geometry::import::load_model(path).map_err(|e| e.to_string())?;
    if mesh.facets.is_empty() {
        return Err("the file holds no facets".into());
    }
    let source = path.file_name().map_or_else(|| path.display().to_string(), |f| f.to_string_lossy().into_owned());
    Ok((source, mesh.facets.to_vec()))
}

/// The placement as `SetToolBody` would judge it, without landing it:
/// the body placed in the tool frame with its envelope, or the refusal
/// by name. (The tool-fit law is the command's; this sees the mesh.)
pub fn preview(model: &[Triangle], axis: [f64; 3], offset: f64) -> Result<ToolBody, String> {
    let placed = tool_body::place(model, axis, offset)?;
    tool_body::build("preview", axis, offset, placed)
}

/// What a click on the model-frame body found: the facet, the hit
/// point, the facet's corner nearest the hit, and its unit normal (`None`
/// for a zero-area facet).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyPick {
    pub facet: usize,
    pub hit: [f64; 3],
    pub vertex: [f64; 3],
    pub normal: Option<[f64; 3]>,
}

/// Cast `origin + t*dir` at the body's facets (model frame).
#[must_use]
pub fn pick(model: &[Triangle], origin: [f64; 3], dir: [f64; 3]) -> Option<BodyPick> {
    let (facet, hit) = crate::pick::soup_hit(model, origin, dir)?;
    let t = model.get(facet)?;
    let corners = [t.a, t.b, t.c].map(|p| [p.x, p.y, p.z]);
    let d2 = |q: [f64; 3]| {
        let e = sub(q, hit);
        dot(e, e)
    };
    let vertex = corners.into_iter().min_by(|a, b| d2(*a).total_cmp(&d2(*b)))?;
    let normal = unit(cross(sub(corners[1], corners[0]), sub(corners[2], corners[0])));
    Some(BodyPick { facet, hit, vertex, normal })
}

/// The tip offset that puts `point` (model frame) at the tool tip along
/// `axis`: the placement maps height as `dot(q, axis) + offset`, so the
/// tip (height 0) at `point` is `-dot(point, axis)`.
#[must_use]
pub fn tip_offset_at(point: [f64; 3], axis: [f64; 3]) -> Option<f64> {
    let a = unit(axis)?;
    let o = -dot(point, a);
    o.is_finite().then_some(o)
}

/// Tool-frame facets (tip at the origin, +Z to the spindle) posed at a
/// tool position: `tip` in world, `axis` the unit tip-to-spindle
/// direction (`sim::Frame::axis`). +Z maps onto `axis` by the shortest
/// rotation; the turn about the axis is free (the body is judged by its
/// revolved envelope), and a vertical tool takes the identity.
#[must_use]
pub fn posed(facets: &[Triangle], tip: [f64; 3], axis: [f64; 3]) -> Vec<Triangle> {
    let a = unit(axis).unwrap_or([0.0, 0.0, 1.0]);
    let (c, k) = (a[2], cross([0.0, 0.0, 1.0], a));
    let s = dot(k, k).sqrt();
    let rot = |v: [f64; 3]| -> [f64; 3] {
        if s < 1e-12 {
            // Parallel: identity, or a half turn about X when flipped.
            return if c >= 0.0 { v } else { [v[0], -v[1], -v[2]] };
        }
        // Rodrigues about the unit k by the angle whose cos is c, sin s.
        let k = [k[0] / s, k[1] / s, k[2] / s];
        let kv = cross(k, v);
        let kd = dot(k, v) * (1.0 - c);
        [v[0] * c + kv[0] * s + k[0] * kd, v[1] * c + kv[1] * s + k[1] * kd, v[2] * c + kv[2] * s + k[2] * kd]
    };
    let map = |p: Point3| {
        let r = rot([p.x, p.y, p.z]);
        Point3::new(r[0] + tip[0], r[1] + tip[1], r[2] + tip[2])
    };
    facets.iter().map(|t| Triangle::new(map(t.a), map(t.b), map(t.c))).collect()
}

/// The body on the tool operation `op` runs, if that tool has one.
#[must_use]
pub fn body_for_operation(world: &World, op: &str) -> Option<ToolBody> {
    let e = model::find_by_name(world, op)?;
    let tool = model::operation_of(world, &e)?.tool;
    let te = model::find_by_name(world, &tool)?;
    tool_body::body_of(world, &te)
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let n = dot(a, a).sqrt();
    (n.is_finite() && n > 1e-12).then(|| [a[0] / n, a[1] / n, a[2] / n])
}
