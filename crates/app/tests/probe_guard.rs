//! #174 acceptance: the positioning feeds of a probe cycle - the descent
//! from the rapid / safe plane to each touch's start, the travel between
//! touches, the rise - are protected moves. The dump marks them in the
//! stream (`GUARD`), and each probing post writes them as a stop-on-
//! contact move that ALARMS on an unexpected trip: the G31 skip read
//! the other way round (#5061-#5063 not at the target) on the Fanuc
//! dialect, G38.3 + a #5070 test on Tormach PathPilot (LinuxCNC). A
//! probing script that cannot voice the move is refused by name.
//!
//! Provenance: Fanuc's published operator documentation (G31 skip,
//! custom macro B system variables, #3000 alarms); the LinuxCNC G-code
//! reference (G38.3, #5070, O-code conditionals, MSG); our own fixtures.
//!
//! Gate map (M4.4): covers post::probe_rec (GUARD), post::cnc, post::engine,
//! post::invert, motion::probing, assets/posts fanuc.cps + tormach.cps
//! (onProbeGuard).

mod common;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::geometry::Point3;
use camcraft_app::post::{self, cnc, engine};
use camcraft_app::probing::{probe_cycle, ProbeDir, ProbeMode, ProbeParams};

const FANUC: &str = include_str!("../../../assets/posts/fanuc.cps");
const TORMACH: &str = include_str!("../../../assets/posts/tormach.cps");

