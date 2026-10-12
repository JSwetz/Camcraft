//! The shell's document state: world + history + deferred actions.
//!
//! Provenance: our own app state, specced from public CAM session behavior.
//! Panel code never mutates the world directly - it queues a `CommandKind`,
//! which is executed once per frame outside the ImGui borrow (dialog code
//! must not run mid-frame).

use std::collections::VecDeque;
use std::path::PathBuf;

use dear_imgui_rs::TextureId;
use flecs_ecs::prelude::World;
use hsmworks_app::commands::{DeleteEntity, History, ImportModel, SelectEntity};
use hsmworks_app::geometry::Point3;
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
        }
    }

    /// Run every queued action; at most one per kind is lost-none, FIFO.
    pub fn drain_actions(&mut self) {
        while let Some(action) = self.pending.pop_front() {
            self.run(action);
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
            let loc = model::location_of(&self.world, &entity).unwrap_or(Point3::ZERO);
            for t in tris {
                for p in [t.a, t.b, t.c] {
                    for axis in 0..3 {
                        let v = [p.x + loc.x, p.y + loc.y, p.z + loc.z][axis];
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
        // Validation happens at command construction: a corrupt file never
        // enters the history (AGENTS.md "validate early").
        match ImportModel::new(&path) {
            Ok(command) => {
                let name = command.name.clone();
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = format!("imported {name}");
            }
            Err(err) => self.status = format!("import refused: {err}"),
        }
    }

    fn open_project_path(&mut self, path: PathBuf) {
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
