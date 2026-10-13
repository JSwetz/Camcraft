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
    CommandError, DeleteEntity, History, ImportModel, SelectEntity, SetStock, SetWcsOrigin,
};
use hsmworks_app::geometry::{quat_from_yaw, transform_point, Point3};
use hsmworks_app::schema::{Location, STOCK_NAME, WCS_SLOTS};
use hsmworks_app::stock::{self, StockShape};
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
    SetWcs {
        slot: String,
        pos: [f64; 3],
        yaw_deg: f64,
    },
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
    pub yaw_deg: f64,
    /// XY grid point last clicked (row-major north-to-south, west-to-east;
    /// 4 = center). Display-only highlight; the world stores the position.
    pub anchor_xy: usize,
    /// Z level grid clicks use: 0 bottom, 1 middle, 2 top of the anchor box.
    pub anchor_z: usize,
    /// Whether the selected slot currently has a placed origin entity. Lets
    /// `sync_setup_ui` zero the readout when an origin is undone without
    /// wiping typing into a never-placed slot.
    pub(crate) placed: bool,
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
    pub cam: Camera,
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
    /// `doc_version` the drafts were last refetched at.
    ui_synced_doc: u64,
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
            cam: Camera::default(),
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
            ui_synced_doc: 0,
        }
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
                self.wcs_ui.yaw_deg = quat_yaw_deg(loc.rot);
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
                    self.wcs_ui.yaw_deg = 0.0;
                    self.wcs_ui.placed = false;
                }
            }
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
                    Err(err) => self.status = format!("delete failed: {err}"),
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
                    Err(err) => self.status = format!("stock not set: {err}"),
                }
            }
            CommandKind::StockFit { margin, shape: radio } => {
                if radio == 2 {
                    // From-model stock IS its source's shape; there is
                    // nothing to wrap. Refuse visibly, never guess.
                    self.status = String::from(
                        "from-model stock follows its source: choose Box or Cylinder to fit",
                    );
                    return;
                }
                let Some(bounds) = model::solid_world_aabb(&self.world) else {
                    self.status = String::from("nothing to fit: no models in the document");
                    return;
                };
                let Some((shape, pos)) = (if radio == 1 {
                    stock::fit_cylinder(&bounds, margin)
                } else {
                    stock::fit_box(&bounds, margin)
                }) else {
                    self.status = String::from("stock fit refused: models have no usable extent");
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
                    Err(err) => self.status = format!("stock not fitted: {err}"),
                }
            }
            CommandKind::SetWcs { slot, pos, yaw_deg } => {
                let location = Location {
                    pos: Point3::new(pos[0], pos[1], pos[2]),
                    rot: quat_from_yaw(yaw_deg.to_radians()),
                };
                match SetWcsOrigin::new(&self.world, &slot, location) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("{slot} origin set");
                    }
                    Err(err) => self.status = format!("offset not set: {err}"),
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