fn cycle(mode: ProbeMode, dir: ProbeDir, dir_y: ProbeDir, target: Point3, radius: f64) -> ProbeParams {
    ProbeParams {
        mode,
        dir,
        dir_y,
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

fn corner() -> Toolpath {
    probe_cycle(&cycle(ProbeMode::Corner, ProbeDir::XPos, ProbeDir::YPos, Point3::new(0.0, 0.0, 20.0), 0.0))
        .expect("corner")
}

fn records(t: &Toolpath) -> Vec<cnc::Record> {
    let d = post::Dump {
        op_name: "Probe1",
        tool_number: 7,
        tool_name: "PROBE-4",
        tool_diameter: 4.0,
        rpm: 0.0,
        clearance_z: 50.0,
        retract_z: 50.0,
        probe_result: Some(post::probe_rec::ProbeResult { set_wcs: Some(0), tol_mm: 0.5 }),
        ..post::Dump::default()
    };
    post::dump(t, &d)
}

/// Every motion of a probe path is a touch, a back-off, a protected
/// move or a rapid - never a plain feed - and each mode's positioning
/// feeds (descent, travel, rise) are exactly the GUARD records. A -Z
/// face starts its touch at the rapid plane, so only its rise is a feed.
#[test]
fn the_positioning_feeds_of_a_probe_path_are_marked_protected() {
    let modes = [
        ("face", cycle(ProbeMode::Face, ProbeDir::ZNeg, ProbeDir::YPos, Point3::new(10.0, 5.0, 20.0), 0.0), 1),
        ("edge", cycle(ProbeMode::Edge, ProbeDir::XPos, ProbeDir::YPos, Point3::new(0.0, 5.0, 20.0), 0.0), 2),
        ("corner", cycle(ProbeMode::Corner, ProbeDir::XPos, ProbeDir::YPos, Point3::new(0.0, 0.0, 20.0), 0.0), 4),
        ("bore", cycle(ProbeMode::Bore, ProbeDir::ZNeg, ProbeDir::YPos, Point3::new(30.0, 30.0, 20.0), 10.0), 2),
        ("boss", cycle(ProbeMode::Boss, ProbeDir::ZNeg, ProbeDir::YPos, Point3::new(30.0, 30.0, 20.0), 10.0), 8),
    ];
    for (name, p, want) in modes {
        let t = probe_cycle(&p).expect(name);
        let recs = records(&t);
        assert!(!recs.iter().any(|r| matches!(r, cnc::Record::Warn { .. })), "{name}: no WARN");
        let feeds = recs.iter().filter(|r| matches!(r, cnc::Record::Move(m) if m.kind != MoveKind::Rapid)).count();
        assert_eq!(feeds, 0, "{name}: no positioning feed posts as a plain G1");
        let guards: Vec<[f64; 3]> = recs
            .iter()
            .filter_map(|r| match r {
                cnc::Record::Guard(g) => Some(g.to),
                _ => None,
            })
            .collect();
        assert_eq!(guards.len(), want, "{name}: the descents, the travel and the rises");
        // Each guard is one of the path's non-rapid moves, not a touch.
        let touch_ix: Vec<usize> = t.probes.iter().map(|p| p.index).collect();
        for g in &guards {
            assert!(
                t.moves.iter().enumerate().any(|(i, m)| m.kind != MoveKind::Rapid
                    && !touch_ix.contains(&i)
                    && (m.to.x - g[0]).abs() < 1e-9
                    && (m.to.y - g[1]).abs() < 1e-9
                    && (m.to.z - g[2]).abs() < 1e-9),
                "{name}: guard {g:?} is a positioning feed of the path"
            );
        }
        // The motion count the viewer map relies on is unchanged.
        let motions = recs
            .iter()
            .filter(|r| {
                matches!(r, cnc::Record::Move(_) | cnc::Record::Touch(_) | cnc::Record::Retreat(_) | cnc::Record::Guard(_))
            })
            .count();
        assert_eq!(motions, t.moves.len(), "{name}: one record per move");
    }
    // A cutting path (no touches) carries no GUARD.
    let mut plain = corner();
    plain.probes.clear();
    let d = post::Dump { op_name: "Cut", tool_number: 1, tool_name: "EM", tool_diameter: 6.0, ..post::Dump::default() };
    assert!(!post::dump(&plain, &d).iter().any(|r| matches!(r, cnc::Record::Guard(_))));
}

/// GUARD round-trips through the stream text, and malformed lines are
/// refused by the parser, not guessed at.
#[test]
fn the_guard_record_round_trips_and_refuses_malformed_lines() {
    let recs = records(&corner());
    let stream = cnc::serialize(&recs);
    assert!(stream.contains("\nGUARD "), "{stream}");
    assert_eq!(cnc::parse(&stream).expect("parse"), recs);
    for bad in ["GUARD 1 2 3\n", "GUARD 1 2 F 300\n", "GUARD 1 2 3 F 0\n", "GUARD 1 2 x F 300\n"] {
        assert!(cnc::parse(bad).is_err(), "{bad:?} is refused");
    }
}

/// The Fanuc dialect: each guard is a G31 followed by one alarm test
/// per moving axis; the corner golden pins the travel between touches.
#[test]
fn fanuc_writes_protected_moves_as_g31_with_a_contact_alarm() {
    let stream = cnc::serialize(&records(&corner()));
    let out = engine::run(FANUC, &serde_json::Value::Null, &stream).expect("post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<&str> = out.program.lines().collect();
    assert!(
        !lines.iter().any(|l| l.starts_with("G01 ") || l.starts_with("G1 ")),
        "no absolute feed is left in a probe program:\n{}",
        out.program
    );
    let alarms = lines.iter().filter(|l| l.ends_with("THEN#3000=3(PROBE HIT ON POSITIONING)")).count();
    assert_eq!(alarms, 4, "a descent and a rise per touch, each a Z test:\n{}", out.program);
    // The first descent: from the rapid plane to the touch's start.
    let at = lines.iter().position(|l| l.starts_with("G31 ") && l.contains("F300")).expect("a G31");
    assert_eq!(lines[at + 1], "IF[ABS[#5063-15]GT0.001]THEN#3000=3(PROBE HIT ON POSITIONING)", "\n{}", out.program);
    common::write_golden("nc/fanuc/probe_corner.nc", &out.program);
}

/// Tormach PathPilot: G38.3 then the #5070 test that ends the program.
/// The touches themselves (G38.2) are #362's; this golden pins the
/// protected moves alone (the corner's guard records and rapids).
#[test]
fn tormach_writes_protected_moves_as_g38_3_with_a_trip_test() {
    let recs: Vec<cnc::Record> = records(&corner())
        .into_iter()
        .filter(|r| !matches!(r, cnc::Record::Touch(_) | cnc::Record::Retreat(_) | cnc::Record::Found(_)))
        .collect();
    let out = engine::run(TORMACH, &serde_json::Value::Null, &cnc::serialize(&recs)).expect("post");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(out.program.matches("G38.3 ").count(), 4, "\n{}", out.program);
    assert_eq!(out.program.matches("[#5070 EQ 1]").count(), 4, "\n{}", out.program);
    assert!(out.program.contains("o5001 if [#5070 EQ 1]\n(MSG, PROBE HIT ON POSITIONING)\nM2\no5001 endif\n"));
    assert!(!out.program.lines().any(|l| l.starts_with("G1 ")), "no plain feed:\n{}", out.program);
    common::write_golden("nc/tormach/probe_guard_moves.nc", &out.program);
}

/// A post that probes but cannot write a protected move is refused by
/// name, once per operation - never a plain G1 in its place.
#[test]
fn a_probing_post_without_on_probe_guard_is_refused_by_name() {
    let script = FANUC.replace("function onProbeGuard()", "function onProbeGuardNotHere()");
    let stream = cnc::serialize(&records(&corner()));
    let out = engine::run(&script, &serde_json::Value::Null, &stream).expect("post");
    let named: Vec<&String> = out.warnings.iter().filter(|w| w.contains("onProbeGuard")).collect();
    assert_eq!(named.len(), 1, "one refusal for the operation: {:?}", out.warnings);
    assert!(named[0].contains("Probe1"), "names the operation: {}", named[0]);
    assert!(!out.program.lines().any(|l| l.starts_with("G01 ")), "no plain feed stands in:\n{}", out.program);
}
