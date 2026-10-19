//! Arc moves end to end: the fitter recovers circles from fine
//! tessellation and leaves coarse polygons alone, leads post as single
//! arc moves, chords stay inside the sagitta law's word, and paths with
//! arcs still chain and stay finite.

use hsmworks_app::cl::{Move, MoveKind};
use hsmworks_app::geo2d::{Loop, Point2, Region};
use hsmworks_app::geometry::Point3;
use hsmworks_app::machining2d::{ContourParams, Lead, Seg, Side, contour, fit_arcs};

fn p2(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![
        p2(x0, y0),
        p2(x1, y0),
        p2(x1, y1),
        p2(x0, y1),
    ])
    .unwrap()])
    .unwrap()
}

/// A ccw circle closed by repeating its first point - the shape a rim
/// walk carries, which is what the fitter sees.
fn ring(c: Point2, r: f64, n: usize) -> Vec<Point2> {
    (0..=n)
        .map(|k| {
            let t = std::f64::consts::TAU * k as f64 / n as f64;
            p2(c.x + r * t.cos(), c.y + r * t.sin())
        })
        .collect()
}

fn contour_params(lead: Lead) -> ContourParams {
    ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -2.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Auto,
        lead,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
    }
}

#[test]
fn fitted_arcs_recover_a_bore_rim() {
    // A 128-gon on a 12 mm radius sits within 0.002 mm of its circle:
    // the polygonal rim is a tessellation artifact, and the fitter must
    // answer with arcs and nothing else.
    let segs = fit_arcs(&ring(p2(0.0, 0.0), 12.0, 128), 0.01);
    let arcs = segs.iter().filter(|s| matches!(s, Seg::Arc { .. })).count();
    let stray = segs
        .iter()
        .filter(|s| matches!(s, Seg::Line(v) if v.len() > 1))
        .count();
    assert!((1..=4).contains(&arcs), "a fine closed rim is a couple of arcs, got {segs:?}");
    assert_eq!(stray, 0, "no multi-point Line runs survive a fitted ring");

    // A 16-gon on the same radius: 0.23 mm off its circle at tolerance
    // 0.01 - the fitter invents geometry that is not there never.
    let coarse = fit_arcs(&ring(p2(0.0, 0.0), 12.0, 16), 0.01);
    assert!(
        coarse.iter().all(|s| matches!(s, Seg::Line(_))),
        "a coarse polygon stays the honest chord walk it always was: {coarse:?}"
    );
}

#[test]
fn arc_lead_emits_one_arc_move() {
    // The square's offset path is straight runs plus the round-join
    // corners - and the corners ARE circles, tessellated: the fitter
    // recovers them (one arc per corner per level) and invents nothing
    // for the straights. So the only arcs Lead::Arc adds beyond that
    // baseline are the leads themselves, one move each.
    let r = rect(0.0, 0.0, 40.0, 20.0);
    let plain = contour(&r, &contour_params(Lead::None)).unwrap();
    assert_eq!(
        plain.arc_count(),
        8,
        "four r = 3 corner rounds, two levels - recovered, not invented"
    );
    let led = contour(&r, &contour_params(Lead::Arc { radius: 3.0 })).unwrap();
    assert!(led.arc_count() >= 2, "each pass lands and leaves on an arc");
    for m in led.moves.iter().filter(|m| m.arc.is_some()) {
        assert_eq!(m.kind, MoveKind::Feed, "an arc is always feed motion");
        // The helical law (M2.1) belongs to the roughing entries; the
        // 2.5-D generators' arcs - leads, rings, corners - stay planar.
        assert_eq!(m.from.z, m.to.z, "a 2.5-D generator's arc is planar");
        let c = m.arc.unwrap();
        let ra = ((m.from.x - c.center.x).powi(2) + (m.from.y - c.center.y).powi(2)).sqrt();
        let rb = ((m.to.x - c.center.x).powi(2) + (m.to.y - c.center.y).powi(2)).sqrt();
        // The center stands on the 1 um lattice (cl.rs::ArcMove), the
        // endpoints are exact, so "on one circle" holds to the lattice
        // chord-ratio, not to bit equality: <= 1e-6 mm apart at worst.
        assert!((ra - rb).abs() <= 1e-5, "arc endpoints must share their circle: {ra} vs {rb}");
    }
}

