//! The shell's document state: world + history + deferred actions.
//!
//! Provenance: our own app state, specced from public CAM session behavior.
//! Panel code never mutates the world directly - it queues a `CommandKind`,
//! which is executed once per frame outside the ImGui borrow (dialog code
//! must not run mid-frame).
//!
//! File imports run the parse/tessellation on a worker thread so a heavy
//! STEP never freezes the window; the resulting `ImportModel` command is
//! executed on this thread, so the history stays the single source of truth.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, TryRecvError};
use std::time::{Duration, Instant};

use dear_imgui_rs::TextureId;
use flecs_ecs::prelude::World;
use hsmworks_app::commands::{
    AddTool, ChangeTool, CommandError, CreateOperation, DeleteEntity, DeleteOperation, DeleteTool,
    EditOperationLinks, EditOperationParams, GenerateOperation, History, ImportModel, SelectEntity,
    SetStock, SetWcsOrigin,
};
use hsmworks_app::geometry::{quat_from_euler, quat_from_yaw, quat_to_euler, transform_point, Point3};
use hsmworks_app::operation::{OpParams, SrcRef};
use hsmworks_app::schema::{Location, Operation as OperationDef, STOCK_NAME, WCS_SLOTS};
use hsmworks_app::stock::{self, StockShape};
use hsmworks_app::tools::{catalog, Tool, ToolKind};
use hsmworks_app::{model, project, world::new_world};

use crate::viewport::Camera;

#[derive(Debug)]
pub enum CommandKind {
    OpenModelDialog,
    ImportFile(PathBuf),
    NewDocument,
    OpenProjectDialog,
    OpenProjectFile(PathBuf),
    SaveProjectDialog,
    SaveProjectFile(PathBuf),
    Undo,
    Redo,
    DeleteSelected,
    DeleteByName(String),
    Select(Option<String>),
    FrameAll,
    Quit,
    /// The Setup pane's shape+placement draft; the command itself is built
    /// here, so validation and status reporting live in one place.
    SetStock {
        shape: StockShape,
        pos: [f64; 3],
        yaw_deg: f64,
    },
    /// Stock that wraps every model's world AABB plus `margin` mm, shaped
    /// like the pane's radio selection at click time (0 box, 1 cylinder).
    StockFit {
        margin: f64,
        shape: i32,
    },
    /// Work offset: slot origin plus full orientation as yaw-pitch-roll
    /// degrees (R = Rz ⊗ Ry ⊗ Rx). The world stores a unit quaternion;
    /// pure-yaw setups use roll = pitch = 0.
    SetWcs {
        slot: String,
        pos: [f64; 3],
        roll_deg: f64,
        pitch_deg: f64,
        yaw_deg: f64,
    },
    /// Tool library: create a tool entity (catalog row or hand-typed page).
    AddTool { name: String, tool: Tool },
    /// Tool library: replace a tool entity's numbers (referencing
    /// operations read Stale from their stamp - no cascade writes).
    ChangeTool { name: String, tool: Tool },
    DeleteTool(String),
    /// New operation entity; deliberately never generated until
    /// [`CommandKind::GenerateOperation`] runs.
    CreateOperation {
        name: String,
        tool: String,
        src: SrcRef,
        params: OpParams,
    },
    /// The properties page's parameter draft.
    EditOperationParams { name: String, params: OpParams },
    /// Retarget an operation's tool and/or source (combo changes).
    EditOperationLinks {
        name: String,
        tool: Option<String>,
        src: Option<SrcRef>,
    },
    DeleteOperation(String),
    /// Generate and verify the cached path. The command does the work at
    /// construction, synchronously; status arrives with the result.
    GenerateOperation(String),
}

/// Editable fields of the Setup pane's stock group. Transient UI state,
/// never authoritative: the world's `StockDef` is, refetched by
/// [`State::sync_setup_ui`] whenever the document moves under the pane.
#[derive(Clone, Debug)]
pub struct StockUi {
    /// 0 = box, 1 = cylinder, 2 = from model (radio order in the pane).
    pub shape: i32,
    pub dx: f64,
    pub dy: f64,
    pub dz: f64,
    pub diameter: f64,
    pub height: f64,
    pub source: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw_deg: f64,
    pub fit_margin: f64,
}

impl Default for StockUi {
    fn default() -> Self {
        // A billet that fits a hand-held part: plausible first numbers beat
        // zeros, which the command layer would only refuse.
        StockUi {
            shape: 0,
            dx: 60.0,
            dy: 60.0,
            dz: 30.0,
            diameter: 40.0,
            height: 20.0,
            source: String::new(),
            x: 0.0,
            y: 0.0,
            z: 0.0,
            yaw_deg: 0.0,
            fit_margin: 2.0,
        }
    }
}

