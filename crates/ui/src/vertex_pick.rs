//! Vertex picking (M13.4): the feature corner nearest the cursor in
//! screen space, depth-tested so a corner hidden behind the part never
//! wins. Hover lights it, a click folds it into the open operation's
//! geometry list through the same command the face and edge picks queue.
//!
//! Provenance: standard screen-space point picking (project every
//! candidate through the view-projection, nearest within a pixel radius,
//! then an occlusion ray - from the eye, or in orthographic back along
//! the parallel view direction) - our own code from general graphics practice;
//! no vendor material consulted.

use camcraft_app::geometry::feature::{self, VertexId};
use camcraft_app::geometry::{Point3, transform_point};
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::{model, pick};

use crate::interact::{Mods, project_uv, queue_feature_pick};

use crate::state::{CommandKind, HoverHit, State};
use crate::viewport::{Camera, ProjectionMode};

/// How close a corner must draw to the cursor to be pickable, logical px.
pub(crate) const PICK_VERTEX_PX: f32 = 8.0;

/// The pure law: of the world-space `cands`, the index of the one whose
/// projection through `mvp` lands nearest `cursor` (logical px, inside the
/// image rect `rect`), within `radius` px, among those `visible` accepts.
/// Candidates are measured nearest-first and only the winners pay for the
/// visibility test - a hidden corner yields to the next-nearest visible
/// one, never to nothing. Ties go to the lower index (deterministic).
pub fn nearest_vertex(
    cands: &[[f64; 3]],
    mvp: &[[f32; 4]; 4],
    rect: ([f32; 2], [f32; 2]),
    cursor: [f32; 2],
    radius: f32,
    visible: impl Fn(usize) -> bool,
) -> Option<usize> {
    let ([x0, y0], [x1, y1]) = rect;
    let (w, h) = (x1 - x0, y1 - y0);
    if !(w > 1.0 && h > 1.0) || !radius.is_finite() || radius < 0.0 {
        return None;
    }
    let mut near: Vec<(f32, usize)> = cands
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let uv = project_uv(mvp, *c)?; // behind the camera: not on screen
            let (dx, dy) = (x0 + uv[0] * w - cursor[0], y0 + uv[1] * h - cursor[1]);
            let d2 = dx * dx + dy * dy;
            (d2.is_finite() && d2 <= radius * radius).then_some((d2, i))
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    near.into_iter().map(|(_, i)| i).find(|&i| visible(i))
}

/// Where the occlusion ray toward world point `q` starts: the eye in
/// perspective; in orthographic (#610) `q`'s own point on the eye plane,
/// so the ray runs back along the parallel view direction as
/// `Camera::ray_through` casts it. `None` for a point at or behind the
/// eye plane (nothing on screen to test) or degenerate math.
pub(crate) fn sight_origin(cam: &Camera, q: [f64; 3]) -> Option<[f64; 3]> {
    let eye = cam.eye();
    if cam.projection == ProjectionMode::Perspective {
        return Some(eye);
    }
    let (_, _, f) = cam.view_basis();
    let depth = (q[0] - eye[0]) * f.0 + (q[1] - eye[1]) * f.1 + (q[2] - eye[2]) * f.2;
    let o = [q[0] - f.0 * depth, q[1] - f.1 * depth, q[2] - f.2 * depth];
    (depth.is_finite() && depth > 1e-9 && o.iter().all(|v| v.is_finite())).then_some(o)
}

/// Depth tolerance for the occlusion ray: the facets meeting AT a corner
/// hit at its own distance; this absorbs their float noise.
fn depth_tol(dist: f64) -> f64 {
    (dist.abs() * 1e-4).max(1e-3)
}

