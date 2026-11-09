//! The tool editor's body draft (#371): an imported holder mesh in its
//! own model frame, the numeric placement (axis preset or picked face
//! normal, tip offset), the pick mode, and the one command Apply body
//! queues. The draft is ephemeral pane state like `ToolUi`: picks and
//! previews only fill it; `SetToolBody` (one undo step) is the only
//! thing that reaches the world, and its refusal lands on the Tool pane
//! by name. Provenance: our own pane state over `app::tool_body_view`;
//! no vendor material consulted.

use std::path::Path;
use std::sync::Arc;

use camcraft_app::commands::{SetToolBody, ToolBodySpec};
use camcraft_app::geometry::Triangle;
use camcraft_app::schema::ToolBody;
use camcraft_app::tool_body_view::{self, BodyPick};
use camcraft_app::{model, tool_body};

use super::{CommandKind, ErrorScope, State};

/// What the next click on the editor's body preview does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BodyPickMode {
    /// Drag orbits; clicks do nothing.
    #[default]
    Off,
    /// The click's nearest facet corner becomes the tool tip.
    Tip,
    /// The clicked facet's outward normal becomes the tool axis.
    Axis,
}

/// The editor's body draft for the tool it is open on.
#[derive(Clone, Default)]
pub struct ToolBodyUi {
    /// The tool the draft belongs to (the editor's `tool_ui.name`).
    pub tool: String,
    /// Where the mesh came from, for display.
    pub source: String,
    /// The mesh in its own frame: as imported, or a landed body's
    /// tool-frame facets (tip at the origin, +Z up) when re-edited.
    pub facets: Arc<[Triangle]>,
    /// The model direction that is the tool axis (tip to spindle).
    pub axis: [f64; 3],
    /// The model origin's height above the tip, along `axis`.
    pub offset: f64,
    pub pick: BodyPickMode,
    /// The preview's orbit, radians off its default view.
    pub yaw: f32,
    pub pitch: f32,
    /// The preview's zoom, log2 of the magnification over the fit view
    /// (0 = fit), clamped to [`ZOOM_MIN`, `ZOOM_MAX`].
    pub zoom: f32,
    /// The view centre's shift off the fit centre, view units (right,
    /// up): what keeps the point under the cursor fixed while zooming.
    pub pan: [f64; 2],
    /// The world body the draft last mirrored (identity, not value):
    /// a change there (Apply, Undo, Redo, Open) re-mirrors.
    seen: Option<Arc<[Triangle]>>,
    /// Placement preview, cached against the (mesh, axis, offset) it read.
    preview: Option<(PreviewKey, Result<ToolBody, String>)>,
}

type PreviewKey = (usize, [u64; 4]);

/// The preview zoom's bounds, log2: a quarter of the fit view out to
/// 32x in - far enough to pick a corner on a fine holder, never so far
/// a wheel spin loses the body.
pub const ZOOM_MIN: f32 = -2.0;
pub const ZOOM_MAX: f32 = 5.0;

/// The zoom law: `z` moved by `by` (log2), clamped; non-finite input
/// leaves it where it was.
#[must_use]
pub fn clamp_zoom(z: f32, by: f32) -> f32 {
    let next = z + by;
    if next.is_finite() { next.clamp(ZOOM_MIN, ZOOM_MAX) } else { z.clamp(ZOOM_MIN, ZOOM_MAX) }
}

/// A body as Apply body states it, for the command queue. Debug prints
/// the facet count, never the mesh.
#[derive(Clone)]
pub struct BodyDraft {
    pub source: String,
    pub facets: Arc<[Triangle]>,
    pub axis: [f64; 3],
    pub offset: f64,
}

impl std::fmt::Debug for BodyDraft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BodyDraft")
            .field("source", &self.source)
            .field("facets", &self.facets.len())
            .field("axis", &self.axis)
            .field("offset", &self.offset)
            .finish()
    }
}

fn same(a: Option<&Arc<[Triangle]>>, b: Option<&Arc<[Triangle]>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => Arc::ptr_eq(x, y),
        _ => false,
    }
}

impl State {
    /// The body landed on the editor's tool, if any.
    pub(crate) fn editor_tool_body(&self) -> Option<ToolBody> {
        let e = model::find_by_name(&self.world, &self.tool_ui.name)?;
        tool_body::body_of(&self.world, &e)
    }

    /// Keep the draft on the editor's tool: a new tool, or a world body
    /// that changed under it, re-mirrors (a landed body re-edits in its
    /// tool frame: +Z, offset 0, which re-applies to the same facets).
    /// An unapplied import survives unrelated document edits.
    pub(crate) fn sync_tool_body_ui(&mut self) {
        let world = self.editor_tool_body();
        let facets = world.as_ref().map(|b| Arc::clone(&b.facets));
        if self.tool_body_ui.tool == self.tool_ui.name && same(facets.as_ref(), self.tool_body_ui.seen.as_ref()) {
            return;
        }
        let mut d = ToolBodyUi { tool: self.tool_ui.name.clone(), axis: [0.0, 0.0, 1.0], ..ToolBodyUi::default() };
        if let Some(b) = world {
            d.source = b.source;
            d.facets = b.facets;
        }
        d.seen = facets;
        self.tool_body_ui = d;
    }

    /// "Import body…": pick a mesh file, then read it into the draft.
    pub(crate) fn import_tool_body_dialog(&mut self) {
        if let Some(path) = super::pick_mesh_file() {
            self.import_tool_body_from(&path);
        }
    }

