//! Post engine core tests (M1.7a): the intermediate's two directions,
//! the driver's hook contract, and the clean-refusal law on every
//! malformed path.

use hsmworks_app::cl::{CycleKind, MoveKind, Toolpath};
use hsmworks_app::post::cnc::{self, CycleRec, MoveRec, Record};
use hsmworks_app::post::{engine, machine, normalize_program_order, Dump, PostError};
use serde_json::json;

fn golden(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(name);
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("golden {name}: {e}"))
}

/// A stream with one of everything, in program order.
fn sample() -> Vec<Record> {
    vec![
        Record::Param { name: "SpindleSpeed".into(), value: json!(8000.0) },
        Record::Param { name: "OperationName".into(), value: json!("Face Mill 40") },
        Record::Warn { text: "shank band grazes stock".into() },
        Record::Tool { number: 3, name: "1/4 INCH 4FL".into() },
        Record::Section { begin: true },
        Record::Move(MoveRec {
            kind: MoveKind::Rapid,
            x: 1.5,
            y: -2.0,
            z: 0.0,
            feed: None,
            center: None,
            ccw: false,
        }),
        Record::Move(MoveRec {
            kind: MoveKind::Feed,
            x: 1.5,
            y: -2.0,
            z: -1.0,
            feed: Some(1200.0),
            center: None,
            ccw: false,
        }),
        Record::Move(MoveRec {
            kind: MoveKind::Feed,
            x: 2.0,
            y: -2.0,
            z: -1.0,
            feed: Some(1200.0),
            center: Some((1.5, -2.0)),
            ccw: true,
        }),
        Record::Section { begin: false },
    ]
}

#[test]
fn v1_roundtrip_is_exact() {
    let records = sample();
    let text = cnc::serialize(&records);
    let back = cnc::parse(&text).expect("own output must parse");
    assert_eq!(back, records, "serialize/parse must be an identity");
}

#[test]
fn malformed_streams_refuse_cleanly() {
    for bad in [
        "Goto X 1 Y 2\n",
        "Feed X\n",
        "Feed X q Y 0 Z 0\n",
        "Arc 1 2\n",
        "Arc CW X 1 Y 2 I 0\n",
        "PARAM Speed\n",
        "TOOL abc Name\n",
        "; CYCLE NOPE X0\n",
        "; CYCLE DRILL ZAP 3\n",
        "Feed X nan Y 0 Z 0\n",
        "Feed 1 2 3 4 5 6\n",
    ] {
        let e = cnc::parse(bad).expect_err("degenerate input must error, never crash");
        assert!(format!("{e}").contains("line"), "error must name its line: {e}");
    }
}

#[test]
fn every_golden_is_a_legal_post_input() {
    // The superset law: `.cl` goldens parse by the legacy rules.
    for name in [
        "contour_disc_hole.cl",
        "contour_plate_with_hole.cl",
        "contour_tangential_lead.cl",
        "face_square_40x40.cl",
        "slot_wide_channel_rows.cl",
        "drill_plate.cl",
        "drill_peck.cl",
        "tap_plate.cl",
    ] {
        let records = cnc::parse(&golden(name))
            .unwrap_or_else(|e| panic!("golden {name} must parse: {e}"));
        let moves = records.iter().filter(|r| matches!(r, Record::Move(_))).count();
        // Drilling goldens are hole-group chains - a tap can be one
        // cycle on four motion records. Count intent, not just moves.
        let records_here = moves
            + records.iter().filter(|r| matches!(r, Record::Cycle(_))).count();
        let floor = if name.starts_with("drill") || name.starts_with("tap") { 4 } else { 10 };
        assert!(records_here > floor, "{name}: {records_here} records is not a toolpath");
        if name.starts_with("drill") || name.starts_with("tap") {
            assert!(
                records.iter().any(|r| matches!(r, Record::Cycle(_))),
                "{name} carries v3 cycle markers"
            );
        }
    }
}

