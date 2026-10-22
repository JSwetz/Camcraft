//! Machine limits (M2.3 stage C): what a program asks that the
//! machine cannot give. Two halves - the `.mch` reader (every field
//! optional by the unstated-limit law, garbage refuses, never panics)
//! and `check_document` on a live world. The check reuses the shared
//! gate: it walks `posting::program_ops`, the same walk the sim and
//! the post make, so all three judge exactly the same program.
//!
//! Fixture law (clean-room): the machine JSON below mirrors the
//! observed shape of a `.mch` file - the layout user instruction
//! authorizes us to reuse - while every VALUE is our own invention.
//! Program tests build `Machine` structs directly; only the reader
//! tests feed it text.

use flecs_ecs::prelude::World;

use hsmworks_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock, SetWcsOrigin,
};
use hsmworks_app::geometry::{IDENTITY_Q, Point3, cube};
use hsmworks_app::machine::{AxisLimits, Machine, check_document, parse_mch};
use hsmworks_app::model;
use hsmworks_app::operation::{Height, OpParams, SrcRef};
use hsmworks_app::posting;
use hsmworks_app::schema::Location;
use hsmworks_app::stock::StockShape;
use hsmworks_app::tools::catalog;
use hsmworks_app::world::new_world;

fn catalog_tool(name: &str) -> hsmworks_app::tools::Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

/// The reference machine: a 3-axis VMC whose window admits the
/// EM-0400-2FL (12000 rpm at the inclusive top, 4 mm across, 87 mm
/// out) and a face program running XY 1200 / plunge 300 inside
/// 5000/5000/3000 ceilings, all inside a ±100 mm box of travel.
fn reference() -> Machine {
    Machine {
        model: "TestMill 1".into(),
        vendor: "Hsmworks Testbench".into(),
        spindle_min: Some(50.0),
        spindle_max: Some(12000.0),
        axes: vec![
            AxisLimits { name: "X".into(), feed: Some(5000.0), rapid: Some(15000.0), travel: Some((-100.0, 100.0)) },
            AxisLimits { name: "Y".into(), feed: Some(5000.0), rapid: Some(15000.0), travel: Some((-100.0, 100.0)) },
            AxisLimits { name: "Z".into(), feed: Some(3000.0), rapid: Some(10000.0), travel: Some((-100.0, 100.0)) },
        ],
        max_tool_length: Some(150.0),
        max_tool_diameter: Some(20.0),
        // No spindle body stated: this reference machine keeps the
        // pre-M3.2B verdicts byte-identical - the spindle band only
        // exists when the `.mch` says it does.
        spindle_d: None,
        spindle_len: None,
    }
}

