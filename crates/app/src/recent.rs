//! Recent files (issue #662): the last N projects and the last N models
//! the user opened, imported or saved, kept per user in the OS config
//! folder so a re-open is one click instead of a file-dialog walk.
//!
//! The lists are app PREFERENCE state, not document state: nothing here
//! touches the world or the command history (AGENTS.md "Everything Is a
//! Command" - opening a recent file rides the same command as the
//! dialog; the list itself is outside undo). Each list is newest first,
//! de-duplicated by canonical path and capped at `limit` (default 10,
//! 0 turns the feature off). A missing, unreadable or corrupt file
//! starts both lists empty and says why, never a crash.
//!
//! Provenance: our own design, specced from issue #662's "Proposed
//! behaviour"; the folder per the platforms' published per-user config
//! locations (Windows `%APPDATA%`, macOS `~/Library/Application
//! Support`, the XDG Base Directory spec's `$XDG_CONFIG_HOME` /
//! `~/.config`) - the same resolver shape as `machine::store`'s data
//! folder (#287); the write is that module's write-temp-then-rename.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The override: an absolute folder that holds `recent.json`.
pub const ENV_VAR: &str = "CAMCRAFT_CONFIG_DIR";
/// The recents file's name inside the config folder.
pub const FILE_NAME: &str = "recent.json";
/// The format tag the file carries; any other tag is refused.
pub const FORMAT: &str = "camcraft-recent/1";
/// How many entries each list keeps unless the file says otherwise.
pub const DEFAULT_LIMIT: usize = 10;
/// The most the `limit` key may ask for; larger values clamp here.
pub const MAX_LIMIT: usize = 50;

/// Which of the two lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RecentKind {
    /// `.camproj` documents (Open).
    Project,
    /// Mesh models (Model import).
    Model,
}

impl RecentKind {
    /// The list's name in words, for status lines.
    #[must_use]
    pub fn noun(self) -> &'static str {
        match self {
            RecentKind::Project => "project",
            RecentKind::Model => "model",
        }
    }
}

/// The two recent lists and their cap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentFiles {
    limit: usize,
    projects: Vec<PathBuf>,
    models: Vec<PathBuf>,
}

impl Default for RecentFiles {
    fn default() -> Self {
        Self::with_limit(DEFAULT_LIMIT)
    }
}

/// The file layout. Unknown keys are ignored so a newer build's file
/// still reads; a missing `limit` is the default.
#[derive(Serialize, Deserialize)]
struct OnDisk {
    format: String,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    projects: Vec<String>,
    #[serde(default)]
    models: Vec<String>,
}

fn default_limit() -> usize {
    DEFAULT_LIMIT
}

/// The path a list stores for `path`: canonical when the file can be
/// resolved (so `a/../b.stl` and `b.stl` are one entry), else made
/// absolute as given. `None` = no absolute form (an empty path).
fn stored_form(path: &Path) -> Option<PathBuf> {
    if path.as_os_str().is_empty() {
        return None;
    }
    std::fs::canonicalize(path)
        .ok()
        .or_else(|| std::path::absolute(path).ok())
        .filter(|p| p.is_absolute())
}

impl RecentFiles {
    /// Empty lists with this cap (clamped to [`MAX_LIMIT`]).
    #[must_use]
    pub fn with_limit(limit: usize) -> Self {
        Self { limit: limit.min(MAX_LIMIT), projects: Vec::new(), models: Vec::new() }
    }