/// The visible feature corner nearest a logical-px point, within
/// `radius`: (entity, vertex id). Every visible model's corners plus the
/// active setup's stock are candidates, in world space.
pub(crate) fn vertex_nearest(state: &State, p: [f32; 2], radius: f32) -> Option<(String, VertexId)> {
    let ([x0, y0], [x1, y1]) = state.viewport_rect;
    let (w, h) = (x1 - x0, y1 - y0);
    if w <= 1.0 || h <= 1.0 {
        return None;
    }
    let mvp = state.cam.mvp(f64::from(w / h));
    let hidden = |name: &str| state.hidden_in_view(name);
    let mut entities: Vec<(String, flecs_ecs::prelude::Entity)> =
        model::model_entities(&state.world).into_iter().filter(|(n, _)| !hidden(n)).collect();
    let stock_name = camcraft_app::setup::stock_entity_name(
        &state.world,
        camcraft_app::setup::active(&state.world).as_deref(),
    );
    if !state.active_stock_hidden()
        && let Some(stock) = model::find_by_name(&state.world, &stock_name)
    {
        entities.push((stock_name, stock));
    }
    // (entity, id, world position) per candidate, aligned with `pts`.
    let mut tags: Vec<(usize, VertexId)> = Vec::new();
    let mut pts: Vec<[f64; 3]> = Vec::new();
    for (k, (_, entity)) in entities.iter().enumerate() {
        let Some(features) = model::features_of(&state.world, entity) else {
            continue;
        };
        let loc = camcraft_app::setup_align::view_pose(&state.world, entity).unwrap_or_default(); // #326
        for vi in feature::corner_verts(&features) {
            let (Some(v), Some(id)) = (features.verts.get(vi as usize), feature::vertex_id(&features, vi)) else {
                continue;
            };
            let w = transform_point(Point3::new(v[0], v[1], v[2]), loc.pos, loc.rot);
            tags.push((k, id));
            pts.push([w.x, w.y, w.z]);
        }
    }
    let best = nearest_vertex(&pts, &mvp, state.viewport_rect, p, radius, |i| {
        let q = pts[i];
        let Some(o) = sight_origin(&state.cam, q) else { return false };
        let dist = ((q[0] - o[0]).powi(2) + (q[1] - o[1]).powi(2) + (q[2] - o[2]).powi(2)).sqrt();
        pick::point_visible(&state.world, o, q, depth_tol(dist), hidden)
    })?;
    let (k, id) = *tags.get(best)?;
    Some((entities.get(k)?.0.clone(), id))
}

/// The world position of a picked vertex, if it still resolves.
pub(crate) fn vertex_world(state: &State, entity: &str, id: VertexId) -> Option<[f64; 3]> {
    let found = model::find_by_name(&state.world, entity)?;
    let features = model::features_of(&state.world, &found)?;
    let vi = feature::resolve_vertex(&features, id)?;
    let v = features.verts.get(vi as usize)?;
    let loc = camcraft_app::setup_align::view_pose(&state.world, &found).unwrap_or_default(); // #326
    let w = transform_point(Point3::new(v[0], v[1], v[2]), loc.pos, loc.rot);
    Some([w.x, w.y, w.z])
}

/// Hover in Vertex mode: the corner under the cursor, or nothing.
pub(crate) fn hover_vertex(state: &mut State, p: [f32; 2]) {
    state.hover = vertex_nearest(state, p, PICK_VERTEX_PX).map(|(entity, id)| HoverHit {
        entity,
        facet: usize::MAX, // no facet drives this highlight
        feature: Some(FeatureSel::Vertex(id)),
    });
}

/// Click in Vertex mode: the corner joins the open operation's geometry
/// (plain replaces, Ctrl/Shift adds) - the face/edge pick's own command.
pub(crate) fn pick_vertex(state: &mut State, p: [f32; 2], mods: Mods) {
    hover_vertex(state, p);
    let Some(hit) = state.hover.clone() else {
        state.status = String::from("no vertex under the cursor");
        return;
    };
    let Some(sel) = hit.feature else {
        return;
    };
    let pick = GeoSel { entity: hit.entity, feature: sel };
    if crate::axis_pick::collecting(state) {
        crate::axis_pick::take(state, pick); // #557: a point of an axis
        return;
    }
    if crate::plane_pick::collecting(state) {
        crate::plane_pick::take(state, pick); // #554: a point sets no plane, refused by name
        return;
    }
    if crate::axis_pick::hint_stray(state) {
        return; // #557: the axes rows are open, nothing armed
    }
    if state.boundary_pick.is_some() {
        // A point bounds nothing: refused by name, never drafted.
        state.status = format!("{} of '{}' is a point - a boundary takes faces or edges", pick.feature, pick.entity);
        return;
    }
    queue_feature_pick(state, pick, mods.ctrl || mods.shift);
}