/// Editable fields of the Setup pane's WCS group; see [`StockUi`] for the
/// transient-ness rule. `slot` indexes [`WCS_SLOTS`].
#[derive(Clone, Debug, Default)]
pub struct WcsUi {
    pub slot: usize,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// Orientation as yaw-pitch-roll degrees (see [`CommandKind::SetWcs`]).
    /// The quick-turn buttons in the pane compose onto these, so the draft
    /// always round-trips with the world's quaternion.
    pub roll_deg: f64,
    pub pitch_deg: f64,
    pub yaw_deg: f64,
    /// XY grid point last clicked (row-major north-to-south, west-to-east;
    /// 4 = center). Display-only highlight; the world stores the position.
    pub anchor_xy: usize,
    /// Z level grid clicks use: 0 bottom, 1 middle, 2 top of the anchor box.
    pub anchor_z: usize,
    /// Which box the anchor points live on: 0 = stock box, 1 = model bounds.
    /// The pane states it explicitly; a chosen source that doesn't exist
    /// falls back to the other, with a note (app::anchor_reference).
    pub anchor_source: usize,
    /// Viewport picking mode: the anchor points of the current level show as
    /// clickable markers in the viewport. Transient UI mode, not undoable
    /// (AGENTS.md) - the origin change it triggers is the command.
    pub pick_anchor: bool,
    /// Whether the selected slot currently has a placed origin entity. Lets
    /// `sync_setup_ui` zero the readout when an origin is undone without
    /// wiping typing into a never-placed slot.
    pub(crate) placed: bool,
}

/// Editable fields of the Properties pane's operation page. Transient UI
/// state, never authoritative: the world's `Operation` component is,
/// refetched by [`State::sync_ops_ui`] whenever the selection or the
/// document moves under the pane (same typing rule as [`StockUi`]). With
/// no operation selected the same draft is the "new operation" form.
#[derive(Clone, Debug)]
pub struct OpUi {
    /// Operation this draft mirrors; empty = nothing selected.
    pub name: String,
    /// Tool entity the operation cuts with.
    pub tool: String,
    /// 0 = the stock solid, 1 = a named model (`SrcRef`).
    pub src_kind: usize,
    /// Model name when `src_kind == 1`.
    pub src_name: String,
    /// The parameter set; the kind the fields show is `params.kind()`.
    pub params: OpParams,
}

impl Default for OpUi {
    fn default() -> Self {
        OpUi {
            name: String::new(),
            tool: String::new(),
            src_kind: 0,
            src_name: String::new(),
            // A facing pass over a small billet: sane numbers until edited,
            // zeros would only be refused by the command layer.
            params: OpParams::Face {
                stepover: 3.0,
                clearance_z: 25.0,
                cut_z: 0.0,
                feed: 1200.0,
                plunge_feed: 300.0,
                climb: true,
            },
        }
    }
}

impl OpUi {
    /// Snapshot an operation component into draft form.
    #[must_use]
    pub fn from_op(name: &str, op: &OperationDef) -> Self {
        let (src_kind, src_name) = match &op.src {
            SrcRef::Stock => (0, String::new()),
            SrcRef::Entity(n) => (1, n.clone()),
        };
        OpUi {
            name: name.to_string(),
            tool: op.tool.clone(),
            src_kind,
            src_name,
            params: op.params.clone(),
        }
    }

    /// The source the drafts describe, for the queue.
    #[must_use]
    pub fn src(&self) -> SrcRef {
        if self.src_kind == 0 {
            SrcRef::Stock
        } else {
            SrcRef::Entity(self.src_name.clone())
        }
    }
}

/// Editable fields of the Tool Library pane's tool page; see [`StockUi`]
/// for the transient-ness rule. `exists` records whether `name` resolved
/// to a tool entity when the draft was fetched: Apply edits, Add creates.
#[derive(Clone, Debug)]
pub struct ToolUi {
    pub name: String,
    pub exists: bool,
    /// 0 end mill, 1 ball nose, 2 bull nose.
    pub kind: usize,
    pub diameter: f64,
    pub corner_r: f64,
    pub flute_len: f64,
    pub shank_d: f64,
    pub shank_len: f64,
    pub holder_d: f64,
    pub holder_len: f64,
    pub material_group: String,
    pub rpm: f64,
    pub feed: f64,
}

impl Default for ToolUi {
    fn default() -> Self {
        // An unnamed copy of the catalog's smallest mill: plausible
        // starting numbers beat zeros the commands would only refuse.
        let starter = catalog().remove(0);
        ToolUi::from_tool("", &starter.1, false)
    }
}

impl ToolUi {
    #[must_use]
    pub fn from_tool(name: &str, t: &Tool, exists: bool) -> Self {
        ToolUi {
            name: name.to_string(),
            exists,
            kind: match t.kind {
                ToolKind::EndMill => 0,
                ToolKind::BallNose => 1,
                ToolKind::BullNose => 2,
            },
            diameter: t.diameter,
            corner_r: t.corner_r,
            flute_len: t.flute_len,
            shank_d: t.shank_d,
            shank_len: t.shank_len,
            holder_d: t.holder_d,
            holder_len: t.holder_len,
            material_group: t.material_group.clone(),
            rpm: t.rpm,
            feed: t.feed,
        }
    }

    #[must_use]
    pub fn to_tool(&self) -> Tool {
        Tool {
            kind: match self.kind {
                1 => ToolKind::BallNose,
                2 => ToolKind::BullNose,
                _ => ToolKind::EndMill,
            },
            diameter: self.diameter,
            corner_r: self.corner_r,
            flute_len: self.flute_len,
            shank_d: self.shank_d,
            shank_len: self.shank_len,
            holder_d: self.holder_d,
            holder_len: self.holder_len,
            material_group: self.material_group.clone(),
            rpm: self.rpm,
            feed: self.feed,
        }
    }
}

/// Yaw in degrees of a (pure-yaw) quaternion: the exact inverse of
/// `quat_from_yaw`, and the Z-twist of any other rotation. Display/draft
/// only - the data model keeps the full quaternion through Apply.
#[must_use]
pub fn quat_yaw_deg(q: [f64; 4]) -> f64 {
    (2.0 * q[2].atan2(q[3])).to_degrees()
}

