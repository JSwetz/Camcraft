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