#[test]
fn tangential_leads_post_as_half_sweeps() {
    // Same baseline as the arc lead test: 8 recovered corner arcs for
    // the two levels. A tangential lead adds exactly one posted arc
    // move per entry and exit - and each of those is a half circle,
    // endpoints antipodal about the center (which no corner round
    // ever is), so the sweep law is counted, not assumed.
    let r = rect(0.0, 0.0, 40.0, 20.0);
    let led = contour(&r, &contour_params(Lead::Tangential { radius: 3.0 })).unwrap();
    assert_eq!(led.arc_count(), 8 + 4, "four lead joins, two levels");
    let halves = led.moves.iter().filter(|m| {
        m.arc.is_some_and(|c| {
            (m.from.x + m.to.x - 2.0 * c.center.x).abs() < 1e-6
                && (m.from.y + m.to.y - 2.0 * c.center.y).abs() < 1e-6
        })
    }).count();
    assert_eq!(halves, 4, "the joins are the only antipodal arcs");
    assert!(led.is_chained() && led.all_finite());
}

#[test]
fn arc_moves_chain_end_to_end() {
    // Plate with a round bore, arcs from both leads and the fitted rim:
    // the chain invariant is exact, not approximate - every arc's `to`
    // is the next move's `from`, vertex for vertex.
    let mut loops = rect(0.0, 0.0, 40.0, 20.0).loops().to_vec();
    let pts: Vec<Point2> = ring(p2(20.0, 10.0), 6.0, 128)[..128].to_vec();
    loops.push(Loop::new(pts).unwrap());
    let region = Region::new(loops).unwrap();
    let t = contour(&region, &contour_params(Lead::Arc { radius: 3.0 })).unwrap();
    assert!(t.has_arcs());
    assert!(t.is_chained(), "move.to must equal the next move.from exactly");
    assert!(t.all_finite());
}

#[test]
fn arc_chords_stay_inside_gouge_tolerance() {
    // The sagitta law is a promise, so measure it being kept: a quarter
    // of r = 25 flattened at 0.01 - chord midpoints fall at most that
    // far inside the circle, endpoints are exact, and the chords chain.
    let m = Move::new(MoveKind::Feed, Point3::new(0.0, 25.0, -1.0), Point3::new(25.0, 0.0, -1.0), Some(500.0))
        .with_arc(p2(0.0, 0.0), false);
    let cs = m.chords(0.01);
    assert!(cs.len() >= 4, "a quarter arc at this tolerance fans into many chords");
    assert_eq!(cs.first().unwrap().0, p2(0.0, 25.0));
    assert_eq!(cs.last().unwrap().1, p2(25.0, 0.0));
    assert!(cs.windows(2).all(|w| w[0].1 == w[1].0), "chords chain");
    for (a, b) in &cs {
        for p in [a, b] {
            let r = (p.x * p.x + p.y * p.y).sqrt();
            assert!((r - 25.0).abs() <= 1e-9, "endpoints ride the circle: {r}");
        }
        let (mx, my) = ((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        let rad = (mx * mx + my * my).sqrt();
        assert!(25.0 - rad <= 0.01 + 1e-9, "sagitta past tolerance: {}", 25.0 - rad);
    }
    // Degenerate asks stay honest: a zero or NAN tolerance falls back to
    // the law's default, never a hang, never an empty result.
    assert_eq!(m.chords(0.0).len(), cs.len(), "tol 0 uses the default, not infinity");
    assert_eq!(m.chords(f64::NAN).len(), cs.len(), "NAN tol likewise");
}
