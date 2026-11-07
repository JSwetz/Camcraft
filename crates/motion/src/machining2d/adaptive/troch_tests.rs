//! The trochoidal fallback's plan, pinned (issue #5 round 2; Gate map
//! (M11.4 C): covers motion::machining2d::adaptive::troch_plan).

use super::*;
use crate::geo2d::Loop;

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    let pts = vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)];
    let Ok(l) = Loop::new(pts) else { panic!("loop") };
    l
}

/// The plan as `pocket_troch` asks it of a pocket's center area.
fn troch(center: &Region, landing: bool) -> (AdaptivePlan, Region) {
    let r = 5.0;
    let Ok(Ok((seed, hr))) = troch_plan::seed(center, 2.0) else { panic!("seed") };
    let Ok(reach) = center.offset(r) else { panic!("reach") };
    let spec = AdaptiveSpec {
        r,
        load: crate::load::DEFAULT_OPTIMAL_LOAD,
        tol: 0.05,
        climb: true,
        seed,
        start: Point2::new(seed.x - hr, seed.y),
        bore: r + hr,
        helix_r: hr,
        smooth: 2.5,
    };
    let Ok(pl) = troch_plan::plan_with(&reach, center, &spec, landing) else { panic!("plan") };
    (pl, reach)
}

/// The 22 x 60 channel (adaptive_hsm's "channel 22x60", a D10): one
/// opening at the medial line's start, then front arcs each reached by
/// a microlift return, the wall laps last - no crossing, every point in
/// the center area, the load held (the plan refuses past it), the rest
/// only the named corner scallop. The landing law relaxed: it is what
/// is owed (`a_landing_on_a_loop_start_in_cut_is_refused_by_name`).
#[test]
fn the_channel_trochoid_lifts_every_return_never_crosses_and_holds_the_load() {
    let Ok(center) = Region::new(vec![rect(5.0, 5.0, 55.0, 17.0)]) else { panic!("center") };
    let (pl, reach) = troch(&center, false);
    assert!(pl.refused.is_none(), "{:?} {:?}", pl.refused, pl.notes);
    assert!(pl.passes.len() > 20 && pl.passes.len() <= troch_plan::LOOP_CAP + 1, "{} passes", pl.passes.len());
    assert!(pl.joins.len() == pl.passes.len() && pl.joins[0].is_none());
    assert!(pl.joins.iter().skip(1).all(|j| j.as_ref().is_some_and(|r| r.lift >= lift::MICROLIFT && !r.high)), "every return lifted");
    assert_eq!(pl.travel.hops, 0, "no retract hop");
    assert!(first_crossing(&pl.passes).is_none(), "{:?}", pl.notes);
    let Ok(grown) = center.offset(1e-3) else { panic!("grown") };
    assert!(pl.passes.iter().flatten().all(|&q| grown.contains(q)), "never leaves the center area");
    assert!(pl.notes.iter().any(|n| n.contains("trochoidal loops")), "{:?}", pl.notes);
    assert!(pl.notes.iter().any(|n| n.contains("corner scallop")), "{:?}", pl.notes);
    assert!(pl.rest < crate::machining2d::pocket_hsm::HSM_REST_MAX * reach.area(), "rest {} {:?}", pl.rest, pl.notes);
}

/// What keeps the trochoid off (the one-descent law): a lifted return
/// lands on the next loop's start, already in cut - refused by name, so
/// the pocket says why and the Offset rings cut instead. The 14 mm band
/// (#13) is refused by name before that (no room to open out a 1.9 mm
/// loop behind a helix; past it, 2575 loops over the loop cap).
#[test]
fn a_landing_on_a_loop_start_in_cut_is_refused_by_name() {
    let Ok(center) = Region::new(vec![rect(5.0, 5.0, 55.0, 17.0)]) else { panic!("center") };
    let (pl, _) = troch(&center, true);
    assert!(pl.refused.as_deref().is_some_and(|w| w.contains("lands on a loop start")), "{:?}", pl.refused);
    assert!(pl.passes.is_empty());
    let Ok(part) = Region::new(vec![rect(0.0, 0.0, 60.0, 50.0)]) else { panic!("part") };
    let (Ok(o), Ok(i)) = (part.offset(9.0), part.offset(5.0)) else { panic!("offsets") };
    let Ok(band) = o.difference(&i) else { panic!("band") };
    let (pl, _) = troch(&band, true);
    assert!(pl.refused.is_some() && pl.passes.is_empty(), "the band is refused by name: {:?}", pl.notes);
}
