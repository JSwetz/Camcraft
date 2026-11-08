//! #170: the Tormach mill family's machine files - each states its own
//! operator's manual figures (travel, spindle, belt ranges, feeds, rigid
//! tapping, coolant) - and the PathPilot post speaking them: the belt note
//! from the machine's own ranges, a rigid tap refused by name on a machine
//! that cannot rigid-tap.
//! Provenance: our own tests over our own machine files and post; the
//! figures are each machine's Tormach operator's manual, § "Machine
//! Specifications" and § "Performance Expectations" (UM10587 770MX MF,
//! UM10864 770MX ML, UM10539 770M, UM10586 1100MX, UM10811 1500MX,
//! UM10372 PCNC 440).
//! Gate map (M4.4): covers post::machine (spindle ranges, rigid tapping),
//! app::post (tormach.cps), app::posting (MACHINES registry).

use camcraft_app::machine::{Machine, parse_mch};
use camcraft_app::post::{engine, machine};
use camcraft_app::posting::MACHINES;
use std::path::{Path, PathBuf};

const IN: f64 = 25.4;

fn read(rel: &str) -> String {
    let p: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn shipped(name: &str) -> Machine {
    let text = MACHINES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| *t)
        .unwrap_or_else(|| panic!("{name} is not a shipped machine"));
    parse_mch(text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The script's view of a shipped machine, as `posting::machine_value`
/// hands it over (parser, then mirror).
fn mirrored(name: &str) -> serde_json::Value {
    machine::mirror(&shipped(name))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// One row of a manual's specification table.
struct Spec {
    name: &'static str,
    travel_in: [f64; 3],
    rpm: (f64, f64),
    kw: f64,
    ipm: (f64, f64),
    ranges: &'static [(f64, f64)],
    rigid: bool,
}

const SPECS: [Spec; 6] = [
    Spec {
        name: "Tormach 770MX",
        travel_in: [14.0, 7.5, 13.25],
        rpm: (175.0, 10000.0),
        kw: 1.1,
        ipm: (300.0, 250.0),
        ranges: &[(175.0, 3250.0), (525.0, 10000.0)],
        rigid: true,
    },
    Spec {
        name: "Tormach 770MX (ML)",
        travel_in: [14.0, 7.5, 13.25],
        rpm: (250.0, 10000.0),
        kw: 1.5,
        ipm: (450.0, 420.0),
        ranges: &[],
        rigid: true,
    },
    Spec {
        name: "Tormach 770M",
        travel_in: [14.0, 7.5, 13.25],
        rpm: (175.0, 10000.0),
        kw: 1.1,
        ipm: (135.0, 110.0),
        ranges: &[(175.0, 3250.0), (525.0, 10000.0)],
        rigid: false,
    },
    Spec {
        name: "Tormach 1100MX",
        travel_in: [18.0, 11.0, 16.25],
        rpm: (70.0, 10000.0),
        kw: 1.5,
        ipm: (300.0, 230.0),
        ranges: &[(70.0, 2000.0), (250.0, 10000.0)],
        rigid: true,
    },
    Spec {
        name: "Tormach 1500MX",
        travel_in: [19.7, 13.8, 14.0],
        rpm: (250.0, 10000.0),
        kw: 3.0,
        ipm: (1200.0, 600.0),
        ranges: &[],
        rigid: true,
    },
    Spec {
        name: "Tormach PCNC 440",
        travel_in: [10.0, 6.25, 10.0],
        rpm: (300.0, 10000.0),
        kw: 0.56,
        ipm: (135.0, 110.0),
        ranges: &[(300.0, 4500.0), (650.0, 10000.0)],
        rigid: false,
    },
];

#[test]
fn every_tormach_machine_states_its_manuals_figures() {
    for s in &SPECS {
        let m = shipped(s.name);
        let n = s.name;
        assert_eq!(m.model, n);
        assert_eq!(
            m.units.as_deref(),
            Some("inch"),
            "{n} is programmed in inches"
        );
        assert_eq!(m.axes.len(), 3, "{n}: X, Y, Z");
        for (a, want) in m.axes.iter().zip(s.travel_in) {
            // Travel is program coordinates from the work zero: the full
            // stroke either way, so any zero on the table is reachable.
            let (lo, hi) = a.travel.unwrap_or_else(|| panic!("{n} {} travel", a.name));
            assert!(
                close(hi, want * IN) && close(lo, -want * IN),
                "{n} {}: {lo}..{hi}",
                a.name
            );
        }
        assert_eq!(
            (m.spindle_min, m.spindle_max),
            (Some(s.rpm.0), Some(s.rpm.1)),
            "{n} rpm"
        );
        assert_eq!(m.spindle_power, Some(s.kw), "{n} kW");
        for a in &m.axes {
            let want = if a.name == "Z" { s.ipm.1 } else { s.ipm.0 };
            assert!(
                a.feed.is_some_and(|f| close(f, want * IN)),
                "{n} {} feed {:?}",
                a.name,
                a.feed
            );
        }
        assert_eq!(m.spindle_ranges, s.ranges.to_vec(), "{n} belt ranges");
        assert_eq!(m.rigid_tapping, Some(s.rigid), "{n} rigid tapping");
        let codes: Vec<(&str, &str)> = m
            .coolants
            .iter()
            .map(|c| (c.name.as_str(), c.m_code.as_str()))
            .collect();
        assert_eq!(codes, [("Flood", "M8"), ("Mist", "M7")], "{n} coolant");
    }
}

#[test]
fn the_script_sees_the_ranges_and_the_tapping_word() {
    let p = machine::script_props(&mirrored("Tormach 1100MX"));
    assert_eq!(
        p["spindleRanges"],
        serde_json::json!([[70.0, 2000.0], [250.0, 10000.0]])
    );
    assert_eq!(p["rigidTapping"], true);
    let p = machine::script_props(&mirrored("Tormach PCNC 440"));
    assert_eq!(p["rigidTapping"], false);
    // Unstated: no ranges, and tapping is not limited.
    let p = machine::script_props(&mirrored("Tormach 1500MX"));
    assert_eq!(p["spindleRanges"], serde_json::json!([]));
    let p = machine::script_props(&serde_json::json!({}));
    assert_eq!(
        (&p["spindleRanges"], &p["rigidTapping"]),
        (&serde_json::json!([]), &serde_json::json!(true))
    );
}

fn post_at(name: &str, rpm: u32) -> engine::PostOutput {
    let cps = read("assets/posts/tormach.cps");
    let stream =
        format!("PARAM SpindleSpeed {rpm}\nTOOL 1 T1\nSECTION\nRapid X 0 Y 0 Z 5000\nENDSECTION\n");
    engine::run(&cps, &mirrored(name), &stream).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The belt note reads the machine's own ranges: the 1100MX's low range
/// ends at 2000 rpm (the 770's at 3250), and a one-range machine has no
/// belt to name.
#[test]
fn the_belt_note_follows_the_machines_own_ranges() {
    let note = |name: &str, rpm: u32| {
        post_at(name, rpm)
            .program
            .lines()
            .find(|l| l.contains("BELT RANGE"))
            .map(str::to_owned)
    };
    assert_eq!(
        note("Tormach 1100MX", 2500).as_deref(),
        Some("(SPINDLE 2500 RPM NEEDS THE HIGH BELT RANGE)")
    );
    assert_eq!(
        note("Tormach 770MX", 2500),
        None,
        "both 770 ranges serve 2500"
    );
    assert_eq!(
        note("Tormach 1100MX", 100).as_deref(),
        Some("(SPINDLE 100 RPM NEEDS THE LOW BELT RANGE)")
    );
    assert_eq!(note("Tormach 1100MX", 1500), None);
    assert_eq!(
        note("Tormach PCNC 440", 500).as_deref(),
        Some("(SPINDLE 500 RPM NEEDS THE LOW BELT RANGE)")
    );
    assert_eq!(
        note("Tormach PCNC 440", 6000).as_deref(),
        Some("(SPINDLE 6000 RPM NEEDS THE HIGH BELT RANGE)")
    );
    assert_eq!(note("Tormach 1500MX", 8000), None, "one range: no belt");
    assert_eq!(note("Tormach 770MX (ML)", 300), None, "one range: no belt");
}

fn tap_on(name: &str) -> engine::PostOutput {
    let cps = read("assets/posts/tormach.cps");
    let stream =
        read("crates/app/tests/golden/tap_plate.cl").replace("DWELL0 AT1", "DWELL500 F1270000 AT1");
    engine::run(&cps, &mirrored(name), &stream).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The R8 mills (770M, PCNC 440) cannot rigid-tap: a tap is refused by
/// name, once, and no G84 reaches the program. The MX mills post it.
#[test]
fn a_rigid_tap_is_refused_on_a_machine_that_cannot_rigid_tap() {
    for name in ["Tormach 770M", "Tormach PCNC 440"] {
        let out = tap_on(name);
        assert_eq!(
            out.warnings.len(),
            1,
            "{name}: one named refusal: {:?}",
            out.warnings
        );
        assert!(
            out.warnings[0].contains("rigid tapping") && out.warnings[0].contains(name),
            "{:?}",
            out.warnings
        );
        assert!(!out.program.contains("G84"), "{name}:\n{}", out.program);
    }
    for name in [
        "Tormach 770MX",
        "Tormach 770MX (ML)",
        "Tormach 1100MX",
        "Tormach 1500MX",
    ] {
        let out = tap_on(name);
        assert!(out.warnings.is_empty(), "{name}: {:?}", out.warnings);
        assert!(
            out.program.lines().any(|l| l.starts_with("G84")),
            "{name}:\n{}",
            out.program
        );
    }
}
