//! M13.1 (#24): the hidden-rest scan answers past its holding budget.
//! A lattice whose plane claims do not fit at once is streamed in row
//! bands through the same judging body, so the verdict is the
//! whole-lattice verdict, bit for bit - a hidden slab or a small
//! hidden ledge is still found, a clean flat stays clean, and only
//! the walk's own work budget (or a single row that cannot be held)
//! still says NOT JUDGED.
//!
//! Provenance: our own fixtures, specced from the Prime Directive (a
//! false clean is a gouge-class bug); no vendor material.
//!
//! Gate map (M4.4): covers camcraft_sim::stock_sim (scan_hidden, scan_near).

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::{Point3, Triangle};
use camcraft_sim::stock_sim::{self, EnvelopeReport, HiddenRest, ScanBudget};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn feed(from: Point3, to: Point3) -> Move {
    Move { kind: MoveKind::Feed, from, to, feed: Some(500.0), arc: None, axis: None }
}

/// A closed box [x0,x1] x [y0,y1] x [z0,z1]: two triangles per face.
fn closed_box(lo: Point3, hi: Point3) -> Vec<Triangle> {
    let (x0, y0, z0, x1, y1, z1) = (lo.x, lo.y, lo.z, hi.x, hi.y, hi.z);
    let quad = |q: [Point3; 4]| [Triangle::new(q[0], q[1], q[2]), Triangle::new(q[0], q[2], q[3])];
    let mut f = Vec::new();
    f.extend(quad([p(x0, y0, z1), p(x1, y0, z1), p(x1, y1, z1), p(x0, y1, z1)]));
    f.extend(quad([p(x0, y0, z0), p(x0, y1, z0), p(x1, y1, z0), p(x1, y0, z0)]));
    f.extend(quad([p(x0, y0, z0), p(x1, y0, z0), p(x1, y0, z1), p(x0, y0, z1)]));
    f.extend(quad([p(x1, y0, z0), p(x1, y1, z0), p(x1, y1, z1), p(x1, y0, z1)]));
    f.extend(quad([p(x1, y1, z0), p(x0, y1, z0), p(x0, y1, z1), p(x1, y1, z1)]));
    f.extend(quad([p(x0, y1, z0), p(x0, y0, z0), p(x0, y0, z1), p(x0, y1, z1)]));
    f
}

fn slab(z0: f64, z1: f64) -> Vec<Triangle> {
    closed_box(p(-6.0, -6.0, z0), p(6.0, 6.0, z1))
}

/// Nine raster rows at tip height `z`, stepover 1.4 on the r = 3 ball.
fn raster(z: f64) -> Vec<Move> {
    (0..9)
        .map(|k| feed(p(-8.0, -5.3 + 1.4 * k as f64, z), p(8.0, -5.3 + 1.4 * k as f64, z)))
        .collect()
}

/// The same scan whole and starved: the starved one must stream and
/// answer exactly what the whole one did.
fn whole_and_banded(f: &[Triangle], moves: &[Move], budget: ScanBudget) -> (EnvelopeReport, EnvelopeReport) {
    let whole = stock_sim::envelope_scan(f, moves, 3.0, 0.1).expect("whole scan");
    let banded = stock_sim::envelope_scan_within(f, moves, 3.0, 0.1, budget).expect("banded scan");
    (whole, banded)
}

#[test]
fn past_the_claim_budget_the_hidden_slab_is_still_found() {
    // The ceiling fixture: a floor slab 0..2 under a plate 9..10, the
    // path riding the plate. At the r/8 lattice the 12 x 12 footprint
    // holds 32 x 32 columns of 4 claims - ~4096, far past the 600
    // allowed at once, so the old law refused the whole verdict.
    let mut f = slab(0.0, 2.0);
    f.extend(slab(9.0, 10.0));
    let budget = ScanBudget { cells: usize::MAX, claims: 600, work: usize::MAX };
    let (whole, banded) = whole_and_banded(&f, &raster(10.0), budget);
    assert_eq!(whole.hidden, HiddenRest::Judged);
    assert_eq!(banded.hidden, HiddenRest::Judged, "the stream judges what one pass cannot hold");
    assert!(banded.rest_mm > 7.0 && banded.rest_mm < 8.5, "hidden slab ~8 mm: {}", banded.rest_mm);
    assert_eq!(banded.rest_mm.to_bits(), whole.rest_mm.to_bits(), "bit for bit");
    assert_eq!((banded.at.x, banded.at.y), (whole.at.x, whole.at.y), "same place, same tie");
    assert_eq!(banded.gouge_mm, whole.gouge_mm);
    // And the 600-claim budget truly cannot hold the lattice: with the
    // stream starved of rows it still refuses by name.
    let row_starved = ScanBudget { cells: 10, claims: 600, work: usize::MAX };
    let rep = stock_sim::envelope_scan_within(&f, &raster(10.0), 3.0, 0.1, row_starved).expect("scan");
    assert!(matches!(rep.hidden, HiddenRest::OverBudget { .. }), "{:?}", rep.hidden);
}

