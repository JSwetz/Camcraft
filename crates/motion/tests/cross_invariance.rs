//! Cross-drill topology ignores sub-nanometre arithmetic residue (#151).
//! Gate map (M4.4): covers motion::drilling::cross.
//! Provenance: synthetic stations and perturbed radii; our own design notes.

use camcraft_motion::cl::Toolpath;
use camcraft_motion::drilling::{CrossHole, CrossParams, CycleChoice, cross_drill};
use camcraft_motion::geometry::Point3;
use camcraft_motion::tools::catalog;

fn chain(stock_r: f64) -> Toolpath {
    let tool = catalog().into_iter().find(|(name, _)| *name == "DR-0400").expect("catalog drill").1;
    let hole = |origin, dir| CrossHole { origin, dir, radius: 2.0, entry_d: 0.0, floor_d: -6.0, through: false };
    let holes = [
        hole(Point3::new(10.0, 0.0, 10.0), Point3::new(-1.0, 0.0, 0.0)),
        hole(Point3::new(0.0, 10.0, 0.0), Point3::new(0.0, -1.0, 0.0)),
        hole(Point3::new(-10.0, 0.0, -10.0), Point3::new(1.0, 0.0, 0.0)),
    ];
    cross_drill(&CrossParams {
        clearance_d: 5.0, r_d: 5.0, hop_d: None, bottom_d: None,
        cycle: CycleChoice::Auto, peck: 0.0, dwell: 0.0,
        feed: 120.0, plunge_feed: 80.0, stock_r, stock_z_max: Some(20.0),
        tool: &tool, holes: &holes,
    }).expect("cross drill")
}

#[test]
fn a_sub_nanometre_radius_residual_does_not_add_a_cross_drill_move() {
    let plain = chain(10.0);
    for radius in [10.0_f64.next_up(), 10.0 - 1e-10, 10.0 + 1e-10] {
        let noisy = chain(radius);
        assert_eq!(plain.moves.len(), noisy.moves.len(), "radius {radius:.17}: unchanged topology");
        for (a, b) in plain.moves.iter().zip(&noisy.moves) {
            assert_eq!((a.kind, a.feed, a.axis, a.arc), (b.kind, b.feed, b.axis, b.arc));
            for (p, q) in [(a.from, b.from), (a.to, b.to)] {
                let d = p - q;
                assert!(d.dot(d).sqrt() <= 1e-9, "radius {radius:.17}: {p:?} != {q:?}");
            }
        }
        assert!(noisy.is_chained() && noisy.all_finite());
    }
}

#[test]
fn a_real_clearance_move_and_the_initial_station_pose_are_kept() {
    let plain = chain(10.0);
    let wider = chain(10.0 + 1e-6);
    assert_eq!(wider.moves.len(), plain.moves.len() + 1, "1e-6 mm of real clearance is motion");
    let last = wider.moves.last().expect("final clearance");
    assert!((last.to.x + 15.000001).abs() <= 1e-12);
    let first = plain.moves.first().expect("initial station index");
    assert_eq!(first.from, first.to, "the initial move only sets the station pose");
    assert_eq!(first.axis, Some([1.0, 0.0, 0.0]), "a stationary index must still post");
}
