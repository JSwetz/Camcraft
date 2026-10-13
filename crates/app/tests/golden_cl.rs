//! Committed golden CL dumps for the 2D generators - the AGENTS.md
//! testing rule made concrete. Format, naming and the regeneration
//! workflow: tests/golden/CLAUDE.md.

mod common;

use common::{gen_to_cltext, write_golden};

use hsmworks_app::geo2d::{Loop, Point2, Region};
use hsmworks_app::machining2d::{
    contour, face, pocket, ContourParams, FaceParams, Lead, PocketParams, PocketStrategy,
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
    let r = {
        let outer = rect(0.0, 0.0, 40.0, 20.0);
        let hole = rect(16.0, 6.0, 24.0, 14.0);
        let mut loops: Vec<Loop> = outer.loops().to_vec();
        loops.extend_from_slice(hole.loops());
        Region::new(loops).unwrap()
    };
    let t = contour(&r, &p).unwrap();
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
