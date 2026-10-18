//! Verification warnings: clean generated paths stay silent, and each of
//! the five planted offenders yields exactly its warning class.

use hsmworks_app::cl::{Cycle, CycleKind, Move, MoveKind, Toolpath};
use hsmworks_app::geo2d::{Loop, Point2, Region};
use hsmworks_app::geometry::Point3;
use hsmworks_app::machining2d::{
    contour, pocket, trace, ContourParams, PocketParams, PocketStrategy, Side, TraceParams,
};
use hsmworks_app::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn p2(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![
        p2(x0, y0),
        p2(x1, y0),
        p2(x1, y1),
        p2(x0, y1),
    ])
    .unwrap()])
    .unwrap()
}

/// Pocketable 30x30 with a 10x10 part island at the center.
fn plate_with_island() -> Region {
    let mut loops = vec![rect(0.0, 0.0, 30.0, 30.0).loops()[0].clone()];
    loops.push(
        Loop::new(vec![p2(10.0, 10.0), p2(20.0, 10.0), p2(20.0, 20.0), p2(10.0, 20.0)]).unwrap(),
    );
    Region::new(loops).unwrap()
}

fn deep_tool() -> ToolGeom {
    ToolGeom {
        radius: 4.0,
        flute_len: 30.0,
        shank_radius: 9.0,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    }
}

fn short_tool() -> ToolGeom {
    ToolGeom {
        flute_len: 2.0,
        ..deep_tool()
    }
}

fn ctx<'a>(region: &'a Region, stock: Option<&'a Region>, tool: &'a ToolGeom, mode: Mode) -> VerifyContext<'a> {
    VerifyContext {
        region,
        stock,
        tool,
        mode,
        stock_top_z: 0.0,
        clearance_z: 5.0,
        stock_to_leave: 0.0,
        // 10 um: the pocket's corner-avoidance samples can land a chord
        // error inside the reach circle (geo2d round-join flattening is
        // 5 um); anything past that is a real gouge, the plant below is
        // 500 um deep.
        gouge_tol: 0.01,
        z_tol: 1e-6,
    }
}

#[test]
fn clean_pocket_verifies_silent() {
    // Zigzag pocket around an island, deep flute: no law may fire.
    let r = plate_with_island();
    let stock = rect(0.0, 0.0, 30.0, 30.0);
    let t = pocket(
        &r,
        &PocketParams {
            tool_diameter: 8.0,
            stepover: 3.0,
            stock_to_leave: 0.0,
            clearance_z: 5.0,
            retract_z: None,
            top_z: 0.0,
            bottom_z: -4.0,
            stepdown: 2.0,
            feed: 400.0,
            plunge_feed: 80.0,
            climb: true,
            strategy: PocketStrategy::Zigzag,
        },
    )
    .unwrap();
    let tool = deep_tool();
    assert_eq!(verify(&t, &ctx(&r, Some(&stock), &tool, Mode::Area)), Vec::new());
}

#[test]
fn clean_contour_verifies_silent() {
    // 30x30 plate, 10x10 through hole: the finish ring with a deep tool
    // touches but never bites, and its shank clears the wall top.
    let r = plate_with_island();
    let t = contour(
        &r,
        &ContourParams {
            tool_diameter: 8.0,
            stock_to_leave: 0.0,
            clearance_z: 5.0,
            retract_z: 8.0,
            top_z: 0.0,
            bottom_z: -4.0,
            stepdown: 2.0,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            side: Side::Auto,
            lead: hsmworks_app::machining2d::Lead::None,
            tolerance: 0.01,
            tab_count: 0,
            tab_width: 0.0,
            tab_height: 0.0,
        },
    )
    .unwrap();
    let tool = deep_tool();
    assert_eq!(verify(&t, &ctx(&r, None, &tool, Mode::Boundary)), Vec::new());
}

#[test]
fn crossing_feeds_are_flagged() {
    // An X at shared depth, well clear of the walls so only the crossing
    // law can fire: one Crossing, nothing else.
    let t = Toolpath {
        moves: vec![
            Move::new(MoveKind::Feed, p(4.0, 4.0, -1.0), p(16.0, 16.0, -1.0), Some(500.0)),
            Move::new(MoveKind::Feed, p(4.0, 16.0, -1.0), p(16.0, 4.0, -1.0), Some(500.0)),
        ],
        ..Toolpath::default()
    };
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let tool = deep_tool();
    assert_eq!(verify(&t, &ctx(&r, None, &tool, Mode::Area)), vec![Warning::Crossing { moves: [0, 1] }]);
}