    /// Read a STEP / STL / OBJ into the draft (+Z axis, origin at the
    /// tip until placed). Nothing reaches the world until Apply body.
    pub(crate) fn import_tool_body_from(&mut self, path: &Path) {
        self.sync_tool_body_ui();
        match tool_body_view::load(path) {
            Ok((source, facets)) => {
                let d = &mut self.tool_body_ui;
                self.status = format!(
                    "body {source}: {} facets - set the axis and tip, then Apply body",
                    facets.len()
                );
                d.source = source;
                d.facets = Arc::from(facets);
                d.axis = [0.0, 0.0, 1.0];
                d.offset = 0.0;
                d.pick = BodyPickMode::Off;
                d.preview = None;
            }
            Err(err) => self.refuse(ErrorScope::Tool, format!("tool body not imported: {err}")),
        }
    }

    /// The draft's placement as `SetToolBody` would read it, cached.
    pub(crate) fn tool_body_preview(&mut self) -> Option<&Result<ToolBody, String>> {
        let d = &mut self.tool_body_ui;
        if d.facets.is_empty() {
            return None;
        }
        let key: PreviewKey = (
            d.facets.as_ptr() as usize,
            [d.axis[0].to_bits(), d.axis[1].to_bits(), d.axis[2].to_bits(), d.offset.to_bits()],
        );
        if d.preview.as_ref().is_none_or(|(k, _)| *k != key) {
            d.preview = Some((key, tool_body_view::preview(&d.facets, d.axis, d.offset)));
        }
        d.preview.as_ref().map(|(_, r)| r)
    }

    /// A click on the preview in a pick mode fills the numbers (ephemeral).
    pub(crate) fn apply_body_pick(&mut self, hit: BodyPick) {
        let d = &mut self.tool_body_ui;
        match d.pick {
            BodyPickMode::Off => return,
            BodyPickMode::Tip => match tool_body_view::tip_offset_at(hit.vertex, d.axis) {
                Some(o) => {
                    d.offset = o;
                    self.status = format!("tip picked: offset {o:.3} mm along the axis");
                }
                None => self.status = "tip not picked: set a usable axis first".into(),
            },
            BodyPickMode::Axis => match hit.normal {
                Some(n) => {
                    d.axis = n;
                    self.status = format!(
                        "axis picked: {}",
                        tool_body_view::axis_label(n).map_or_else(|| format!("({:.3}, {:.3}, {:.3})", n[0], n[1], n[2]), str::to_owned)
                    );
                }
                None => self.status = "axis not picked: that facet has no area".into(),
            },
        }
        self.tool_body_ui.pick = BodyPickMode::Off;
    }

    /// Zoom the body preview by `by` (log2) about `about` - the cursor's
    /// offset from the view centre in view units (right, up) at the
    /// current zoom - so the point under it stays put. View state only.
    pub(crate) fn zoom_body_preview(&mut self, by: f32, about: [f64; 2]) {
        let d = &mut self.tool_body_ui;
        let next = clamp_zoom(d.zoom, by);
        let k = f64::from(next - d.zoom).exp2();
        if k.is_finite() && k > 0.0 && about.iter().all(|v| v.is_finite()) {
            d.pan[0] += about[0] * (1.0 - 1.0 / k);
            d.pan[1] += about[1] * (1.0 - 1.0 / k);
        }
        d.zoom = next;
    }

    /// Fit: the default view again - orbit, zoom and pan reset.
    pub(crate) fn fit_body_preview(&mut self) {
        let d = &mut self.tool_body_ui;
        (d.yaw, d.pitch, d.zoom, d.pan) = (0.0, 0.0, 0.0, [0.0; 2]);
    }

    /// "Apply body": queue the one `SetToolBody` for the draft.
    pub(crate) fn queue_apply_tool_body(&mut self) {
        let d = &self.tool_body_ui;
        if d.facets.is_empty() {
            return;
        }
        let body = BodyDraft { source: d.source.clone(), facets: Arc::clone(&d.facets), axis: d.axis, offset: d.offset };
        self.pending.push_back(CommandKind::SetToolBody { name: self.tool_ui.name.clone(), body: Some(body) });
    }

    /// "Clear body": a landed body is cleared by command (undoable); an
    /// unapplied import is only dropped from the draft.
    pub(crate) fn clear_tool_body(&mut self) {
        if self.editor_tool_body().is_some() {
            self.pending.push_back(CommandKind::SetToolBody { name: self.tool_ui.name.clone(), body: None });
        } else {
            self.tool_body_ui = ToolBodyUi { tool: self.tool_ui.name.clone(), axis: [0.0, 0.0, 1.0], ..ToolBodyUi::default() };
        }
    }

    /// The command handler: build (place, envelope, fit) now; a refusal
    /// lands on the Tool pane by name and the draft stays for correction.
    pub(crate) fn run_set_tool_body(&mut self, name: &str, body: Option<BodyDraft>) {
        let clearing = body.is_none();
        let spec = body.map(|b| ToolBodySpec { source: b.source, facets: b.facets.to_vec(), axis: b.axis, offset: b.offset });
        match SetToolBody::new(&self.world, name, spec) {
            Ok(command) => {
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = if clearing {
                    format!("tool {name}: body cleared")
                } else {
                    format!("tool {name}: body set")
                };
            }
            Err(err) => self.refuse(ErrorScope::Tool, format!("tool body not set: {err}")),
        }
    }
}