    /// The cap each list keeps; 0 = recent files are off.
    #[must_use]
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Change the cap; both lists drop what no longer fits.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit.min(MAX_LIMIT);
        self.projects.truncate(self.limit);
        self.models.truncate(self.limit);
    }

    /// One list, newest first.
    #[must_use]
    pub fn list(&self, kind: RecentKind) -> &[PathBuf] {
        match kind {
            RecentKind::Project => &self.projects,
            RecentKind::Model => &self.models,
        }
    }

    fn list_mut(&mut self, kind: RecentKind) -> &mut Vec<PathBuf> {
        match kind {
            RecentKind::Project => &mut self.projects,
            RecentKind::Model => &mut self.models,
        }
    }

    /// Put `path` at the top of its list: an earlier entry for the same
    /// file (by canonical path) moves up rather than repeating, and the
    /// oldest entry past the cap drops off. Returns whether the list
    /// changed (false when the feature is off or the path is empty).
    pub fn add(&mut self, kind: RecentKind, path: &Path) -> bool {
        if self.limit == 0 {
            return false;
        }
        let Some(entry) = stored_form(path) else {
            return false;
        };
        let limit = self.limit;
        let list = self.list_mut(kind);
        if list.first() == Some(&entry) {
            return false;
        }
        list.retain(|p| stored_form(p).as_ref() != Some(&entry) && *p != entry);
        list.insert(0, entry);
        list.truncate(limit);
        true
    }

    /// Empty one list. Returns whether anything was removed.
    pub fn clear(&mut self, kind: RecentKind) -> bool {
        let list = self.list_mut(kind);
        let had = !list.is_empty();
        list.clear();
        had
    }

    /// The file text: our own `camcraft-recent/1` JSON. A path that is
    /// not valid UTF-8 cannot ride JSON text whole and is left out
    /// rather than stored mangled.
    #[must_use]
    pub fn to_json(&self) -> String {
        let text = |l: &[PathBuf]| l.iter().filter_map(|p| p.to_str().map(str::to_owned)).collect();
        let disk = OnDisk {
            format: FORMAT.to_owned(),
            limit: self.limit,
            projects: text(&self.projects),
            models: text(&self.models),
        };
        serde_json::to_string_pretty(&disk).unwrap_or_else(|e| {
            tracing::warn!("recent files not serialised: {e}");
            String::new()
        })
    }

    /// Read the file text. Entries that are empty or not absolute are
    /// dropped, repeats keep their newest place, and each list is cut
    /// to the cap - a hand-edited file can never grow a list past it.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let disk: OnDisk = serde_json::from_str(text).map_err(|e| format!("not a recents file ({e})"))?;
        if disk.format != FORMAT {
            return Err(format!("format '{}' is not {FORMAT}", disk.format));
        }
        let mut out = Self::with_limit(disk.limit);
        let read = |l: Vec<String>, limit: usize| -> Vec<PathBuf> {
            let mut kept: Vec<PathBuf> = Vec::new();
            for p in l.into_iter().map(PathBuf::from) {
                if p.is_absolute() && !kept.contains(&p) {
                    kept.push(p);
                }
            }
            kept.truncate(limit);
            kept
        };
        out.projects = read(disk.projects, out.limit);
        out.models = read(disk.models, out.limit);
        Ok(out)
    }
}

/// One drop-down entry's facts: its file name, its full path (the
/// hover text) and whether the file is gone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentEntry {
    /// The file name alone (the full path when there is none).
    pub name: String,
    /// The stored absolute path.
    pub path: PathBuf,
    /// The file no longer exists (or is not a file): shown greyed,
    /// refused by name on click.
    pub missing: bool,
}

impl RecentEntry {
    /// Probe one stored path.
    #[must_use]
    pub fn of(path: &Path) -> Self {
        let name = path
            .file_name()
            .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
        Self { name, path: path.to_path_buf(), missing: !path.is_file() }
    }

    /// The refusal a click on a missing entry earns, naming the file.
    #[must_use]
    pub fn refusal(&self, kind: RecentKind) -> String {
        format!("{} is missing - recent {} not opened ({})", self.name, kind.noun(), self.path.display())
    }
}

/// The per-user config folder (`<platform config>/camcraft`) from an
/// environment lookup, for the named platform (`std::env::consts::OS`).
/// Pure, so the order is testable without the process environment.
/// `None` = no home to keep one in: the lists then live for the session.
#[must_use]
pub fn config_dir_for(os: &str, get: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let abs = |k: &str| get(k).map(PathBuf::from).filter(|p| p.is_absolute());
    if let Some(dir) = abs(ENV_VAR) {
        return Some(dir);
    }
    let base = match os {
        "windows" => abs("APPDATA"),
        "macos" => abs("HOME").map(|h| h.join("Library").join("Application Support")),
        _ => abs("XDG_CONFIG_HOME").or_else(|| abs("HOME").map(|h| h.join(".config"))),
    }?;
    Some(base.join("camcraft"))
}

/// This process's recents file, if a config folder resolves.
#[must_use]
pub fn store_path() -> Option<PathBuf> {
    config_dir_for(std::env::consts::OS, |k| std::env::var_os(k)).map(|d| d.join(FILE_NAME))
}

/// Read the recents file. No file yet is a quiet empty start; an
/// unreadable or corrupt one is an empty start too, with the reason
/// logged and returned so the caller can show it - never a crash.
pub fn load(path: &Path) -> (RecentFiles, Option<String>) {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (RecentFiles::default(), None),
        Err(e) => {
            let why = format!("recent files not read ({}): {e} - starting empty", path.display());
            tracing::warn!("{why}");
            return (RecentFiles::default(), Some(why));
        }
    };
    match RecentFiles::from_json(&text) {
        Ok(r) => (r, None),
        Err(e) => {
            let why = format!("recent files ignored ({}): {e} - starting empty", path.display());
            tracing::warn!("{why}");
            (RecentFiles::default(), Some(why))
        }
    }
}

/// Write the recents file atomically (temp file, then rename): a crash
/// mid-write leaves the previous file whole. A refusal is logged here
/// and returned.
pub fn save(path: &Path, recent: &RecentFiles) -> Result<(), String> {
    crate::machine::store::write_atomic(path, &recent.to_json()).map_err(|e| {
        let why = e.to_string();
        tracing::warn!("recent files {why} ({})", path.display());
        why
    })
}