#[test]
fn feeds_on_different_levels_may_cross() {
    // Same X, but 2 mm apart in Z: air over the lower level, no warning.
    let t = Toolpath {
        moves: vec![
            Move::new(MoveKind::Feed, p(4.0, 4.0, -1.0), p(16.0, 16.0, -1.0), Some(500.0)),
            Move::new(MoveKind::Feed, p(4.0, 16.0, -3.0), p(16.0, 4.0, -3.0), Some(500.0)),
        ],
        ..Toolpath::default()
    };
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let tool = deep_tool();
    assert_eq!(verify(&t, &ctx(&r, None, &tool, Mode::Area)), Vec::new());
}

#[test]
fn gouge_ring_is_flagged() {
    // A square ring 0.5 mm closer to the walls than the r = 4 tool's
    // tangency circle, in a 20x20 plate: every point of it bites past
    // the finish surface.
    let mut moves = vec![Move::new(MoveKind::Plunge, p(3.5, 3.5, 5.0), p(3.5, 3.5, -1.0), Some(100.0))];
    for (from, to) in [((3.5, 3.5), (16.5, 3.5)), ((16.5, 3.5), (16.5, 16.5)), ((16.5, 16.5), (3.5, 16.5)), ((3.5, 16.5), (3.5, 3.5))] {
        moves.push(Move::new(MoveKind::Feed, p(from.0, from.1, -1.0), p(to.0, to.1, -1.0), Some(500.0)));
    }
    let t = Toolpath {
        moves,
        ..Toolpath::default()
    };
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let tool = deep_tool();
    let warns = verify(&t, &ctx(&r, None, &tool, Mode::Boundary));
    assert_eq!(warns.len(), 5);
    assert!(warns.iter().all(|w| matches!(w, Warning::Gouge { .. })), "{warns:?}");
    assert_eq!(warns[0], Warning::Gouge { at: 0 });
}

#[test]
fn arcs_keep_warning_indices() {
    // A shallow arc from (4,10) to (16,10) whose chords sweep down to 2
    // mm of the y = 20 wall, while both endpoints sit exactly on the
    // r = 4 tool's tangency line: the law must sample inside the arc,
    // yet the warning still speaks a move index - one warning, at the
    // arc's own slot, whatever its chord count.
    let arc = Move::new(MoveKind::Feed, p(4.0, 10.0, -1.0), p(16.0, 10.0, -1.0), Some(500.0))
        .with_arc(p2(10.0, 11.75), false);
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let tool = deep_tool();
    let t = Toolpath {
        moves: vec![arc],
        ..Toolpath::default()
    };
    assert_eq!(verify(&t, &ctx(&r, None, &tool, Mode::Boundary)), vec![Warning::Gouge { at: 0 }]);
    // The straight chord between the same endpoints never dips: silent.
    let straight = Toolpath {
        moves: vec![Move::new(MoveKind::Feed, p(4.0, 10.0, -1.0), p(16.0, 10.0, -1.0), Some(500.0))],
        ..Toolpath::default()
    };
    assert_eq!(verify(&straight, &ctx(&r, None, &tool, Mode::Boundary)), Vec::new());
    // Flattening a path to chords never shifts an index: crossing X,
    // one arm drawn as a full turn of chords-equivalent arc.
    let crossed = Toolpath {
        moves: vec![
            Move::new(MoveKind::Feed, p(4.0, 4.0, -1.0), p(16.0, 16.0, -1.0), Some(500.0)),
            Move::new(MoveKind::Feed, p(4.0, 16.0, -1.0), p(16.0, 4.0, -1.0), Some(500.0))
                .with_arc(p2(10.0, 10.0), true),
        ],
        ..Toolpath::default()
    };
    let warns = verify(&crossed, &ctx(&r, None, &tool, Mode::Area));
    assert!(warns.contains(&Warning::Crossing { moves: [0, 1] }), "{warns:?}");
}

#[test]
fn holder_hits_a_part_island_when_the_flute_is_short() {
    // Same pocket as the clean case with a 2 mm flute: the shank rides
    // below the island top, so the island is struck at the deepest
    // levels. part = true distinguishes it from uncut swarf.
    let r = plate_with_island();
    let stock = rect(0.0, 0.0, 30.0, 30.0);
    let t = pocket(
        &r,
        &PocketParams {
            tool_diameter: 8.0,
            stepover: 3.0,
            stock_to_leave: 0.0,
            clearance_z: 5.0,
            retract_z: None,
            top_z: 0.0,
            bottom_z: -4.0,
            stepdown: 2.0,
            feed: 400.0,
            plunge_feed: 80.0,
            climb: true,
            strategy: PocketStrategy::Zigzag,
        },
    )
    .unwrap();
    let tool = short_tool();
    let warns = verify(&t, &ctx(&r, Some(&stock), &tool, Mode::Area));
    let hits: Vec<usize> = warns
        .iter()
        .filter_map(|w| match w {
            Warning::ShankHit { part: true, .. } => Some(1),
            _ => None,
        })
        .collect();
    assert!(!hits.is_empty(), "expected part-island ShankHit, got {warns:?}");
    assert!(warns.iter().all(|w| matches!(w, Warning::ShankHit { .. })), "{warns:?}");
}

