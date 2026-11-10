//! Recent files (issue #662): the two lists are newest first,
//! de-duplicated by canonical path, capped (0 = off), survive a restart
//! through the per-user file, and a corrupt or unreadable file starts
//! empty with a reason - never a crash. Missing files stay listed and
//! are flagged so the drop-down can grey and refuse them by name.
//!
//! Provenance: our own tests, specced from the issue's "Done means".
//!
//! Gate map (M4.4): covers app::recent.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use camcraft_app::recent::{
    self, RecentEntry, RecentFiles, RecentKind, DEFAULT_LIMIT, ENV_VAR, FILE_NAME, MAX_LIMIT,
};

/// A fresh, empty folder for one test.
fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft_recent_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the test folder");
    // Canonical, so comparisons hold where temp is a symlink (macOS /var).
    std::fs::canonicalize(&d).expect("canonical test folder")
}

/// A real file in `d`, returned as its canonical path.
fn touch(d: &Path, name: &str) -> PathBuf {
    let p = d.join(name);
    std::fs::write(&p, b"x").expect("write a fixture file");
    p
}

#[test]
fn add_puts_the_newest_first_per_list() {
    let d = dir("order");
    let (a, b, m) = (touch(&d, "a.camproj"), touch(&d, "b.camproj"), touch(&d, "part.stl"));
    let mut r = RecentFiles::default();
    assert!(r.add(RecentKind::Project, &a));
    assert!(r.add(RecentKind::Project, &b));
    assert!(r.add(RecentKind::Model, &m));
    assert_eq!(r.list(RecentKind::Project), &[b.clone(), a.clone()]);
    assert_eq!(r.list(RecentKind::Model), &[m], "the model list is its own");
    assert!(!r.add(RecentKind::Project, &b), "re-adding the top entry changes nothing");
}

#[test]
fn a_repeat_moves_up_by_canonical_path() {
    let d = dir("dedup");
    std::fs::create_dir_all(d.join("sub")).expect("sub folder");
    let (a, b) = (touch(&d, "a.stl"), touch(&d, "b.stl"));
    let mut r = RecentFiles::default();
    r.add(RecentKind::Model, &a);
    r.add(RecentKind::Model, &b);
    // The same file spelled through `sub/..` is the same entry.
    r.add(RecentKind::Model, &d.join("sub").join("..").join("a.stl"));
    assert_eq!(r.list(RecentKind::Model), &[a, b], "one entry per file, the repeat on top");
}

#[test]
fn each_list_is_capped_and_a_smaller_cap_trims() {
    let d = dir("cap");
    let mut r = RecentFiles::default();
    assert_eq!(r.limit(), DEFAULT_LIMIT);
    for k in 0..DEFAULT_LIMIT + 3 {
        r.add(RecentKind::Project, &touch(&d, &format!("p{k}.camproj")));
    }
    let list = r.list(RecentKind::Project);
    assert_eq!(list.len(), DEFAULT_LIMIT, "capped at the default");
    assert_eq!(list[0], d.join(format!("p{}.camproj", DEFAULT_LIMIT + 2)), "newest kept");
    r.set_limit(3);
    assert_eq!(r.list(RecentKind::Project).len(), 3);
    assert_eq!(RecentFiles::with_limit(10_000).limit(), MAX_LIMIT, "a silly cap clamps");
}

#[test]
fn a_zero_cap_turns_recent_files_off() {
    let d = dir("off");
    let mut r = RecentFiles::with_limit(0);
    assert!(!r.add(RecentKind::Project, &touch(&d, "a.camproj")));
    assert!(r.list(RecentKind::Project).is_empty());
    assert!(!r.add(RecentKind::Model, Path::new("")), "an empty path is never an entry");
}

#[test]
fn clear_empties_one_list_only() {
    let d = dir("clear");
    let mut r = RecentFiles::default();
    r.add(RecentKind::Project, &touch(&d, "a.camproj"));
    r.add(RecentKind::Model, &touch(&d, "a.stl"));
    assert!(r.clear(RecentKind::Project));
    assert!(r.list(RecentKind::Project).is_empty());
    assert_eq!(r.list(RecentKind::Model).len(), 1, "the other list stays");
    assert!(!r.clear(RecentKind::Project), "clearing an empty list changes nothing");
}

#[test]
fn a_missing_file_is_flagged_by_name() {
    let d = dir("missing");
    let gone = touch(&d, "gone.camproj");
    let mut r = RecentFiles::default();
    r.add(RecentKind::Project, &gone);
    std::fs::remove_file(&gone).expect("delete the fixture");
    let e = RecentEntry::of(&r.list(RecentKind::Project)[0]);
    assert!(e.missing, "a deleted file reads as missing");
    assert_eq!(e.name, "gone.camproj");
    let why = e.refusal(RecentKind::Project);
    assert!(why.contains("gone.camproj") && why.contains("missing"), "{why}");
    assert!(!RecentEntry::of(&touch(&d, "here.stl")).missing);
}

