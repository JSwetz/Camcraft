//! #50: the Heidenhain DIN/ISO post writes the block form the control
//! documents, and no word the control reads as something else.
//! Provenance: HEIDENHAIN "TNC 640 User's Manual, ISO programming", ID
//! 892909-25, English, 9/2016 (content.heidenhain.de) - p. 130: blocks
//! are numbered, the first is `%name G71 *`, the last `N99999999 %name
//! G71 *`; pp. 66/71/131: blocks end in `*`, coordinates are signed
//! (`X+0 Y+38.5`); p. 752: the drilling cycles are G200-G209 (no
//! G81-G89); p. 756: G54 = datum shift, G73 = rotation, G80 = working
//! plane; p. 456: `FUNCTION DWELL TIME<seconds>`. These tests parse the
//! posted programs and assert exactly that.
//!
//! Gate map (M4.4): covers app::post (assets/posts/heidenhain.cps).

use camcraft_app::cl::{CycleKind, MoveKind};
use camcraft_app::post::{cnc, engine};
use serde_json::json;
use std::path::Path;

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn run(stream: &str) -> engine::PostOutput {
    let mch: serde_json::Value =
        serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    engine::run(&read("assets/posts/heidenhain.cps"), &mch, stream).unwrap_or_else(|e| panic!("must run: {e}"))
}

/// Every mill golden the conformance matrix posts, minus the tap.
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
    "arc_planes",
];

/// The manual's program frame, block by block; returns the block bodies.
fn check_form(nc: &str, what: &str) -> Vec<String> {
    let lines: Vec<&str> = nc.lines().collect();
    assert!(lines.len() >= 2, "{what}: a program has a begin and an end block");
    let first = lines[0];
    let name = first
        .strip_prefix('%')
        .and_then(|l| l.strip_suffix(" G71 *"))
        .unwrap_or_else(|| panic!("{what}: begin block is `%name G71 *`, got {first:?}"));
    assert!(
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)),
        "{what}: program name {name:?} uses only the manual's file-name characters"
    );
    assert_eq!(lines[lines.len() - 1], format!("N99999999 %{name} G71 *"), "{what}: end block");
    let mut last = 0u64;
    let mut bodies = Vec::new();
    for l in &lines[1..lines.len() - 1] {
        let (n, body) = l
            .strip_prefix('N')
            .and_then(|r| r.split_once(' '))
            .unwrap_or_else(|| panic!("{what}: unnumbered block {l:?}"));
        let n: u64 = n.parse().unwrap_or_else(|_| panic!("{what}: block number in {l:?}"));
        assert!(n > last && n < 99_999_999, "{what}: N{n} after N{last} - numbers rise, below the end block");
        last = n;
        let body = body.strip_suffix('*').unwrap_or_else(|| panic!("{what}: no `*` terminator: {l:?}"));
        for w in body.split(' ') {
            let mut c = w.chars();
            let head = c.next().unwrap_or(' ');
            if "XYZIJ".contains(head) {
                let rest = c.as_str();
                assert!(rest.starts_with('+') || rest.starts_with('-'), "{what}: unsigned coordinate {w} in {l:?}");
                assert!(rest[1..].parse::<f64>().is_ok(), "{what}: {w} is a number");
            }
            if head == 'G' {
                assert!(
                    matches!(w, "G00" | "G01" | "G02" | "G03" | "G17" | "G40" | "G90"),
                    "{what}: {w} is not a word this post may write (G54/G55/G73/G80/G81-G89 mean \
                     other things on a TNC): {l:?}"
                );
            }
            assert!(w != "M6", "{what}: M6 is machine-dependent; the T block calls the tool: {l:?}");
        }
        bodies.push(body.to_string());
    }
    bodies
}

#[test]
fn heidenhain_posts_the_manuals_numbered_signed_block_form() {
    for g in GOLDENS {
        let out = run(&read(&format!("crates/app/tests/golden/{g}.cl")));
        assert!(out.warnings.is_empty(), "{g}: {:?}", out.warnings);
        check_form(&out.program, g);
    }
}

fn mv(kind: MoveKind, x: f64, y: f64, z: f64, feed: Option<f64>) -> cnc::Record {
    cnc::Record::Move(cnc::MoveRec { kind, x, y, z, feed, center: None, ccw: false, plane: Default::default(), abc: None })
}