/// Which viewport drag is live; ephemeral UI state, deliberately not
/// undoable (AGENTS.md). app.rs owns it from raw winit events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragMode {
    Orbit,
    Pan,
}

/// The facet under the cursor, from the viewport hover raycast: the named
/// entity and the facet index the pick resolved (into `model::facets_of`'s
/// order for that entity). Ephemeral view state, not a command (AGENTS.md)
/// - it only drives the screen-space edge highlight, never selection.
#[derive(Clone, Debug, PartialEq)]
pub struct HoverHit {
    pub entity: String,
    pub facet: usize,
}

pub struct State {
    pub world: World,
    pub history: History,
    pub status: String,
    /// Bumped whenever documents changed; drives GPU re-upload of meshes.
    pub doc_version: u64,
    /// Bumped whenever selection changed; re-sends the vertex stream (the
    /// selection tint is baked per vertex) without re-uploading geometry.
    pub selection_version: u64,
    /// 3D render target exposed to ImGui (None while headless / pre-GPU).
    pub viewport_tex: Option<TextureId>,
    /// Logical-pixel size the Viewport panel reported last frame; drives
    /// offscreen target sizing (shell writes it, app reads it).
    pub viewport_size: [f32; 2],
    /// Viewport image rect in logical screen coords; the shell publishes it
    /// each frame, app.rs hit-tests raw pointer events against it.
    pub viewport_rect: ([f32; 2], [f32; 2]),
    /// Live viewport drag (orbit/pan), driven by app.rs from winit events.
    pub drag: Option<DragMode>,
    /// Facet currently under the cursor (app.rs refreshes it from raw
    /// `CursorMoved` events with a filtered raycast; it clears when the
    /// pointer leaves the viewport). View state like the camera and the
    /// drag - not undoable, not saved.
    pub hover: Option<HoverHit>,
    /// True when the Viewport window was the topmost hovered window last
    /// frame (the shell writes it, app.rs gates raw pointer drags on it -
    /// a click on a menu or popup over the image must never orbit).
    pub viewport_hot: bool,
    /// Loaded UI glyph textures; `None` headless and pre-GPU, so every
    /// consumer falls back to text (see [`crate::icons`]).
    pub icons: Option<crate::icons::Icons>,
    pub cam: Camera,
    /// Models hidden in the viewport (eye toggle in the browser). View
    /// state like the camera - not undoable, not saved; a hidden model
    /// still cuts: visibility is display, not suppression (a toolpath
    /// computed from it keeps running - hiding geometry must never
    /// silently change what the machine does).
    pub hidden_models: Vec<String>,
    /// Operations whose cached path stays undrawn (eye toggle per op).
    /// Same rule: the path still exists and posts.
    pub hidden_paths: Vec<String>,

    /// Show the dimmed world origin axes in the viewport. View state like
    /// the camera - not undoable, not saved - toggled from the Models panel;
    /// the triads of real work offsets are always shown, because they are
    /// setup, not decoration. Hiding the world axes is for the moment a
    /// G5x offset sits away from machine zero and two gizmos read as clutter.
    pub show_world_axes: bool,
    /// Show cached toolpaths beyond the selected operation's: with this on,
    /// every generated path draws (dimmed where unselected) instead of only
    /// the one the tree highlights. View state like the camera - not
    /// undoable, not saved; the overlay rebuild gate reads it.
    pub show_all_paths: bool,
    pub pending: VecDeque<CommandKind>,
    pub request_quit: bool,
    /// Receiver for the in-flight import worker (None when idle). The worker
    /// runs `ImportModel::new` off-thread and sends the finished command back.
    import_rx: Option<mpsc::Receiver<Result<ImportModel, CommandError>>>,
    /// Path currently being imported on the worker; drives the status text.
    import_file: Option<PathBuf>,
    /// Wall-clock start of the live import, for an elapsed-seconds readout.
    import_started: Option<Instant>,
    /// Imports queued while one is already running (drained FIFO on finish).
    import_queue: VecDeque<PathBuf>,
    /// Setup pane drafts (ephemeral; see [`StockUi`]).
    pub stock_ui: StockUi,
    pub wcs_ui: WcsUi,
    /// Properties / Tool Library pane drafts (ephemeral; see [`OpUi`]).
    pub op_ui: OpUi,
    pub tool_ui: ToolUi,
    /// `doc_version` the setup drafts were last refetched at.
    ui_synced_doc: u64,
    /// (draft identity, `doc_version` it was fetched at) per pane; see
    /// [`State::sync_ops_ui`] for the rule.
    op_synced: (String, u64),
    tool_synced: (String, u64),
    /// Which entity the docked context panes edit right now. Ephemeral view
    /// state like the camera - not a command, not undoable (AGENTS.md); the
    /// selection its clicks mirror is the command. [`State::pane_target`]
    /// validates it against the world every frame, so a target that
    /// disappears under it (delete, undo) falls back to the selection on
    /// its own. Replaces the old per-pane show-flags and the `.opened()`
    /// dance: the panes are persistent, what changes is their subject.
    pub ui_target: UiTarget,
    /// The last command's refusal, tagged with the pane it belongs to, so
    /// the failing pane can show the reason inline. The bottom status strip
    /// alone was invisible enough that "Add does nothing" read as a dead
    /// button. Cleared by every command that runs (see [`State::run`]).
    pub feedback: Option<(ErrorScope, String)>,
}

