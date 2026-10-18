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
/// (Rapid/Feed/Plunge, or `Arc` when the move carries a circular
/// segment), destination coordinates, feed in mm/min when the move has
/// one, and for an arc the center `I`/`J` plus sense `S` (+1 ccw, -1
/// cw). v2 (the `Arc` token) appears only when the path carries arcs;
/// straight paths dump exactly as in v1. v3 adds `; CYCLE` comment
/// lines - one per canned cycle, in hole order - so drilling goldens
/// diff the POSTING INTENT (what M1.7's post must emit as G73/G81/G83/
/// G84 blocks) alongside the expanded move chain that verify and the
/// simulator read. Lengths are microns like coordinates; dwell is
/// milliseconds. `AT` is the cycle's first move index: the anchor of
/// the agreement law that pins chain and record to each other. Nothing
/// in the dump depends on the run: no timestamps, no hash-map iteration
/// order, full-precision-integer coordinates.
pub fn gen_to_cltext(t: &Toolpath) -> String {
    let version = if !t.cycles.is_empty() {
        3
    } else if t.has_arcs() {
        2
    } else {
        1
    };
    let mut s = format!("; HSMW-CL v{version} scale={CL_SCALE}mm moves={}\n", t.len());
    match t.start() {
        Some(p) => s.push_str(&format!("START {} {} {}\n", q(p.x), q(p.y), q(p.z))),
        None => s.push_str("START\n"),
    }
    for m in &t.moves {
        let mut k = match m.kind {
            MoveKind::Rapid => "Rapid",
            MoveKind::Feed => "Feed",
            MoveKind::Plunge => "Plunge",
        };
        let mut arc = String::new();
        if let Some(a) = m.arc {
            // An arc is always a planar feed (`push_arc` refuses
            // anything else), so the token loses nothing but the
            // redundancy.
            k = "Arc";
            arc = format!(
                " I{} J{} S{}",
                q(a.center.x),
                q(a.center.y),
                if a.ccw { 1 } else { -1 }
            );
        }
        s.push_str(&format!("{k} {} {} {}", q(m.to.x), q(m.to.y), q(m.to.z)));
        if let Some(f) = m.feed {
            s.push_str(&format!(" F{}", (f * 1000.0).round() as i64));
        }
        s.push_str(&arc);
        s.push('\n');
    }
    for c in &t.cycles {
        let kind = match c.kind {
            hsmworks_app::cl::CycleKind::Spot => "Spot",
            hsmworks_app::cl::CycleKind::Drill => "Drill",
            hsmworks_app::cl::CycleKind::Peck => "Peck",
            hsmworks_app::cl::CycleKind::Deep => "Deep",
            hsmworks_app::cl::CycleKind::Tap => "Tap",
            hsmworks_app::cl::CycleKind::Bore => "Bore",
            hsmworks_app::cl::CycleKind::BackBore => "BackBore",
        };
        s.push_str(&format!(
            "; CYCLE {kind} X{} Y{} RZ{} BZ{} ENTRY{} PECK{} DWELL{} AT{}\n",
            q(c.xy.x),
            q(c.xy.y),
            q(c.r_z),
            q(c.bottom_z),
            q(c.entry_z),
            q(c.peck),
            (c.dwell * 1000.0).round() as i64,
            c.first_move,
        ));
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
