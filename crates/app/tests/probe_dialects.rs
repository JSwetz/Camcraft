//! #173 acceptance: the probing hooks of the Haas, Mazak and Siemens 840D
//! posts, and the Heidenhain post's named refusal. Each voiced dialect
//! posts the same bore-centre probe the generic dialect's golden
//! (`nc/fanuc/probe_bore_centre.nc`, tests/probe_op.rs) posts, through
//! the whole post seam (dump -> stream text -> engine -> <dialect>.cps).
//!
//! Provenance: the public programming documentation each post cites in
//! its probing section (Haas Mill Operator's Manual; Mazak EIA/ISO
//! programming manual; SINUMERIK 840D sl Fundamentals / Job Planning);
//! our own fixtures.
//!
//! Gate map (M4.4): covers assets/posts (haas, mazak, sinumerik,
//! heidenhain probing), post::probe_rec, motion::probing.

mod common;

use camcraft_app::geometry::Point3;
use camcraft_app::post::{self, cnc, engine};
use camcraft_app::probing::{probe_cycle, ProbeDir, ProbeMode, ProbeParams};

const HAAS: &str = include_str!("../../../assets/posts/haas.cps");
const MAZAK: &str = include_str!("../../../assets/posts/mazak.cps");
const SINUMERIK: &str = include_str!("../../../assets/posts/sinumerik.cps");
const HEIDENHAIN: &str = include_str!("../../../assets/posts/heidenhain.cps");

/// The bore centre of tests/probe_op.rs: a 20 mm bore at (30, 30),
/// rim Z 20, four touches (seek + measure each).
fn bore_stream(set_wcs: Option<u8>, wcs: &str) -> String {
    let p = ProbeParams {
        mode: ProbeMode::Bore,
        dir: ProbeDir::ZNeg,
        dir_y: ProbeDir::YPos,
        target: Point3::new(30.0, 30.0, 20.0),
        radius: 10.0,
        ball_diameter: 4.0,
        approach: 5.0,
        overtravel: 5.0,
        backoff: 1.0,
        fast_feed: 300.0,
        slow_feed: 30.0,
        depth: 3.0,
        inset: 5.0,
        clearance_z: 50.0,
    };
    let t = probe_cycle(&p).expect("bore");
    let d = post::Dump {
        op_name: "Probe1",
        tool_number: 7,
        tool_name: "PROBE-4",
        tool_diameter: 4.0,
        rpm: 0.0,
        clearance_z: 50.0,
        retract_z: 50.0,
        wcs,
        probe_result: Some(post::probe_rec::ProbeResult { set_wcs, tol_mm: 0.5 }),
        ..post::Dump::default()
    };
    let recs = post::dump(&t, &d);
    assert!(!recs.iter().any(|r| matches!(r, cnc::Record::Warn { .. })), "a sane probe path carries no WARN");
    cnc::serialize(&recs)
}

fn post_ok(cps: &str, stream: &str) -> String {
    let out = engine::run(cps, &serde_json::Value::Null, stream).expect("post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(
        !out.program.lines().any(|l| l.split_whitespace().any(|w| w == "M3" || w == "M8" || w == "MOTON")),
        "a touch probe starts no spindle and no coolant:\n{}",
        out.program
    );
    out.program
}

#[test]
fn haas_bore_centre_posts_g31_skips() {
    let prog = post_ok(HAAS, &bore_stream(Some(1), "G54"));
    assert_eq!(prog.lines().filter(|l| l.starts_with("G31 ")).count(), 8, "four touches, seek + measure");
    assert!(prog.contains("IF[ABS[#5061-43]LT0.001]THEN#3000=2(PROBE NO CONTACT)"), "{prog}");
    assert!(prog.contains("#100=#5061"), "the measured trip is kept");
    assert!(prog.contains("G91 G1 X-1 Y0 Z0 F300"), "the back-off is incremental");
    assert!(prog.contains("G10 L2 P2 X[#5221+#190] Y[#5222+#191]"), "the result sets G55 from G54: {prog}");
    common::write_golden("nc/haas/probe_bore_centre.nc", &prog);
}

