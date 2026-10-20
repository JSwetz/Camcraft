//! Committed golden CL dumps for the 2D generators - the AGENTS.md
//! testing rule made concrete. Format, naming and the regeneration
//! workflow: tests/golden/CLAUDE.md.

mod common;

use common::{gen_to_cltext, write_golden};

use hsmworks_app::cl::MoveKind;
use hsmworks_app::geo2d::{Loop, Point2, Region};
use hsmworks_app::machining2d::{
    contour, face, pocket, slot, trace, ContourParams, FaceParams, Lead, PocketParams,
    PocketStrategy, Side, SlotParams, TraceParams,
};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .unwrap()])
    .unwrap()
}

/// 40x20 plate with an 8x8 through hole centered on (20,10).
fn plate_with_hole() -> Region {
    let mut loops = rect(0.0, 0.0, 40.0, 20.0).loops().to_vec();
    loops.extend_from_slice(rect(16.0, 6.0, 24.0, 14.0).loops());
    Region::new(loops).unwrap()
}

/// Two loops through Region::new: outer square with a centered square
/// void (a hole/island, depending on the operation reading it).
fn square_with_center(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let hx0 = (2.0 * x0 + x1) / 3.0;
    let hy0 = (2.0 * y0 + y1) / 3.0;
    let hx1 = (x0 + 2.0 * x1) / 3.0;
    let hy1 = (y0 + 2.0 * y1) / 3.0;
    Region::new(
        vec![
            Loop::new(vec![
                Point2::new(x0, y0),
                Point2::new(x1, y0),
                Point2::new(x1, y1),
                Point2::new(x0, y1),
            ]),
            Loop::new(vec![
                Point2::new(hx0, hy0),
                Point2::new(hx1, hy0),
                Point2::new(hx1, hy1),
                Point2::new(hx0, hy1),
            ]),
        ]
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap(),
    )
    .unwrap()
}

/// A closed ccw circle about `c`, tessellated into `n` segments - fine
/// enough that the arc fitter sees the circle in it (sagitta
/// `r(1 - cos(pi/n))` well under any sane tolerance), which is exactly
/// what STEP tessellation hands us for a bore.
fn circle(c: Point2, r: f64, n: usize) -> Loop {
    Loop::new(
        (0..n)
            .map(|k| {
                let t = std::f64::consts::TAU * k as f64 / n as f64;
                Point2::new(c.x + r * t.cos(), c.y + r * t.sin())
            })
            .collect(),
    )
    .unwrap()
}

#[test]
fn contour_disc_hole_matches_golden() {
    // A plate with a 128-gon approximation of a round bore: the rim is
    // polygonal only because import tessellates it, so the fitter must
    // replace the chord runs with arc moves - this golden is the v2
    // format's committed witness.
    let mut loops = rect(0.0, 0.0, 40.0, 20.0).loops().to_vec();
    loops.push(circle(Point2::new(20.0, 10.0), 6.0, 128));
    let region = Region::new(loops).unwrap();
    let p = ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::Arc { radius: 3.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
    };
    let t = contour(&region, &p).unwrap();
    assert!(t.has_arcs(), "a fine polygon bore rim must fit to arcs");
    write_golden("contour_disc_hole.cl", &gen_to_cltext(&t));
}

#[test]
fn contour_tangential_lead_matches_golden() {
    // The same plate-and-bore job, tangential entry instead of quarter:
    // this golden is the byte-witness that the half-arc leads post as
    // single S{±1} arc moves with a 2*radius standoff - rerun
    // WRITE_GOLDEN=1 only together with a deliberate law change.
    let mut loops = rect(0.0, 0.0, 40.0, 20.0).loops().to_vec();
    loops.push(circle(Point2::new(20.0, 10.0), 6.0, 128));
    let region = Region::new(loops).unwrap();
    let p = ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::Tangential { radius: 3.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
    };
    let t = contour(&region, &p).unwrap();
    assert!(t.has_arcs());
    write_golden("contour_tangential_lead.cl", &gen_to_cltext(&t));
}