#[test]
fn the_lists_survive_a_restart() {
    let d = dir("restart");
    let file = d.join("cfg").join(FILE_NAME);
    let (p, m) = (touch(&d, "a.camproj"), touch(&d, "a.stl"));
    let mut r = RecentFiles::with_limit(4);
    r.add(RecentKind::Project, &p);
    r.add(RecentKind::Model, &m);
    recent::save(&file, &r).expect("atomic save makes the folder and writes");
    // A "restart": a fresh read from disk only.
    let (back, note) = recent::load(&file);
    assert_eq!(note, None);
    assert_eq!(back, r, "the lists and the cap round-trip");
    let leftovers: Vec<_> = std::fs::read_dir(file.parent().expect("folder"))
        .expect("list the folder")
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "the atomic write leaves no temp file");
}

#[test]
fn no_file_yet_is_a_quiet_empty_start() {
    let (r, note) = recent::load(&dir("none").join(FILE_NAME));
    assert_eq!(r, RecentFiles::default());
    assert_eq!(note, None);
}

#[test]
fn a_corrupt_or_unreadable_file_starts_empty_with_a_reason() {
    let d = dir("corrupt");
    for (name, text) in [
        ("junk.json", "{ this is not json"),
        ("wrong.json", r#"{"format":"someone-else/9","projects":["/a"]}"#),
        ("types.json", r#"{"format":"camcraft-recent/1","projects":42}"#),
    ] {
        let f = d.join(name);
        std::fs::write(&f, text).expect("write the corrupt fixture");
        let (r, note) = recent::load(&f);
        assert_eq!(r, RecentFiles::default(), "{name}: starts empty");
        let why = note.unwrap_or_default();
        assert!(why.contains("starting empty"), "{name}: {why}");
    }
    // A folder where the file should be cannot be read as text.
    let folder = d.join("is_a_dir.json");
    std::fs::create_dir_all(&folder).expect("make the folder");
    let (r, note) = recent::load(&folder);
    assert_eq!(r, RecentFiles::default());
    assert!(note.is_some_and(|w| w.contains("not read")));
}

#[test]
fn a_hand_edited_file_is_cleaned_on_read() {
    let root = if cfg!(windows) { "C:\\x" } else { "/x" };
    let (a, b, c) = (Path::new(root).join("a.stl"), Path::new(root).join("b.stl"), Path::new(root).join("c.stl"));
    let text = serde_json::json!({
        "format": "camcraft-recent/1",
        "limit": 2,
        "projects": ["relative.camproj", ""],
        "models": [a, a, b, c],
        "future": true,
    })
    .to_string();
    let r = RecentFiles::from_json(&text).expect("a readable file");
    assert_eq!(r.limit(), 2);
    assert!(r.list(RecentKind::Project).is_empty(), "relative and empty paths are dropped");
    assert_eq!(r.list(RecentKind::Model), &[a, b], "repeats folded, then capped");
    let no_limit = RecentFiles::from_json(r#"{"format":"camcraft-recent/1"}"#).expect("minimal file");
    assert_eq!(no_limit.limit(), DEFAULT_LIMIT, "a missing limit is the default");
}

#[test]
fn the_config_folder_resolves_per_platform() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| OsString::from(*v))
    };
    let home = if cfg!(windows) { "C:\\Users\\u" } else { "/home/u" };
    let leak = |v: Vec<(&'static str, &'static str)>| -> &'static [(&'static str, &'static str)] {
        Box::leak(v.into_boxed_slice())
    };
    let over = leak(vec![(ENV_VAR, home), ("HOME", "/elsewhere")]);
    assert_eq!(recent::config_dir_for("linux", env(over)), Some(PathBuf::from(home)), "override wins");
    let rel = leak(vec![(ENV_VAR, "rel/dir"), ("HOME", home)]);
    assert_eq!(
        recent::config_dir_for("linux", env(rel)),
        Some(Path::new(home).join(".config").join("camcraft")),
        "a relative override is ignored"
    );
    let mac = leak(vec![("HOME", home)]);
    assert_eq!(
        recent::config_dir_for("macos", env(mac)),
        Some(Path::new(home).join("Library").join("Application Support").join("camcraft"))
    );
    let xdg = leak(vec![("HOME", home), ("XDG_CONFIG_HOME", home)]);
    assert_eq!(recent::config_dir_for("linux", env(xdg)), Some(Path::new(home).join("camcraft")));
    let win = leak(vec![("APPDATA", home)]);
    assert_eq!(recent::config_dir_for("windows", env(win)), Some(Path::new(home).join("camcraft")));
    assert_eq!(recent::config_dir_for("linux", env(leak(vec![]))), None, "no home, no file");
}