/// One Face op on a 60x60x20 box at the origin, faced to z 18 - the
/// same program shape the stage B sim tests judge, so the machine
/// verdict reads the moves the viewer already draws.
fn faced_doc() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let loc = Location { pos: Point3::new(0.0, 0.0, 0.0), rot: IDENTITY_Q };
    let cmd = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, loc);
    history.execute(&world, cmd.expect("box stock"));
    let cmd = AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL"));
    history.execute(&world, cmd.expect("catalog tool"));
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::new(-30.0, -30.0, 0.0), Point3::new(30.0, 30.0, 18.0))),
    );
    let cmd = CreateOperation::new(
        &world,
        "Face1",
        "EM-0400-2FL",
        SrcRef::Stock,
        OpParams::Face {
            stepover: 3.0,
            clearance_z: Height::fixed(25.0),
            cut_z: Height::fixed(18.0),
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
        },
    );
    history.execute(&world, cmd.expect("create face"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate face"));
    (world, history)
}

// ---- workspaces by kind (M3.2 stage E) ----

#[test]
fn the_axis_inventory_picks_the_workspace() {
    use hsmworks_app::machine::{Kind, kind};
    let axis = |n: &str| AxisLimits {
        name: n.to_string(),
        ..Default::default()
    };
    let m = |names: &[&str]| Machine {
        axes: names.iter().copied().map(axis).collect(),
        ..Default::default()
    };
    assert_eq!(kind(None), Kind::Mill, "no machine is today's document");
    assert_eq!(kind(Some(&m(&["X", "Y", "Z"]))), Kind::Mill);
    assert_eq!(kind(Some(&m(&["Z", "X"]))), Kind::Lathe, "order-free");
    assert_eq!(kind(Some(&m(&["x", "z"]))), Kind::Lathe, "names, not cases");
    assert_eq!(kind(Some(&m(&["X", "Y", "Z", "C"]))), Kind::MillTurn);
    assert_eq!(
        kind(Some(&m(&["X", "Z", "C"]))),
        Kind::Lathe,
        "a C axis alone is a turning center, not a mill-turn - no Y"
    );
    assert_eq!(
        kind(Some(&m(&[]))),
        Kind::Mill,
        "an inventory that cannot decide falls back to the assumption the code already is"
    );
    assert_eq!(Kind::Lathe.label(), "turning");
}

// ---- the reader ----

#[test]
fn parse_reads_the_limits() {
    // Our own file, our own values; the layout is the .mch shape the
    // kinematics chain (head -> X -> Y -> Z) takes in the wild.
    let text = r#"{
      "general": {"model": "TestMill 1", "vendor": "Hsmworks Testbench"},
      "kinematics": {"default": {
        "units": {"angle": "degrees", "length": "mm"},
        "parts": [
          {"type": "static", "name": "base", "parts": [
            {"type": "head", "name": "s",
             "spindle": {"min_speed": 50, "max_speed": 12000},
             "tool_station": {"max_tool_length": 150, "max_tool_diameter": 20},
             "parts": [
               {"type": "linear", "name": "X", "control": "driven",
                "travel": {"min": -100, "max": 100},
                "parts": [
                  {"type": "linear", "name": "Y", "control": "driven",
                   "travel": {"min": -100, "max": 100},
                   "parts": [
                     {"type": "linear", "name": "Z", "control": "driven",
                      "travel": {"min": -100, "max": 100}}
                   ]}
                ]}
             ]}
          ]}
        ]}},
      "controller": {"synced_configuration": {"parts": {
        "X": {"coordinate": 0, "max_normal_speed": 5000, "max_rapid_speed": 15000},
        "Y": {"coordinate": 1, "max_normal_speed": 5000, "max_rapid_speed": 15000},
        "Z": {"coordinate": 2, "max_normal_speed": 3000, "max_rapid_speed": 10000}
      }}}
    }"#;
    let m = parse_mch(text).expect("well-formed file");
    assert_eq!(m, reference(), "the reader and the fixture struct are the same machine");
    assert!(m.summary().starts_with("Hsmworks Testbench TestMill 1"));
    assert!(m.summary().contains("3 axes (3 with a known stroke)"));
}

#[test]
fn zero_means_the_file_does_not_say() {
    // The Minimill-shaped case: a real vendor file that carries zeros
    // for speeds nobody configured. Zeros are NOT ceilings - a green
    // checkmark earned by an invented limit is the silent wrong
    // answer this law exists to refuse.
    let text = r#"{
      "general": {},
      "kinematics": {"default": {"units": {"length": "mm"}, "parts": [
        {"type": "head", "name": "h",
         "spindle": {"min_speed": 0, "max_speed": 0},
         "tool_station": {"max_tool_length": 0, "max_tool_diameter": 0},
         "parts": [{"type": "linear", "name": "X"}]}
      ]}},
      "controller": {"synced_configuration": {"parts": {
        "X": {"coordinate": 0, "max_normal_speed": 0, "max_rapid_speed": 0}
      }}}
    }"#;
    let m = parse_mch(text).expect("well-formed");
    assert_eq!(m.spindle_min, None);
    assert_eq!(m.spindle_max, None);
    assert_eq!(m.max_tool_length, None);
    assert_eq!(m.axes.len(), 1);
    assert_eq!(m.axes[0].feed, None);
    assert_eq!(m.axes[0].travel, None);
    assert!(m.summary().contains("?-? rpm"), "unknown shows as unknown: {}", m.summary());
}