#[test]
fn face_square_40x40_matches_golden() {
    // The same plain 40x40 square the row-count tests use, at the
    // default parameters: five rows, rapids only at the ends.
    let p = FaceParams {
        tool_diameter: 10.0,
        stepover: 8.0,
        clearance_z: 5.0,
        cut_z: -2.0,
        top_z: None,
        stepdown: None,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    let t = face(&rect(0.0, 0.0, 40.0, 40.0), &p).unwrap();
    write_golden("face_square_40x40.cl", &gen_to_cltext(&t));
}

#[test]
fn face_square_40x40_stepped_matches_golden() {
    // The same square staged from 2.0 down to -2.0 in 1.5 steps: the row
    // band is identical at every level, and each level change lifts to
    // clearance and re-enters above the first row's start.
    let p = FaceParams {
        tool_diameter: 10.0,
        stepover: 8.0,
        clearance_z: 5.0,
        cut_z: -2.0,
        top_z: Some(2.0),
        stepdown: Some(1.5),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    let t = face(&rect(0.0, 0.0, 40.0, 40.0), &p).unwrap();
    assert_eq!(
        t.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count(),
        4,
        "levels 2.0, 0.5, -1.0, -2.0"
    );
    write_golden("face_square_40x40_stepped.cl", &gen_to_cltext(&t));
}

#[test]
fn contour_plate_with_hole_matches_golden() {
    // 40x20 plate with a 8x8 through hole: contour offsets material
    // away from every loop at each level, so both the outer and the hole
    // boundary must appear. Lead-in arcs included.
    let p = ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::Arc { radius: 3.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
    };
    let t = contour(&plate_with_hole(), &p).unwrap();
    write_golden("contour_plate_with_hole.cl", &gen_to_cltext(&t));
}

#[test]
fn pocket_zigzag_plate_with_island_matches_golden() {
    // 30x30 region with a centered 10x10 island; zigzag rows must stop
    // at the island and step over it between levels.
    let p = PocketParams {
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
    };
    let t = pocket(&square_with_center(0.0, 0.0, 30.0, 30.0), &p).unwrap();
    write_golden("pocket_zigzag_plate_with_island.cl", &gen_to_cltext(&t));
}

#[test]
fn pocket_waterline_matches_golden() {
    // Same region, waterline rings. Committed as its own golden because
    // ring ordering is a place where a "small" generator change tends to
    // reorder everything.
    let p = PocketParams {
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
        strategy: PocketStrategy::Waterline,
    };
    let t = pocket(&square_with_center(0.0, 0.0, 30.0, 30.0), &p).unwrap();
    write_golden("pocket_waterline_plate_with_island.cl", &gen_to_cltext(&t));
}

#[test]
fn slot_fullwidth_centerline_matches_golden() {
    // Tool fills the channel (dia 10 = width 10): one centerline row per
    // level, the everyday slot.
    let p = SlotParams {
        tool_diameter: 10.0,
        width: 10.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -6.0,
        stepdown: 3.0,
        stepover: 5.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    let t = slot(Point2::new(0.0, 0.0), Point2::new(60.0, 0.0), &p).unwrap();
    write_golden("slot_fullwidth_centerline.cl", &gen_to_cltext(&t));
}

#[test]
fn slot_wide_channel_rows_matches_golden() {
    // Width 20 with a 6 mm tool, stepover 4: several serpentine rows,
    // each shortened by the round-end reach sqrt(h^2 - d^2) - the case
    // where the closed-form stadium math must agree with a Clipper offset.
    let p = SlotParams {
        tool_diameter: 6.0,
        width: 20.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 4.0,
        stepover: 4.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    let t = slot(Point2::new(-10.0, 0.0), Point2::new(50.0, 0.0), &p).unwrap();
    write_golden("slot_wide_channel_rows.cl", &gen_to_cltext(&t));
}

#[test]
fn trace_plate_with_hole_matches_golden() {
    // Engraving: cutter center exactly on both loops, at every level.
    let p = TraceParams {
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -2.0,
        stepdown: 1.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    let t = trace(&plate_with_hole(), &p).unwrap();
    write_golden("trace_plate_with_hole.cl", &gen_to_cltext(&t));
}

// ------------------------------------------------------------ drilling

use hsmworks_app::drilling::{CycleChoice, CycleParams, Hole, drill};
use hsmworks_app::tools::{Tool, ToolKind};

fn cycle_tool(kind: ToolKind, diameter: f64, tip_angle: f64, pitch: f64) -> Tool {
    Tool {
        kind,
        diameter,
        corner_r: 0.0,
        tip_angle,
        pitch,
        flute_len: 40.0,
        shank_d: diameter.min(10.0),
        shank_len: 30.0,
        holder_d: 20.0,
        holder_len: 40.0,
        material_group: String::from("steel"),
        rpm: 500.0,
        feed: 200.0,
    }
}

#[test]
fn drill_plate_matches_golden() {
    // Auto law, shallow side: both holes are within one diameter deep,
    // so both post as plain G81s. The through hole's floor carries the
    // 1 mm exit allowance; the `; CYCLE` lines are the v3 witness that
    // the intent rides alongside the chain.
    let tool = cycle_tool(ToolKind::Drill, 6.0, 118.0, 0.0);
    let holes = [
        Hole {
            xy: Point2::new(10.0, 10.0),
            radius: 3.0,
            entry_z: 0.0,
            floor_z: -4.0,
            through: true,
        },
        Hole {
            xy: Point2::new(30.0, 10.0),
            radius: 3.0,
            entry_z: 0.0,
            floor_z: -3.0,
            through: false,
        },
    ];
    let p = CycleParams {
        clearance_z: 5.0,
        r_z: 0.0,
        hop_z: None,
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 200.0,
        plunge_feed: 60.0,
        tool: &tool,
        holes: &holes,
    };
    let t = drill(&p).unwrap();
    assert_eq!(t.cycles.len(), 2);
    assert!(
        t.cycles.iter().all(|c| c.kind == hsmworks_app::cl::CycleKind::Drill),
        "one-diameter depths must Auto to plain Drill"
    );
    write_golden("drill_plate.cl", &gen_to_cltext(&t));
}

#[test]
fn drill_peck_matches_golden() {
    // Auto law, deep side: 24 mm on a 6 mm drill is exactly 4
    // diameters - the chip-break Peck band (G73 shape), auto step 3d.
    // The record says the same: one hop, two plunges, one path.
    let tool = cycle_tool(ToolKind::Drill, 6.0, 118.0, 0.0);
    let holes = [Hole {
        xy: Point2::new(20.0, 10.0),
        radius: 3.0,
        entry_z: 0.0,
        floor_z: -23.0,
        through: true,
    }];
    let p = CycleParams {
        clearance_z: 6.0,
        r_z: 0.0,
        hop_z: None,
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 200.0,
        plunge_feed: 60.0,
        tool: &tool,
        holes: &holes,
    };
    let t = drill(&p).unwrap();
    assert_eq!(t.cycles.len(), 1);
    assert_eq!(t.cycles[0].kind, hsmworks_app::cl::CycleKind::Peck);
    assert!((t.cycles[0].peck - 18.0).abs() < 1e-9, "auto chip-break step = 3d");
    write_golden("drill_peck.cl", &gen_to_cltext(&t));
}

#[test]
fn tap_plate_matches_golden() {
    // Rigid tapping: Auto sees the Tap tool and taps - feed down and
    // back at rpm x pitch, no rapids inside the hole. The chain is the
    // truth for verify; the cycle line is what the post emits as G84.
    let tool = cycle_tool(ToolKind::Tap, 10.0, 0.0, 1.5);
    let holes = [Hole {
        xy: Point2::new(15.0, 15.0),
        radius: 2.5,
        entry_z: 0.0,
        floor_z: -12.0,
        through: false,
    }];
    let p = CycleParams {
        clearance_z: 5.0,
        r_z: 0.0,
        hop_z: None,
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 200.0,
        plunge_feed: 60.0,
        tool: &tool,
        holes: &holes,
    };
    let t = drill(&p).unwrap();
    assert_eq!(t.cycles.len(), 1);
    assert_eq!(t.cycles[0].kind, hsmworks_app::cl::CycleKind::Tap);
    write_golden("tap_plate.cl", &gen_to_cltext(&t));
}

// ---------------------------------------------------------------------------
// M2.2a - the 3D waterline. The same rule as the 2D generators: the exact
// ring set, their arc fits, and the level order must not move unnoticed.
// ---------------------------------------------------------------------------

#[test]
fn finish3d_waterline_cone() {
    use hsmworks_app::finishing3d::{waterline, Finish3DParams, FinishPattern};
    use hsmworks_app::geometry::{Point3, Triangle};
    let p3 = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    // 45-degree cone, apex (0,0,20), R20 base ring at z 0, closed bottom.
    let n = 96;
    let mut facets = Vec::new();
    for j in 0..n {
        let a = std::f64::consts::TAU * j as f64 / n as f64;
        let b = std::f64::consts::TAU * (j + 1) as f64 / n as f64;
        facets.push(Triangle::new(
            p3(0.0, 0.0, 20.0),
            p3(20.0 * a.cos(), 20.0 * a.sin(), 0.0),
            p3(20.0 * b.cos(), 20.0 * b.sin(), 0.0),
        ));
        facets.push(Triangle::new(
            p3(0.0, 0.0, 0.0),
            p3(20.0 * b.cos(), 20.0 * b.sin(), 0.0),
            p3(20.0 * a.cos(), 20.0 * a.sin(), 0.0),
        ));
    }
    let t = waterline(
        &facets,
        &Finish3DParams {
            pattern: FinishPattern::Waterline,
            stepover: None,
            tool_diameter: 6.0,
            tolerance: 0.1,
            clearance_z: 25.0,
            retract_z: None,
            top_z: 16.0,
            bottom_z: 0.5,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        },
    )
    .unwrap();
    write_golden("finish3d_waterline_cone.cl", &gen_to_cltext(&t));
}
