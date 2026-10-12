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
use hsmworks_app::commands::{CommandError, DeleteEntity, History, ImportModel, SelectEntity};
use hsmworks_app::geometry::transform_point;
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
    Select(Option<String>),
    FrameAll,
    Quit,
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
