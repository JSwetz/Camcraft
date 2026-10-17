//! S2: the demo document and the per-frame scene collectors.
//!
//! `seed` builds what the panels and viewport show without a file dialog:
//! a 60x60x30 block, fitted stock, a 6 mm end mill from the catalog, and one
//! Contour operation picked on the block's top face - the same command walk
//! `crates/ui/tests/operations.rs` proves, through the same `State` the
//! imgui app runs. Nothing here pokes world state ad hoc: every change is a
//! `CommandKind` through `State::drain_actions`, so the spike panels exercise
//! the real validation, undo and staleness machinery (AGENTS.md: everything
//! is a command - the spike must not become an exception).
//!
//! `collect` mirrors app.rs's private collectors (collect_parts, the region
//! outlines, the draft height planes, committed-pick outlines, cached-path
//! display) as world-space `View3D` buffers. The mirrors are slim - no hover
//! highlight, no screen-space bake - but the gates are copied line for line,
//! so a plane the imgui shell would refuse to draw is a plane the spike
//! does not draw either. Provenance: our own code, mirrored from app.rs.

use hsmworks_app::commands::ImportModel;
use hsmworks_app::geometry::{Point3, cube, feature, transform_point};
use hsmworks_app::machining2d::{Lead, Side};
use hsmworks_app::model;
use hsmworks_app::operation::{self, FeatureSel, GeoSel, Height, HeightReference, OpParams, SrcRef};
use hsmworks_app::schema::{STOCK_NAME, WCS_SLOTS};
use hsmworks_app::tools::{Tool, catalog};
use hsmworks_ui::state::{CommandKind, State, UiTarget};
use hsmworks_ui::viewport::GpuPart;

/// The spike's whole scene for one frame: what `View3D::set_document` and
/// `set_overlay` take.
pub struct Scene {
    pub parts: Vec<GpuPart>,
    pub overlay: Vec<[f32; 6]>,
}

impl Scene {
    /// Content digest for the upload gate: the scene is rebuilt every frame
    /// (it must track camera-independent world truth), but re-uploading
    /// unchanged vertex buffers is traffic for nothing. `GpuPart` derives
    /// nothing, so the digest walks the floats' bits - exact, no hash-skein.
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        let mut eat = |v: u64| {
            h ^= v;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        };
        for p in &self.parts {
            eat(u64::from(p.selected) | (u64::from(p.stock) << 1) | (u64::from(p.fill) << 2));
            eat(p.verts.len() as u64);
            for v in &p.verts {
                for f in v {
                    eat(u64::from(f.to_bits()));
                }
            }
        }
        eat(!0);
        eat(self.overlay.len() as u64);
        for e in &self.overlay {
            for f in e {
                eat(u64::from(f.to_bits()));
            }
        }
        h
    }
}

/// Build the demo document: block, fitted stock, catalog tool, one Contour
/// op on the top face, selected and generated, with the Properties pane
/// open on it. Idempotent by construction - call once after `State::new`.
pub fn seed(state: &mut State) {
    state.history.execute(
        &state.world,
        ImportModel::from_mesh("Block", cube(Point3::ZERO, Point3::new(60.0, 60.0, 30.0))),
    );
    let Some(tool) = catalog_tool("EM-0600-4FL") else {
        state.status = String::from("seed: catalog has no EM-0600-4FL - panels show an empty doc");
        return;
    };
    let Some(top) = top_face_pick(state, "Block") else {
        state.status = String::from("seed: the block has no top face - panels show an empty doc");
        return;
    };
    state.pending.push_back(CommandKind::StockFit { margin: 2.0, shape: 0 });
    state.pending.push_back(CommandKind::AddTool { name: "EM-0600-4FL".into(), tool });
    // The seed's Contour bottoms out at work offset G54 (hand-test 4's
    // fixture law), so G54 must exist before generate or the command is
    // refused by name. The block sits z 0..30; the offset at the world
    // origin puts Cut Z at the model bottom.
    state.pending.push_back(CommandKind::SetWcs {
        slot: WCS_SLOTS[0].to_string(),
        pos: [0.0, 0.0, 0.0],
        roll_deg: 0.0,
        pitch_deg: 0.0,
        yaw_deg: 0.0,
    });
    state.pending.push_back(CommandKind::CreateOperation {
        name: "Contour1".into(),
        tool: "EM-0600-4FL".into(),
        src: SrcRef::Selection(vec![top]),
        params: contour_params(),
    });
    // Generate through the command machinery, not a raw command: a refusal
    // would then land in the status strip exactly where a user would see it
    // (an ungenerated seed still shows every panel mechanism, minus the
    // green path - the honest degraded case).
    state.pending.push_back(CommandKind::GenerateOperation("Contour1".into()));
    state.pending.push_back(CommandKind::Select(Some("Contour1".into())));
    state.settle();
    state.ui_target = UiTarget::Operation("Contour1".into());
    state.sync_ops_ui();
}

