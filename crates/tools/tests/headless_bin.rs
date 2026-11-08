//! M13.3: the `camcraft-tools` headless binary - the built-in catalog
//! lists sane, speeds come from the cutting table, and an unknown
//! material or nonsense diameter exits non-zero with a message, never
//! a panic (Prime Directive).
//!
//! Gate map (M4.4): covers camcraft-tools src/bin/camcraft-tools.rs.
//!
//! Provenance: our own fixtures; n = Vc*1000/(pi*D) per the handbooks.

use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_camcraft-tools")).args(args).output().expect("spawn")
}

#[test]
fn tools_bin_lists_a_sane_catalog() {
    let o = run(&["list"]);
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success(), "{out}{}", String::from_utf8_lossy(&o.stderr));
    assert!(out.lines().count() > 5 && !out.contains("INSANE"), "{out}");
}

#[test]
fn tools_bin_computes_speeds() {
    let o = run(&["speeds", "aluminium_6061", "carbide", "10", "3"]);
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success(), "{out}");
    // 250 m/min on a 10 mm cutter: 250000 / (pi * 10) = 7958 rpm.
    assert!(out.contains("rpm: 7958.."), "{out}");
}

#[test]
fn tools_bin_refuses_bad_input() {
    for args in [
        &["speeds", "unobtainium", "carbide", "10", "3"][..],
        &["speeds", "brass", "diamond", "10", "3"],
        &["speeds", "brass", "hss", "NaN", "3"],
        &["speeds", "brass", "hss", "-4", "3"],
        &["speeds", "brass", "hss", "10", "0"],
        &["speeds", "brass"],
        &["frobnicate"],
        &[],
    ] {
        let o = run(args);
        let err = String::from_utf8_lossy(&o.stderr);
        assert_eq!(o.status.code(), Some(1), "{args:?}: {err}");
        assert!(err.starts_with("camcraft-tools: ") && !err.contains("panicked"), "{args:?}: {err}");
    }
}