#[test]
fn legacy_markers_normalize_to_program_order() {
    // v3 goldens append their `; CYCLE` lines after the chain; the
    // driver re-seats each cycle before its AT-anchored move.
    let records = cnc::parse(&golden("drill_plate.cl")).expect("parses");
    let norm = normalize_program_order(records.clone());
    assert_ne!(norm, records, "the golden appends its markers: order changes");
    // In the normalized stream each cycle sits at (or before) its
    // anchor: the moves counted when the cycle emits equal AT.
    let mut mi = 0usize;
    for r in &norm {
        match r {
            Record::Move(_) => mi += 1,
            Record::Cycle(c) => assert!(
                mi <= c.first_move,
                "cycle for move {} emitted after {mi} moves",
                c.first_move
            ),
            _ => {}
        }
    }
    assert!(norm.iter().any(|r| matches!(r, Record::Move(_))));
    // Idempotence: re-normalizing changes nothing.
    assert_eq!(normalize_program_order(norm.clone()), norm);
}

const ECHO_CPS: &str = r#"
setLanguageVersion(3);
function onOpen() { writeln("ECHO"); }
function onCommand(c) {
  writeln(";cmd " + c + " from " + formatUnit(currentPosition.X) + " " + formatUnit(currentPosition.Z));
  return true;
}
function onRapid() { writeBlock("RAPID", formatUnit(properties.X), formatUnit(properties.Y), formatUnit(properties.Z)); }
function onLinear() {
  writeBlock("LIN", formatUnit(properties.X), formatUnit(properties.Y), formatUnit(properties.Z), properties.FeedRate === null ? "" : "F" + formatFeed(properties.FeedRate));
}
function onCircular() { writeBlock("ARC", properties.CircularDirection, formatUnit(properties.X)); }
function onCyclePoint() { writeBlock("CYC", properties.CycleType, formatUnit(properties.X), formatUnit(properties.R), formatUnit(properties.Z)); }
function onClose() { writeln("DONE"); }
"#;

#[test]
fn echo_script_drives_hooks_and_line_map() {
    let text = cnc::serialize(&sample());
    let out = engine::run(ECHO_CPS, &json!(null), &text).expect("echo post");
    assert!(out.program.starts_with("ECHO\n"));
    assert!(out.program.contains("RAPID 1.5 -2 0\n"));
    assert!(out.program.contains("LIN 1.5 -2 -1 F1200\n"));
    assert!(out.program.contains("ARC CCW 2\n"));
    assert!(out.program.ends_with("DONE\n"));
    // The stream warning rode through, counted not printed.
    assert_eq!(out.warnings, vec!["shank band grazes stock".to_string()]);
    // line_map: the RAPID line belongs to the first Move record.
    let rapid_line = out.program.lines().position(|l| l.starts_with("RAPID")).unwrap();
    let mapped = out.line_map.iter().find(|li| li.line == rapid_line).expect("mapped");
    let rec = mapped.record.expect("a record line");
    // The mapped record's index is past the params/tool block.
    assert!(rec >= 4, "params and tool come first; got record {rec}");
    // Header/footer lines: onClose's DONE maps to None.
    let done_line = out.program.lines().count() - 1;
    assert!(out.line_map.iter().any(|li| li.line == done_line && li.record.is_none()));
}

#[test]
fn on_command_false_swallows_but_chains() {
    let cps = "function onCommand(c) { return c !== \"Rapid\"; }\n\
               function onLinear() { writeBlock(\"LIN\", formatUnit(properties.X)); }\n\
               function onRapid() { writeBlock(\"SHOULD NOT APPEAR\"); }";
    let text = cnc::serialize(&sample());
    let out = engine::run(cps, &json!(null), &text).expect("post");
    assert!(!out.program.contains("SHOULD NOT APPEAR"));
    assert_eq!(out.swallowed, 1);
    // The feed after the swallowed rapid still starts from its end
    // point: onCommand's currentPosition proves it.
    let cps2 = "function onCommand(c) { if (c === \"Linear\") writeln(\"AT \" + formatUnit(currentPosition.X) + \" \" + formatUnit(currentPosition.Y)); return false; }";
    let out2 = engine::run(cps2, &json!(null), &text).expect("post");
    assert!(out2.program.contains("AT 1.5 -2\n"), "{}", out2.program);
}

