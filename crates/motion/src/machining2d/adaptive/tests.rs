//! The planner's own laws (Gate map (M11.4 C): covers
//! motion::machining2d::adaptive).

use super::*;

#[test]
fn the_planner_stock_reads_a_side_cut_as_the_yardstick_does() {
    // A channel, then a parallel pass 2 mm beside it: the planner's
    // stock reads the angle the measuring stock's law gives a straight
    // wall at that width on the same rim (`quality::rim_for`).
    let Ok(sq) = Region::square(60.0) else { panic!("square") };
    let Ok(mut ras) = Raster::from_region(&sq, 0.1, 0.0) else { panic!("ras") };
    let rim = crate::quality::rim_for(10.0, 0.1);
    ras.walk(&[Point2::new(5.0, 30.0), Point2::new(55.0, 30.0)], 5.0, rim, 0.625);
    let (deg, at) = ras.walk(&[Point2::new(5.0, 32.0), Point2::new(40.0, 32.0)], 5.0, rim, 0.625);
    let want = crate::load::deg_at_rim(0.2, 10.0, rim);
    assert!((deg - want).abs() < 3.0, "a 2 mm side cut reads {deg} at {at:?}, want {want}");
}

#[test]
fn a_degenerate_spec_is_refused_not_planned() {
    let Ok(sq) = Region::square(30.0) else { panic!("square") };
    let spec = AdaptiveSpec {
        r: 0.0,
        load: 0.1,
        tol: 0.0,
        climb: true,
        seed: Point2::new(15.0, 15.0),
        start: Point2::new(15.0, 15.0),
        bore: 0.0,
        helix_r: 0.0,
        smooth: 1.0,
    };
    assert!(plan(&sq, &sq, &spec).is_err());
}

/// Round 3: a 60 x 14 slot (1.4 D wide) is no star cell from its middle; it
/// is grown as one ring family - never crossing itself, the load held,
/// nothing but the named corner scallops left. Round 4: cut where it
/// bites, its pieces joined by microlift returns (one join a pass).
#[test]
fn a_channel_is_grown_as_one_spiral_that_never_crosses() {
    let pts = vec![Point2::new(0.0, 0.0), Point2::new(60.0, 0.0), Point2::new(60.0, 14.0), Point2::new(0.0, 14.0)];
    let (Ok(l), r) = (crate::geo2d::Loop::new(pts), 5.0) else { panic!("loop") };
    let Ok(slot) = Region::new(vec![l]) else { panic!("slot") };
    // As `pocket_hsm` plans it: the center area, and what the cutter
    // standing in it can reach.
    let (Ok(center), r) = (slot.offset(-r), r) else { panic!("center") };
    let Ok(reach) = center.offset(r) else { panic!("reach") };
    let seed = Point2::new(30.0, 7.0);
    let spec = AdaptiveSpec {
        r,
        load: crate::load::DEFAULT_OPTIMAL_LOAD,
        tol: 0.05,
        climb: true,
        seed,
        start: Point2::new(30.0 - 1.2, 7.0),
        bore: r + 1.2,
        helix_r: 1.2,
        smooth: 2.5,
    };
    let Ok(pl) = plan(&reach, &center, &spec) else { panic!("plan") };
    assert!(pl.passes.len() > 1 && pl.joins.len() == pl.passes.len(), "lifted pieces: {:?}", pl.notes);
    assert!(pl.joins.iter().skip(1).filter(|j| j.is_some()).count() * 10 >= 9 * (pl.passes.len() - 1), "returns lifted: {:?}", pl.notes);
    assert!(first_crossing(&pl.passes).is_none(), "{:?}", pl.notes);
    assert!(!pl.notes.iter().any(|n| n.contains("exceeds") || n.contains("overload")), "{:?}", pl.notes);
    assert!(pl.rest < crate::machining2d::pocket_hsm::HSM_REST_MAX * reach.area(), "rest {} {:?}", pl.rest, pl.notes);
    // Every point stays in the center area (the walls are never cut).
    let Ok(grown) = center.offset(1e-3) else { panic!("grown") };
    assert!(pl.passes.iter().flatten().all(|&q| grown.contains(q)));
}
