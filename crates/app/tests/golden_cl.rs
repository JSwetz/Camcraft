//! Committed golden CL dumps for the 2D generators - the AGENTS.md
//! testing rule made concrete. Format, naming and the regeneration
//! workflow: tests/golden/CLAUDE.md.

mod common;

use common::{gen_to_cltext, write_golden};

use hsmworks_app::geo2d::{Loop, Point2, Region};
use hsmworks_app::machining2d::{
    contour, face, pocket, slot, trace, ContourParams, FaceParams, Lead, PocketParams,
    PocketStrategy, SlotParams, TraceParams,
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
fn face_square_40x40_matches_golden() {
    // The same plain 40x40 square the row-count tests use, at the
    // default parameters: five rows, rapids only at the ends.
    let p = FaceParams {
        tool_diameter: 10.0,
        stepover: 8.0,
        clearance_z: 5.0,
        cut_z: -2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    let t = face(&rect(0.0, 0.0, 40.0, 40.0), &p).unwrap();
    write_golden("face_square_40x40.cl", &gen_to_cltext(&t));
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
