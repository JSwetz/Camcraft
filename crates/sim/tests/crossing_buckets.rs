//! The verifier's crossing law, bucketed (M11.4 C round 4): the grid
//! broad phase reports exactly the all-pairs law's move pairs - on
//! random walks, star bursts (every chord through one point), long
//! chords over a dense field, ramps against planes, arcs, shared
//! vertices and non-finite moves. Our own harness; no vendor material.
//!
//! Gate map (M4.4): covers camcraft_sim::verify (crossing).

use camcraft_sim::cl::{MoveKind, PathBuilder, Toolpath};
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{crossed_pairs, crossed_pairs_all};

/// A small deterministic generator (xorshift64*): no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn same(path: &Toolpath) -> usize {
    let all = crossed_pairs_all(path, 0.01);
    let fast = crossed_pairs(path, 0.01);
    assert_eq!(fast, all, "the bucketed pair set is the all-pairs set");
    all.len()
}

#[test]
fn random_walks_on_two_planes_cross_the_same_pairs() {
    for seed in 1..=12u64 {
        let mut g = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut b = PathBuilder::new(Point3::new(0.0, 0.0, 0.0));
        for k in 0..600 {
            let z = if k % 97 < 50 { 0.0 } else { -1.0 };
            let (x, y) = (g.next() * 40.0, g.next() * 30.0);
            match k % 7 {
                // A ramp across the band of a plane.
                3 => b.push(MoveKind::Feed, Point3::new(x, y, z - 0.5), Some(100.0)),
                // An arc about a random center.
                5 => b.push_arc(MoveKind::Feed, Point3::new(x, y, z), Some(100.0), Point2::new(20.0, 15.0), k % 2 == 0),
                // A rapid (never in the law).
                6 => b.push(MoveKind::Rapid, Point3::new(x, y, z), None),
                _ => b.push(MoveKind::Feed, Point3::new(x, y, z), Some(100.0)),
            }
        }
        let n = same(&b.finish());
        assert!(n > 0, "a random walk crosses itself");
    }
}

#[test]
fn star_bursts_long_chords_and_grid_lines_cross_the_same_pairs() {
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, 0.0));
    // Every chord through the origin and back: shared vertices only.
    for k in 0..90 {
        let a = k as f64 * 0.07;
        b.push(MoveKind::Feed, Point3::new(10.0 * a.cos(), 10.0 * a.sin(), 0.0), Some(1.0));
        b.push(MoveKind::Feed, Point3::new(0.0, 0.0, 0.0), Some(1.0));
    }
    // A dense comb, then two long diagonals over it (wide chords).
    for k in 0..300 {
        let x = -20.0 + k as f64 * 0.13;
        b.push(MoveKind::Feed, Point3::new(x, -15.0, 0.0), Some(1.0));
        b.push(MoveKind::Feed, Point3::new(x, 15.0, 0.0), Some(1.0));
    }
    b.push(MoveKind::Feed, Point3::new(-20.0, -15.0, 0.0), Some(1.0));
    b.push(MoveKind::Feed, Point3::new(400.0, 300.0, 0.0), Some(1.0));
    b.push(MoveKind::Feed, Point3::new(-400.0, 300.0, 0.0), Some(1.0));
    // Collinear overlap and a touching T (not proper crossings).
    b.push(MoveKind::Feed, Point3::new(-400.0, -300.0, 0.0), Some(1.0));
    b.push(MoveKind::Feed, Point3::new(-400.0, 300.0, 0.0), Some(1.0));
    let n = same(&b.finish());
    assert!(n > 300, "the diagonals cross the comb: {n}");
}

#[test]
fn non_finite_and_degenerate_moves_cross_the_same_pairs() {
    let mut moves = PathBuilder::new(Point3::new(0.0, 0.0, 0.0));
    moves.push(MoveKind::Feed, Point3::new(5.0, 5.0, 0.0), Some(1.0));
    moves.push(MoveKind::Feed, Point3::new(5.0, 0.0, 0.0), Some(1.0));
    moves.push(MoveKind::Feed, Point3::new(0.0, 5.0, 0.0), Some(1.0));
    moves.push(MoveKind::Feed, Point3::new(f64::NAN, 5.0, 0.0), Some(1.0));
    moves.push(MoveKind::Feed, Point3::new(2.0, f64::INFINITY, 0.0), Some(1.0));
    moves.push(MoveKind::Feed, Point3::new(2.0, 2.0, 0.0), Some(1.0));
    moves.push(MoveKind::Feed, Point3::new(2.0, 2.0, 0.0), Some(1.0));
    same(&moves.finish());
    same(&Toolpath::new(Vec::new()));
}