#[test]
fn haas_without_an_offset_to_set_stops_for_the_operator() {
    let prog = post_ok(HAAS, &bore_stream(None, "G54"));
    assert!(!prog.contains("G10"), "nothing is set");
    assert!(prog.contains("#3006=1(CHECK PROBE RESULT #190..)"), "{prog}");
}

#[test]
fn mazak_bore_centre_posts_g31_skips_with_goto_tests() {
    let prog = post_ok(MAZAK, &bore_stream(Some(1), "G54"));
    assert_eq!(prog.lines().filter(|l| l.starts_with("G31 ")).count(), 8, "four touches, seek + measure");
    assert!(!prog.contains("THEN"), "the branch is IF..GOTO, not IF..THEN");
    assert!(prog.contains("IF[ABS[#5061-43]GE0.001]GOTO9001\n#3000=2(PROBE NO CONTACT)\nN9001\n"), "{prog}");
    assert!(prog.contains("G91 G01 X-1 Y0 Z0 F300"), "the back-off is incremental");
    assert!(prog.contains("G10 L2 P2 X[#5221+#190] Y[#5222+#191]"), "the result sets G55 from G54: {prog}");
    // Every GOTO label is unique, so no jump can land on another test's.
    let labels: Vec<&str> = prog.lines().filter(|l| l.starts_with("N9")).collect();
    let mut uniq = labels.clone();
    uniq.sort_unstable();
    uniq.dedup();
    assert_eq!(labels.len(), uniq.len(), "unique labels: {labels:?}");
    common::write_golden("nc/mazak/probe_bore_centre.nc", &prog);
}

#[test]
fn mazak_without_an_offset_to_set_stops_for_the_operator() {
    let prog = post_ok(MAZAK, &bore_stream(None, "G54"));
    assert!(!prog.contains("G10"), "nothing is set");
    assert!(prog.contains("(CHECK PROBE RESULT #190..)\nM00\n"), "{prog}");
}

#[test]
fn sinumerik_bore_centre_posts_meas_blocks() {
    let prog = post_ok(SINUMERIK, &bore_stream(Some(1), "G54"));
    assert_eq!(prog.lines().filter(|l| l.contains(" MEAS=1 G1 ")).count(), 8, "four touches, seek + measure");
    assert!(!prog.contains("G31"), "a Siemens control measures with MEAS, not G31");
    assert!(prog.contains(" IF $AC_MEA[1]==0\n"), "a touch that never trips alarms: {prog}");
    assert!(prog.contains(" R50=$AA_MW[X]\n"), "the trip is read in workpiece coordinates");
    assert!(prog.contains(" G91 G1 X-1 Y0 Z0 F300\n"), "the back-off is incremental");
    assert!(prog.contains(" $P_UIFR[2,X,TR]=$P_UIFR[1,X,TR]+R90\n"), "the result sets G55 from G54: {prog}");
    assert!(prog.contains(" $P_UIFR[2,X,FI]=$P_UIFR[1,X,FI]\n"), "the fine part follows the active frame");
    common::write_golden("nc/sinumerik/probe_bore_centre.nc", &prog);
}

/// G58 / G59 are axial programmable offsets on an 840D, not settable
/// frames: a probe asked to set one is refused by name, never posted.
#[test]
fn sinumerik_refuses_setting_g59_by_name() {
    let out = engine::run(SINUMERIK, &serde_json::Value::Null, &bore_stream(Some(5), "G54")).expect("post");
    assert!(out.warnings.iter().any(|w| w.contains("G59") && w.contains("settable frames")), "{:?}", out.warnings);
    assert!(!out.program.contains("$P_UIFR"), "nothing is set");
}

/// Heidenhain's ISO manual states no skip move and no trip-position
/// variable (probing is the touch-probe cycles, another manual): the
/// post defines no probing hook and the engine refuses by name - a
/// counted warning, which blocks the post (posting::post_script).
#[test]
fn heidenhain_refuses_a_probe_by_name() {
    let out = engine::run(HEIDENHAIN, &serde_json::Value::Null, &bore_stream(Some(1), "G54")).expect("engine runs");
    for hook in ["onProbeMove", "onProbeRetreat", "onProbeResult"] {
        assert!(out.warnings.iter().any(|w| w.contains(hook)), "refused by {hook}: {:?}", out.warnings);
    }
}
