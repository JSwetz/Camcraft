//! Doosan DNM post processor tests (Issue #614).
//!
//! Verifies the Doosan DNM Fanuc 0i/31i-M dialect:
//! - Through-spindle coolant (TSC) support (M88 or M8)
//! - 4th-axis rotary table brake clamp/unclamp (M10/M11)
//! - Fanuc AI Contour Control (AICC) high-speed smoothing (G05.1 Q1 / G05.1 Q0)
//! - Chip conveyor start/stop (M31/M33)
//! - Probing: standard Fanuc G31 skip and Renishaw Inspection Plus macros
//!
//! Gate map: covers app::post (doosan-dnm.cps), app::posting.

use camcraft_app::post::engine;
use std::path::{Path, PathBuf};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn dnm_post() -> String {
    read("assets/posts/doosan-dnm.cps")
}

fn dnm_machine() -> serde_json::Value {
    serde_json::from_str(&read("assets/machines/doosan-dnm.mch")).expect("dnm machine json")
}

fn default_vmc() -> serde_json::Value {
    serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("vmc machine json")
}

#[test]
fn doosan_dnm_posts_basic_milling_clean() {
    let cps = dnm_post();
    let mch = dnm_machine();
    let stream = read("crates/app/tests/golden/face_square_40x40.cl");
    let out = engine::run(&cps, &mch, &stream).expect("runs clean");

    assert!(out.warnings.is_empty(), "unexpected warnings: {:?}", out.warnings);
    let p = out.program;

    // Header & preamble
    assert!(p.starts_with("%\nO0001 (Doosan DNM 5700)\n"));
    assert!(p.contains("G90 G17 G54 G40 G49 G80"));

    // Linear motion & feed
    assert!(p.contains("G01 X45 Y5 Z-2 F500"));

    // Coolant and spindle stop
    assert!(p.contains("M9"));
    assert!(p.contains("M5"));

    // Retract and safe end
    assert!(p.contains("G91 G28 Z0"));
    assert!(p.contains("G28 X0 Y0"));
    assert!(p.contains("M30\n%"));
}

#[test]
fn doosan_dnm_through_spindle_coolant_modes() {
    let cps = dnm_post();
    let mch = dnm_machine();

    // 1. Through-tool coolant with default Doosan high-pressure TSC (M88 / M89)
    let stream_tsc = r#"; HSMW-CL v1 scale=0.001mm moves=2
PARAM Coolant through-tool
PARAM WorkOffset G54
TOOL 1 "10mm Flat Endmill"
PARAM SpindleSpeed 6000
SECTION FaceTSC
Rapid 0 0 10000
Feed 50000 0 -1000 F500000
SECTION END
"#;
    let out_tsc = engine::run(&cps, &mch, stream_tsc).expect("posts TSC");
    assert!(out_tsc.warnings.is_empty(), "{:?}", out_tsc.warnings);
    assert!(out_tsc.program.contains("M88"), "program should start TSC with M88:\n{}", out_tsc.program);
    assert!(out_tsc.program.contains("M89"), "program should shut off TSC with M89:\n{}", out_tsc.program);
    assert!(out_tsc.program.contains("M9"), "program should shut off coolant with M9:\n{}", out_tsc.program);

    // 2. Through-tool coolant configured with useM8ForTSC
    let stream_m8 = r#"; HSMW-CL v1 scale=0.001mm moves=2
PARAM Coolant through-tool
PARAM WorkOffset G54
TOOL 1 "10mm Flat Endmill"
PARAM SpindleSpeed 6000
SECTION FaceM8
Rapid 0 0 10000
Feed 50000 0 -1000 F500000
SECTION END
"#;
    let options_m8 = serde_json::json!({ "useM8ForTSC": true });
    let out_m8 = engine::run_with(&cps, &default_vmc(), stream_m8, &options_m8).expect("posts TSC M8");
    assert!(out_m8.warnings.is_empty(), "{:?}", out_m8.warnings);
    assert!(out_m8.program.contains("M8"), "program should command M8:\n{}", out_m8.program);
    assert!(!out_m8.program.contains("M88"), "program should not command M88 when useM8ForTSC is true");
}

