//! Pins the release cut: the version bump on Cargo.toml / Cargo.lock fixture text, the Cargo.lock diff
//! guard, the RELEASE_NOTES.md check, the staged zip layout, and the dry run's phase list on a
//! throwaway git repo.
//! Provenance: our own release law (AGENTS.md "Commits and Releases"), #749.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::release::{Cut, ReleaseArgs, parse};
use crate::release_bump::{bump_lock, bump_toml, check_notes, lock_diff_versions_only, unexpected_changes};
use crate::release_pack::{self as pack, Os};

struct Repo(PathBuf);

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).expect("fixture dir");
    }
    std::fs::write(p, text).expect("fixture file");
}

fn git(root: &Path, args: &[&str]) {
    let st = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .expect("git runs");
    assert!(st.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&st.stderr));
}

const TOML: &str = "[workspace]\nmembers = [\"a\"]\n\n[workspace.package]\nversion = \"0.16.0\"\nedition = \"2024\"\n\n[workspace.dependencies]\nregex = { version = \"1\" }\n";
const LOCK: &str = "# auto\nversion = 4\n\n[[package]]\nname = \"camcraft-app\"\nversion = \"0.16.0\"\ndependencies = [\n \"regex\",\n]\n\n[[package]]\nname = \"other\"\nversion = \"0.16.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"ab\"\n\n[[package]]\nname = \"xtask\"\nversion = \"0.16.0\"\n";

/// A fixture workspace: manifest, lock, notes for `version`, the shipped assets and docs.
fn repo(branch: &str, version: &str) -> Repo {
    static N: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "xtask-release-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&root);
    write(&root, "Cargo.toml", TOML);
    write(&root, "Cargo.lock", LOCK);
    write(&root, "RELEASE_NOTES.md", &format!("# CamCraft {version}\n\nnew things\n\n---\n\n# CamCraft 0.16.0\n"));
    for f in [
        "assets/posts/fanuc.cps",
        "assets/machines/vmc.json",
        "assets/ui/cube.png",
        "assets/ui/fonts/sans.ttf",
        "assets/scripts/job.js",
        "assets/themes/dark.json",
        "TESTING.md",
        "LICENSE",
        "NOTICE",
    ] {
        write(&root, f, "x");
    }
    git(&root, &["init", "-q", "-b", branch]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "fixture"]);
    Repo(root)
}

fn args(version: &str) -> ReleaseArgs {
    ReleaseArgs { version: version.into(), title: "release".into(), no_tag: true, dry: true, ..ReleaseArgs::default() }
}

fn banners(cut: &Cut) -> Vec<String> {
    cut.out.iter().filter(|l| l.starts_with("=== ")).cloned().collect()
}

#[test]
fn bump_toml_moves_only_the_workspace_version() {
    let (old, new) = bump_toml(TOML, "0.17.0").expect("bump");
    assert_eq!(old, "0.16.0");
    assert_eq!(new, TOML.replace("version = \"0.16.0\"", "version = \"0.17.0\""));
    assert!(new.contains("regex = { version = \"1\" }"));
    let crlf = TOML.replace('\n', "\r\n");
    let (_, new) = bump_toml(&crlf, "0.17.0").expect("crlf bump");
    assert!(new.contains("[workspace.package]\r\nversion = \"0.17.0\"\r\n"));
    assert!(bump_toml("[package]\nversion = \"1.0.0\"\n", "2.0.0").is_err());
}

#[test]
fn bump_lock_moves_workspace_packages_not_registry_ones() {
    let (new, n) = bump_lock(LOCK, "0.16.0", "0.17.0");
    assert_eq!(n, 2, "camcraft-app and xtask");
    assert!(new.contains("name = \"camcraft-app\"\nversion = \"0.17.0\"\n"));
    assert!(new.contains("name = \"xtask\"\nversion = \"0.17.0\"\n"));
    assert!(new.contains("name = \"other\"\nversion = \"0.16.0\"\nsource"), "registry package untouched");
    assert!(new.starts_with("# auto\nversion = 4\n"), "the lock format version is not a package version");
    let (crlf, n) = bump_lock(&LOCK.replace('\n', "\r\n"), "0.16.0", "0.17.0");
    assert_eq!(n, 2);
    assert!(crlf.contains("name = \"xtask\"\r\nversion = \"0.17.0\"\r\n"));
}

#[test]
fn lock_diff_guard_takes_version_lines_only() {
    let ok = "--- a/Cargo.lock\n+++ b/Cargo.lock\n@@ -1 +1 @@\n-version = \"0.16.0\"\n+version = \"0.17.0\"\n \n";
    assert_eq!(lock_diff_versions_only(ok), Ok(2));
    let bad = format!("{ok}-checksum = \"a\"\n+checksum = \"b\"\n");
    assert!(lock_diff_versions_only(&bad).is_err());
}

#[test]
fn notes_need_this_versions_section_above_the_history() {
    assert_eq!(check_notes("\u{feff}# CamCraft 0.17.0\n\nnew\n---\nold\n", "0.17.0"), Ok(2));
    assert!(check_notes("# CamCraft 0.17.0\nnew\n", "0.17.0").expect_err("no ---").contains("no '---'"));
    assert!(check_notes("# CamCraft 0.17.0\n---\n", "0.17.0").expect_err("none").contains("no notes"));
    assert!(check_notes("# CamCraft 0.16.0\nx\n---\n", "0.17.0").expect_err("wrong").contains("not 0.17.0"));
    assert!(check_notes("# CamCraft 0.17.0\n \n\n---\n", "0.17.0").expect_err("empty").contains("empty"));
}