fn cyc(kind: CycleKind, bottom: f64, peck: f64, dwell: f64, first_move: usize) -> cnc::Record {
    cnc::Record::Cycle(cnc::CycleRec {
        kind,
        x: 10.0,
        y: 10.0,
        r_z: 2.0,
        bottom_z: bottom,
        entry_z: 0.0,
        peck,
        dwell,
        first_move,
        feed: Some(100.0),
    })
}

/// A dwelled drill and a full-retract deep hole, each with its real
/// expanded chain behind it, under a tool call and a spindle speed.
fn drilling(dwell_reaches_bottom: bool) -> String {
    use cnc::Record as R;
    let drill_bottom = if dwell_reaches_bottom { -6.0 } else { -5.0 };
    cnc::serialize(&[
        R::Param { name: "SpindleSpeed".into(), value: json!(3000) },
        R::Tool { number: 4, name: "6MM DRILL".into() },
        R::Section { begin: true },
        mv(MoveKind::Rapid, 10.0, 10.0, 10.0, None),
        cyc(CycleKind::Drill, -6.0, 0.0, 0.25, 1),
        mv(MoveKind::Rapid, 10.0, 10.0, 2.0, None),
        mv(MoveKind::Plunge, 10.0, 10.0, drill_bottom, Some(100.0)),
        mv(MoveKind::Rapid, 10.0, 10.0, 2.0, None),
        cyc(CycleKind::Deep, -10.0, 6.0, 0.0, 4),
        mv(MoveKind::Rapid, 10.0, 10.0, 2.0, None),
        mv(MoveKind::Plunge, 10.0, 10.0, -4.0, Some(100.0)),
        mv(MoveKind::Rapid, 10.0, 10.0, 2.0, None),
        mv(MoveKind::Plunge, 10.0, 10.0, -10.0, Some(100.0)),
        mv(MoveKind::Rapid, 10.0, 10.0, 2.0, None),
        mv(MoveKind::Rapid, 10.0, 10.0, 10.0, None),
        R::Section { begin: false },
    ])
}

#[test]
fn heidenhain_drilling_posts_its_chain_with_the_dwell_at_the_bottom() {
    let out = run(&drilling(true));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let b = check_form(&out.program, "drilling");
    let at = |s: &str| b.iter().position(|x| x == s).unwrap_or_else(|| panic!("no {s:?} in\n{}", out.program));
    assert_eq!(b[0], "T4 G17 S3000", "the manual's T block: number, spindle axis, speed");
    assert_eq!(b[1], "M3");
    // The drill: down at feed, dwell 0.25 s in seconds, out at rapid.
    let bottom = at("G01 X+10 Y+10 Z-6 F100");
    assert_eq!(b[bottom + 1], "FUNCTION DWELL TIME0.25", "{}", out.program);
    assert_eq!(b[bottom + 2], "G00 X+10 Y+10 Z+2");
    // The deep hole: its two plunges and full retracts, no dwell.
    let zs: Vec<&str> = b[bottom + 3..]
        .iter()
        .filter_map(|x| x.split(' ').find(|w| w.starts_with('Z')))
        .collect();
    assert_eq!(zs, ["Z+2", "Z-4", "Z+2", "Z-10", "Z+2", "Z+10"], "the chain, move for move");
    assert_eq!(b.iter().filter(|x| x.starts_with("FUNCTION DWELL")).count(), 1);
    assert_eq!(b[b.len() - 1], "M2");
}

#[test]
fn heidenhain_refuses_a_dwell_its_chain_never_reaches() {
    let out = run(&drilling(false));
    assert!(out.warnings.iter().any(|w| w.contains("drilling dwell")), "{:?}", out.warnings);
}

#[test]
fn heidenhain_refuses_tapping_and_a_non_first_work_offset_by_name() {
    let out = run(&read("crates/app/tests/golden/tap_plate.cl"));
    assert!(out.warnings.iter().any(|w| w.contains("tapping")), "{:?}", out.warnings);
    use cnc::Record as R;
    let s = cnc::serialize(&[
        R::Param { name: "WorkOffset".into(), value: json!("G55") },
        R::Section { begin: true },
        mv(MoveKind::Rapid, 0.0, 0.0, 5.0, None),
        R::Section { begin: false },
    ]);
    let out = run(&s);
    assert!(out.warnings.iter().any(|w| w.contains("work offset G55")), "{:?}", out.warnings);
    // The first work offset is the active preset: no word, no refusal.
    let s = s.replace("G55", "G54");
    let out = run(&s);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(!out.program.contains("G54"), "{}", out.program);
}
