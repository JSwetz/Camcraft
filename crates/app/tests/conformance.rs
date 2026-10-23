//! The built-in post conformance law (M3.3 B): every shipped `.cps`
//! posts every committed `.cl` golden to byte-identical G-code. This
//! is OUR conformance - the goldens lock each dialect's decisions
//! (word style, framing, cycle spelling), not any vendor's output; a
//! script change that moves a byte shows up here as a diff, and the
//! diff is the review (UPDATE workflow per tests/golden/CLAUDE.md).
//!
//! Regenerate after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p hsmworks-app --test conformance`
//! Fanuc's goldens live at the top of `golden/nc/` (M1.7b law, the
//! suite `fanuc_post.rs` already locks); the four stage-B dialects get
//! one subdirectory each.

use hsmworks_app::cl::{CycleKind, MoveKind};
use hsmworks_app::post::{cnc, engine};
use serde_json::json;
use std::path::{Path, PathBuf};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

const GOLDENS: &[&str] = &[
    "contour_disc_hole",
    "contour_plate_with_hole",
    "contour_tangential_lead",
    "face_square_40x40",
    "face_square_40x40_stepped",
    "pocket_waterline_plate_with_island",
    "pocket_zigzag_plate_with_island",
    "slot_fullwidth_centerline",
    "slot_wide_channel_rows",
    "trace_plate_with_hole",
    "drill_plate",
    "drill_peck",
    "tap_plate",
];

/// (registry name, .cps file, golden subdir - "" means the shared top).
const POSTS: &[(&str, &str, &str)] = &[
    ("Fanuc", "fanuc", ""),
    ("Haas", "haas", "haas"),
    ("Siemens 840D", "sinumerik", "sinumerik"),
    ("Heidenhain", "heidenhain", "heidenhain"),
    ("Mazak", "mazak", "mazak"),
];

#[test]
fn every_builtin_posts_every_golden_byte_exact() {
    let mch: serde_json::Value =
        serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    let write = std::env::var_os("WRITE_GOLDEN").is_some();
    let mut written = 0usize;
    for (name, cps_stem, sub) in POSTS {
        let cps = read(&format!("assets/posts/{cps_stem}.cps"));
        for g in GOLDENS {
            let stream = read(&format!("crates/app/tests/golden/{g}.cl"));
            let out = engine::run(&cps, &mch, &stream)
                .unwrap_or_else(|e| panic!("{name} on {g}: must post: {e}"));
            assert!(
                out.warnings.is_empty(),
                "{name} on {g}: the gate counts warnings: {:?}",
                out.warnings
            );
            let rel = if sub.is_empty() {
                format!("crates/app/tests/golden/nc/{g}.nc")
            } else {
                format!("crates/app/tests/golden/nc/{sub}/{g}.nc")
            };
            let path = repo(&rel);
            if write {
                // The golden_cl workflow: WRITE_GOLDEN rewrites and the
                // test passes quietly - the review is the git diff, not
                // a red run.
                std::fs::create_dir_all(path.parent().unwrap()).expect("golden dir");
                std::fs::write(&path, &out.program).unwrap_or_else(|e| panic!("{rel}: {e}"));
                written += 1;
                continue;
            }
            let want = read(&rel);
            assert_eq!(
                out.program, want,
                "{name}: {rel} is stale; regenerate with WRITE_GOLDEN=1"
            );
        }
    }
    let _ = written;
}