#[test]
fn doosan_dnm_rotary_brake_clamp_unclamp() {
    let cps = dnm_post();
    let mch = default_vmc();

    // Rotary table 4th-axis indexing move
    let stream_rotary = r#"; HSMW-CL v1 scale=0.001mm moves=4
PARAM WorkOffset G54
TOOL 1 "10mm Endmill"
PARAM SpindleSpeed 5000
SECTION RotaryIndex
Rapid 0 0 10000 A90000
Feed 50000 0 -1000 F400000
Rapid 50000 0 10000 A180000
Feed 0 0 -1000 F400000
SECTION END
"#;
    let out = engine::run(&cps, &mch, stream_rotary).expect("posts rotary moves");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<&str> = out.program.lines().collect();

    // Find the A90 move
    let idx_a90 = lines.iter().position(|l| l.contains("A90")).expect("A90 commanded");
    // Verify unclamp M11 precedes A90 or is on A move
    let prior_lines = &lines[..idx_a90];
    assert!(
        prior_lines.iter().any(|l| l.contains("M11")),
        "M11 unclamp must precede rotary A positioning:\n{}",
        out.program
    );
    // Verify clamp M10 is commanded after positioning
    let post_lines = &lines[idx_a90..];
    assert!(
        post_lines.iter().any(|l| l.contains("M10")),
        "M10 clamp must follow rotary A positioning before/with cutting:\n{}",
        out.program
    );
}

#[test]
fn doosan_dnm_high_speed_smoothing_aicc() {
    let cps = dnm_post();
    let mch = dnm_machine();

    let stream = r#"; HSMW-CL v1 scale=0.001mm moves=5
PARAM WorkOffset G54
TOOL 1 "10mm Ball Endmill"
PARAM SpindleSpeed 8000
SECTION 3DFinish
Rapid 0 0 10000
Feed 10000 10000 -2000 F800000
Feed 20000 10000 -2000 F800000
CYCLE DRILL X20000 Y10000 Z-5000 R2000 F200000
Feed 30000 10000 -2000 F800000
SECTION END
"#;
    let options = serde_json::json!({ "smoothing": true });
    let out = engine::run_with(&cps, &mch, stream, &options).expect("posts smoothing");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);

    let lines: Vec<&str> = out.program.lines().collect();

    // G05.1 Q1 engaged for high-speed cutting
    assert!(lines.iter().any(|l| l.contains("G05.1 Q1")), "G05.1 Q1 must be armed:\n{}", out.program);

    // G05.1 Q0 disabled before cycle
    let cycle_idx = lines.iter().position(|l| l.contains("G81")).expect("cycle present");
    let smoothing_off = lines[..cycle_idx].iter().rposition(|l| l.contains("G05.1 Q0"));
    assert!(
        smoothing_off.is_some(),
        "G05.1 Q0 must precede canned drilling cycles:\n{}",
        out.program
    );
}

#[test]
fn doosan_dnm_chip_conveyor() {
    let cps = dnm_post();
    let mch = dnm_machine();

    let stream = r#"; HSMW-CL v1 scale=0.001mm moves=2
PARAM WorkOffset G54
TOOL 1 "12mm Endmill"
PARAM SpindleSpeed 4000
SECTION Rough
Rapid 0 0 10000
Feed 50000 0 -2000 F600000
SECTION END
"#;
    let options = serde_json::json!({ "chipConveyor": true });
    let out = engine::run_with(&cps, &mch, stream, &options).expect("posts conveyor");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);

    assert!(out.program.contains("M31"), "M31 chip conveyor start must be commanded:\n{}", out.program);
    assert!(out.program.contains("M33"), "M33 chip conveyor stop must be commanded before end:\n{}", out.program);
}

#[test]
fn doosan_dnm_probing_modes() {
    let cps = dnm_post();
    let mch = dnm_machine();

    // 1. Standard Fanuc G31 skip probing
    let stream_g31 = r#"; HSMW-CL v1 scale=0.001mm moves=3
PARAM WorkOffset G54
TOOL 99 "Touch Probe"
SECTION ProbeTouch
GUARD 0 0 5000 F 300000
TOUCH 0 0 -2000 EXPECT 0 0 0 F 50000 M 0
RETREAT 0 0 5000 EXPECT 0 0 5000 F 300000
SECTION END
"#;
    let out_g31 = engine::run(&cps, &mch, stream_g31).expect("posts standard probe");
    assert!(out_g31.warnings.is_empty(), "{:?}", out_g31.warnings);
    assert!(out_g31.program.contains("G31"), "standard probing uses G31:\n{}", out_g31.program);
    assert!(out_g31.program.contains("#3000=2(PROBE NO CONTACT)"));

    // 2. Renishaw Inspection Plus macro style
    let options_insp = serde_json::json!({ "probingStyle": "inspection-plus" });
    let out_insp = engine::run_with(&cps, &mch, stream_g31, &options_insp).expect("posts inspection plus");
    assert!(out_insp.warnings.is_empty(), "{:?}", out_insp.warnings);
    assert!(out_insp.program.contains("G65 P9810"), "Inspection Plus uses P9810 for protected positioning:\n{}", out_insp.program);
    assert!(out_insp.program.contains("G65 P9811"), "Inspection Plus uses P9811 for surface measurement:\n{}", out_insp.program);
}
