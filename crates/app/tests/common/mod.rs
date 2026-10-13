//! Shared helpers for the app integration tests.
//!
//! Provenance: the CL text format is ours - a plain-text dump of a
//! generated [`Toolpath`], one line per move, coordinates as integers at
//! [`CL_SCALE`] resolution (the same fixed-point idea as the geo2d
//! bridge). Goldens under `tests/golden/` are committed generated
//! artifacts per AGENTS.md; `write_golden` makes a stale golden a test
//! failure, never a silent pass.

#![allow(dead_code)] // each test binary uses part of the surface

use std::path::PathBuf;

use hsmworks_app::cl::{MoveKind, Toolpath};

/// Golden coordinate resolution in mm: 0.001 = 1 um.
pub const CL_SCALE: f64 = 0.001;

/// Millimeters to golden units (microns), rounded. Generator output is
/// already validated finite, so the cast is safe here.
fn q(mm: f64) -> i64 {
    (mm / CL_SCALE).round() as i64
}

/// Deterministic text form of a toolpath: a header naming the format and
/// move count, the start point, then one line per move - kind
/// (Rapid/Feed/Plunge), destination coordinates, feed in mm/min when the
/// move has one. Nothing in the dump depends on the run: no timestamps,
/// no hash-map iteration order, full-precision-integer coordinates.
pub fn gen_to_cltext(t: &Toolpath) -> String {
    let mut s = format!("; HSMW-CL v1 scale={}mm moves={}\n", CL_SCALE, t.len());
    match t.start() {
        Some(p) => s.push_str(&format!("START {} {} {}\n", q(p.x), q(p.y), q(p.z))),
        None => s.push_str("START\n"),
    }
    for m in &t.moves {
        let k = match m.kind {
            MoveKind::Rapid => "Rapid",
            MoveKind::Feed => "Feed",
            MoveKind::Plunge => "Plunge",
        };
        s.push_str(&format!("{k} {} {} {}", q(m.to.x), q(m.to.y), q(m.to.z)));
        if let Some(f) = m.feed {
            s.push_str(&format!(" F{}", (f * 1000.0).round() as i64));
        }
        s.push('\n');
    }
    s
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("golden")
}

/// Compare `text` against `tests/golden/<name>`; with WRITE_GOLDEN set in
/// the environment, rewrite the file instead (regeneration workflow:
/// `WRITE_GOLDEN=1 cargo test -p hsmworks-app`, then read the diff and
/// commit only intended changes). A missing golden fails with a hint
/// rather than quietly generating a fresh one.
pub fn write_golden(name: &str, text: &str) {
    let dir = golden_dir();
    let path = dir.join(name);
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::create_dir_all(&dir).expect("cannot create tests/golden");
        std::fs::write(&path, text).expect("cannot write golden file");
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("cannot read golden {}: {e}; create it with WRITE_GOLDEN=1 cargo test", path.display())
    });
    assert_eq!(want, text, "golden {} is stale; regenerate with WRITE_GOLDEN=1 cargo test", path.display());
}