fn catalog_tool(name: &str) -> Option<Tool> {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t)
}

/// The top horizontal planar face as a pick (the test helper's law: face
/// normals are sign-canonical, so the signed offset alone tells top from
/// bottom).
fn top_face_pick(state: &State, name: &str) -> Option<GeoSel> {
    let entity = model::find_by_name(&state.world, name)?;
    let f = model::features_of(&state.world, &entity)?;
    let mut hs: Vec<_> = f
        .faces
        .iter()
        .filter(|fa| matches!(fa.kind, feature::FaceKind::Planar) && fa.normal[2].abs() > 0.9)
        .collect();
    hs.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    let fa = hs.pop()?;
    Some(GeoSel { entity: name.to_string(), feature: FeatureSel::Face(fa.id) })
}

/// The seed's Contour draft - the numbers hand-test 4 exercises: clearance
/// anchored 25 mm above the stock top, retract above it, the full model
/// height stepped 5, one work-offset bottom, a straight lead-in.
fn contour_params() -> OpParams {
    OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
        retract_z: Height::of(HeightReference::TopOfStock, 10.0),
        top_z: Height::of(HeightReference::TopOfModel, 0.0),
        bottom_z: Height::of(HeightReference::WorkOffset(0), 0.0),
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::Linear { length: 8.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
    }
}

/// Rebuild the frame's scene from the world. Mirrors `App::sync_gpu`'s
/// per-frame work: parts from the document, overlay from selection and
/// draft. Returns BOTH and lets the caller's digest gate decide what to
/// upload.
#[must_use]
pub fn collect(state: &State) -> Scene {
    Scene { parts: collect_parts(state), overlay: collect_overlay(state) }
}

// ---------------------------------------------------------------- parts

fn ftoz(v: f64) -> f32 {
    if v.is_finite() { v as f32 } else { 0.0 }
}

fn world_verts(state: &State, entity: &flecs_ecs::prelude::Entity) -> Vec<[f32; 3]> {
    let tris = model::facets_of(&state.world, entity).unwrap_or_default();
    let loc = model::transform_of(&state.world, entity).unwrap_or_default();
    let mut verts = Vec::with_capacity(tris.len() * 3);
    for t in tris {
        for p in [t.a, t.b, t.c] {
            let p = transform_point(p, loc.pos, loc.rot);
            verts.push([ftoz(p.x), ftoz(p.y), ftoz(p.z)]);
        }
    }
    verts
}

/// Models (minus browser-eye-hidden), then the stock solid, then the
/// translucent cut-region fills - `App::collect_parts`'s exact law, fills
/// included so the region reads as an area, not only an outline.
fn collect_parts(state: &State) -> Vec<GpuPart> {
    let mut parts: Vec<GpuPart> = model::model_entities(&state.world)
        .into_iter()
        .filter(|(name, _)| !state.hidden_models.iter().any(|h| h == name))
        .map(|(_, entity)| GpuPart {
            verts: world_verts(state, &entity),
            selected: model::is_selected(&state.world, &entity),
            stock: false,
            fill: false,
        })
        .collect();
    if let Some(entity) = model::find_by_name(&state.world, STOCK_NAME)
        && model::is_stock(&state.world, &entity)
    {
        parts.push(GpuPart {
            verts: world_verts(state, &entity),
            selected: model::is_selected(&state.world, &entity),
            stock: true,
            fill: false,
        });
    }
    for (z, region) in shown_regions(state) {
        let Some(region) = region else { continue };
        let Ok(tris) = region.triangles() else { continue };
        if tris.is_empty() {
            continue;
        }
        let mut verts = Vec::with_capacity(tris.len() * 3);
        for t in &tris {
            for p in t {
                verts.push([ftoz(p.x), ftoz(p.y), ftoz(z)]);
            }
        }
        parts.push(GpuPart { verts, selected: false, stock: false, fill: true });
    }
    parts
}

// -------------------------------------------------------------- overlay

/// `App::push_line`'s law, verbatim: two `[f32;6]` rows (xyz, rgb).
fn push_line(out: &mut Vec<[f32; 6]>, a: Point3, b: Point3, c: [f32; 3]) {
    out.push([ftoz(a.x), ftoz(a.y), ftoz(a.z), c[0], c[1], c[2]]);
    out.push([ftoz(b.x), ftoz(b.y), ftoz(b.z), c[0], c[1], c[2]]);
}

