//! Tool bodies (#368): a holder or assembly mesh attached to a tool,
//! placed in the tool frame by numbers (the model direction that is the
//! tool axis, the model origin's height above the tip), and the reads
//! every judging law takes from it - the tool with its holder widened to
//! the cylinder enclosing the body, and the body's own band stack for
//! the fixture laws. Specced from the revolved-solid reading of a tool
//! assembly in machine-tool handbooks; the frame change is plain linear
//! algebra. Our own design; no vendor file or source consulted.
//!
//! The laws here:
//! - **No body, no change.** A tool without a [`ToolBody`] reads exactly
//!   its own record, so every output stays byte-identical.
//! - **Never smaller than the body.** Laws that read one holder cylinder
//!   get the cylinder enclosing the body ([`HolderProfile::enclose`]);
//!   the fixture laws, the ones that name a collision, read its bands.
//! - **Refusals by name**, at the command and again at use: a body that
//!   is empty, non-finite, degenerate, flat, off the axis, below the
//!   tip, too large, or reaching into the flutes.

use std::sync::Arc;

use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::{Entity, World};

use camcraft_tools::holder_profile::{HolderProfile, HolderStack, MAX_BODY_HEIGHT};

use crate::geometry::{Point3, Triangle};
use crate::schema::ToolBody;
use crate::tools::{Tool, ToolGeom, ToolKind};

/// The body on a tool entity, if it has one.
#[must_use]
pub fn body_of(world: &World, entity: &Entity) -> Option<ToolBody> {
    entity.entity_view(world).try_get::<&ToolBody>(|b| b.clone())
}

/// Land (`Some`) or clear (`None`) a tool's body - the one writer,
/// called by `SetToolBody` and the project loader.
pub fn set_body(world: &World, entity: &Entity, body: Option<&ToolBody>) {
    let view = entity.entity_view(world);
    match body {
        Some(b) => {
            view.set(b.clone());
        }
        None => {
            view.remove(world.component::<ToolBody>());
        }
    }
}

/// Place a model-frame mesh in the tool frame: `axis` (any non-zero
/// direction, model frame) becomes +Z through the model origin, and the
/// origin stands `offset` above the tip. Refuses a zero or non-finite
/// axis and a non-finite or absurd offset by name.
pub fn place(model: &[Triangle], axis: [f64; 3], offset: f64) -> Result<Vec<Triangle>, String> {
    let n = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    if !(n.is_finite() && n > 1e-12) {
        return Err("the tool axis direction must be a finite, non-zero vector".into());
    }
    if !(offset.is_finite() && offset.abs() <= MAX_BODY_HEIGHT) {
        return Err(format!("the tip offset must be a finite length within {MAX_BODY_HEIGHT} mm"));
    }
    let a = [axis[0] / n, axis[1] / n, axis[2] / n];
    // The perpendicular pair: +Z maps X to X and Y to Y (the identity
    // a vendor file modelled along Z expects); any other axis takes a
    // right-handed pair off the least-aligned helper.
    let helper = if a[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
    let u = unit(cross(helper, a)).ok_or("the tool axis direction must be a finite, non-zero vector")?;
    let v = cross(a, u);
    let map = |p: Point3| {
        let q = [p.x, p.y, p.z];
        Point3::new(dot(q, u), dot(q, v), dot(q, a) + offset)
    };
    Ok(model.iter().map(|t| Triangle::new(map(t.a), map(t.b), map(t.c))).collect())
}

/// A body built from placed facets: its envelope derived, refused by
/// name when the mesh is no tool body.
pub fn build(source: &str, axis: [f64; 3], offset: f64, placed: Vec<Triangle>) -> Result<ToolBody, String> {
    let tris: Vec<[[f64; 3]; 3]> = placed
        .iter()
        .map(|t| [[t.a.x, t.a.y, t.a.z], [t.b.x, t.b.y, t.b.z], [t.c.x, t.c.y, t.c.z]])
        .collect();
    let profile = HolderProfile::envelope(&tris).map_err(|e| e.to_string())?;
    Ok(ToolBody { source: source.to_owned(), axis, offset, facets: Arc::from(placed), profile })
}

/// Can `tool` carry `body`? The insert kinds are lathe-held (their
/// holders are #369's), and a body reaching into the flutes has no
/// cylinder stack that could stand it.
pub fn fits(tool: &Tool, body: &ToolBody) -> Result<(), String> {
    if matches!(tool.kind, ToolKind::Turning | ToolKind::Groove | ToolKind::Thread | ToolKind::Form) {
        return Err(format!("a {} takes no modelled body yet - lathe holders are a later slice", tool.kind));
    }
    if body.profile.enclose(tool).is_none() {
        return Err(format!(
            "the tool body reaches into the flutes (its foot is {:.3} mm above the tip, the flutes run {:.3} mm)",
            body.profile.bottom(),
            tool.flute_len
        ));
    }
    Ok(())
}

/// The tool as every judging law should read it, and the body behind
/// it: unchanged with no body; with one, the holder widened to the
/// cylinder enclosing the body. Refuses by name when the body no longer
/// fits the tool (a later edit shortened the shank under it).
pub fn judged(world: &World, entity: &Entity, tool: Tool) -> Result<(Tool, Option<HolderProfile>), String> {
    let Some(body) = body_of(world, entity) else {
        return Ok((tool, None));
    };
    fits(&tool, &body)?;
    let enclosed = body.profile.enclose(&tool).ok_or("the tool body reaches into the flutes")?;
    Ok((enclosed, Some(body.profile)))
}

/// What the fixture laws sweep above the shank of `geom`: the body's
/// bands, or the holder cylinder when there is no body.
#[must_use]
pub fn stack(geom: &ToolGeom, body: Option<&HolderProfile>) -> HolderStack {
    body.map_or_else(|| HolderStack::cylinder(geom), |p| p.stack(geom))
}

/// The body's term in an operation's stamp: nothing without one (no
/// digest moves), the envelope's bands with one - the only numbers a
/// law reads off it.
pub fn digest(world: &World, entity: &Entity, h: &mut impl std::hash::Hasher) {
    if let Some(b) = body_of(world, entity) {
        h.write(b"tool-body");
        for band in b.profile.bands() {
            for v in [band.above, band.len, band.r] {
                h.write(&v.to_bits().to_le_bytes());
            }
        }
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let n = dot(a, a).sqrt();
    (n.is_finite() && n > 1e-12).then(|| [a[0] / n, a[1] / n, a[2] / n])
}
