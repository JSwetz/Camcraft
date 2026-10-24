//! Split from `machining3d.rs` by M5.0.1; structure only.

use crate::geo2d::Point2;
use crate::stock_sim::Dexel;
use super::*;

#[test]
fn a_chord_over_a_step_reads_half_engagement() {
    // The chord law at its plainest: travel along a wall, half the
    // disc in standing material, half in air - the measurement says
    // 0.5 up to the 17-point quantization (8 hits, 9 misses).
    let mut dex = Dexel::block(0.0, 0.0, 16.0, 16.0, 10.0, 1.0).unwrap();
    for iy in 0..dex.ny {
        for ix in 0..dex.nx {
            dex.top[iy * dex.nx + ix] = if ix < 8 { 10.0 } else { 0.0 };
        }
    }
    let ae = engagement(&dex, Point2::new(8.0, 4.0), Point2::new(8.0, 12.0), 5.0, 8.0);
    assert!((ae - 0.5).abs() < 0.05, "half-bite read {ae}");
    // Pure air, and pure solid, at the same place.
    let out = dex.top.len();
    let _ = out;
    for t in dex.top.iter_mut() {
        *t = 0.0;
    }
    let ae = engagement(&dex, Point2::new(8.0, 4.0), Point2::new(8.0, 12.0), 5.0, 8.0);
    assert!(ae < 1e-9, "air must read zero, got {ae}");
    for (iy, t) in dex.top.chunks_mut(dex.nx).enumerate() {
        for (ix, c) in t.iter_mut().enumerate() {
            *c = if (iy * dex.nx + ix) < 8 * dex.nx { 10.0 } else { 0.0 };
            let _ = ix;
        }
    }
    let ae = engagement(&dex, Point2::new(8.0, 1.0), Point2::new(8.0, 2.0), 5.0, 0.5);
    assert!((ae - 1.0).abs() < 1e-9, "solid must read full, got {ae}");
}

#[test]
fn a_zero_length_planar_move_reads_as_a_full_bite() {
    // The ambiguity law: no direction, no chord normal to measure -
    // the safe read is the heavy one.
    let dex = Dexel::block(0.0, 0.0, 4.0, 4.0, 10.0, 1.0).unwrap();
    let ae = engagement(&dex, Point2::new(2.0, 2.0), Point2::new(2.0, 2.0), 5.0, 1.0);
    assert!((ae - 1.0).abs() < 1e-12, "got {ae}");
}