/// Which page of the Operations pane a refused command's message belongs
/// to (so a refusal lands where the user is looking, not only in the
/// status strip).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ErrorScope {
    /// The Tool page (add/update/delete a tool).
    Tool,
    /// The Operation page (create/edit/generate/delete an operation).
    Operation,
    /// The Stock and WCS pages.
    Setup,
}

/// What the docked context panes should edit. Set by browser-tree and
/// ribbon clicks and the menu through [`crate::actions::target`]; view
/// state, never a command (the `Select` a click mirrors into the world is
/// the command).
#[derive(Clone, Debug, PartialEq)]
pub enum UiTarget {
    /// Nothing targeted: panes fall back to the world selection.
    None,
    /// The stock solid (its page doubles as the creation form).
    Stock,
    /// One of [`hsmworks_app::schema::WCS_SLOTS`], by index.
    WcsSlot(usize),
    /// A model entity: the info page.
    Model(String),
    /// A tool entity, or the empty name for the tool page as destination
    /// (the catalog / new-draft form, e.g. CAM > Tool Library).
    Tool(String),
    /// An operation entity: its properties.
    Operation(String),
}

impl UiTarget {
    /// Whether this target still has something to show: named entities
    /// must exist with the component they were targeted for (an undone
    /// operation stops holding the pane), slot indexes must be real, and
    /// the pages that double as creation forms - Stock, and the tool page
    /// with an empty name - survive having no entity yet.
    #[must_use]
    pub fn resolves_in(&self, world: &World) -> bool {
        match self {
            UiTarget::None => false,
            UiTarget::Stock => true,
            UiTarget::WcsSlot(i) => *i < WCS_SLOTS.len(),
            UiTarget::Model(name) => model::model_entities(world)
                .into_iter()
                .any(|(n, _)| *name == n),
            UiTarget::Tool(name) => {
                name.is_empty()
                    || model::find_by_name(world, name).is_some_and(|e| model::is_tool(world, &e))
            }
            UiTarget::Operation(name) => model::find_by_name(world, name)
                .is_some_and(|e| model::is_operation(world, &e)),
        }
    }
}

impl State {
    #[must_use]
    pub fn new() -> Self {
        State {
            world: new_world(),
            history: History::default(),
            status: String::from("ready"),
            doc_version: 0,
            selection_version: 0,
            viewport_tex: None,
            viewport_size: [960.0, 540.0],
            viewport_rect: ([0.0, 0.0], [0.0, 0.0]),
            drag: None,
            hover: None,
            viewport_hot: false,
            icons: None,
            hidden_models: Vec::new(),
            hidden_paths: Vec::new(),
            cam: Camera::default(),
            show_world_axes: true,
            show_all_paths: false,
            pending: VecDeque::new(),
            request_quit: false,
            import_rx: None,
            import_file: None,
            import_started: None,
            import_queue: VecDeque::new(),
            stock_ui: StockUi::default(),
            wcs_ui: WcsUi {
                // Neutral highlight: center-middle, before any pick.
                anchor_xy: 4,
                anchor_z: 1,
                ..WcsUi::default()
            },
            // First-launch drafts carry plausible names: an empty Name
            // field made the Add buttons look dead (refused silently).
            op_ui: OpUi {
                name: String::from("Operation1"),
                ..OpUi::default()
            },
            tool_ui: ToolUi {
                name: String::from("Tool1"),
                ..ToolUi::default()
            },
            ui_synced_doc: 0,
            op_synced: (String::new(), 0),
            tool_synced: (String::new(), 0),
            ui_target: UiTarget::None,
            feedback: None,
        }
    }

    /// The refusal a pane of this scope should show inline, if the last
    /// command to run was a refusal that belongs to it.
    #[must_use]
    pub fn error_for(&self, scope: ErrorScope) -> Option<&str> {
        self.feedback
            .as_ref()
            .filter(|(s, _)| *s == scope)
            .map(|(_, msg)| msg.as_str())
    }

    /// A blank tool page with a numbered name: New starts a real draft,
    /// not an empty field the Add button would only refuse.
    #[must_use]
    pub fn new_tool_draft(&self) -> ToolUi {
        let n = self.tool_names().len() + 1;
        let starter = catalog().remove(0);
        ToolUi::from_tool(&format!("Tool{n}"), &starter.1, false)
    }

    /// Record a refusal: status strip for everyone, tagged message for the
    /// pane that caused it.
    fn refuse(&mut self, scope: ErrorScope, msg: String) {
        self.status.clone_from(&msg);
        self.feedback = Some((scope, msg));
    }

