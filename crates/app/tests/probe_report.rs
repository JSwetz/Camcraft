//! #575 acceptance: a probe program reports the deviation it measured on
//! the control before it writes the work offset, in the dialect's own
//! non-stopping message, or writes nothing where the dialect has none.
//! - Tormach PathPilot / LinuxCNC: one `(DEBUG, PROBE <axis> ERROR #19n
//!   <unit>)` per probed axis, after that axis's tolerance test and before
//!   the `G10 L20` (or the measure-only MSG/M0 stop); the unit word is
//!   `IN` in a G20 program and `MM` in a G21 one.
//! - Fanuc (custom macro B): no non-stopping display exists (#3000 alarms,
//!   #3006 stops), so nothing is added before the `G10 L2`.
//!
//! Provenance: the LinuxCNC G-code reference ("Messages" / "Comments":
//! DEBUG shows like MSG and substitutes #n), Fanuc's published custom
//! macro B documentation (#3000 / #3006); our own fixtures.
//!
//! Gate map (M4.4): covers assets/posts (tormach + fanuc probe result),
//! post::probe_rec, motion::probing.

use camcraft_app::geometry::Point3;
use camcraft_app::machine::parse_mch;
use camcraft_app::post::{self, cnc, engine, machine};
use camcraft_app::probing::{probe_cycle, ProbeDir, ProbeMode, ProbeParams};

const TORMACH: &str = include_str!("../../../assets/posts/tormach.cps");
const FANUC: &str = include_str!("../../../assets/posts/fanuc.cps");
const MX770: &str = include_str!("../../../assets/machines/tormach-770mx.mch");

fn cycle(mode: ProbeMode, dir: ProbeDir, target: Point3, radius: f64) -> ProbeParams {
    ProbeParams {
        mode,
        dir,
        dir_y: ProbeDir::YPos,
        target,
        radius,
        ball_diameter: 4.0,
        approach: 5.0,
        overtravel: 5.0,
        backoff: 1.0,
        fast_feed: 300.0,
        slow_feed: 30.0,
        depth: 3.0,
        inset: 5.0,
        clearance_z: 50.0,
    }
}

fn face() -> ProbeParams {
    cycle(ProbeMode::Face, ProbeDir::ZNeg, Point3::new(10.0, 5.0, 20.0), 0.0)
}
fn corner() -> ProbeParams {
    cycle(ProbeMode::Corner, ProbeDir::XPos, Point3::new(0.0, 0.0, 20.0), 0.0)
}

fn stream(p: &ProbeParams, set_wcs: Option<u8>) -> String {
    let t = probe_cycle(p).expect("probe cycle");
    let d = post::Dump {
        op_name: "Probe1",
        tool_number: 99,
        tool_name: "PROBE-4",
        tool_diameter: 4.0,
        rpm: 0.0,
        clearance_z: 50.0,
        retract_z: 50.0,
        probe_result: Some(post::probe_rec::ProbeResult { set_wcs, tol_mm: 0.5 }),
        ..post::Dump::default()
    };
    cnc::serialize(&post::dump(&t, &d))
}

fn tormach(p: &ProbeParams, set_wcs: Option<u8>) -> String {
    let m = machine::mirror(&parse_mch(MX770).expect("770MX parses"));
    engine::run(TORMACH, &m, &stream(p, set_wcs)).expect("post").program
}

/// A machine with no machine file posts in mm (G21).
fn tormach_mm(p: &ProbeParams, set_wcs: Option<u8>) -> String {
    engine::run(TORMACH, &serde_json::Value::Null, &stream(p, set_wcs)).expect("post").program
}

fn line_of(prog: &str, text: &str) -> usize {
    prog.lines().position(|l| l == text).unwrap_or_else(|| panic!("no line {text:?}:\n{prog}"))
}

fn line_starting(prog: &str, prefix: &str) -> usize {
    prog.lines().position(|l| l.starts_with(prefix)).unwrap_or_else(|| panic!("no line {prefix:?}..:\n{prog}"))
}

#[test]
fn tormach_face_reports_z_after_its_tolerance_test_and_before_g10() {
    let prog = tormach(&face(), Some(1));
    assert!(prog.starts_with("(Tormach 770MX)\nG20 "), "the 770MX is an inch machine:\n{prog}");
    let report = line_of(&prog, "(DEBUG, PROBE Z ERROR #190 IN)");
    assert_eq!(prog.matches("(DEBUG,").count(), 1, "one report per probed axis:\n{prog}");
    assert!(report > line_of(&prog, "(MSG, PROBE Z OUT OF TOLERANCE)"), "after the tolerance test:\n{prog}");
    assert!(prog.contains("o901 endif\n(DEBUG, PROBE Z ERROR #190 IN)\nG10 L20 P2 Z[#5422-#190]\n"), "{prog}");
}

/// The report names the program's unit: a G21 program says MM, so the
/// operator never reads a mm deviation as inches (or the other way round).
#[test]
fn tormach_mm_program_reports_in_mm() {
    let prog = tormach_mm(&face(), Some(1));
    assert!(prog.contains("\nG21 "), "a metric program:\n{prog}");
    assert!(prog.contains("o901 endif\n(DEBUG, PROBE Z ERROR #190 MM)\nG10 L20 P2 Z[#5422-#190]\n"), "{prog}");
    assert!(!prog.contains(" IN)"), "no inch word in a G21 program:\n{prog}");
    let both = tormach_mm(&corner(), Some(0));
    assert!(both.contains("(DEBUG, PROBE X ERROR #190 MM)\n"), "{both}");
    assert!(both.contains("(DEBUG, PROBE Y ERROR #191 MM)\n"), "{both}");
}

#[test]
fn tormach_corner_reports_each_axis_before_g10() {
    let prog = tormach(&corner(), Some(0));
    let g10 = line_starting(&prog, "G10 L20 ");
    let x = line_of(&prog, "(DEBUG, PROBE X ERROR #190 IN)");
    let y = line_of(&prog, "(DEBUG, PROBE Y ERROR #191 IN)");
    assert!(x > line_of(&prog, "(MSG, PROBE X OUT OF TOLERANCE)") && x < g10, "{prog}");
    assert!(y > line_of(&prog, "(MSG, PROBE Y OUT OF TOLERANCE)") && y < g10, "{prog}");
    assert_eq!(prog.matches("(DEBUG,").count(), 2, "{prog}");
}

#[test]
fn tormach_measure_only_reports_before_its_stop() {
    let prog = tormach(&face(), None);
    assert!(!prog.contains("G10"), "nothing is set:\n{prog}");
    assert!(prog.contains("(DEBUG, PROBE Z ERROR #190 IN)\n(MSG, CHECK PROBE RESULT #190..)\nM0\n"), "{prog}");
}

#[test]
fn fanuc_has_no_non_stopping_message_so_writes_no_report() {
    let out = engine::run(FANUC, &serde_json::Value::Null, &stream(&corner(), Some(0))).expect("post");
    let prog = out.program;
    let g10 = line_starting(&prog, "G10 L2 ");
    let y_test = line_starting(&prog, "IF[ABS[#191]GT");
    assert_eq!(g10, y_test + 1, "the offset write follows the last tolerance test directly:\n{prog}");
    assert!(!prog.contains("DEBUG") && !prog.contains("#3006"), "no report and no stop on a good probe:\n{prog}");
}
