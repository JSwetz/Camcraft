//! The Fanuc post golden law (M1.7b): `assets/posts/fanuc.cps` posts
//! every committed `.cl` golden to byte-identical G-code. The .nc
//! files join generator code and the post script in one lock - if
//! either changes meaning, this test says so, and the diff is the
//! review (UPDATE workflow per tests/golden/CLAUDE.md).

use hsmworks_app::post::{cnc, engine};
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

#[test]
fn fanuc_posts_every_golden_byte_exact() {
    let cps = read("assets/posts/fanuc.cps");
    let mch: serde_json::Value =
        serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    for name in GOLDENS {
        let stream = read(&format!("crates/app/tests/golden/{name}.cl"));
        let out = engine::run(&cps, &mch, &stream)
            .unwrap_or_else(|e| panic!("{name} must post: {e}"));
        let want = read(&format!("crates/app/tests/golden/nc/{name}.nc"));
        assert_eq!(out.program, want, "{name}.nc is stale; regenerate with the CLI (see tests/golden/CLAUDE.md)");
        assert!(out.warnings.is_empty(), "{name}: the gate counts warnings: {:?}", out.warnings);
    }
}

#[test]
fn cnc_roundtrip_posts_the_same_program() {
    // The superset law end-to-end: reading a .cl, serializing v1, and
    // reposting gives the identical program (the two input dialects
    // differ only in what they carry, not in what the script sees).
    let cps = read("assets/posts/fanuc.cps");
    let mch: serde_json::Value =
        serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    let stream = read("crates/app/tests/golden/drill_peck.cl");
    let a = engine::run(&cps, &mch, &stream).expect("legacy posts");
    let records = cnc::parse(&stream).expect("parses");
    let b = engine::run(&cps, &mch, &cnc::serialize(&records)).expect("v1 posts");
    assert_eq!(a.program, b.program);
}

#[test]
fn post_errors_are_refusals_not_output() {
    // Degenerate intermediates must refuse cleanly through the whole
    // stack (parse -> drive), never emit a partial program.
    let cps = read("assets/posts/fanuc.cps");
    for bad in ["Goto 1 2\n", "Feed X zz Y 0 Z 0\n", "TOOL abc\n"] {
        let e = engine::run(&cps, &serde_json::Value::Null, bad);
        assert!(e.is_err(), "degenerate input must error: {bad:?}");
    }
}
