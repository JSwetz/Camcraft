//! The Tormach PathPilot post (#131): the program shape PathPilot reads
//! (no % fence, no O program number, no N numbers, M30), the machine's
//! own unit (an inch `.mch` posts G20 with every length and feed / 25.4,
//! a metric one G21 and mm), dwell in seconds, and the machine's stated
//! units reaching the script as `machine.units`.
//! Provenance: our own tests over our own post and machine file.
//!
//! Gate map (M4.4): covers app::post (tormach.cps), post::machine (units),
//! app::posting (registry).

use camcraft_app::machine::parse_mch;
use camcraft_app::post::{engine, machine};
use std::path::{Path, PathBuf};

const TORMACH_MCH: &str = "assets/machines/tormach-770mx.mch";
const METRIC_MCH: &str = "assets/machines/3axis-vmc.mch";

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn json(rel: &str) -> serde_json::Value {
    serde_json::from_str(&read(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// A machine as the app hands it to a post: the Tormach file goes through
/// the parser and the mirror (`posting::machine_value`'s path), the metric
/// reference file is already in the mirror's layout.
fn machine_for(mch: &str) -> serde_json::Value {
    if mch == TORMACH_MCH {
        machine::mirror(&parse_mch(&read(mch)).unwrap_or_else(|e| panic!("{mch}: {e}")))
    } else {
        json(mch)
    }
}

fn post(mch: &str, golden: &str) -> String {
    let cps = read("assets/posts/tormach.cps");
    let stream = read(&format!("crates/app/tests/golden/{golden}.cl"));
    let out =
        engine::run(&cps, &machine_for(mch), &stream).unwrap_or_else(|e| panic!("{golden}: {e}"));
    assert!(
        out.warnings.is_empty(),
        "{golden} on {mch}: {:?}",
        out.warnings
    );
    out.program
}

/// The letter and number of a word like `X-1.25`, if it is one.
fn word(t: &str) -> Option<(char, f64)> {
    let mut c = t.chars();
    let l = c.next()?;
    let n: f64 = c.as_str().parse().ok()?;
    Some((l, n))
}

#[test]
fn the_machine_file_states_inches_and_the_script_sees_them() {
    let m = parse_mch(&read(TORMACH_MCH)).expect("the Tormach machine parses");
    assert_eq!(m.units.as_deref(), Some("inch"));
    assert_eq!(
        machine::mirror(&m)["units"],
        "inch",
        "the mirror carries the unit"
    );
    assert_eq!(machine::script_props(&machine::mirror(&m))["units"], "inch");
    assert_eq!(
        machine::script_props(&json(TORMACH_MCH))["units"],
        "inch",
        "the raw door too"
    );
    // A metric file, and a file that states nothing, are mm.
    let metric = parse_mch(&read(METRIC_MCH)).expect("metric machine");
    assert_eq!(metric.units, None);
    assert_eq!(
        machine::script_props(&machine::mirror(&metric))["units"],
        "mm"
    );
    assert_eq!(machine::script_props(&serde_json::json!({}))["units"], "mm");
}

#[test]
fn a_pathpilot_program_has_no_fence_and_no_o_or_n_words() {
    for golden in ["face_square_40x40", "contour_disc_hole", "drill_peck"] {
        let nc = post(TORMACH_MCH, golden);
        let lines: Vec<&str> = nc.lines().collect();
        assert!(
            lines.first().is_some_and(|l| l.starts_with('(')),
            "{golden}: opens with the name comment"
        );
        assert_eq!(lines.last(), Some(&"M30"), "{golden}: M30 ends it");
        for l in &lines {
            // Tormach's guidelines: no % or $ fence. LinuxCNC reads an O
            // word as a subroutine label, and the post numbers no blocks.
            assert!(!l.contains('%') && !l.contains('$'), "{golden}: {l}");
            assert!(!l.starts_with('O') && !l.starts_with('N'), "{golden}: {l}");
        }
        assert!(
            nc.contains("G20 G17 G90 G91.1 G94"),
            "{golden}: inch header\n{nc}"
        );
        assert!(
            lines.contains(&"G64 P0.001"),
            "{golden}: blending tolerance on its own line"
        );
        // Z straight up, then G30, right before every tool change (the
        // tool-change streams are pinned in the belt-range test below).
        for (i, l) in lines.iter().enumerate() {
            if l.ends_with(" M6") {
                assert_eq!(
                    lines.get(i.saturating_sub(2)..i),
                    Some(&["G53 G0 Z0", "G30"][..]),
                    "{golden}: Z up and G30 before {l}"
                );
            }
        }
        assert!(
            !nc.contains("G21"),
            "{golden}: an inch program never states mm"
        );
        // The end: Z straight up in machine coordinates, then the
        // operator's preset G30 location, then M30 - PathPilot's own
        // ending order.
        let n = lines.len();
        assert_eq!(
            lines.get(n.saturating_sub(3)..),
            Some(&["G53 G0 Z0", "G30", "M30"][..]),
            "{golden}: the program ends Z up, G30, M30"
        );
    }
}

/// The same stream posted for an inch and a metric machine: the same
/// blocks, every length / 25.4 at four decimals, every feed / 25.4.
#[test]
fn an_inch_machine_posts_every_length_and_feed_in_inches() {
    for golden in [
        "face_square_40x40",
        "contour_disc_hole",
        "drill_peck",
        "pocket_zigzag_plate_with_island",
    ] {
        let inch = post(TORMACH_MCH, golden);
        let mm = post(METRIC_MCH, golden);
        assert!(
            mm.contains("G21 G17 G90 G91.1 G94"),
            "{golden}: metric header"
        );
        let (a, b): (Vec<&str>, Vec<&str>) = (inch.lines().collect(), mm.lines().collect());
        assert_eq!(
            a.len(),
            b.len(),
            "{golden}: the same blocks\n{inch}\n----\n{mm}"
        );
        let mut compared = 0usize;
        for (la, lb) in a.iter().zip(&b) {
            if la.starts_with('(') {
                assert!(lb.starts_with('('), "{golden}: {la} / {lb}");
                continue; // the machine-name comment
            }
            let (ta, tb): (Vec<&str>, Vec<&str>) =
                (la.split(' ').collect(), lb.split(' ').collect());
            assert_eq!(ta.len(), tb.len(), "{golden}: {la} / {lb}");
            for (wa, wb) in ta.iter().zip(&tb) {
                match (word(wa), word(wb)) {
                    (Some((l, x)), Some((lm, y))) if l == lm && "XYZIJKRQ".contains(l) => {
                        assert!(
                            (x * 25.4 - y).abs() <= 0.002,
                            "{golden}: {wa} is not {wb} in inches"
                        );
                        if let Some((_, dec)) = wa.split_once('.') {
                            assert!(dec.len() <= 4, "{golden}: {wa} has more than four decimals");
                        }
                        compared += 1;
                    }
                    (Some(('F', x)), Some(('F', y))) => {
                        assert!(
                            (x * 25.4 - y).abs() <= 0.5 + 0.13,
                            "{golden}: {wa} is not {wb} per minute"
                        );
                        compared += 1;
                    }
                    (Some(('G', _)), _) if *wa == "G20" => assert_eq!(*wb, "G21"),
                    // The G64 tolerance: 0.001 in, and its round metric twin.
                    (Some(('P', _)), _) if *wa == "P0.001" => assert_eq!(*wb, "P0.025"),
                    (Some(('M', _)), _) => {} // coolant codes are the machine's own
                    _ => assert_eq!(wa, wb, "{golden}: {la} / {lb}"),
                }
            }
        }
        assert!(compared > 0, "{golden}: some length was compared");
    }
}

/// LinuxCNC's G82/G86/G89 dwell P is in seconds, not milliseconds.
#[test]
fn a_dwell_posts_in_seconds() {
    let cps = read("assets/posts/tormach.cps");
    let stream = read("crates/app/tests/golden/drill_peck.cl")
        .replace("; CYCLE Peck", "; CYCLE Spot")
        .replace("DWELL0", "DWELL500");
    let out = engine::run(&cps, &machine_for(TORMACH_MCH), &stream).expect("posts");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let g82 = out
        .program
        .lines()
        .find(|l| l.starts_with("G82"))
        .unwrap_or_else(|| panic!("{}", out.program));
    assert!(
        g82.split(' ').any(|w| w == "P0.5"),
        "half a second, not 500: {g82}"
    );
}

#[test]
fn the_registry_ships_the_tormach_post_for_mills() {
    use camcraft_app::posting;
    assert!(
        posting::BUILTINS
            .iter()
            .any(|(n, _)| *n == "Tormach PathPilot")
    );
    assert!(posting::post_serves(
        "Tormach PathPilot",
        camcraft_app::machine::Kind::Mill
    ));
    assert!(!posting::post_serves(
        "Tormach PathPilot",
        camcraft_app::machine::Kind::Lathe
    ));
}

/// The 770-family belt: a speed only one range serves names that range
/// in a comment before the spindle starts; a tool number outside
/// PathPilot's table is refused by name.
#[test]
fn the_belt_range_is_named_and_a_stray_tool_number_is_refused() {
    let cps = read("assets/posts/tormach.cps");
    let mch = machine_for(TORMACH_MCH);
    let run = |rpm: u32, tool: u32| {
        let stream = format!(
            "PARAM SpindleSpeed {rpm}\nTOOL {tool} T{tool}\nSECTION\nRapid X 0 Y 0 Z 5000\nENDSECTION\n"
        );
        engine::run(&cps, &mch, &stream).expect("runs")
    };
    let high = run(8000, 1);
    assert!(
        high.program
            .contains("(SPINDLE 8000 RPM NEEDS THE HIGH BELT RANGE)"),
        "{}",
        high.program
    );
    let low = run(300, 1);
    assert!(
        low.program
            .contains("(SPINDLE 300 RPM NEEDS THE LOW BELT RANGE)"),
        "{}",
        low.program
    );
    let both = run(2000, 1);
    assert!(!both.program.contains("BELT RANGE"), "{}", both.program);
    assert!(both.warnings.is_empty(), "{:?}", both.warnings);
    let stray = run(2000, 1001);
    assert!(
        stray.warnings.iter().any(|w| w.contains("1-1000")),
        "{:?}",
        stray.warnings
    );
}

/// Tormach documents G84 for PathPilot: the tap posts as G84 with the
/// stream's pitch-times-rpm feed (in inches per minute on an inch
/// machine) and no P - PathPilot sizes the bottom dwell itself.
#[test]
fn a_tap_posts_g84_with_its_feed_and_no_dwell_word() {
    let cps = read("assets/posts/tormach.cps");
    // A tap marker carrying its synthesized feed: 1270 mm/min = 50 in/min.
    let stream =
        read("crates/app/tests/golden/tap_plate.cl").replace("DWELL0 AT1", "DWELL500 F1270000 AT1");
    let out = engine::run(&cps, &machine_for(TORMACH_MCH), &stream).expect("posts");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let g84 = out
        .program
        .lines()
        .find(|l| l.starts_with("G84"))
        .unwrap_or_else(|| panic!("{}", out.program));
    assert!(
        !g84.split(' ').any(|w| w.starts_with('P')),
        "no dwell word, even with a dwell asked: {g84}"
    );
    assert!(
        g84.split(' ').any(|w| w == "F50"),
        "the tap feed in inches per minute: {g84}"
    );
}

/// Between tools, Z goes straight up in machine coordinates before the
/// G30 that may move X, Y and Z together - PathPilot's own Z-first order -
/// and the same holds for the first tool of the program.
#[test]
fn every_tool_change_lifts_z_before_g30() {
    let cps = read("assets/posts/tormach.cps");
    let stream = "PARAM SpindleSpeed 2000\nTOOL 1 T1\nSECTION\nRapid X 0 Y 0 Z 5000\nFeed X 1000 Y 0 Z -1000 F 500000\nENDSECTION\n\
                  PARAM SpindleSpeed 2000\nTOOL 2 T2\nSECTION\nRapid X 0 Y 0 Z 5000\nFeed X 1000 Y 0 Z -1000 F 500000\nENDSECTION\n";
    let out = engine::run(&cps, &machine_for(TORMACH_MCH), stream).expect("posts");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<&str> = out.program.lines().collect();
    let changes: Vec<usize> = (0..lines.len())
        .filter(|i| lines[*i].ends_with(" M6"))
        .collect();
    assert_eq!(changes.len(), 2, "two tool changes\n{}", out.program);
    for i in changes {
        assert_eq!(
            lines.get(i.saturating_sub(2)..i),
            Some(&["G53 G0 Z0", "G30"][..]),
            "Z up, then G30, before {}\n{}",
            lines[i],
            out.program
        );
    }
}