/// A stream the legacy `.cl` goldens cannot be: params (spindle, work
/// offset, slot origin), a tool block, a contour (rapid, feed, arc) and
/// a five-hole drilling group - dwelled drill, a two-hole peck run
/// (the modal-continuation branch), a deep hole, a tap. Every
/// dialect-specific word - `S.. M3`, `MOTON`, `T2 M6`, `G43 H2`, `D2`,
/// `G98`/its absence, `P..` - appears here, so the goldens that lock
/// them exist for all five scripts.
///
/// Each cycle anchors (`AT`) on the opening rapid of its own hole
/// chain, exactly as a v1 dump from the generator lays one out: that
/// is what puts the contour BEFORE the cycle group, where the
/// chain-suppression law (`if (inCycle) return`) leaves it alone.
/// Anchor every cycle at move 0 instead and normalization re-seats
/// the whole group ahead of the contour, whose moves then die in
/// that same guard - the stream must be shaped like a real one.
fn synth_stream() -> String {
    use cnc::Record as R;
    let rec = |kind: MoveKind, x: f64, y: f64, z: f64, feed: Option<f64>| {
        R::Move(cnc::MoveRec {
            kind,
            x,
            y,
            z,
            feed,
            center: None,
            ccw: false,
            abc: None,
        })
    };
    cnc::serialize(&[
        R::Param { name: "SpindleSpeed".into(), value: json!(12000) },
        R::Param { name: "WorkOffset".into(), value: json!("G55") },
        R::Param { name: "OriginX".into(), value: json!(10.0) },
        R::Param { name: "OriginY".into(), value: json!(20.0) },
        R::Param { name: "OriginZ".into(), value: json!(5.0) },
        R::Tool { number: 2, name: "10MM END MILL".into() },
        R::Section { begin: true },
        // The contour: moves 0..2, no cycle anchored at or below 2
        // until the hole group opens, so all three post as motion.
        rec(MoveKind::Rapid, 30.0, 30.0, 2.0, None),
        rec(MoveKind::Feed, 40.0, 30.0, -1.0, Some(800.0)),
        R::Move(cnc::MoveRec {
            kind: MoveKind::Feed,
            x: 40.0,
            y: 40.0,
            z: -1.0,
            feed: Some(500.0),
            center: Some((40.0, 35.0)),
            ccw: true,
            abc: None,
        }),
        // Hole chains: the approach rapid (moves 3..7) then the cycle
        // anchored on it. The rapids are the chain the cycle replaces
        // - suppressed by design while the group is open; the goldens
        // are what prove they stay suppressed identically everywhere.
        rec(MoveKind::Rapid, 50.0, 50.0, 2.0, None),
        R::Cycle(cnc::CycleRec {
            kind: CycleKind::Drill,
            x: 50.0,
            y: 50.0,
            r_z: 2.0,
            bottom_z: -8.0,
            entry_z: 0.0,
            peck: 0.0,
            dwell: 0.5,
            first_move: 3,
            feed: Some(120.0),
        }),
        rec(MoveKind::Rapid, 60.0, 50.0, 2.0, None),
        R::Cycle(cnc::CycleRec {
            kind: CycleKind::Peck,
            x: 60.0,
            y: 50.0,
            r_z: 2.0,
            bottom_z: -20.0,
            entry_z: 0.0,
            peck: 5.0,
            dwell: 0.0,
            first_move: 4,
            feed: Some(150.0),
        }),
        rec(MoveKind::Rapid, 60.0, 60.0, 2.0, None),
        R::Cycle(cnc::CycleRec {
            kind: CycleKind::Peck,
            x: 60.0,
            y: 60.0,
            r_z: 2.0,
            bottom_z: -20.0,
            entry_z: 0.0,
            peck: 5.0,
            dwell: 0.0,
            first_move: 5,
            feed: Some(150.0),
        }),
        rec(MoveKind::Rapid, 70.0, 60.0, 2.0, None),
        R::Cycle(cnc::CycleRec {
            kind: CycleKind::Deep,
            x: 70.0,
            y: 60.0,
            r_z: 2.0,
            bottom_z: -30.0,
            entry_z: 0.0,
            peck: 8.0,
            dwell: 0.0,
            first_move: 6,
            feed: Some(90.0),
        }),
        rec(MoveKind::Rapid, 70.0, 70.0, 2.0, None),
        R::Cycle(cnc::CycleRec {
            kind: CycleKind::Tap,
            x: 70.0,
            y: 70.0,
            r_z: 2.0,
            bottom_z: -12.0,
            entry_z: 0.0,
            peck: 0.0,
            dwell: 0.0,
            first_move: 7,
            feed: Some(240.0),
        }),
        R::Section { begin: false },
    ])
}

#[test]
fn the_synthesized_stream_locks_the_tool_and_spindle_words() {
    let mch: serde_json::Value =
        serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    let stream = synth_stream();
    let write = std::env::var_os("WRITE_GOLDEN").is_some();
    for (name, cps_stem, _) in POSTS {
        let cps = read(&format!("assets/posts/{cps_stem}.cps"));
        let out = engine::run(&cps, &mch, &stream)
            .unwrap_or_else(|e| panic!("{name} on the synth stream: must post: {e}"));
        assert!(
            out.warnings.is_empty(),
            "{name} synth: the gate counts warnings: {:?}",
            out.warnings
        );
        let rel = format!("crates/app/tests/golden/nc/synth_{cps_stem}.nc");
        let path = repo(&rel);
        if write {
            std::fs::write(&path, &out.program).unwrap_or_else(|e| panic!("{rel}: {e}"));
            continue;
        }
        let want = read(&rel);
        assert_eq!(
            out.program, want,
            "{name}: {rel} is stale; regenerate with WRITE_GOLDEN=1"
        );
    }
}

#[test]
fn the_dialects_actually_differ() {
    // A registry of five byte-identical scripts would pass the matrix
    // while shipping one post - the diversity check is part of the
    // conformance, so compare every pair on one stream (drill_peck:
    // cycles, tool change, spindle, the lot).
    let mch: serde_json::Value =
        serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    let stream = read("crates/app/tests/golden/drill_peck.cl");
    let texts: Vec<(&str, String)> = POSTS
        .iter()
        .map(|(name, stem, _)| {
            let cps = read(&format!("assets/posts/{stem}.cps"));
            (
                *name,
                engine::run(&cps, &mch, &stream)
                    .unwrap_or_else(|e| panic!("{name} on drill_peck: {e}"))
                    .program,
            )
        })
        .collect();
    for (i, (a, x)) in texts.iter().enumerate() {
        for (b, y) in &texts[i + 1..] {
            assert_ne!(x, y, "posts {a} and {b} emit identical programs");
        }
    }
}

#[test]
fn every_builtin_registry_entry_compiles() {
    // The registry text the app embeds must match the shipped files -
    // a stale include or a renamed file is a compile-time... no:
    // include_str! is compile-time, but the NAMES are the persistence
    // contract, so spell the registry out here against posting.rs.
    let names: Vec<&str> = hsmworks_app::posting::BUILTINS.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        names,
        POSTS.iter().map(|(n, _, _)| *n).collect::<Vec<_>>(),
        "the picker lists what the registry ships"
    );
}
