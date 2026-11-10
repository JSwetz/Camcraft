//! Pins the gate's plan selection (changed files -> planned steps), the slow list, the scanners and the
//! decision collection, on a throwaway workspace fixture.
//! Provenance: our own build law (AGENTS.md "Three gate tiers"), #748.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::plan::{self, SLOW_BINARIES, Step};
use crate::quick::{self, Diff, Quick};
use crate::{decisions, exec, parse_gate, scan, single};

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> Fixture {
    static N: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "xtask-fixture-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&root);
    let w = |rel: &str, text: &str| {
        let p = root.join(rel);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).expect("fixture dir");
        }
        std::fs::write(p, text).expect("fixture file");
    };
    w(
        "crates/app/tests/ops.rs",
        "//! Ops suite.\n//! Gate map (M4.4): covers app::operation (area,\n//! boundary), app::commands.\n//!\nuse x;\n\n#[test]\nfn op_area() {\n    assert!(true);\n}\n\n/// Heavy.\n#[test]\n#[ignore = \"heavy lane\"]\nfn op_heavy() {\n    big();\n}\n",
    );
    w(
        "crates/app/tests/cmds.rs",
        "//! Gate map: covers app::commands, motion::finish3d.\n\n#[test]\nfn cmd_undo() {}\n",
    );
    w("crates/app/tests/center_cut_multiaxis.rs", "//! Gate map: covers app::multiaxis.\n");
    w("crates/app/src/operation/area.rs", "pub fn a() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn area_unit() {}\n}\n");
    w("crates/app/src/commands.rs", "pub fn c() {}\n");
    w("crates/motion/tests/finish3d.rs", "//! Gate map: covers motion::finish3d.\n#[test]\nfn f3d() {}\n");
    w("crates/motion/tests/waterline.rs", "//! Gate map: covers motion::waterline.\n");
    w("crates/ui/src/egui_app/tests/structural.rs", "#[test]\nfn op_area_label() {}\n");
    w("docs/decisions/README.md", "format\n");
    w("docs/decisions/12-b.md", "\u{feff}# 12 b\n\n");
    w("docs/decisions/9-a.md", "# 9 a\n");
    Fixture(root)
}

fn quick_plan(root: &Path, changed: &[&str], untracked: &[&str], hunks: &[(&str, &str)], heavy: bool) -> Quick {
    let changed: Vec<String> = changed.iter().map(|s| (*s).to_string()).collect();
    let untracked: Vec<String> = untracked.iter().map(|s| (*s).to_string()).collect();
    let hunks: Vec<(String, String)> = hunks.iter().map(|(f, h)| ((*f).into(), (*h).into())).collect();
    let provider = move |f: &str| -> Vec<String> {
        hunks.iter().filter(|(hf, _)| hf == f).map(|(_, h)| h.clone()).collect()
    };
    let diff = Diff { changed: &changed, untracked: &untracked, hunks: &provider };
    quick::plan(root, &diff, heavy).expect("quick plan")
}

fn steps(q: Quick) -> Vec<Step> {
    match q {
        Quick::Plan(p) => p.steps,
        Quick::Batch(f) => panic!("unexpected batch fallback on {f}"),
    }
}

fn line(s: &Step) -> String {
    format!("{}{} | {}", if s.in_ui { "[ui] " } else { "" }, s.name, s.args.join(" "))
}

#[test]
fn slow_list_is_pinned() {
    assert_eq!(
        SLOW_BINARIES,
        &["motion:finish3d", "motion:pivot5d", "motion:flow", "app:center_cut_multiaxis"]
    );
    assert!(plan::is_slow("app", "center_cut_multiaxis"));
    assert!(!plan::is_slow("motion", "waterline"));
}

#[test]
fn manifest_or_shared_crate_falls_back_to_batch() {
    let fx = fixture();
    for f in ["Cargo.toml", "Cargo.lock", ".cargo/config.toml", "crates/geometry/src/a.rs", "crates/cl/x"] {
        assert_eq!(quick_plan(&fx.0, &[f], &[], &[], false), Quick::Batch(f.into()), "{f}");
    }
}

#[test]
fn changed_test_fn_runs_exact_and_heavy_fn_only_builds() {
    let fx = fixture();
    let f = "crates/app/tests/ops.rs";
    // Line 9 is inside op_area; line 16 inside the ignored op_heavy.
    let heavy_only = [(f, "@@ -16,0 +16,1 @@")];
    let got: Vec<String> = steps(quick_plan(&fx.0, &[f], &[], &heavy_only, false)).iter().map(line).collect();
    assert_eq!(got, ["app build-only (changed fns are heavy lane) | test -j 6 -p camcraft-app --no-run --test ops"]);
    // With a plain fn changed too, the fn step compiles the suite: no separate build-only step.
    let hunks = [(f, "@@ -9 +9 @@"), (f, "@@ -16,0 +16,1 @@")];
    let got: Vec<String> = steps(quick_plan(&fx.0, &[f], &[], &hunks, false)).iter().map(line).collect();
    assert_eq!(got, ["app ops (1 fn) | test -j 6 -p camcraft-app --test ops -- --test-threads=6 --exact op_area"]);
    let heavy: Vec<String> = steps(quick_plan(&fx.0, &[f], &[], &hunks, true)).iter().map(line).collect();
    assert_eq!(heavy.len(), 2);
    assert_eq!(heavy[1], "app ops (1 fn, ignored, release) | test --release -j 6 -p camcraft-app --test ops -- --test-threads=6 --ignored --exact op_heavy");
}

