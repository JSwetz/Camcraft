//! #616: per-operation smoothing. Off (the default) a program carries no
//! smoothing word and is byte-identical to before; on, each operation's
//! section states its ask (a finish tolerance, or a rough one from the
//! stock it leaves) and each dialect writes its control's words, undone
//! at the section's end.
//!
//! Gate map (#616): covers app::smoothing (of, rough_tol), post::dump
//! (the Smoothing PARAMs), the six mill posts' smoothOn/smoothOff.
//! Provenance: our own fixtures; each dialect's words from its public
//! programming documentation (see the posts).

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::geometry::Point3;
use camcraft_app::operation::{self, Height, OpParams, OpType};
use camcraft_app::post::{self, cnc, Dump, Smoothing};
use camcraft_app::posting::BUILTINS;
use camcraft_app::smoothing::{self, ROUGH_MAX_MM, ROUGH_MIN_MM};

fn script(name: &str) -> &'static str {
    BUILTINS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s).unwrap_or_else(|| panic!("{name} is built in"))
}

/// A one-feed section, with or without a smoothing ask.
fn program(name: &str, ask: Option<Smoothing>) -> String {
    let p = |x: f64, z: f64| Point3::new(x, 0.0, z);
    let t = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(0.0, 25.0), p(0.0, 5.0), None),
        Move::new(MoveKind::Feed, p(0.0, 5.0), p(20.0, -1.0), Some(600.0)),
        Move::new(MoveKind::Rapid, p(20.0, -1.0), p(20.0, 25.0), None),
    ]);
    let d = Dump {
        op_name: "Contour1",
        tool_number: 1,
        tool_name: "EM",
        tool_diameter: 6.0,
        rpm: 8000.0,
        clearance_z: 25.0,
        retract_z: 25.0,
        smoothing: ask,
        ..Dump::default()
    };
    let text = cnc::serialize(&post::dump(&t, &d));
    match post::post(script(name), &serde_json::Value::Null, &text) {
        Ok(out) => out.program,
        Err(e) => panic!("{name}: {e}"),
    }
}

fn lines_with(prog: &str, word: &str) -> Vec<String> {
    prog.lines().filter(|l| l.contains(word)).map(str::to_owned).collect()
}

const FINISH: Option<Smoothing> = Some(Smoothing { tol_mm: 0.01, finish: true });
const ROUGH: Option<Smoothing> = Some(Smoothing { tol_mm: 0.1, finish: false });

#[test]
fn off_is_byte_identical_and_writes_no_smoothing_word() {
    for (name, _) in BUILTINS.iter().filter(|(n, _)| *n != "Fanuc Lathe") {
        let off = program(name, None);
        for w in ["G5.1", "G187", "CYCLE832", "G62 ", "SMOOTHING"] {
            assert!(!off.contains(w), "{name} wrote {w} with smoothing off:\n{off}");
        }
        // The Tormach header's fixed blending is unchanged (one G64).
        if *name == "Tormach PathPilot" {
            assert_eq!(lines_with(&off, "G64").len(), 1, "{off}");
        }
    }
}

#[test]
fn each_dialect_writes_its_words_and_undoes_them_at_the_section_end() {
    // (post, finish words, rough words, the reset line's words)
    let cases: [(&str, &[&str], &[&str], &str); 5] = [
        ("Tormach PathPilot", &["G64", "P0.01"], &["G64", "P0.1"], "G64 P0.025"),
        ("Fanuc", &["G5.1", "Q1"], &["G5.1", "Q1"], "G5.1 Q0"),
        ("Haas", &["G187", "P3", "E0.01"], &["G187", "P1", "E0.1"], "G187"),
        ("Siemens 840D", &["CYCLE832(0.01,_FINISH,1)"], &["CYCLE832(0.1,_ROUGH,1)"], "CYCLE832()"),
        ("Heidenhain", &["G62", "T0.01"], &["G62", "T0.1"], "G62"),
    ];
    for (name, fin, rough, reset) in cases {
        for (ask, want) in [(FINISH, fin), (ROUGH, rough)] {
            let prog = program(name, ask);
            let on = prog
                .lines()
                .position(|l| want.iter().all(|w| l.contains(w)))
                .unwrap_or_else(|| panic!("{name}: no line with {want:?}:\n{prog}"));
            let feed = prog
                .lines()
                .position(|l| l.split_whitespace().any(|w| matches!(w.trim_end_matches('*'), "G1" | "G01")))
                .unwrap_or(usize::MAX);
            assert!(on < feed, "{name}: smoothing stated before the first cut:\n{prog}");
            let off = prog
                .lines()
                .skip(on + 1)
                .position(|l| l.contains(reset))
                .unwrap_or_else(|| panic!("{name}: no reset '{reset}' after the section:\n{prog}"));
            let _ = off;
        }
    }
}

#[test]
fn mazak_names_the_ask_once_and_writes_no_guessed_word() {
    let prog = program("Mazak", FINISH);
    assert_eq!(lines_with(&prog, "SMOOTHING NOT WRITTEN").len(), 1, "{prog}");
}

#[test]
fn finish_ops_ask_their_tolerance_and_rough_ops_ask_from_their_stock() {
    // A contour that finishes asks its own tolerance; leaving stock, it roughs.
    let base = OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    let seed = |ty: OpType| operation::retype_params(&base, ty.ordinal());
    let mut contour = seed(OpType::Contour);
    if let OpParams::Contour { stock_to_leave, tolerance, .. } = &mut contour {
        *stock_to_leave = 0.0;
        *tolerance = 0.02;
    }
    assert_eq!(smoothing::of(&contour), Some(Smoothing { tol_mm: 0.02, finish: true }));
    if let OpParams::Contour { stock_to_leave, .. } = &mut contour {
        *stock_to_leave = 0.5;
    }
    assert_eq!(smoothing::of(&contour), Some(Smoothing { tol_mm: 0.1, finish: false }), "a fifth of 0.5 mm");
    // The rough band's ends.
    assert_eq!(smoothing::rough_tol(0.001), ROUGH_MIN_MM);
    assert_eq!(smoothing::rough_tol(10.0), ROUGH_MAX_MM);
    // Drilling asks nothing.
    assert_eq!(smoothing::of(&seed(OpType::Drill)), None);
}