#[test]
fn past_the_cell_budget_the_bands_answer_too() {
    let mut f = slab(0.0, 2.0);
    f.extend(slab(9.0, 10.0));
    let budget = ScanBudget { cells: 300, claims: usize::MAX, work: usize::MAX };
    let (whole, banded) = whole_and_banded(&f, &raster(10.0), budget);
    assert_eq!(banded.hidden, HiddenRest::Judged);
    assert_eq!(banded.rest_mm.to_bits(), whole.rest_mm.to_bits());
    assert_eq!((banded.at.x, banded.at.y), (whole.at.x, whole.at.y));
}

#[test]
fn a_small_hidden_ledge_in_one_band_is_not_skipped() {
    // A 1.5 mm ledge block standing in one corner under the plate: it
    // lives in a handful of rows, so a band that dropped or sampled
    // rows would lose it. Tight bands (~1 row each) still find it.
    let mut f = closed_box(p(3.0, 3.0, 0.0), p(4.5, 4.5, 1.0));
    f.extend(slab(9.0, 10.0));
    let budget = ScanBudget { cells: usize::MAX, claims: 100, work: usize::MAX };
    let (whole, banded) = whole_and_banded(&f, &raster(10.0), budget);
    assert!(whole.rest_mm > 7.5, "the ledge top sits ~9 mm under the ball: {}", whole.rest_mm);
    assert!(whole.at.x > 2.9 && whole.at.y > 2.9, "at the ledge: {:?}", whole.at);
    assert_eq!(banded.hidden, HiddenRest::Judged);
    assert_eq!(banded.rest_mm.to_bits(), whole.rest_mm.to_bits());
    assert_eq!((banded.at.x, banded.at.y), (whole.at.x, whole.at.y));
}

#[test]
fn a_clean_flat_stays_clean_when_streamed() {
    // No ceiling: ordinary milled ground. The stream must not invent
    // hidden rest where the whole scan finds none.
    let f = slab(0.0, 2.0);
    let budget = ScanBudget { cells: 300, claims: 200, work: usize::MAX };
    let (whole, banded) = whole_and_banded(&f, &raster(2.0), budget);
    assert_eq!(banded.hidden, HiddenRest::Judged);
    assert!(banded.rest_mm < 0.5, "flat ride leaves only the ridge reading: {}", banded.rest_mm);
    assert_eq!(banded.rest_mm.to_bits(), whole.rest_mm.to_bits());
}

#[test]
fn the_walk_stays_bounded_and_says_so() {
    // Past the work budget nothing is judged - the stated refusal, in
    // the banded pass as in the whole one, never a hang or a zero.
    let mut f = slab(0.0, 2.0);
    f.extend(slab(9.0, 10.0));
    for work in [50, 2_000, 6_000] {
        let budget = ScanBudget { cells: usize::MAX, claims: 600, work };
        let rep = stock_sim::envelope_scan_within(&f, &raster(10.0), 3.0, 0.1, budget).expect("scan");
        assert!(matches!(rep.hidden, HiddenRest::OverBudget { .. }), "work {work}: {:?}", rep.hidden);
        assert!(rep.rest_mm < 7.0, "work {work}: the hidden 8 mm is not folded in: {}", rep.rest_mm);
    }
}

#[test]
#[ignore = "heavy lane (release, M13.1 #24): a 44 M-claim stack past the SHIPPED holding budget, ~1 M columns"]
fn a_stack_past_the_shipped_budget_gets_a_real_verdict() {
    // Twenty-one 1 mm plates stacked 2 mm apart over 100 x 100, a ball
    // r = 0.5 riding the top plate: ~1.05 M columns x 42 claims ~ 44 M,
    // past the shipped 40 M. The old scan read NOT JUDGED; now the
    // verdict names the deepest plate's top, ~40 mm under the ball.
    let mut f = Vec::new();
    for k in 0..21 {
        let z = 2.0 * k as f64;
        f.extend(closed_box(p(-50.0, -50.0, z), p(50.0, 50.0, z + 1.0)));
    }
    let top = 41.0;
    let moves: Vec<Move> = (0..5)
        .map(|k| feed(p(-52.0, -40.0 + 20.0 * k as f64, top), p(52.0, -40.0 + 20.0 * k as f64, top)))
        .collect();
    // The rest of the verdict (ridge, gouge) alone, for the reading:
    // a zero work budget stops the hidden half at its first visit.
    let t = std::time::Instant::now();
    let starved = ScanBudget { work: 0, ..ScanBudget::SHIPPED };
    let base = stock_sim::envelope_scan_within(&f, &moves, 0.5, 0.1, starved).expect("scan");
    assert!(matches!(base.hidden, HiddenRest::OverBudget { .. }));
    eprintln!("without the hidden half: {:.1} s", t.elapsed().as_secs_f64());
    let t = std::time::Instant::now();
    let rep = stock_sim::envelope_scan(&f, &moves, 0.5, 0.1).expect("scan");
    let secs = t.elapsed().as_secs_f64();
    eprintln!("stack verdict: rest {:.3} mm at {:?}, {:?}, {secs:.1} s", rep.rest_mm, rep.at, rep.hidden);
    assert_eq!(rep.hidden, HiddenRest::Judged, "past the holding budget, judged");
    assert!(rep.rest_mm > 30.0, "the deepest plate's top is found: {}", rep.rest_mm);
    assert!(secs < 120.0, "bounded: {secs} s");
}