#[test]
fn suite_edit_outside_fns_and_new_suite_run_whole() {
    let fx = fixture();
    let f = "crates/app/tests/ops.rs";
    let got = steps(quick_plan(&fx.0, &[f], &[], &[(f, "@@ -5 +5 @@")], false));
    assert_eq!(line(&got[0]), "app suites | test -j 6 -p camcraft-app --no-fail-fast --test ops -- --test-threads=6");
    let got = steps(quick_plan(&fx.0, &[f], &[f], &[], false));
    assert_eq!(got.len(), 1);
}

#[test]
fn src_module_maps_to_gate_mapped_suites() {
    let fx = fixture();
    // operation/area.rs: the submodule is named in ops.rs's map, so only ops runs, plus the lib.
    let got = steps(quick_plan(&fx.0, &["crates/app/src/operation/area.rs"], &[], &[], false));
    assert_eq!(
        line(&got[0]),
        "app suites | test -j 6 -p camcraft-app --no-fail-fast --lib --test ops -- --test-threads=6"
    );
    // commands.rs: both app suites name app::commands.
    let got = steps(quick_plan(&fx.0, &["crates/app/src/commands.rs"], &[], &[], false));
    assert_eq!(
        line(&got[0]),
        "app suites | test -j 6 -p camcraft-app --no-fail-fast --lib --test cmds --test ops -- --test-threads=6"
    );
    // motion::finish3d is named by motion's own suite and, cross-crate, by app's cmds - slow or not,
    // quick runs what the map names.
    let got: Vec<String> =
        steps(quick_plan(&fx.0, &["crates/motion/src/finish3d.rs"], &[], &[], false)).iter().map(line).collect();
    assert_eq!(got.len(), 2);
    assert!(got[0].starts_with("motion suites") && got[0].contains("--lib --test finish3d"), "{got:?}");
    assert!(got[1].starts_with("app suites") && got[1].contains("--test cmds"), "{got:?}");
}

#[test]
fn shared_test_input_runs_every_suite_of_the_crate() {
    let fx = fixture();
    let got = steps(quick_plan(&fx.0, &["crates/motion/tests/common/mod.rs"], &[], &[], false));
    assert_eq!(
        line(&got[0]),
        "motion suites | test -j 6 -p camcraft-motion --no-fail-fast --lib --test finish3d --test waterline -- --test-threads=6"
    );
}

#[test]
fn nothing_in_crates_plans_nothing_and_xtask_runs_its_own_tests() {
    let fx = fixture();
    let q = quick_plan(&fx.0, &["AGENTS.md"], &[], &[], false);
    let Quick::Plan(p) = q else { panic!("batch") };
    assert!(p.steps.is_empty());
    assert!(p.notes.iter().any(|n| n.contains("nothing in crates/ changed")));
    let got = steps(quick_plan(&fx.0, &["xtask/src/main.rs"], &[], &[], false));
    assert_eq!(line(&got[0]), "xtask unit tests | test -j 6 -p xtask -- --test-threads=6");
}

#[test]
fn batch_skips_slow_binaries_and_release_runs_everything() {
    let fx = fixture();
    let b: Vec<String> = plan::batch(&fx.0).steps.iter().map(line).collect();
    assert!(b.contains(&"app suites (slow, release tier: center_cut_multiaxis) | test -j 6 -p camcraft-app --no-fail-fast --lib --test cmds --test ops -- --test-threads=6".to_string()), "{b:#?}");
    assert!(b.contains(&"motion suites (slow, release tier: finish3d) | test -j 6 -p camcraft-motion --no-fail-fast --lib --test waterline -- --test-threads=6".to_string()));
    assert_eq!(b[b.len() - 2], "[ui] ui suites (cwd crates/ui) | test -j 6 -p camcraft-ui --no-fail-fast -- --test-threads=6");
    assert!(b[b.len() - 1].starts_with("clippy -D warnings | clippy -j 6 -p camcraft-geometry -p camcraft-cl"));
    let r: Vec<String> = plan::release(&fx.0).steps.iter().map(line).collect();
    assert_eq!(r.len(), 8 + 1 + 1 + 2 + 1);
    assert!(r.iter().any(|s| s.starts_with("kernel suites")));
    assert_eq!(r[12], "[ui] ui snapshot lane (debug --ignored, cwd crates/ui) | test -j 6 -p camcraft-ui --lib --no-fail-fast -- --test-threads=6 --ignored snapshot");
}