/// Queue the work origin onto a picked vertex: the WCS pane's angles, the vertex's world position, through the one `SetWcs` command
/// the anchor grid also queues (undoable like any origin move).
pub fn queue_origin_at_vertex(state: &mut State, entity: &str, id: VertexId) {
    let Some(pos) = vertex_world(state, entity, id) else {
        state.status = format!("the picked vertex of '{entity}' is no longer in the model");
        return;
    };
    state.status = format!("WCS origin set at a vertex of '{entity}'");
    state.pending.push_back(CommandKind::SetWcs {
        pos,
        roll_deg: state.wcs_ui.roll_deg,
        pitch_deg: state.wcs_ui.pitch_deg,
        yaw_deg: state.wcs_ui.yaw_deg,
    });
}

/// Queue the work origin onto a picked face (M13.4): its area-weighted
/// centroid, or - `z_only`, horizontal planar faces only - the pane's
/// current XY at the face's plane height. Same `SetWcs` command, same
/// angles as the vertex chip; the targets come from the app.
pub fn queue_origin_at_face(state: &mut State, entity: &str, sel: FeatureSel, z_only: bool) {
    let target = match pick::face_origin(&state.world, entity, sel) {
        Ok(t) => t,
        Err(why) => {
            state.status = format!("origin not set: {why}");
            return;
        }
    };
    let pos = if z_only {
        let Some(z) = target.plane_z else {
            state.status = String::from("origin not set: Z only needs a horizontal planar face");
            return;
        };
        // The WCS's placed origin is "the current XY"; an unplaced WCS
        // falls back to the pane's draft numbers.
        let active = camcraft_app::setup::active(&state.world);
        let placed = camcraft_app::setup::wcs_entity(&state.world, active.as_deref())
            .and_then(|e| model::transform_of(&state.world, &e));
        match placed {
            Some(loc) => [loc.pos.x, loc.pos.y, z],
            None => [state.wcs_ui.x, state.wcs_ui.y, z],
        }
    } else {
        target.centroid
    };
    state.status = format!("WCS origin set on a face of '{entity}'");
    state.pending.push_back(CommandKind::SetWcs {
        pos,
        roll_deg: state.wcs_ui.roll_deg,
        pitch_deg: state.wcs_ui.pitch_deg,
        yaw_deg: state.wcs_ui.yaw_deg,
    });
}

/// Queue the 3-2-1 alignment (#187) of the open operation's picks onto the
/// active setup's WCS: position and angles both from the app's frame, through
/// the same undoable `SetWcs` command as the origin chips.
pub fn queue_wcs_321(state: &mut State) {
    let a = match camcraft_app::pick::align_321(&state.world, &state.op_ui.geo) {
        Ok(a) => a,
        Err(why) => {
            state.status = format!("WCS not aligned: {why}");
            return;
        }
    };
    state.status = String::from("WCS aligned 3-2-1 to the picked face, edge and origin");
    state.pending.push_back(CommandKind::SetWcs {
        pos: a.pos,
        roll_deg: a.roll_deg,
        pitch_deg: a.pitch_deg,
        yaw_deg: a.yaw_deg,
    });
}

/// Screen-space marker for a vertex: a small diamond around its
/// projection, as viewport-uv segment pairs like `feature_segments`.
pub(crate) fn vertex_marker(
    mvp: &[[f32; 4]; 4],
    world: [f64; 3],
    aspect: f64,
    out: &mut Vec<([f32; 2], [f32; 2])>,
) {
    let Some(c) = project_uv(mvp, world) else {
        return;
    };
    let r = 0.008_f32; // of the image height
    let rx = r / (aspect.max(1e-3) as f32);
    let pts = [[c[0] + rx, c[1]], [c[0], c[1] + r], [c[0] - rx, c[1]], [c[0], c[1] - r]];
    for i in 0..4 {
        out.push((pts[i], pts[(i + 1) % 4]));
    }
}