fn collect_overlay(state: &State) -> Vec<[f32; 6]> {
    let mut out = Vec::new();
    out.extend(toolpath_lines(state));
    out.extend(region_lines(state));
    out.extend(height_plane_lines(state));
    out.extend(selection_feature_lines(state));
    out
}

const FEED_RGB: [f32; 3] = [0.20, 0.85, 0.35];
const RAPID_RGB: [f32; 3] = [0.30, 0.50, 0.95];
const PLUNGE_RGB: [f32; 3] = [0.92, 0.30, 0.25];
const REGION_RGB: [f32; 3] = [0.95, 0.58, 0.15];
const CLEARANCE_RGB: [f32; 3] = [0.55, 0.60, 0.68];
const RETRACT_RGB: [f32; 3] = [0.40, 0.70, 0.95];
const CUT_RGB: [f32; 3] = [0.90, 0.35, 0.30];
const PICK_RGB: [f32; 3] = [0.72, 0.44, 0.86];
const DISPLAY_CHORD_TOL: f64 = 0.02;

/// Cached paths of the SELECTED operations, the cache itself drawn (the
/// imgui law: staleness is the panel's word, the cache is the picture).
/// Chevrons and the vertex cap are the display honesty parts: a feed line
/// says which way the tool ran, and a pathological cache truncates loudly.
fn toolpath_lines(state: &State) -> Vec<[f32; 6]> {
    const MAX_OVERLAY_VERTICES: usize = 1 << 20;
    const CHEVRON_SPACING: f64 = 10.0;
    let mut out = Vec::new();
    let arm = chevron_arm(state);
    for (name, entity) in model::named_entities(&state.world) {
        if state.hidden_paths.contains(&name) {
            continue;
        }
        let Some(cache) = model::toolpath_of(&state.world, &entity) else { continue };
        if !model::is_selected(&state.world, &entity) {
            continue;
        }
        for m in &cache.path.moves {
            let c = match m.kind {
                hsmworks_app::cl::MoveKind::Rapid => RAPID_RGB,
                hsmworks_app::cl::MoveKind::Feed => FEED_RGB,
                hsmworks_app::cl::MoveKind::Plunge => PLUNGE_RGB,
            };
            if m.arc.is_some() {
                let cs = m.chords(DISPLAY_CHORD_TOL);
                if out.len() + 2 * cs.len() + 2 > MAX_OVERLAY_VERTICES {
                    eprintln!("overlay: the cached paths outgrew the display budget; truncated");
                    out.truncate(out.len() & !1);
                    return out;
                }
                for (a, b) in cs {
                    let za = m.from.z;
                    push_line(&mut out, Point3::new(a.x, a.y, za), Point3::new(b.x, b.y, za), c);
                }
                continue;
            }
            if out.len() + 10 > MAX_OVERLAY_VERTICES {
                eprintln!("overlay: the cached paths outgrew the display budget; truncated");
                out.truncate(out.len() & !1);
                return out;
            }
            push_line(&mut out, m.from, m.to, c);
            if m.kind == hsmworks_app::cl::MoveKind::Feed {
                chevrons(&mut out, m.from, m.to, c, arm, CHEVRON_SPACING);
            }
        }
    }
    out
}

fn chevron_arm(state: &State) -> f64 {
    let extent = model::solid_world_aabb(&state.world)
        .map(|b| (b.max.x - b.min.x).max(b.max.y - b.min.y).max(b.max.z - b.min.z))
        .unwrap_or(0.0);
    (extent * 0.01).max(0.6)
}

fn chevrons(
    out: &mut Vec<[f32; 6]>,
    from: Point3,
    to: Point3,
    c: [f32; 3],
    arm: f64,
    spacing: f64,
) {
    let d = to - from;
    let len2 = d.dot(d);
    let min2 = (2.0 * arm) * (2.0 * arm);
    if !len2.is_finite() || len2 <= min2 {
        return;
    }
    let len = len2.sqrt();
    let dir = Point3::new(d.x / len, d.y / len, d.z / len);
    let refv =
        if dir.z.abs() > 0.99 { Point3::new(1.0, 0.0, 0.0) } else { Point3::new(0.0, 0.0, 1.0) };
    let n0 = refv.cross(dir);
    let n2 = n0.dot(n0);
    if !n2.is_finite() || n2 <= f64::EPSILON {
        return;
    }
    let nl = n2.sqrt();
    let n = Point3::new(n0.x / nl, n0.y / nl, n0.z / nl);
    let count = ((len / spacing).floor().max(1.0) as usize).min(10_000);
    let (back, spread) = (arm * 0.8, arm * 0.45);
    for i in 0..count {
        let t = len * (f64::from(i as u32) + 0.5) / count as f64;
        let q = Point3::new(from.x + dir.x * t, from.y + dir.y * t, from.z + dir.z * t);
        for s in [-1.0, 1.0] {
            let tip = Point3::new(
                q.x - dir.x * back + n.x * spread * s,
                q.y - dir.y * back + n.y * spread * s,
                q.z - dir.z * back + n.z * spread * s,
            );
            push_line(out, q, tip, c);
        }
    }
}