#[test]
fn garbage_refuses_and_emptiness_judges_nothing() {
    // Degenerate input must error cleanly - the reader walks a
    // serde_json::Value, so no shape can panic it.
    for bad in ["", "not json at all", "{", "{\"kinematics\": 5"] {
        assert!(parse_mch(bad).is_err(), "{bad:?} must refuse, not guess");
    }
    // Well-formed JSON that names no machine is legal and mute - the
    // reader describes what the file says, and these files say
    // nothing. (A bare array or null is JSON that merely isn't a
    // machine file: refuse.)
    assert!(parse_mch("[]").is_err());
    assert!(parse_mch("null").is_err());
    for mute in ["{}", "{\"general\": {}}", "{\"kinematics\": 5}"] {
        parse_mch(mute).expect("well-formed, mute");
    }
    let m = parse_mch("{}").expect("the empty file is legal, just mute");
    assert_eq!(m.axes.len(), 0);
    assert_eq!(m.spindle_max, None);
    assert_eq!(m.vendor, "unknown");
}

#[test]
fn inches_scale_into_millimeters() {
    // One factor at the door, and the whole checker stays mm-only.
    // rpm is not a length and rides along unscaled.
    let text = r#"{
      "general": {"model": "Inchmill", "vendor": "Testbench"},
      "kinematics": {"default": {"units": {"length": "inch"}, "parts": [
        {"type": "head", "name": "h", "spindle": {"min_speed": 100, "max_speed": 8000},
         "tool_station": {"max_tool_length": 4},
         "parts": [{"type": "linear", "name": "X", "travel": {"min": -10, "max": 10}}]}
      ]}},
      "controller": {"synced_configuration": {"parts": {
        "X": {"coordinate": 0, "max_normal_speed": 200}
      }}}
    }"#;
    let m = parse_mch(text).expect("well-formed");
    let near = |a: Option<f64>, b: f64| a.is_some_and(|x| (x - b).abs() < 1e-9);
    assert!(near(m.axes[0].feed, 5080.0), "200 ipm = 5080 mm/min: {:?}", m.axes[0].feed);
    assert!(m.axes[0].travel.is_some_and(|(lo, hi)| (lo + 254.0).abs() < 1e-9 && (hi - 254.0).abs() < 1e-9));
    assert!(near(m.max_tool_length, 101.6));
    assert!(near(m.spindle_max, 8000.0), "rpm is not a length");
}

// ---- the checker on a live program ----

#[test]
fn the_reference_machine_accepts_the_reference_program() {
    // The honest-green case: every stated limit stated, every ask
    // inside it. This is the verdict stage D will make structural.
    let (world, _h) = faced_doc();
    let c = check_document(&world, &reference()).expect("a fresh program checks");
    assert!(c.is_ok(), "reference program on reference machine: {:?}", c.problems);
}

#[test]
fn a_spindle_outside_the_window_names_the_op() {
    // The tool's rpm IS the program's spindle word today, so the
    // window is a tool-table truth - located by op.
    let (world, _h) = faced_doc();
    let mut slow = reference();
    slow.spindle_max = Some(1000.0);
    let c = check_document(&world, &slow).expect("checks");
    assert_eq!(c.problems.len(), 1, "{:?}", c.problems);
    assert!(c.problems[0].starts_with("Face1:") && c.problems[0].contains("above the machine's 1000"));
    let mut fast = reference();
    fast.spindle_min = Some(20000.0);
    let c = check_document(&world, &fast).expect("checks");
    assert!(c.problems.iter().any(|p| p.contains("below the machine's 20000")), "{:?}", c.problems);
}

#[test]
fn an_unstated_machine_passes_anything_it_cannot_judge() {
    // Same program, mute file: nothing to violate. A green verdict
    // here must read "the machine said nothing", which is what stage
    // D's gate text will say - stage C's law is that it is NOT a
    // claim of safety.
    let (world, _h) = faced_doc();
    let c = check_document(&world, &Machine::default()).expect("checks");
    assert!(c.is_ok());
}