    /// Refetch the Setup pane's drafts from the world after any document
    /// change (Apply, Undo, Open...). Between bumps the user's in-progress
    /// typing is left alone; a refused command changes nothing, so the
    /// offending values stay visible for correction (Prime Directive).
    pub fn sync_setup_ui(&mut self) {
        if self.ui_synced_doc == self.doc_version {
            return;
        }
        self.ui_synced_doc = self.doc_version;
        if let Some(entity) = model::find_by_name(&self.world, STOCK_NAME)
            && model::is_stock(&self.world, &entity)
        {
            if let Some(loc) = model::transform_of(&self.world, &entity) {
                self.stock_ui.x = loc.pos.x;
                self.stock_ui.y = loc.pos.y;
                self.stock_ui.z = loc.pos.z;
                self.stock_ui.yaw_deg = quat_yaw_deg(loc.rot);
            }
            match model::stock_def_of(&self.world, &entity) {
                Some(StockShape::Box { dx, dy, dz }) => {
                    self.stock_ui.shape = 0;
                    self.stock_ui.dx = dx;
                    self.stock_ui.dy = dy;
                    self.stock_ui.dz = dz;
                }
                Some(StockShape::Cylinder { diameter, height }) => {
                    self.stock_ui.shape = 1;
                    self.stock_ui.diameter = diameter;
                    self.stock_ui.height = height;
                }
                Some(StockShape::FromModel { source }) => {
                    self.stock_ui.shape = 2;
                    self.stock_ui.source = source;
                }
                None => {}
            }
        }
        let placed = WCS_SLOTS
            .get(self.wcs_ui.slot)
            .and_then(|slot| model::find_by_name(&self.world, slot))
            .and_then(|entity| model::transform_of(&self.world, &entity));
        match placed {
            Some(loc) => {
                self.wcs_ui.x = loc.pos.x;
                self.wcs_ui.y = loc.pos.y;
                self.wcs_ui.z = loc.pos.z;
                let (roll, pitch, yaw) = quat_to_euler(loc.rot);
                self.wcs_ui.roll_deg = roll.to_degrees();
                self.wcs_ui.pitch_deg = pitch.to_degrees();
                self.wcs_ui.yaw_deg = yaw.to_degrees();
                self.wcs_ui.placed = true;
            }
            None => {
                // Show zero only when an origin this slot *had* disappeared
                // (undo): stale numbers for a deleted entity are a lie.
                // Typing into a never-placed slot survives other edits.
                if self.wcs_ui.placed {
                    self.wcs_ui.x = 0.0;
                    self.wcs_ui.y = 0.0;
                    self.wcs_ui.z = 0.0;
                    self.wcs_ui.roll_deg = 0.0;
                    self.wcs_ui.pitch_deg = 0.0;
                    self.wcs_ui.yaw_deg = 0.0;
                    self.wcs_ui.placed = false;
                }
            }
        }
    }

    /// What the context panes actually edit this frame: the explicit
    /// target while it resolves, else the world selection, else nothing
    /// (the creation forms). Panes and drafts route through this instead
    /// of reading the raw target - a vanished one never leaves them
    /// pointing at a ghost.
    #[must_use]
    pub fn pane_target(&self) -> UiTarget {
        if self.ui_target.resolves_in(&self.world) {
            return self.ui_target.clone();
        }
        match self.selected_operation() {
            Some(name) => UiTarget::Operation(name),
            None => UiTarget::None,
        }
    }

    /// Refetch the Properties pane's draft after any document or selection
    /// change. While a real operation is targeted (or falls out of the
    /// selection) its draft always mirrors the world; the "new operation"
    /// form survives document bumps, since there is no reality to refetch
    /// until it is added.
    pub fn sync_ops_ui(&mut self) {
        let ident = match self.pane_target() {
            UiTarget::Operation(name) => name,
            _ => String::new(),
        };
        if self.op_synced == (ident.clone(), self.doc_version) {
            return;
        }
        let identity_changed = self.op_synced.0 != ident;
        self.op_synced = (ident, self.doc_version);
        if !self.op_synced.0.is_empty() {
            let name = self.op_synced.0.clone();
            if let Some(entity) = model::find_by_name(&self.world, &name)
                && let Some(op) = model::operation_of(&self.world, &entity)
            {
                self.op_ui = OpUi::from_op(&name, &op);
            }
        } else if identity_changed {
            let n = self.operation_count() + 1;
            self.op_ui = OpUi {
                name: format!("Operation{n}"),
                tool: self.tool_names().pop().unwrap_or_default(),
                ..OpUi::default()
            };
        }
    }

    /// Refetch the Tool Library pane's draft after document changes. The
    /// name decides the mode: it resolves to a tool entity (mirror it,
    /// `exists = true` - even after an undo brought it back), to nothing
    /// (keep the typed numbers with `exists = false`, so a deleted tool's
    /// page doubles as re-add), or is empty (the untouched New page).
    pub fn sync_tool_ui(&mut self) {
        if self.tool_synced == (self.tool_ui.name.clone(), self.doc_version) {
            return;
        }
        let ident = self.tool_ui.name.clone();
        self.tool_synced = (ident.clone(), self.doc_version);
        if ident.is_empty() {
            return;
        }
        match model::find_by_name(&self.world, &ident).and_then(|e| model::tool_of(&self.world, &e))
        {
            Some(tool) => self.tool_ui = ToolUi::from_tool(&ident, &tool, true),
            None => self.tool_ui.exists = false,
        }
    }

    /// Name of the selected entity if it is an operation (single selection
    /// in practice; the first one wins).
    #[must_use]
    pub fn selected_operation(&self) -> Option<String> {
        model::selected_names(&self.world).into_iter().find(|name| {
            model::find_by_name(&self.world, name)
                .is_some_and(|e| model::is_operation(&self.world, &e))
        })
    }

    /// Names of the tool entities, in document order.
    #[must_use]
    pub fn tool_names(&self) -> Vec<String> {
        model::named_entities(&self.world)
            .into_iter()
            .filter(|(_, e)| model::is_tool(&self.world, e))
            .map(|(name, _)| name)
            .collect()
    }

    #[must_use]
    pub(crate) fn operation_count(&self) -> usize {
        model::named_entities(&self.world)
            .into_iter()
            .filter(|(_, e)| model::is_operation(&self.world, e))
            .count()
    }