/// The cut regions of the selected operations (outline source and the fill
/// gate share this walk - `App::shown_regions` verbatim).
fn shown_regions(state: &State) -> Vec<(f64, Option<hsmworks_app::geo2d::Region>)> {
    let mut out = Vec::new();
    for (_, entity) in model::named_entities(&state.world) {
        if !model::is_operation(&state.world, &entity)
            || !model::is_selected(&state.world, &entity)
        {
            continue;
        }
        let Some(op) = model::operation_of(&state.world, &entity) else { continue };
        let region = operation::region_of(&state.world, &op);
        if let Some(z) = operation::region_z(&state.world, &op.params)
            && (region.is_some() || !operation::chains_of(&state.world, &op).is_empty())
        {
            out.push((z, region));
        }
    }
    out
}

/// Region outlines + open centerline runs, orange, at the region level
/// (`App::region_lines` verbatim).
fn region_lines(state: &State) -> Vec<[f32; 6]> {
    let mut out = Vec::new();
    for (_, entity) in model::named_entities(&state.world) {
        if !model::is_operation(&state.world, &entity)
            || !model::is_selected(&state.world, &entity)
        {
            continue;
        }
        let Some(op) = model::operation_of(&state.world, &entity) else { continue };
        let Some(z) = operation::region_z(&state.world, &op.params) else {
            continue; // source gone - never an outline at a guessed level
        };
        if let Some(region) = operation::region_of(&state.world, &op) {
            for l in region.loops() {
                let pts = l.points();
                if pts.len() < 3 {
                    continue;
                }
                for i in 0..pts.len() {
                    let a = Point3::new(pts[i].x, pts[i].y, z);
                    let q = pts[(i + 1) % pts.len()];
                    push_line(&mut out, a, Point3::new(q.x, q.y, z), REGION_RGB);
                }
            }
        }
        for run in operation::chains_of(&state.world, &op) {
            for w in run.windows(2) {
                let a = Point3::new(w[0].x, w[0].y, z);
                let b = Point3::new(w[1].x, w[1].y, z);
                push_line(&mut out, a, b, REGION_RGB);
            }
        }
    }
    out
}

/// The DRAFT parameters' height planes as rectangles around the stock
/// (`App::height_plane_lines` + `height_planes` + `plane_rect` laws, the
/// rectangle corners in the same order so a spike eye matches the shell's).
fn height_plane_lines(state: &State) -> Vec<[f32; 6]> {
    let Some(rect) = plane_rect(state) else { return Vec::new() };
    let levels: Vec<f64> = shown_regions(state).into_iter().map(|(z, _)| z).collect();
    let stands = |z: f64| levels.contains(&z);
    let mut planes: Vec<(f64, [f32; 3], bool)> = Vec::new();
    if !state.op_ui.name.is_empty()
        && model::find_by_name(&state.world, &state.op_ui.name)
            .and_then(|e| model::operation_of(&state.world, &e))
            .is_some()
    {
        let world = &state.world;
        let draft = &state.op_ui.params;
        let clr = draft.clearance();
        if let Some(z) = clr.z(world) {
            planes.push((z, CLEARANCE_RGB, false));
        }
        let hop = match draft {
            OpParams::Contour { retract_z, .. } => Some(*retract_z),
            OpParams::Pocket { retract_z, .. }
            | OpParams::Slot { retract_z, .. }
            | OpParams::Trace { retract_z, .. } => *retract_z,
            OpParams::Face { .. } => None,
        };
        if let Some(hop) = hop
            && let (Some(r), Some(c)) = (hop.z(world), draft.clearance().z(world))
        {
            planes.push((r.min(c), RETRACT_RGB, false));
        }
        match draft {
            OpParams::Face { top_z, cut_z, .. } => {
                let cut = cut_z.z(world);
                if let (Some(t), Some(c)) = ((*top_z).to_owned(), cut)
                    && let Some(tz) = t.z(world)
                    && tz != c
                {
                    planes.push((tz, REGION_RGB, stands(tz)));
                }
                if let Some(z) = cut {
                    planes.push((z, CUT_RGB, stands(z)));
                }
            }
            OpParams::Contour { top_z, bottom_z, .. }
            | OpParams::Pocket { top_z, bottom_z, .. }
            | OpParams::Slot { top_z, bottom_z, .. }
            | OpParams::Trace { top_z, bottom_z, .. } => {
                if let Some(z) = top_z.z(world) {
                    planes.push((z, REGION_RGB, stands(z)));
                }
                if let Some(z) = bottom_z.z(world) {
                    planes.push((z, CUT_RGB, false));
                }
            }
        }
    }
    planes.retain(|(z, _, _)| z.is_finite());
    let [x0, y0, x1, y1] = rect;
    let ring = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    let mut out = Vec::new();
    for (z, rgb, _skip) in planes.iter().filter(|(_, _, skip)| !skip) {
        for i in 0..4 {
            let a = ring[i];
            let b = ring[(i + 1) % 4];
            push_line(
                &mut out,
                Point3::new(a[0], a[1], *z),
                Point3::new(b[0], b[1], *z),
                *rgb,
            );
        }
    }
    out
}

