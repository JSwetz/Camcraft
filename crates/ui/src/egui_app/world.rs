//! The egui shell's demo document and per-frame scene collectors, graduated
//! from the M1.4.1 spike (world.rs) — the mirror law of app.rs's private
//! collectors.
//!
//! `seed` builds what the panels and viewport show without a file dialog:
//! a 60x60x30 block, fitted stock, a 6 mm end mill from the catalog, and one
//! Contour operation picked on the block's top face - the same command walk
//! `crates/ui/tests/operations.rs` proves, through the same `State` the
//! imgui app runs. Nothing here pokes world state ad hoc: every change is a
//! `CommandKind` through `State::drain_actions`, so the egui shell exercises
//! the real validation, undo and staleness machinery (AGENTS.md: everything
//! is a command - the migration must not become an exception). Since M-d the
//! window does NOT seed at startup - it opens on a plain `State::new()`,
//! exactly like imgui's `App::new`; the ribbon's New/Model/Project/Save are
//! the real file ops (rfd dialog, shared command machinery). This seed is
//! what the frame harnesses and the headless proof call directly.
//!
//! `collect` builds the world-space `View3D` buffers: parts here (models,
//! stock, region fills - the same walk app.rs does), overlay by delegation
//! to `crate::interact` since M-e, the single law both shells draw from.
//! The gates are copied line for line, so a plane the imgui shell would
//! refuse to draw is a plane this shell does not draw either.
//! Provenance: our own code, mirrored from app.rs.

use hsmworks_app::commands::ImportModel;
use hsmworks_app::geometry::{Point3, cube, feature, transform_point};
use hsmworks_app::machining2d::{Lead, Side};
use hsmworks_app::model;
use hsmworks_app::operation::{FeatureSel, GeoSel, Height, HeightReference, OpParams, SrcRef};
use hsmworks_app::schema::{STOCK_NAME, WCS_SLOTS};
use hsmworks_app::tools::{Tool, catalog};

use crate::interact::ftoz;
use crate::state::{CommandKind, State, UiTarget};
use crate::viewport::GpuPart;

/// The shell's whole scene for one frame: what `View3D::set_document` and
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
    for (z, region) in crate::interact::shown_regions(state) {
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

/// The overlay is the SHARED law since M-e: `crate::interact::collect_overlay`
/// — cached paths with chevrons, region outlines, draft height planes,
/// committed-pick outlines, and the gizmos, built by the same functions the
/// imgui shell uploads. (The pre-M-e mirror skipped the gizmos and the
/// `show_all_paths` dimming; this delegation closes both gaps for free.)
fn collect_overlay(state: &State) -> Vec<[f32; 6]> {
    crate::interact::collect_overlay(state)
}
