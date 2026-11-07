//! Law (d) for radial programs (#79): a cross-drill rapid is judged as
//! its posed flute body against the stock prism minus the kerf the
//! cuts before it swept. Before #79 the law stood down for
//! `Mode::Radial` and a station change through uncut stock posted
//! silently; here it convicts by name, while the retracts up a hole's
//! own kerf, the peck's chip break and the approach to a mouth on the
//! stock surface stay clean.
//!
//! Provenance: our own hand-made streams, shaped like the cross-drill
//! kernel's (approach, plunge, retract, hop, station change), specced
//! from the Prime Directive and the shop rule "never traverse through
//! uncut stock"; no vendor material.
//!
//! Gate map (law d radial): covers camcraft_sim::verify (rapids,
//! rapids_radial, midpose, rest).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{verify, Mode, ToolGeom, VerifyContext, Warning};

/// Bar stock radius about the spindle (world Z), mm.
const STOCK_R: f64 = 20.0;

fn disc(r: f64) -> Region {
    let n = 128;
    let pts = (0..n)
        .map(|j| {
            let t = std::f64::consts::TAU * j as f64 / n as f64;
            Point2::new(r * t.cos(), r * t.sin())
        })
        .collect();
    match Loop::new(pts).and_then(|l| Region::new(vec![l])) {
        Ok(r) => r,
        Err(e) => panic!("disc: {e:?}"),
    }
}

/// A 4 mm drill, 20 mm of flute.
fn drill() -> ToolGeom {
    ToolGeom {
        radius: 2.0,
        nose_radius: 0.0,
        flute_len: 20.0,
        shank_radius: 2.0,
        shank_len: 20.0,
        holder_radius: 8.0,
        holder_len: 20.0,
    }
}

/// Judge `p` in `mode` on an r-20 bar stock, Z top 30.
fn judge_in(p: &Toolpath, mode: Mode) -> Vec<Warning> {
    let stock = disc(STOCK_R);
    let tool = drill();
    let cx = VerifyContext {
        region: &stock,
        stock: Some(&stock),
        model: None,
        tool: &tool,
        cutter: None,
        mode,
        stock_top_z: 30.0,
        clearance_z: 40.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-3,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
    };
    verify(p, &cx)
}

fn judge(p: &Toolpath) -> Vec<Warning> {
    judge_in(p, Mode::Radial)
}

const PX: [f64; 3] = [1.0, 0.0, 0.0];
const PY: [f64; 3] = [0.0, 1.0, 0.0];

/// One step of a hand-made chain: (kind, to, axis stated).
type Step = (MoveKind, (f64, f64, f64), Option<[f64; 3]>);

/// A chain from `start`.
fn chain(start: (f64, f64, f64), steps: &[Step]) -> Toolpath {
    let mut at = Point3::new(start.0, start.1, start.2);
    let moves = steps
        .iter()
        .map(|&(k, (x, y, z), axis)| {
            let to = Point3::new(x, y, z);
            let feed = (k != MoveKind::Rapid).then_some(80.0);
            let mut m = Move::new(k, at, to, feed);
            m.axis = axis;
            at = to;
            m
        })
        .collect();
    Toolpath { moves, cycles: Vec::new(), notes: Vec::new(), comps: Vec::new() }
}

const R: MoveKind = MoveKind::Rapid;
const P: MoveKind = MoveKind::Plunge;

/// Two holes on the +X and +Y stations, mouths on the r-20 stock,
/// drilled to r 10, approached from a 5 mm standoff. `hop` is the
/// radius both stations lift to before the quarter-turn chord.
fn two_stations(hop: f64) -> Toolpath {
    chain(
        (30.0, 0.0, 0.0),
        &[
            (R, (25.0, 0.0, 0.0), Some(PX)),
            (P, (10.0, 0.0, 0.0), None),
            (R, (25.0, 0.0, 0.0), None),
            (R, (hop, 0.0, 0.0), None),
            // The station change, on the old station's pose.
            (R, (0.0, hop, 0.0), None),
            (R, (0.0, 25.0, 0.0), Some(PY)),
            (P, (0.0, 10.0, 0.0), None),
            (R, (0.0, 25.0, 0.0), None),
            (R, (0.0, 30.0, 0.0), None),
        ],
    )
}

#[test]
fn a_station_change_hopped_outside_the_stock_is_clean() {
    // Hop at 25 * sqrt(2): the chord's closest approach is r 25, the
    // drill's rim at 23 - clear of the r-20 bar all the way.
    let w = judge(&two_stations(25.0 * std::f64::consts::SQRT_2));
    assert!(w.is_empty(), "a clear radial program reported: {w:?}");
}

