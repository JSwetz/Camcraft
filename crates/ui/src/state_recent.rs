//! Recent projects and models on the State (issue #662): the lists are
//! `camcraft_app::recent` preference state - read once at startup from
//! the per-user file, noted after a successful open / import / save,
//! written back atomically. Opening an entry rides the same paths as the
//! dialogs (`open_project_path`, `import_path`), so the unsaved-changes
//! prompt, the import worker and every status line are the ones Open and
//! Model already use; the lists themselves are outside the history.
//! Provenance: our own wiring over our own `app::recent`; no HSMWorks
//! material.

use std::path::{Path, PathBuf};

use camcraft_app::recent::{self, RecentEntry, RecentKind};

use super::State;

impl State {
    /// Startup: the per-user recents file (the real window only - the
    /// headless suites never touch the user's file). A corrupt or
    /// unreadable file starts empty and says so on the status line.
    pub(crate) fn load_recent(&mut self) {
        let Some(path) = recent::store_path() else {
            eprintln!("recent files: no config folder - the lists last for this session only");
            return;
        };
        self.load_recent_from(path);
    }

    /// Read the recents from `path` and keep writing back there.
    pub fn load_recent_from(&mut self, path: PathBuf) {
        let (lists, note) = recent::load(&path);
        self.recent = lists;
        self.recent_store = Some(path);
        if let Some(why) = note {
            self.status = why;
        }
    }

    /// The entries one drop-down lists, newest first, each probed for
    /// a missing file.
    #[must_use]
    pub fn recent_entries(&self, kind: RecentKind) -> Vec<RecentEntry> {
        self.recent.list(kind).iter().map(|p| RecentEntry::of(p)).collect()
    }

    /// Note a successful open / import / save. A failed write is
    /// logged (by `recent::save`), never shown over the open's own
    /// status line.
    pub(crate) fn note_recent(&mut self, kind: RecentKind, path: &Path) {
        if self.recent.add(kind, path) {
            self.persist_recent();
        }
    }

    /// A landed import's source file (none: an inline test import).
    pub(crate) fn note_recent_model(&mut self, file: Option<&Path>) {
        if let Some(path) = file {
            self.note_recent(RecentKind::Model, path);
        }
    }

    /// Open one entry through the dialog's own path. A file that is
    /// gone is refused by name and nothing else changes.
    pub(crate) fn open_recent(&mut self, kind: RecentKind, path: PathBuf) {
        let entry = RecentEntry::of(&path);
        if entry.missing {
            self.status = entry.refusal(kind);
            return;
        }
        match kind {
            RecentKind::Project => self.open_project_path(path),
            RecentKind::Model => self.import_path(path),
        }
    }

    /// "Clear recent": empty one list and write it back.
    pub(crate) fn clear_recent(&mut self, kind: RecentKind) {
        if self.recent.clear(kind) {
            self.persist_recent();
        }
        self.status = format!("recent {}s cleared", kind.noun());
    }

    fn persist_recent(&self) {
        let Some(path) = &self.recent_store else { return };
        // The refusal is logged where it happens (app::recent::save);
        // the lists in memory stay right for this session either way.
        let _ = recent::save(path, &self.recent);
    }
}
