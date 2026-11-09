//! #370: a vendor tool STEP's named coordinate systems (ISO 13399 PCS,
//! MCS, CIP, CSW...) read as plain data; a file without them names why;
//! degenerate, non-finite or ambiguous ones are refused by name.
//! Fixtures are hand-written Part 21 text in testdata/models/.
//!
//! Gate map (M4.4): covers app::geometry.

use std::path::{Path, PathBuf};

use camcraft_app::geometry::import::step_csys::{read_tool_csys, CsysRole, ToolCsysReport};
use camcraft_app::geometry::import::ImportError;

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models")
        .join(name)
}

fn read(name: &str) -> ToolCsysReport {
    let bytes = std::fs::read(testdata(name)).expect("fixture reads");
    read_tool_csys(&bytes).expect("fixture parses")
}

fn close(a: [f64; 3], b: [f64; 3]) -> bool {
    a.iter().zip(b).all(|(u, v)| (u - v).abs() < 1e-12)
}

#[test]
fn iso_13399_coordinate_systems_are_found_and_classified() {
    let r = read("tool_csys_iso13399.step");
    assert!(r.refused.is_empty(), "{:?}", r.refused);
    assert_eq!(r.none_found(), None);

    let pcs = r.find(CsysRole::Primary).expect("PCS");
    assert!(close(pcs.origin, [0.0, 0.0, 0.0]));
    assert!(close(pcs.z, [0.0, 0.0, 1.0]) && close(pcs.x, [1.0, 0.0, 0.0]));

    let mcs = r.find(CsysRole::Mounting).expect("MCS");
    assert!(close(mcs.origin, [0.0, 0.0, -40.0]), "{:?}", mcs.origin);

    // The CIP's ref direction (2, 0, 0.5) is not normal to its axis: the
    // x axis is its projection, unit length, and y completes the frame.
    let cip = r.find(CsysRole::InProcess).expect("CIP");
    assert!(close(cip.origin, [0.0, 0.0, -75.0]));
    assert!(close(cip.x, [1.0, 0.0, 0.0]), "{:?}", cip.x);
    assert!(close(cip.y(), [0.0, 1.0, 0.0]), "{:?}", cip.y());

    // Written twice at one placement: one coordinate system, not two.
    let csw = r
        .find(CsysRole::WorkpieceSide {
            level: Some(1),
            number: Some(2),
        })
        .expect("CSW1_2");
    assert_eq!(csw.entity, 60);
    assert!(close(csw.origin, [10.0, 0.0, -20.0]));

    // A vendor's own name is data, not a role; unnamed placements are
    // surfaces, not coordinate systems.
    let names: Vec<&str> = r.systems.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["PCS", "MCS", "CIP", "CSW1_2", "VENDOR_REF"]);
    let vendor = r.systems.iter().find(|c| c.role == CsysRole::Other);
    assert!(vendor.is_some_and(|c| close(c.z, [0.0, 0.0, 1.0])));
}

#[test]
fn a_file_without_tool_coordinate_systems_says_so_by_name() {
    let r = read("tool_csys_none.step");
    assert!(r.systems.is_empty() && r.refused.is_empty(), "{r:?}");
    assert_eq!(
        r.none_found().as_deref(),
        Some("the file names no coordinate systems")
    );
    let missing = r.find(CsysRole::Primary).expect_err("no PCS");
    assert!(missing.contains("no Primary"), "{missing}");

    // Named, but none of them an ISO 13399 name: the reason lists them.
    let text = std::fs::read_to_string(testdata("tool_csys_none.step"))
        .expect("fixture reads")
        .replace(
            "AXIS2_PLACEMENT_3D('', #31",
            "AXIS2_PLACEMENT_3D('Origin', #31",
        );
    let r = read_tool_csys(text.as_bytes()).expect("parses");
    let why = r.none_found().expect("falls back");
    assert!(
        why.contains("no named coordinate system is an ISO 13399 one"),
        "{why}"
    );
    assert!(why.contains("Origin"), "{why}");
}

#[test]
fn degenerate_or_ambiguous_coordinate_systems_are_refused_by_name() {
    let r = read("tool_csys_degenerate.step");
    assert!(r.systems.is_empty(), "{:?}", r.systems);
    let reason = |name: &str| {
        r.refused
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.reason.clone())
            .unwrap_or_default()
    };
    assert!(
        reason("PCS").contains("axis has no direction"),
        "{:?}",
        r.refused
    );
    assert!(
        reason("MCS").contains("origin is not finite"),
        "{:?}",
        r.refused
    );
    assert!(
        reason("CIP").contains("2 different placements"),
        "{:?}",
        r.refused
    );
    assert!(
        reason("CSW").contains("parallel to its axis"),
        "{:?}",
        r.refused
    );

    let why = r.none_found().expect("falls back");
    assert!(
        why.starts_with("every ISO 13399 coordinate system was refused"),
        "{why}"
    );
    let pcs = r.find(CsysRole::Primary).expect_err("PCS refused");
    assert!(pcs.contains("Primary refused"), "{pcs}");
}

#[test]
fn origins_read_in_the_files_length_unit() {
    let text = std::fs::read_to_string(testdata("tool_csys_iso13399.step"))
        .expect("fixture reads")
        .replace(
            "#12 = ( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) );",
            "#12 = ( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.CENTI.,.METRE.) );",
        );
    let r = read_tool_csys(text.as_bytes()).expect("parses");
    let mcs = r.find(CsysRole::Mounting).expect("MCS");
    assert!(close(mcs.origin, [0.0, 0.0, -400.0]), "{:?}", mcs.origin);
}

#[test]
fn iso_13399_names_classify_and_others_do_not() {
    use CsysRole::*;
    assert_eq!(CsysRole::classify(" pcs "), Primary);
    assert_eq!(CsysRole::classify("MCS"), Mounting);
    assert_eq!(CsysRole::classify("CIP"), InProcess);
    assert_eq!(CsysRole::classify("CSIF"), Interface);
    let csw = |level, number| WorkpieceSide { level, number };
    assert_eq!(CsysRole::classify("CSW"), csw(None, None));
    assert_eq!(CsysRole::classify("CSW2"), csw(Some(2), None));
    assert_eq!(CsysRole::classify("CSW1_3"), csw(Some(1), Some(3)));
    for other in ["PCS_1", "CSW_", "CSW1_", "CSWA", "CSW1_2_3", "CSW-1", ""] {
        assert_eq!(CsysRole::classify(other), Other, "{other}");
    }
}

#[test]
fn unreadable_input_is_a_clean_error_never_a_panic() {
    assert!(matches!(read_tool_csys(b""), Err(ImportError::Empty)));
    assert!(matches!(
        read_tool_csys(b"\xff\xfe"),
        Err(ImportError::Step(_))
    ));
    assert!(matches!(
        read_tool_csys(b"ISO-10303-21; junk"),
        Err(ImportError::Step(_))
    ));
    // Dangling references and wrong entity kinds refuse the placement.
    let text = std::fs::read_to_string(testdata("tool_csys_none.step"))
        .expect("fixture reads")
        .replace(
            "AXIS2_PLACEMENT_3D('', #31, #32, #33)",
            "AXIS2_PLACEMENT_3D('PCS', #99, #20, #33)",
        );
    let r = read_tool_csys(text.as_bytes()).expect("parses");
    assert!(
        r.refused
            .iter()
            .any(|f| f.name == "PCS" && f.reason.contains("#99")),
        "{:?}",
        r.refused
    );
}