#[test]
fn a_station_change_chord_through_the_stock_blocks_by_name() {
    // Hop at r 25 with no inflation: the quarter-turn chord's middle
    // stands at r 17.7, inside the r-20 bar (#77's shape).
    let w = judge(&two_stations(25.0));
    assert_eq!(w, vec![Warning::RapidInStock { at: 4 }], "{w:?}");
}

#[test]
fn the_column_reading_still_stands_for_a_mill_mode_twin() {
    // The same stream judged as a mill Area op keeps its own law's
    // verdict (the column reads the plunge's air as a hole down Z):
    // the radial reading is Radial's alone.
    let radial = judge(&two_stations(25.0));
    let area = judge_in(&two_stations(25.0), Mode::Area);
    assert!(radial.iter().all(|w| matches!(w, Warning::RapidInStock { .. })));
    assert!(area.iter().any(|w| matches!(w, Warning::RapidInStock { .. })), "{area:?}");
}

#[test]
fn an_approach_that_stops_inside_the_stock_blocks() {
    // The mouth was read off a smaller part (r 15): the approach
    // rapid ends 3 mm inside the r-20 raw bar.
    let w = judge(&chain(
        (30.0, 0.0, 0.0),
        &[
            (R, (18.0, 0.0, 0.0), Some(PX)),
            (P, (10.0, 0.0, 0.0), None),
            (R, (18.0, 0.0, 0.0), None),
            (R, (30.0, 0.0, 0.0), None),
        ],
    ));
    assert_eq!(w, vec![Warning::RapidInStock { at: 0 }], "{w:?}");
}

#[test]
fn a_rapid_down_a_hole_not_yet_drilled_blocks() {
    // Cut order: the kerf a later plunge makes is no alibi for the
    // rapid that runs down its axis first.
    let w = judge(&chain(
        (30.0, 0.0, 0.0),
        &[
            (R, (12.0, 0.0, 0.0), Some(PX)),
            (P, (10.0, 0.0, 0.0), None),
            (R, (30.0, 0.0, 0.0), None),
        ],
    ));
    assert_eq!(w, vec![Warning::RapidInStock { at: 0 }], "{w:?}");
}

#[test]
fn peck_chip_breaks_and_the_full_retract_ride_the_kerf() {
    // Peck: plunge, back off 0.5 inside the hole, plunge on, retract
    // through the whole kerf and out past the mouth - all clean.
    let w = judge(&chain(
        (30.0, 0.0, 5.0),
        &[
            (R, (25.0, 0.0, 5.0), Some(PX)),
            (P, (14.0, 0.0, 5.0), None),
            (R, (14.5, 0.0, 5.0), None),
            (P, (6.0, 0.0, 5.0), None),
            (R, (6.5, 0.0, 5.0), None),
            (P, (0.0, 0.0, 5.0), None),
            (R, (25.0, 0.0, 5.0), None),
            (R, (30.0, 0.0, 5.0), None),
        ],
    ));
    assert!(w.is_empty(), "the kerf convicted its own retract: {w:?}");
}

#[test]
fn a_lateral_slide_out_of_the_hole_blocks() {
    // The tool leaves the bottom of the hole sideways along Z: the
    // rim runs through the bar the plunge never touched.
    let w = judge(&chain(
        (30.0, 0.0, 0.0),
        &[
            (R, (25.0, 0.0, 0.0), Some(PX)),
            (P, (10.0, 0.0, 0.0), None),
            (R, (10.0, 0.0, 8.0), None),
            (R, (30.0, 0.0, 8.0), None),
        ],
    ));
    assert!(w.contains(&Warning::RapidInStock { at: 2 }), "{w:?}");
}

#[test]
fn the_over_the_end_face_detour_is_clean_and_a_low_one_blocks() {
    // Opposite stations (+X then -X): lift on the old column above
    // the Z-30 top, index there, descend on the new column.
    let detour = |z_top: f64| {
        chain(
            (30.0, 0.0, 0.0),
            &[
                (R, (25.0, 0.0, 0.0), Some(PX)),
                (P, (10.0, 0.0, 0.0), None),
                (R, (25.0, 0.0, 0.0), None),
                (R, (25.0, 0.0, z_top), None),
                (R, (-25.0, 0.0, z_top), Some([-1.0, 0.0, 0.0])),
                (R, (-25.0, 0.0, 0.0), None),
                (P, (-10.0, 0.0, 0.0), None),
                (R, (-30.0, 0.0, 0.0), None),
            ],
        )
    };
    let w = judge(&detour(35.0));
    assert!(w.is_empty(), "the detour above the end face reported: {w:?}");
    // The same swing at Z 25 runs under the stock top, through the bar.
    let w = judge(&detour(25.0));
    assert_eq!(w, vec![Warning::RapidInStock { at: 4 }], "{w:?}");
}