#[test]
fn single_test_finds_suite_and_lib_fns() {
    let fx = fixture();
    let f = single::Filter { name: "op_area", krate: None, binary: None, exact: true };
    let got: Vec<String> = single::plan(&fx.0, &f).expect("plan").steps.iter().map(line).collect();
    assert_eq!(got, ["app ops | test -j 6 -p camcraft-app --test ops -- --test-threads=6 --exact op_area"]);
    // Substring: the ui structural fn lives in a src/ `tests` module, so it is a lib test.
    let f = single::Filter { name: "area", krate: None, binary: None, exact: false };
    let got: Vec<String> = single::plan(&fx.0, &f).expect("plan").steps.iter().map(line).collect();
    assert_eq!(got.len(), 3, "{got:?}");
    assert!(got.contains(&"[ui] ui lib | test -j 6 -p camcraft-ui --lib -- --test-threads=6 area".to_string()));
    let f = single::Filter { name: "op_heavy", krate: Some("camcraft-app"), binary: Some("ops"), exact: true };
    let got: Vec<String> = single::plan(&fx.0, &f).expect("plan").steps.iter().map(line).collect();
    assert_eq!(got, ["app ops (ignored, release) | test --release -j 6 -p camcraft-app --test ops -- --test-threads=6 --ignored --exact op_heavy"]);
    let f = single::Filter { name: "nope", krate: None, binary: None, exact: false };
    assert!(single::plan(&fx.0, &f).is_err());
}

#[test]
fn test_fn_spans_cover_attrs_through_closing_brace() {
    let text = "/// doc\n#[test]\n#[ignore]\nfn a() {\n    if x { y(); }\n}\n#[test]\npub async fn b() { }\n";
    let got = scan::test_fns(text).expect("scan");
    assert_eq!(got.len(), 2);
    assert_eq!((got[0].name.as_str(), got[0].start, got[0].end, got[0].ignored), ("a", 1, 6, true));
    assert_eq!((got[1].name.as_str(), got[1].start, got[1].end, got[1].ignored), ("b", 7, 8, false));
    assert_eq!(scan::gate_map("//! x\n//! Gate map: covers a,\n//! b.\n//!\n//! c\n").expect("map"), "//! Gate map: covers a, //! b.");
}

#[test]
fn cli_parses_the_gate_forms() {
    let a = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
    let g = parse_gate(&a("quick --base origin/main --heavy --dry-run")).expect("quick");
    assert_eq!((g.tier.as_str(), g.base.as_str(), g.heavy, g.dry), ("quick", "origin/main", true, true));
    let g = parse_gate(&a("test op_area --crate app --binary ops --exact --plan")).expect("test");
    assert_eq!(g.filter.as_deref(), Some("op_area"));
    assert_eq!((g.krate.as_deref(), g.binary.as_deref(), g.exact, g.dry), (Some("app"), Some("ops"), true, true));
    assert_eq!(parse_gate(&[]).expect("default").tier, "quick");
    assert!(parse_gate(&a("nightly")).is_err());
    assert!(parse_gate(&a("test")).is_err());
    assert!(parse_gate(&a("test a.b")).is_err());
}

#[test]
fn decisions_gathered_in_issue_order_and_removed() {
    let fx = fixture();
    assert!(decisions::collect(&fx.0, "1.2").is_err());
    let msg = decisions::collect(&fx.0, "0.17.0").expect("collect");
    assert_eq!(msg, "collected 2 decision file(s) into RECOMMENDED.md");
    let text = std::fs::read_to_string(fx.0.join("RECOMMENDED.md")).expect("read");
    assert_eq!(text, "\n## v0.17.0 - decisions gathered at the cut\n\n# 9 a\n\n# 12 b\n");
    assert!(fx.0.join("docs/decisions/README.md").is_file());
    assert_eq!(decisions::collect(&fx.0, "0.17.0").expect("again"), "no decision files to collect");
}

#[test]
fn log_names_stamps_and_notes_match_the_old_script() {
    assert_eq!(exec::log_name(3, "app ops (1 fn)"), "03-app_ops_1_fn_.log");
    assert_eq!(exec::stamp(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_794_355_200)), "20261111-000000");
    let log = "     Running tests\\ops.rs (target)\ntest result: FAILED. 1 passed\n";
    assert_eq!(exec::read_log(log, 101).expect("note"), "failed: tests\\ops.rs");
    assert_eq!(exec::read_log("error: could not compile `x`\n", 101).expect("note"), "compile error");
    assert_eq!(exec::read_log("", 0).expect("note"), "");
}
