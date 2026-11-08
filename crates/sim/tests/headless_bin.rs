//! M13.3: the `camcraft-sim` headless binary - a slot cut in a box
//! removes its own volume and passes; a rapid through the stock is
//! flagged (exit 2); a missing file or a nonsense box exits 1 with a
//! message, never a panic (Prime Directive).
//!
//! Gate map (M4.4): covers camcraft-sim src/bin/camcraft-sim.rs.
//!
//! Provenance: our own fixtures, from the dexel removal law.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn tmp(name: &str, body: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("cc-sim-{}-{name}", std::process::id()));
    std::fs::write(&p, body).expect("write fixture");
    p
}

fn run(args: &[OsString]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_camcraft-sim")).args(args).output().expect("spawn")
}

/// The CLFile plus the stock box 0..40 x 0..20 x -10..0 and a 10 mm tool.
fn with_box(p: &Path) -> Vec<OsString> {
    let mut a = vec![p.as_os_str().to_os_string()];
    a.extend(["0", "0", "-10", "40", "20", "0", "10"].map(OsString::from));
    a
}

/// A 10 mm end mill slotting 1 mm deep from x=10 to x=30 along y=10.
const SLOT: &str = "START 10000 10000 5000\nPlunge 10000 10000 -1000 F300000\n\
Feed 30000 10000 -1000 F1000000\nRapid 30000 10000 5000\n";

#[test]
fn sim_bin_removes_a_slot() {
    let p = tmp("slot.cl", SLOT);
    let o = run(&with_box(&p));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success(), "{out}{}", String::from_utf8_lossy(&o.stderr));
    // A 20 x 10 band plus two 5 mm half discs, 1 mm deep.
    let v: f64 = out
        .lines()
        .find_map(|l| l.strip_prefix("removed volume: "))
        .and_then(|s| s.trim_end_matches(" mm^3").parse().ok())
        .expect("removed volume line");
    let want = 200.0 + 25.0 * std::f64::consts::PI;
    assert!((v - want).abs() < want * 0.03, "removed {v}, want ~{want}\n{out}");
    assert!(out.contains("verdict: ok"), "{out}");
    let _ = std::fs::remove_file(p);
}

#[test]
fn sim_bin_flags_a_rapid_through_stock() {
    let p = tmp(
        "rapid.cl",
        "START 10000 10000 5000\nRapid 10000 10000 -2000\nRapid 30000 10000 -2000\n",
    );
    let o = run(&with_box(&p));
    let out = String::from_utf8_lossy(&o.stdout);
    assert_eq!(o.status.code(), Some(2), "{out}");
    assert!(out.contains("rapids into the stock: 2") && out.contains("verdict: FLAGGED"), "{out}");
    let _ = std::fs::remove_file(p);
}

#[test]
fn sim_bin_refuses_bad_input() {
    let corrupt = tmp("corrupt.cl", "START 0 0 0\nFeed x y z\n");
    let good = tmp("good.cl", SLOT);
    let mut flipped = with_box(&good);
    flipped.swap(1, 4); // x0 = 40 > x1 = 0
    let mut nan = with_box(&good);
    nan[7] = "NaN".into();
    let cases = vec![
        with_box(Path::new("no-such-file.cl")),
        with_box(&corrupt),
        flipped,
        nan,
        vec![good.clone().into_os_string(), "1".into()],
        vec![],
    ];
    for args in cases {
        let o = run(&args);
        let err = String::from_utf8_lossy(&o.stderr);
        assert_eq!(o.status.code(), Some(1), "{args:?}: {err}");
        assert!(err.starts_with("camcraft-sim: ") && !err.contains("panicked"), "{args:?}: {err}");
    }
    let _ = std::fs::remove_file(corrupt);
    let _ = std::fs::remove_file(good);
}