fn plane_rect(state: &State) -> Option<[f64; 4]> {
    let b = model::stock_world_aabb(&state.world).or_else(|| model::solid_world_aabb(&state.world))?;
    let pad = model::find_by_name(&state.world, &state.op_ui.tool)
        .and_then(|e| model::tool_of(&state.world, &e))
        .map(|t| t.diameter)
        .filter(|d| d.is_finite() && *d > 0.0)
        .map_or(0.0, |d| d * 0.5 + 2.0);
    let rect = [b.min.x - pad, b.min.y - pad, b.max.x + pad, b.max.y + pad];
    rect.iter().all(|v| v.is_finite()).then_some(rect)
}

/// The open operation's committed picks in 3D, violet (`App`'s law, with
/// the `Loop.verts` id resolution the hover path also uses).
fn selection_feature_lines(state: &State) -> Vec<[f32; 6]> {
    if state.op_ui.name.is_empty() {
        return Vec::new();
    }
    let Some(op) = model::find_by_name(&state.world, &state.op_ui.name)
        .and_then(|e| model::operation_of(&state.world, &e))
    else {
        return Vec::new();
    };
    let operation::SrcRef::Selection(items) = &op.src else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in items {
        if state.hidden_models.iter().any(|m| m == &item.entity) {
            continue;
        }
        let Some(entity) = model::find_by_name(&state.world, &item.entity) else { continue };
        let Some(features) = model::features_of(&state.world, &entity) else { continue };
        let loc = model::transform_of(&state.world, &entity).unwrap_or_default();
        let draw = |out: &mut Vec<[f32; 6]>, pts: &[[f64; 3]], closed: bool| {
            let n = pts.len();
            if n < 2 || (closed && n < 3) {
                return;
            }
            let last = if closed { n } else { n - 1 };
            for i in 0..last {
                let a =
                    transform_point(Point3::new(pts[i][0], pts[i][1], pts[i][2]), loc.pos, loc.rot);
                let q = &pts[(i + 1) % n];
                let b = transform_point(Point3::new(q[0], q[1], q[2]), loc.pos, loc.rot);
                push_line(out, a, b, PICK_RGB);
            }
        };
        match item.feature {
            FeatureSel::Face(fid) => {
                let Some(face) = features.faces.iter().find(|f| f.id == fid) else { continue };
                for l in &face.loops {
                    let pts: Vec<[f64; 3]> = l
                        .verts
                        .iter()
                        .filter_map(|&vi| features.verts.get(vi as usize).copied())
                        .collect();
                    draw(&mut out, &pts, !matches!(l.kind, feature::LoopKind::Open));
                }
            }
            FeatureSel::Chain(cid) => {
                let Some(chain) = feature::chains(&features).into_iter().find(|c| c.id == cid)
                else {
                    continue;
                };
                let pts = feature::chain_points(&features, &chain);
                draw(&mut out, &pts, chain.closed);
            }
            FeatureSel::Patch(pid) => {
                let Some(rings) = feature::patch_rings(&features, pid) else { continue };
                for r in &rings {
                    let pts: Vec<[f64; 3]> = r
                        .verts
                        .iter()
                        .filter_map(|&vi| features.verts.get(vi as usize).copied())
                        .collect();
                    draw(&mut out, &pts, r.closed);
                }
            }
        }
    }
    out
}