    /// Texture for a glyph slot (`None` headless or after a failed decode;
    /// every consumer must degrade to text, never to nothing).
    #[must_use]
    pub(crate) fn icon(&self, which: crate::icons::Icon) -> Option<TextureId> {
        self.icons.as_ref().and_then(|i| i.texture(which))
    }

    /// Flip one name's visibility in a browser eye list.
    pub(crate) fn toggle_visibility(list: &mut Vec<String>, name: &str) {
        match list.iter().position(|n| n == name) {
            Some(i) => drop(list.remove(i)),
            None => list.push(name.to_string()),
        }
    }

    /// Run every queued action, then poll the import worker once. Safe to
    /// call every frame; never blocks.
    pub fn drain_actions(&mut self) {
        while let Some(action) = self.pending.pop_front() {
            self.run(action);
        }
        self.poll_import();
    }

    /// Drive the queue and the import worker to completion, blocking. For
    /// tests and headless runs only - the interactive loop never calls this.
    pub fn settle(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            self.drain_actions();
            if self.pending.is_empty() && self.import_rx.is_none() && self.import_queue.is_empty() {
                return;
            }
            if Instant::now() >= deadline {
                self.abandon_imports();
                self.status = String::from("import timed out");
                eprintln!("settle: queued work did not finish before the deadline");
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn run(&mut self, action: CommandKind) {
        // Whatever ran, the banner shows only the newest outcome: a later
        // command (even a successful one) retires an old refusal.
        self.feedback = None;
        match action {
            CommandKind::OpenModelDialog => {
                if let Some(path) = pick_mesh_file() {
                    self.import_path(path);
                }
            }
            CommandKind::ImportFile(path) => self.import_path(path),
            CommandKind::NewDocument => {
                self.abandon_imports();
                model::clear_named(&self.world);
                self.history.clear();
                self.doc_version += 1;
                self.status = String::from("new document");
            }
            CommandKind::OpenProjectDialog => {
                if let Some(path) = pick_project_file() {
                    self.open_project_path(path);
                }
            }
            CommandKind::OpenProjectFile(path) => self.open_project_path(path),
            CommandKind::SaveProjectDialog => {
                if let Some(path) = save_project_dialog() {
                    match project::save(&self.world, &path) {
                        Ok(()) => self.status = format!("saved {}", path.display()),
                        Err(err) => self.status = format!("save failed: {err}"),
                    }
                }
            }
            CommandKind::SaveProjectFile(path) => {
                match project::save(&self.world, &path) {
                    Ok(()) => self.status = format!("saved {}", path.display()),
                    Err(err) => self.status = format!("save failed: {err}"),
                }
            }
            CommandKind::Undo => {
                self.status = if self.history.undo(&self.world) {
                    self.doc_version += 1;
                    String::from("undo")
                } else {
                    String::from("nothing to undo")
                };
            }
            CommandKind::Redo => {
                self.status = if self.history.redo(&self.world) {
                    self.doc_version += 1;
                    String::from("redo")
                } else {
                    String::from("nothing to redo")
                };
            }
            CommandKind::DeleteSelected => {
                let names = model::selected_names(&self.world);
                let mut done = 0usize;
                for name in names {
                    match DeleteEntity::new(&self.world, &name) {
                        Ok(command) => {
                            self.history.execute(&self.world, command);
                            done += 1;
                        }
                        Err(err) => self.status = format!("delete failed: {err}"),
                    }
                }
                if done > 0 {
                    self.doc_version += 1;
                    self.status = format!("deleted {done} model(s)");
                }
            }
            CommandKind::Select(name) => {
                History::execute_ephemeral(
                    &self.world,
                    SelectEntity::new(&self.world, name.as_deref()),
                );
                self.selection_version = self.selection_version.wrapping_add(1);
            }
            CommandKind::FrameAll => self.frame_all(),
            CommandKind::Quit => self.request_quit = true,
            CommandKind::DeleteByName(name) => {
                match DeleteEntity::new(&self.world, &name) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("deleted {name}");
                    }
                    Err(err) => self.refuse(ErrorScope::Setup, format!("delete failed: {err}")),
                }
            }
            CommandKind::SetStock { shape, pos, yaw_deg } => {
                let location = Location {
                    pos: Point3::new(pos[0], pos[1], pos[2]),
                    rot: quat_from_yaw(yaw_deg.to_radians()),
                };
                match SetStock::new(&self.world, shape, location) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = String::from("stock set");
                    }
                    Err(err) => self.refuse(ErrorScope::Setup, format!("stock not set: {err}")),
                }
            }
            CommandKind::StockFit { margin, shape: radio } => {
                if radio == 2 {
                    // From-model stock IS its source's shape; there is
                    // nothing to wrap. Refuse visibly, never guess.
                    self.refuse(
                        ErrorScope::Setup,
                        String::from(
                            "from-model stock follows its source: choose Box or Cylinder to fit",
                        ),
                    );
                    return;
                }
                let Some(bounds) = model::solid_world_aabb(&self.world) else {
                    self.refuse(
                        ErrorScope::Setup,
                        String::from("nothing to fit: no models in the document"),
                    );
                    return;
                };
                let Some((shape, pos)) = (if radio == 1 {
                    stock::fit_cylinder(&bounds, margin)
                } else {
                    stock::fit_box(&bounds, margin)
                }) else {
                    self.refuse(
                        ErrorScope::Setup,
                        String::from("stock fit refused: models have no usable extent"),
                    );
                    return;
                };
                // Fit replaces size and XY/Z placement; a deliberately
                // oriented stock keeps its rotation.
                let rot = model::find_by_name(&self.world, STOCK_NAME)
                    .and_then(|entity| model::transform_of(&self.world, &entity))
                    .map_or_else(|| Location::default().rot, |loc| loc.rot);
                let location = Location { pos, rot };
                match SetStock::new(&self.world, shape, location) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = String::from("stock fitted to models");
                    }
                    Err(err) => self.refuse(ErrorScope::Setup, format!("stock not fitted: {err}")),
                }
            }
            CommandKind::SetWcs {
                slot,
                pos,
                roll_deg,
                pitch_deg,
                yaw_deg,
            } => {
                // The axis-angle builders quietly substitute identity for
                // non-finite angles; refuse that here rather than silently
                // dropping a NaN rotation (Prime Directive).
                if !roll_deg.is_finite() || !pitch_deg.is_finite() || !yaw_deg.is_finite() {
                    self.refuse(
                        ErrorScope::Setup,
                        format!("offset not set: {slot} rotation angles must be finite"),
                    );
                    return;
                }
                let location = Location {
                    pos: Point3::new(pos[0], pos[1], pos[2]),
                    rot: quat_from_euler(
                        roll_deg.to_radians(),
                        pitch_deg.to_radians(),
                        yaw_deg.to_radians(),
                    ),
                };
                match SetWcsOrigin::new(&self.world, &slot, location) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("{slot} origin set");
                    }
                    Err(err) => self.refuse(ErrorScope::Setup, format!("offset not set: {err}")),
                }
            }
            CommandKind::AddTool { name, tool } => {
                match AddTool::new(&self.world, &name, tool) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("tool {name} added");
                    }
                    Err(err) => self.refuse(ErrorScope::Tool, format!("tool not added: {err}")),
                }
            }
            CommandKind::ChangeTool { name, tool } => {
                match ChangeTool::new(&self.world, &name, tool) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("tool {name} updated");
                    }
                    Err(err) => self.refuse(ErrorScope::Tool, format!("tool not updated: {err}")),
                }
            }
            CommandKind::DeleteTool(name) => {
                match DeleteTool::new(&self.world, &name) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("deleted tool {name}");
                    }
                    Err(err) => self.refuse(ErrorScope::Tool, format!("tool not deleted: {err}")),
                }
            }
            CommandKind::CreateOperation {
                name,
                tool,
                src,
                params,
            } => {
                match CreateOperation::new(&self.world, &name, &tool, src, params) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("operation {name} created");
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Operation, format!("operation not created: {err}"));
                    }
                }
            }
            CommandKind::EditOperationParams { name, params } => {
                match EditOperationParams::new(&self.world, &name, params) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("parameters updated for {name}");
                    }
                    Err(err) => {
                        self.refuse(
                            ErrorScope::Operation,
                            format!("parameters not updated: {err}"),
                        );
                    }
                }
            }
            CommandKind::EditOperationLinks { name, tool, src } => {
                let links = EditOperationLinks::new(
                    &self.world,
                    &name,
                    tool.as_deref(),
                    src,
                );
                match links {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("links updated for {name}");
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Operation, format!("links not updated: {err}"));
                    }
                }
            }
            CommandKind::DeleteOperation(name) => {
                match DeleteOperation::new(&self.world, &name) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("deleted operation {name}");
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Operation, format!("operation not deleted: {err}"));
                    }
                }
            }
            CommandKind::GenerateOperation(name) => {
                // Generation happens in the constructor (a refusal writes
                // nothing); the status line reports what was produced.
                match GenerateOperation::new(&self.world, &name) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        let cache = model::find_by_name(&self.world, &name)
                            .and_then(|e| model::toolpath_of(&self.world, &e));
                        self.status = match cache {
                            Some(tp) => format!(
                                "generated {name}: {} move(s), {} warning(s)",
                                tp.path.len(),
                                tp.warnings.len()
                            ),
                            None => format!("generated {name}"),
                        };
                    }
                    Err(err) => self.refuse(ErrorScope::Operation, format!("generate failed: {err}")),
                }
            }
        }
    }

    #[must_use]
    pub fn has_selection(&self) -> bool {
        !model::selected_names(&self.world).is_empty()
    }

    /// Center the orbit camera on the document's bounding sphere.
    fn frame_all(&mut self) {
        let mut min = [f64::MAX; 3];
        let mut max = [f64::MIN; 3];
        let mut any = false;
        for (_, entity) in model::named_entities(&self.world) {
            let Some(tris) = model::facets_of(&self.world, &entity) else {
                continue;
            };
            let loc = model::transform_of(&self.world, &entity).unwrap_or_default();
            for t in tris {
                for p in [t.a, t.b, t.c] {
                    let p = transform_point(p, loc.pos, loc.rot);
                    for axis in 0..3 {
                        let v = [p.x, p.y, p.z][axis];
                        if v.is_finite() {
                            min[axis] = min[axis].min(v);
                            max[axis] = max[axis].max(v);
                            any = true;
                        }
                    }
                }
            }
        }
        // A work offset parked far from the parts is still "all": frame
        // their origins even though they carry no geometry.
        for (_, entity) in model::named_entities(&self.world) {
            if !model::is_wcs(&self.world, &entity) {
                continue;
            }
            let Some(loc) = model::transform_of(&self.world, &entity) else {
                continue;
            };
            for axis in 0..3 {
                let v = [loc.pos.x, loc.pos.y, loc.pos.z][axis];
                if v.is_finite() {
                    min[axis] = min[axis].min(v);
                    max[axis] = max[axis].max(v);
                    any = true;
                }
            }
        }
        if !any {
            self.cam = Camera::default();
            self.status = String::from("nothing to frame");
            return;
        }
        let center = std::array::from_fn(|i| (min[i] + max[i]) * 0.5);
        let half: [f64; 3] = std::array::from_fn(|i| (max[i] - min[i]) * 0.5);
        let radius = half
            .iter()
            .fold(0.0f64, |acc, r| acc.max(*r))
            .max(f64::EPSILON)
            * 3.0f64.sqrt();
        self.cam = Camera::frame(center, radius);
    }

    fn import_path(&mut self, path: PathBuf) {
        // Parsing happens at command construction, like everything else -
        // it just happens to be off-thread now, so a heavy STEP tessellates
        // while the window keeps painting.
        if self.import_rx.is_some() {
            self.status = format!("queued {} for import", file_label(&path));
            self.import_queue.push_back(path);
            return;
        }
        self.begin_import(path);
    }

    fn begin_import(&mut self, path: PathBuf) {
        self.status = format!("importing {}…", file_label(&path));
        self.import_file = Some(path.clone());
        self.import_started = Some(Instant::now());
        let (tx, rx) = mpsc::channel();
        let worker_path = path.clone();
        let spawned = std::thread::Builder::new()
            .name("hsmworks-import".to_string())
            .spawn(move || {
                // No World here: pure file + geometry work, Send across.
                let result = ImportModel::new(&worker_path);
                // A dropped rx means the document changed or we are closing;
                // the command is simply never applied.
                if tx.send(result).is_err() {
                    eprintln!("import: result discarded, no receiver");
                }
            });
        match spawned {
            Ok(_) => self.import_rx = Some(rx),
            Err(err) => {
                // Thread exhaustion must not lose the import: run inline.
                eprintln!("import: worker spawn failed ({err}); running inline");
                let result = ImportModel::new(&path);
                self.finish_import(result);
            }
        }
    }

    /// Called on this thread when a worker result arrives (or inline
    /// fallback completes). Execution stays a normal history command.
    fn finish_import(&mut self, result: Result<ImportModel, CommandError>) {
        self.import_file = None;
        let secs = self
            .import_started
            .take()
            .map_or(0.0, |t| t.elapsed().as_secs_f64());
        match result {
            Ok(command) => {
                let name = command.name.clone();
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = if secs >= 2.0 {
                    format!("imported {name} in {secs:.1}s")
                } else {
                    format!("imported {name}")
                };
            }
            Err(err) => self.status = format!("import refused: {err}"),
        }
    }

    /// Non-blocking worker check; call once per frame. Also starts the next
    /// queued import when the worker is free.
    fn poll_import(&mut self) {
        // take() first so no borrow of self straddles the mutation arms.
        let outcome = match &self.import_rx {
            Some(rx) => match rx.try_recv() {
                Ok(result) => Some(Ok(result)),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(())),
            },
            None => None,
        };
        match outcome {
            Some(Ok(result)) => {
                self.import_rx = None;
                self.finish_import(result);
            }
            Some(Err(())) => {
                // Thread died without sending (a panic in the parser): the
                // Prime Directive says surface it, never hang waiting.
                self.import_rx = None;
                self.import_file = None;
                self.import_started = None;
                self.status = String::from("import failed: worker lost");
                eprintln!("import: worker thread exited without a result");
            }
            None => {}
        }
        // Still running: keep the elapsed readout honest in the status bar.
        if self.import_rx.is_some()
            && let Some(t0) = self.import_started
        {
            let secs = t0.elapsed().as_secs_f64();
            if secs >= 1.0 {
                let label =
                    self.import_file.as_deref().map_or_else(|| "model".to_string(), file_label);
                self.status = format!("importing {label}… {secs:.0}s");
            }
        }
        if self.import_rx.is_none()
            && let Some(next) = self.import_queue.pop_front()
        {
            self.begin_import(next);
        }
    }

    /// Drop any live/queued imports because the document is being replaced.
    /// A worker still running finishes in the background and discards itself.
    fn abandon_imports(&mut self) {
        if self.import_rx.take().is_some() {
            eprintln!("import: document changed; pending import discarded");
        }
        self.import_queue.clear();
        self.import_file = None;
        self.import_started = None;
    }

    fn open_project_path(&mut self, path: PathBuf) {
        self.abandon_imports();
        model::clear_named(&self.world);
        self.history.clear();
        match project::load(&self.world, &path) {
            Ok(()) => {
                self.doc_version += 1;
                self.status = format!("opened {}", path.display());
            }
            Err(err) => self.status = format!("open failed: {err}"),
        }
    }
}

/// Short human name for a file path, for status text.
fn file_label(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("model")
        .to_string()
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

fn pick_mesh_file() -> Option<PathBuf> {
    // STEP arrives tessellated through truck (mesh, not B-rep).
    rfd::FileDialog::new()
        .add_filter("Mesh", &["stl", "obj", "step", "stp"])
        .pick_file()
}

fn pick_project_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("HSMWorks Project", &["hsmproj"])
        .pick_file()
}

fn save_project_dialog() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("HSMWorks Project", &["hsmproj"])
        .set_file_name("untitled.hsmproj")
        .save_file()
}