#[test]
fn feed_is_charged_to_the_slowest_axis_that_moves() {
    // X tops out at 500; the face's XY passes hit 1200 on every move
    // that drives X, and the pure-Z plunges (300 under Z's 3000) are
    // not in it. The report names the axis that ran out.
    let (world, _h) = faced_doc();
    let mut m = reference();
    m.axes[0].feed = Some(500.0);
    let c = check_document(&world, &m).expect("checks");
    assert!(!c.problems.is_empty());
    assert!(
        c.problems.iter().all(|p| !p.contains("move") || p.contains("X tops out at 500")),
        "only X-driven moves may be charged: {:?}",
        &c.problems[..c.problems.len().min(3)]
    );
    assert!(c.problems.iter().any(|p| p.contains("feeds 1200") && p.contains("X tops out at 500")));
    assert!(!c.problems.iter().any(|p| p.contains("feeds 300")), "plunges stay under Z's ceiling");
}

#[test]
fn travel_reports_where_the_tool_reaches() {
    // A 20 mm-wide column cannot take a 60 mm face: the path swings
    // past x +/- 30 with the 4 mm tool, and the verdict says the
    // number and the stroke.
    let (world, _h) = faced_doc();
    let mut m = reference();
    m.axes[0].travel = Some((-20.0, 20.0));
    let c = check_document(&world, &m).expect("checks");
    assert!(!c.problems.is_empty(), "a 40 mm stroke cannot hide a 64 mm sweep");
    assert!(
        c.problems.iter().any(|p| p.starts_with("Face1:") && p.contains("in X") && p.contains("the stroke is -20.000..20.000")),
        "{:?}", c.problems[0]
    );
}

#[test]
fn rapids_pay_the_same_travel_bill_as_feeds() {
    // The stroke is geometry, not a feedrate argument: a clearance
    // move across the table crosses the column even though it has no
    // feed word to fail. Same machine, same program as the travel
    // test - the approach moves alone breach it, and the walk never
    // branches on move kind.
    let (world, _h) = faced_doc();
    let mut m = reference();
    m.axes[0].travel = Some((-20.0, 20.0));
    let c = check_document(&world, &m).expect("checks");
    let cache = model::toolpath_of(&world, &model::find_by_name(&world, "Face1").expect("op")).expect("cache");
    let any_rapid_out = cache.path.moves.iter().any(|mv| {
        mv.feed.is_none() && (mv.from.x.abs() > 20.0 || mv.to.x.abs() > 20.0)
    });
    assert!(any_rapid_out, "the fixture must actually rapid outside the column");
    assert!(c.problems.iter().any(|p| p.contains("move")), "{:?}", c.problems[0]);
}

#[test]
fn the_tool_table_refuses_a_cutter_the_magazine_cannot_hold() {
    // Gauge length is the whole protrusion - flute, shank, holder -
    // 87 mm on the 0400; the diameter reads off the tool as stored.
    let (world, _h) = faced_doc();
    let mut m = reference();
    m.max_tool_length = Some(50.0);
    m.max_tool_diameter = Some(2.0);
    let c = check_document(&world, &m).expect("checks");
    assert_eq!(c.problems.len(), 2, "{:?}", c.problems);
    assert!(c.problems[0].contains("is 87.0 mm long, the machine takes 50.0"));
    assert!(c.problems[1].contains("is 4.0 mm across, the machine takes 2.0"));
}

#[test]
fn a_g54_slide_moves_the_stroke_exactly_like_the_g_code() {
    // Travel is a program-coordinate law and the moves are world
    // millimeters; the bridge is the post's own wcs_context, so the
    // same G54 slide that shifts the emitted X word shifts what
    // passes. A 40 mm column holds a 30 mm sweep at the origin and
    // loses it entirely 100 mm over.
    let mut m = reference();
    m.axes[0].travel = Some((-40.0, 40.0));
    let (world, mut history) = faced_doc();
    assert!(check_document(&world, &m).expect("origin check").is_ok(), "±30 inside ±40");
    // The slot is born when it is first placed (the SetWcsOrigin law) -
    // and the slide moves through history, like every other mutation.
    history.execute(
        &world,
        SetWcsOrigin::new(
            &world,
            "G54",
            Location { pos: Point3::new(100.0, 0.0, 0.0), rot: IDENTITY_Q },
        )
        .expect("slide G54"),
    );
    // Regenerate over whatever the digest makes stale (a fresh
    // regenerate is a no-op if nothing staled it), then re-judge.
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate slid"));
    let c = check_document(&world, &m).expect("slid check");
    assert!(!c.is_ok(), "the program swings -70..-100, outside -40..40");
    assert!(
        c.problems.iter().all(|p| !p.contains("reaches") || p.contains("reaches -")),
        "every breach is on the negative side: {:?}",
        &c.problems[..c.problems.len().min(2)]
    );
}