#[test]
fn low_rapid_through_stock_is_flagged() {
    // A lateral rapid 1 mm under the stock top crossing the plate.
    let t = Toolpath {
        moves: vec![
            Move::new(MoveKind::Rapid, p(-5.0, 15.0, 5.0), p(-5.0, 15.0, -1.0), None),
            Move::new(MoveKind::Rapid, p(-5.0, 15.0, -1.0), p(25.0, 15.0, -1.0), None),
        ],
        ..Toolpath::default()
    };
    let r = rect(0.0, 0.0, 30.0, 30.0);
    let tool = deep_tool();
    let warns = verify(&t, &ctx(&r, None, &tool, Mode::Area));
    assert_eq!(warns, vec![Warning::RapidInStock { at: 1 }]);
}

#[test]
fn engrave_paths_skip_the_gouge_law() {
    // Trace sits exactly on the boundary: d = 0 would "gouge" any tool.
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let t = trace(
        &r,
        &TraceParams {
            clearance_z: 5.0,
            retract_z: None,
            top_z: 0.0,
            bottom_z: -1.0,
            stepdown: 1.0,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        },
    )
    .unwrap();
    let tool = deep_tool();
    assert_eq!(verify(&t, &ctx(&r, None, &tool, Mode::Engrave)), Vec::new());
}

#[test]
fn non_finite_moves_are_caught_defensively() {
    let t = Toolpath {
        moves: vec![Move::new(MoveKind::Feed, p(0.0, 0.0, 0.0), p(f64::NAN, 1.0, 0.0), Some(500.0))],
        ..Toolpath::default()
    };
    let r = rect(0.0, 0.0, 10.0, 10.0);
    let tool = deep_tool();
    let warns = verify(&t, &ctx(&r, None, &tool, Mode::Area));
    assert!(warns.contains(&Warning::NonFinite { at: 0 }), "{warns:?}");
}

#[test]
fn warnings_carry_readable_messages() {
    let s = Warning::Crossing { moves: [3, 9] }.to_string();
    assert!(s.contains("3") && s.contains("9"), "{s}");
    let s = Warning::ShankHit { at: 1, part: true }.to_string();
    assert!(s.contains("part"), "{s}");
}

fn cycle(xy_hole: Point2, entry_z: f64, bottom_z: f64, first_move: usize) -> Cycle {
    Cycle {
        kind: CycleKind::Drill,
        xy: xy_hole,
        r_z: entry_z,
        bottom_z,
        entry_z,
        peck: 0.0,
        dwell: 0.0,
        first_move,
    }
}

/// One hole at the middle of a big plate: rapid over, plunge, retract.
/// The plate edges sit 50 mm from the hole (outside every band annulus),
/// so the move laws stay silent at any depth and only the cycle reach
/// law can speak.
fn hole_in_the_middle(bottom_z: f64) -> Toolpath {
    let at = p(50.0, 50.0, 5.0);
    let mut path = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(-50.0, -50.0, 5.0), at, None),
        Move::new(MoveKind::Plunge, at, p(50.0, 50.0, bottom_z), Some(120.0)),
        Move::new(MoveKind::Rapid, p(50.0, 50.0, bottom_z), at, None),
    ]);
    path.cycles = vec![cycle(p2(50.0, 50.0), 5.0, bottom_z, 1)];
    path
}

#[test]
fn a_deeper_than_the_tool_hole_warns_from_its_cycle() {
    // Law (e): entry to bottom spans 136 mm; flute+shank+holder is 130.
    // The plunge itself provokes nothing - the reach lives in the cycle.
    let r = rect(0.0, 0.0, 100.0, 100.0);
    let tool = deep_tool();
    let t = hole_in_the_middle(-131.0);
    assert_eq!(
        verify(&t, &ctx(&r, None, &tool, Mode::Boundary)),
        vec![Warning::ShankHit { at: 1, part: true }]
    );
}

#[test]
fn a_hole_within_reach_verifies_silent() {
    // Same chain, 10 mm short of the reach law: nothing fires - and
    // nothing may, because this is the shape every clean drill op makes.
    let r = rect(0.0, 0.0, 100.0, 100.0);
    let tool = deep_tool();
    let t = hole_in_the_middle(-115.0);
    assert_eq!(verify(&t, &ctx(&r, None, &tool, Mode::Boundary)), Vec::new());
}