#[test]
fn preflight_ignores_the_files_the_cut_commits() {
    let st = " M Cargo.toml\n M RELEASE_NOTES.md\n?? tmp/\n M \"odd name.rs\"\n";
    assert_eq!(unexpected_changes(st), vec!["tmp/".to_string(), "odd name.rs".to_string()]);
}

#[test]
fn zip_layout_mirrors_release_yml() {
    let r = repo("release/v0.17.0", "0.17.0");
    let bin = r.0.join("bin");
    let list = pack::stage_list(&r.0, &bin, Os::Windows).expect("stage list");
    let rel: Vec<String> = list.iter().map(|(_, p)| p.to_string_lossy().replace('\\', "/")).collect();
    assert_eq!(
        rel,
        [
            "camcraft-ui.exe",
            "camcraft-cli.exe",
            "post.exe",
            "assets/posts/fanuc.cps",
            "assets/machines/vmc.json",
            "assets/ui/cube.png",
            "assets/ui/fonts/sans.ttf",
            "assets/scripts/job.js",
            "assets/themes/dark.json",
            "RELEASE_NOTES.md",
            "TESTING.md",
            "LICENSE",
            "NOTICE",
        ]
    );
    assert_eq!(list[0].0, bin.join("camcraft-ui.exe"));
    let linux = pack::stage_list(&r.0, &bin, Os::Linux).expect("linux list");
    assert_eq!(linux[2].1, PathBuf::from("post"), "no .exe off Windows");
    // Staging copies every listed file (fake exes stand in for the build).
    for e in pack::EXES {
        write(&bin, &format!("{e}.exe"), "exe");
    }
    let stage = r.0.join("dist").join(pack::stage_name("0.17.0"));
    assert_eq!(pack::stage(&list, &stage), Ok(13));
    assert!(stage.join("assets/ui/fonts/sans.ttf").is_file());
    std::fs::remove_file(r.0.join("NOTICE")).expect("rm NOTICE");
    assert!(pack::stage_list(&r.0, &bin, Os::Windows).expect_err("no NOTICE").contains("NOTICE"));
}

#[test]
fn release_build_never_builds_xtask() {
    for os in [Os::Windows, Os::Linux, Os::Macos, Os::FreeBsd] {
        for step in pack::build_steps(os) {
            assert!(!step.iter().any(|a| a == "xtask"), "{os:?}: {step:?}");
            assert!(step.iter().any(|a| a == "-p"), "{os:?} builds named packages only");
        }
    }
    assert_eq!(pack::build_steps(Os::Macos).len(), 2, "universal: both architectures");
}

#[test]
fn parse_takes_release_ps1s_switches() {
    let a: Vec<String> =
        ["--version", "1.2.3", "--no-tag", "--skip-gate", "--skip-build", "--installer", "--dry-run", "--title", "T"]
            .map(String::from)
            .to_vec();
    let r = parse(&a).expect("parse");
    assert!(r.no_tag && r.skip_gate && r.skip_build && r.installer && r.dry);
    assert_eq!((r.version.as_str(), r.title.as_str()), ("1.2.3", "T"));
    assert!(parse(&["--no-tag".to_string()]).is_err(), "--version is required");
    assert!(parse(&["--version".to_string(), "1.2.3".into(), "--tag".into()]).is_err());
}

#[test]
fn dry_run_walks_the_ten_phases_and_changes_nothing() {
    let r = repo("release/v9.9.9", "9.9.9");
    let mut cut = Cut::new(&r.0, args("9.9.9"), Os::Windows);
    assert_eq!(cut.phases(), Ok(()));
    let phases = [
        "preflight",
        "gate",
        "version bump",
        "commit and tag",
        "release build",
        "verify post.exe version",
        "sign binaries",
        "assemble zip",
        "installer",
        "done",
    ];
    let want: Vec<String> =
        phases.iter().enumerate().map(|(i, p)| format!("=== Phase {}/10: {p} ===", i + 1)).collect();
    assert_eq!(banners(&cut), want);
    assert!(cut.out.iter().any(|l| l == "workspace version: 0.16.0 -> 9.9.9"));
    assert!(cut.out.iter().any(|l| l.starts_with("[dry-run] cargo build -j 6 --release -p camcraft-ui")));
    assert_eq!(std::fs::read_to_string(r.0.join("Cargo.toml")).expect("toml"), TOML, "dry run wrote nothing");
    assert!(!r.0.join("dist").exists());

    let mut linux = Cut::new(&r.0, ReleaseArgs { skip_gate: true, ..args("9.9.9") }, Os::Linux);
    assert_eq!(linux.phases(), Ok(()));
    assert!(banners(&linux).contains(&"=== Phase 8/10: package (scripts/package-linux.sh) ===".to_string()));
    assert!(linux.out.iter().any(|l| l == "skipped (--skip-gate)"));
}

#[test]
fn refusals_are_named() {
    let r = repo("main", "9.9.9");
    let mut on_main = Cut::new(&r.0, args("9.9.9"), Os::Windows);
    assert!(on_main.phases().expect_err("main").contains("not on main"));
    let mut tagging = Cut::new(&r.0, ReleaseArgs { no_tag: false, ..args("9.9.9") }, Os::Windows);
    assert!(tagging.phases().expect_err("tag").contains("branch-protected"));
    let mut installer = Cut::new(&r.0, ReleaseArgs { installer: true, ..args("9.9.9") }, Os::Linux);
    assert!(installer.phases().expect_err("installer").contains("Windows only"));
    let mut bad = Cut::new(&r.0, args("9.9"), Os::Windows);
    assert!(bad.phases().expect_err("version").contains("X.Y.Z"));
    let rb = repo("release/v9.9.9", "9.9.8");
    let mut notes = Cut::new(&rb.0, args("9.9.9"), Os::Windows);
    assert!(notes.phases().expect_err("notes").contains("not 9.9.9"));
}
