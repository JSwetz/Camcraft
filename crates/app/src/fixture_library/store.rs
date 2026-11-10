//! Where the user's fixture library lives and how it is edited (#339
//! slice 6): one `fixtures.camfixtures` in the user fixtures folder,
//! resolved like the user machines folder (#383 / the #287 decision):
//! `CAMCRAFT_FIXTURES`, else `<platform data dir>/camcraft/fixtures` -
//! a sibling of `camcraft/machines`. Saving an entry and deleting one
//! are file edits, not document commands: nothing here touches a world
//! or the undo history, and every write is all or nothing.
//!
//! Provenance: our own resolver from the platforms' published per-user
//! data locations (Windows `%APPDATA%`, macOS `~/Library/Application
//! Support`, the XDG Base Directory spec's `$XDG_DATA_HOME` /
//! `~/.local/share`); the atomic save is the write-temp-then-rename
//! pattern from the `std::fs::rename` docs.

use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

use super::{FixtureEntry, check_entry, parse, to_text};

/// The override: an absolute folder holding the library file.
pub const ENV_VAR: &str = "CAMCRAFT_FIXTURES";
/// The library's file name inside the folder.
pub const LIBRARY_FILE: &str = "fixtures.camfixtures";

/// The user fixtures folder from an environment lookup, for the named
/// platform (`std::env::consts::OS`). Pure, so the order is testable
/// without touching the process environment. `None` = no home to put
/// one in (the dialogs then say why they cannot save).
#[must_use]
pub fn user_dir_for(os: &str, get: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let abs = |k: &str| get(k).map(PathBuf::from).filter(|p| p.is_absolute());
    if let Some(dir) = abs(ENV_VAR) {
        return Some(dir);
    }
    let data = match os {
        "windows" => abs("APPDATA"),
        "macos" => abs("HOME").map(|h| h.join("Library").join("Application Support")),
        _ => abs("XDG_DATA_HOME").or_else(|| abs("HOME").map(|h| h.join(".local").join("share"))),
    }?;
    Some(data.join("camcraft").join("fixtures"))
}

/// The user fixtures folder for this process.
#[must_use]
pub fn user_dir() -> Option<PathBuf> {
    user_dir_for(std::env::consts::OS, |k| std::env::var_os(k))
}

/// The library file in `dir`.
#[must_use]
pub fn library_path(dir: &Path) -> PathBuf {
    dir.join(LIBRARY_FILE)
}

/// Why a library edit did not happen. Nothing was written in every case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryError {
    /// The file (or an entry to add) is refused, in words.
    Refused(String),
    /// These names are already in the library: ask before replacing.
    Exists(Vec<String>),
    /// No such entry to delete.
    NotFound(String),
    /// The disk refused (the OS error, in words).
    Io(String),
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LibraryError::Refused(why) => write!(f, "{why}"),
            LibraryError::Exists(names) => write!(f, "already in the library: {}", names.join(", ")),
            LibraryError::NotFound(n) => write!(f, "no fixture '{n}' in the library"),
            LibraryError::Io(e) => write!(f, "{e}"),
        }
    }
}

/// The library at `path`. A missing file is the empty library (nothing
/// saved yet); an unreadable or refused one is an error, never "empty".
pub fn load(path: &Path) -> Result<Vec<FixtureEntry>, LibraryError> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text).map_err(LibraryError::Refused),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(LibraryError::Io(format!("{}: {e}", path.display()))),
    }
}

/// Write `entries` to `path` atomically: a temp file in the same folder,
/// flushed, then renamed over the target. The folder is created when
/// missing; on any failure the temp file is removed and the target is
/// as it was.
pub fn save(path: &Path, entries: &[FixtureEntry]) -> Result<(), LibraryError> {
    use std::io::Write;
    let text = to_text(entries).map_err(LibraryError::Refused)?;
    let io = |e: std::io::Error| LibraryError::Io(format!("{}: {e}", path.display()));
    let dir = path.parent().ok_or_else(|| LibraryError::Io(format!("{} has no folder", path.display())))?;
    std::fs::create_dir_all(dir).map_err(io)?;
    let name = path.file_name().map_or_else(|| "fixtures".into(), |n| n.to_string_lossy().into_owned());
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let wrote = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = wrote {
        let _ = std::fs::remove_file(&tmp);
        return Err(io(e));
    }
    Ok(())
}

/// Add `new` to the library at `path`. A name already there is refused
/// as `Exists` (every clashing name) unless `replace` - the dialog asks
/// first; there is no silent "(2)" in a library, where the name is the
/// only handle the user has. Two entries of `new` with one name are
/// refused outright.
pub fn add_entries(path: &Path, new: Vec<FixtureEntry>, replace: bool) -> Result<(), LibraryError> {
    if new.is_empty() {
        return Err(LibraryError::Refused("no fixture to save".into()));
    }
    for (i, e) in new.iter().enumerate() {
        check_entry(e).map_err(LibraryError::Refused)?;
        if new[..i].iter().any(|o| o.name == e.name) {
            return Err(LibraryError::Refused(format!("fixture '{}' is saved twice", e.name)));
        }
    }
    let mut lib = load(path)?;
    let clash: Vec<String> =
        new.iter().filter(|e| lib.iter().any(|o| o.name == e.name)).map(|e| e.name.clone()).collect();
    if !clash.is_empty() && !replace {
        return Err(LibraryError::Exists(clash));
    }
    lib.retain(|o| !new.iter().any(|e| e.name == o.name));
    lib.extend(new);
    save(path, &lib)
}

/// Remove entry `name` from the library at `path` (a confirmed file
/// edit; there is no undo, as for a deleted machine file).
pub fn delete_entry(path: &Path, name: &str) -> Result<(), LibraryError> {
    let mut lib = load(path)?;
    let before = lib.len();
    lib.retain(|e| e.name != name);
    if lib.len() == before {
        return Err(LibraryError::NotFound(name.to_owned()));
    }
    save(path, &lib)
}