#[test]
fn cycle_sequence_and_dwell_gate() {
    let cps = "var seq = [];\n\
               function onCycle() { seq.push(\"C\"); }\n\
               function onCyclePoint() { seq.push(\"P\"); }\n\
               function onDwell(v) { seq.push(\"D\" + v); }\n\n\
               function onCycleEnd() { seq.push(\"E\"); }\n\
               function onClose() { writeln(seq.join(\" \")); }";
    let mut recs = vec![Record::Section { begin: true }];
    recs.push(Record::Cycle(CycleRec {
        kind: CycleKind::Peck,
        x: 1.0,
        y: 2.0,
        r_z: 5.0,
        bottom_z: -3.0,
        entry_z: 0.0,
        peck: 1.0,
        dwell: 0.0,
        first_move: 0,
    }));
    recs.push(Record::Cycle(CycleRec {
        kind: CycleKind::Deep,
        x: 1.0,
        y: 2.0,
        r_z: 5.0,
        bottom_z: -8.0,
        entry_z: 0.0,
        peck: 2.0,
        dwell: 0.5,
        first_move: 0,
    }));
    recs.push(Record::Section { begin: false });
    let out = engine::run(cps, &json!(null), &cnc::serialize(&recs)).expect("post");
    assert!(out.program.contains("C P E C P D0.5 E\n"), "{}", out.program);
    // The chain continues from the last cycle's R plane.
    assert_eq!(out.swallowed, 0);
}

#[test]
fn script_errors_are_refusals() {
    let cps = "function onLinear() { undefinedFn(); }";
    let text = cnc::serialize(&sample());
    let e = engine::run(cps, &json!(null), &text).expect_err("a throwing script must refuse");
    assert!(matches!(e, PostError::Script(_)), "{e:?}");
    assert!(!format!("{e}").is_empty(), "the refusal names the failure");
}

#[test]
fn empty_stream_is_a_clean_two_line_program() {
    let out = engine::run(ECHO_CPS, &json!(null), "").expect("empty parses");
    assert_eq!(out.program, "ECHO\nDONE\n");
    assert!(out.line_map.iter().all(|li| li.record.is_none()));
}

#[test]
fn messages_count_as_warnings() {
    let cps = "function onLinear() { message(\"feed over machine max\"); }";
    let text = cnc::serialize(&sample());
    let out = engine::run(cps, &json!(null), &text).expect("post");
    assert_eq!(out.warnings.len(), 2, "stream warn + script message");
    assert_eq!(out.warnings[1], "feed over machine max");
}

#[test]
fn machine_flattens_for_the_script() {
    let mch = json!({
        "name": "VMC 3-AXIS",
        "capabilities": { "toolChange": true },
        "kinematics": { "static": {
            "head": { "spindle": { "min": 0.0, "max": 12000.0 },
                      "toolStation": { "coolants": [
                          { "name": "Flood", "mCode": "M8" }, "Mist" ] } },
            "axes": [
                { "letter": "X", "speeds": { "rapid": 6000.0, "max": 5000.0 } },
                { "letter": "Z", "speeds": { "rapid": 4000.0, "max": 3000.0 } } ]
        } }
    });
    let p = machine::script_props(&mch);
    assert_eq!(p["rapidRate"], json!(6000.0));
    assert_eq!(p["spindleMax"], json!(12000.0));
    assert_eq!(p["coolants"].as_array().map(Vec::len), Some(2));
    assert_eq!(p["name"], json!("VMC 3-AXIS"));
    // A script reads it as the documented global.
    let cps = "function onClose() { writeln(\"M \" + machine.name + \" \" + machine.rapidRate + \" \" + machine.spindleMax); }";
    let out = engine::run(cps, &mch, "").expect("post");
    assert_eq!(out.program, "M VMC 3-AXIS 6000 12000\n");
}