#[test]
fn the_machine_walk_is_the_post_walk() {
    // The shared-gate law again: check_document validates through
    // posting::program_ops, so an empty or stale document refuses
    // the machine check with the post's own sentence, byte-for-byte.
    let world = new_world();
    let err = check_document(&world, &reference()).expect_err("nothing to check");
    let same = posting::program_ops(&world).expect_err("same law, one copy");
    assert_eq!(err, same);

    let (world, mut history) = faced_doc();
    // Stale it the way stage B's tests do - the stock's placement is
    // in the op's digest.
    let stock = model::find_by_name(&world, hsmworks_app::schema::STOCK_NAME).expect("stock");
    model::set_transform(&world, &stock, hsmworks_app::schema::Location { pos: Point3::new(0.0, 0.0, 1.0), rot: IDENTITY_Q });
    let err = check_document(&world, &reference()).expect_err("stale blocks the check");
    assert!(err.contains("Face1") && err.contains("regenerate"), "{err}");
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    check_document(&world, &reference()).expect("fresh again checks");
}

#[test]
fn a_flood_of_violations_reports_forty_and_counts_the_rest() {
    // The cap is a report bound, not a truth bound: more stays honest.
    let (world, _h) = faced_doc();
    let mut m = reference();
    m.axes[0].travel = Some((-0.0005, 0.0005));
    let c = check_document(&world, &m).expect("checks");
    assert_eq!(c.problems.len(), 40, "the list caps at its bound");
    assert!(c.more > 0, "a 60 mm face crosses a 1 um column more than 40 times");
}

#[test]
fn the_post_mirror_states_only_what_the_machine_stated() {
    // M3.3 A's bridge: a checker-parsed `Machine` reaches the script
    // through `post::machine::mirror` - the static grammar, stated
    // numbers only. An unstated spindle limit is ABSENT (the posts'
    // `> 0` guard keeps the silence honest), rotary axes are not
    // mirrored (no 3-axis post reads them yet), and coolant never
    // appears: a parsed `.mch` carries no coolant list.
    let m = Machine {
        model: "MirrorMill".into(),
        vendor: "Hsmworks Testbench".into(),
        spindle_max: Some(8000.0),
        axes: vec![
            AxisLimits {
                name: "X".into(),
                feed: Some(4000.0),
                rapid: Some(7000.0),
                travel: None,
            },
            AxisLimits { name: "C".into(), ..Default::default() },
        ],
        ..Default::default()
    };
    let v = hsmworks_app::post::machine::mirror(&m);
    assert_eq!(v["name"], "MirrorMill");
    let axes = v["kinematics"]["static"]["axes"].as_array().expect("axes array");
    assert_eq!(axes.len(), 1, "the rotary C is not mirrored in stage A");
    assert_eq!(axes[0]["letter"], "X");
    assert_eq!(axes[0]["speeds"]["rapid"], 7000.0);
    assert_eq!(axes[0]["speeds"]["normal"], 4000.0);
    assert_eq!(axes[0]["direction"], serde_json::json!([1.0, 0.0, 0.0]));
    assert_eq!(v["kinematics"]["static"]["head"]["spindle"]["max"], 8000.0);
    assert_eq!(v["rapidRate"], 7000.0, "the ceiling is the fastest stated axis");

    let props = hsmworks_app::post::machine::script_props(&v);
    assert_eq!(props["name"], "MirrorMill");
    assert_eq!(props["spindleMax"], 8000.0);
    assert_eq!(props["coolants"].as_array().map(Vec::len), Some(0), "silent coolant, stated");

    // The mute machine mirrors to silence, not to zeros that lie.
    let mute = hsmworks_app::post::machine::script_props(
        &hsmworks_app::post::machine::mirror(&Machine::default()),
    );
    assert_eq!(mute["name"], "");
    assert_eq!(mute["spindleMax"], 0.0, "absent reads 0: the `> 0` guards keep it silent");
    assert_eq!(mute["rapidRate"], 0.0);
}
