//! M7.3's cost law, proved by counting: a field update walks only the
//! bricks its sweep can change (work proportional to the tool's own
//! footprint, never to the grid), a batch visits each dirty brick once
//! however many moves reach it, and a brick the tool has already
//! emptied is skipped whole - all while answering bit for bit what the
//! per-move walk answers. The counters are `SdfField::cut_stats`.
//! Provenance: the brick-map dirty-region discipline of the Phase 7
//! reference note, on this codebase's own field engine - nothing from
//! vendor binaries.
//!
//! Gate map (M4.4): covers camcraft_sim::stock_sim::field_batch and
//! the batched entry of camcraft_sim::stock_sim::field_cut.

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::{cube, Point3};
use camcraft_sim::stock_sim::{SdfField, SdfTool, Stock};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn feed(from: Point3, to: Point3) -> Move {
    Move { kind: MoveKind::Feed, from, to, feed: Some(500.0), arc: None, axis: None }
}

/// A `w` x `w` x 5 slab on unit cells (band 4, cap 4).
fn slab(w: f64) -> SdfField {
    SdfField::from_mesh(&cube(p(0.0, 0.0, 0.0), p(w, w, 5.0)).facets, 1.0, 4).expect("the slab builds")
}

/// The verdict world's flat tool: a flank far taller than any stock
/// here, the way `Stock::simulate` gives the column's reach.
const SPANNING: SdfTool = SdfTool::Flat { r: 2.0, flank: 1000.0 };

#[test]
fn a_far_move_touches_its_own_footprint_not_the_grid() {
    // The same plunge-and-feed, far from an earlier cut, on a slab 4x
    // and 16x the area: identical brick and cell work, and inside the
    // tool's own box of bricks - the window lateral (r + cap + cell =
    // 7 cells each side) by the stock's height, never the flank's.
    let earlier = [feed(p(3.0, 3.0, 3.0), p(10.0, 3.0, 3.0))];
    let probe = [feed(p(18.0, 18.0, 8.0), p(18.0, 18.0, 3.0)), feed(p(18.0, 18.0, 3.0), p(21.0, 18.0, 3.0))];
    let mut stats = Vec::new();
    for w in [24.0, 96.0] {
        let mut f = slab(w);
        f.subtract_batch(&earlier, &SPANNING, 0.01).expect("the earlier cut");
        f.reset_cut_stats();
        f.subtract_batch(&probe, &SPANNING, 0.01).expect("the probe");
        stats.push((w, f.brick_count(), f.cut_stats()));
    }
    let (_, small_bricks, a) = stats[0];
    let (_, large_bricks, b) = stats[1];
    assert!(large_bricks > 4 * small_bricks, "fixture sanity: {small_bricks} vs {large_bricks} bricks");
    assert_eq!(a, b, "the work must not grow with the grid");
    // x cells 11..=28 and y 11..=25 -> 3 bricks each; z clipped to the
    // cut region - the slab's band, whose graph horizon reaches
    // (cap + 2 cells) / HOP ~ 10 cells past each face -> 6 bricks.
    // The 1000 mm flank's own length never enters the count.
    assert!(a.bricks_visited <= 3 * 3 * 6, "visited {} bricks: {a:?}", a.bricks_visited);
    assert!(a.bricks_visited > 0 && a.cells_evaluated > 0, "the probe did no work: {a:?}");
}

#[test]
fn a_batch_visits_each_brick_once_and_answers_the_same() {
    // Twenty passes down the same slot: move by move, every pass walks
    // every brick again; as one batch each brick is visited once. And
    // the field the two leave is the same field, bit for bit - both
    // layers commute, and every skip is a proof.
    let mut moves = Vec::new();
    for n in 0..20 {
        let (a, b) = (p(4.0, 12.0, 3.0 - 0.05 * n as f64), p(20.0, 12.0, 3.0 - 0.05 * n as f64));
        moves.push(if n % 2 == 0 { feed(a, b) } else { feed(b, a) });
    }
    let (mut batch, mut single) = (slab(24.0), slab(24.0));
    batch.subtract_batch(&moves, &SPANNING, 0.01).expect("batch");
    for m in &moves {
        single.subtract(m, &SPANNING, 0.01).expect("move");
    }
    for i in -3..28 {
        for j in -3..28 {
            for k in -3..10 {
                let q = p(i as f64 + 0.5, j as f64 + 0.5, k as f64 + 0.5);
                let (x, y) = (batch.distance(q), single.distance(q));
                assert_eq!(x.to_bits(), y.to_bits(), "the batch answered {x} where the moves answer {y} at {q:?}");
            }
        }
    }
    let (sb, ss) = (Stock::Field(batch), Stock::Field(single));
    for i in 0..24 {
        for j in 0..24 {
            let (x, y) = (i as f64 + 0.5, j as f64 + 0.5);
            assert_eq!(sb.top(x, y).to_bits(), ss.top(x, y).to_bits(), "column ({x}, {y}) height differs");
        }
    }
    let (b, s) = (
        sb.field().map(|f| f.cut_stats()).unwrap_or_default(),
        ss.field().map(|f| f.cut_stats()).unwrap_or_default(),
    );
    assert!(b.bricks_visited * 10 <= s.bricks_visited, "batch visited {} vs per-move {}", b.bricks_visited, s.bricks_visited);
    assert!(b.cells_evaluated <= s.cells_evaluated, "batch evaluated more cells: {b:?} vs {s:?}");
}

#[test]
fn an_emptied_brick_is_skipped_whole() {
    // An 18 mm flat swallows a block of bricks; the same cut again
    // finds them full and walks only the surface left at its edges.
    let mut f = slab(24.0);
    let big = SdfTool::Flat { r: 9.0, flank: 1000.0 };
    let m = [feed(p(12.0, 12.0, -6.0), p(12.0, 12.0, -6.0))];
    f.subtract_batch(&m, &big, 0.01).expect("first");
    let first = f.cut_stats();
    f.reset_cut_stats();
    f.subtract_batch(&m, &big, 0.01).expect("again");
    let again = f.cut_stats();
    assert!(again.bricks_skipped > 0, "no full brick was skipped: {again:?}");
    assert!(again.bricks_touched < first.bricks_touched, "{again:?} vs {first:?}");
    assert!(again.cells_evaluated < first.cells_evaluated, "{again:?} vs {first:?}");
}

#[test]
fn a_long_move_pays_only_inside_the_stock() {
    // 400 mm of travel over a 10 mm box at a 0.25 mm cell: the window
    // the old law dilated refused as unpayable; clipped to the region
    // a cut can matter in, the walk is the box's own and the move cuts.
    let mut f = SdfField::from_mesh(&cube(p(0.0, 0.0, 0.0), p(10.0, 10.0, 10.0)).facets, 0.25, 4)
        .expect("the box builds");
    let q = p(5.125, 0.125, 9.875);
    assert!(f.distance(q) < 0.0, "fixture sanity: material before the cut");
    let m = feed(p(0.0, 0.0, 9.5), p(400.0, 5.0, 9.5));
    f.subtract(&m, &SdfTool::Ball { r: 0.5 }, 0.01).expect("a payable move must cut");
    assert!(f.distance(q) > 0.0, "the ball's groove must be air at {q:?}");
    let far = p(300.125, 3.875, 10.125);
    assert!(f.distance(far) > 0.0, "far air stays air");
}