#[test]
fn dump_interleaves_cycles_at_their_anchor_moves() {
    use hsmworks_app::geo2d::Point2;
    use hsmworks_app::geometry::Point3;
    let mut t = Toolpath::default();
    for i in 0..4 {
        let (kind, z) = if i % 2 == 0 { (MoveKind::Rapid, 10.0) } else { (MoveKind::Plunge, -5.0) };
        t.moves.push(hsmworks_app::cl::Move {
            kind,
            from: Point3 { x: 0.0, y: 0.0, z: 10.0 },
            to: Point3 { x: i as f64, y: 0.0, z },
            feed: if kind == MoveKind::Rapid { None } else { Some(900.0) },
            arc: None,
        });
    }
    t.cycles = vec![
        hsmworks_app::cl::Cycle {
            kind: CycleKind::Drill,
            xy: Point2 { x: 0.0, y: 0.0 },
            r_z: 5.0,
            bottom_z: -5.0,
            entry_z: 0.0,
            peck: 0.0,
            dwell: 0.0,
            first_move: 0,
        },
        hsmworks_app::cl::Cycle {
            kind: CycleKind::Tap,
            xy: Point2 { x: 2.0, y: 0.0 },
            r_z: 5.0,
            bottom_z: -5.0,
            entry_z: 0.0,
            peck: 0.0,
            dwell: 0.0,
            first_move: 2,
        },
    ];
    let d = Dump {
        op_name: "Drill1",
        tool_number: 1,
        tool_name: "DR-0300",
        tool_diameter: 3.0,
        rpm: 2500.0,
        clearance_z: 12.0,
        retract_z: 10.0,
        top_z: 0.0,
        wcs: "G54",
        origin: [0.0; 3],
        stock: [-20.0, 20.0, -20.0, 20.0],
        stock_top_z: 0.0,
        warnings: vec!["drill vs hole dia advisory".into()],
    };
    let recs = hsmworks_app::post::dump(&t, &d);
    // dump emits each cycle with first_move <= mi just before move mi:
    // the shape is c m m c m m ENDSECTION.
    let mut mi = 0usize;
    let mut seen_cycle_at: Vec<usize> = Vec::new();
    for r in &recs {
        match r {
            Record::Move(_) => mi += 1,
            Record::Cycle(c) => {
                assert!(c.first_move <= mi, "cycle after its anchor: {:?} vs {mi}", c.kind);
                seen_cycle_at.push(c.first_move);
            }
            _ => {}
        }
    }
    assert_eq!(seen_cycle_at, vec![0, 2]);
    // And the text round-trips through parse unchanged (program order
    // is stable under normalize).
    let text = cnc::serialize(&recs);
    let back = normalize_program_order(cnc::parse(&text).expect("parses"));
    assert_eq!(back, recs, "dump -> text -> parse is an identity");
}

#[test]
fn spaced_and_joined_key_forms_agree() {
    let spaced = "SECTION\nRapid X 1000 Y 2000 Z 3000\nENDSECTION\n";
    let joined = "SECTION\nRapid X1000 Y2000 Z3000\nENDSECTION\n";
    let legacy = "SECTION\nRapid 1000 2000 3000\nENDSECTION\n";
    let a = cnc::parse(spaced).expect("spaced");
    let b = cnc::parse(joined).expect("joined");
    let c = cnc::parse(legacy).expect("legacy positional");
    assert_eq!(a, b);
    assert_eq!(b, c);
}

#[test]
fn keyed_with_equals_and_colon_forms_agree() {
    let a = cnc::parse("Feed X=1000 Y=2000 Z=3000 F=500000\n").expect("equals");
    let b = cnc::parse("Feed X:1000 Y:2000 Z:3000 F:500000\n").expect("colons");
    let (Some(Record::Move(m1)), Some(Record::Move(m2))) = (a.first(), b.first()) else {
        panic!("moves");
    };
    assert_eq!(m1, m2);
    assert_eq!(m1.feed, Some(500.0));
}
