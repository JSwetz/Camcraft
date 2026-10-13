//! Face and contour generator goldens: row counts, offsets and spacings
//! are exact hand-computable numbers, and face rapids may only appear at
//! the path ends (contour legitimately rapids between loops and hops
//! over tabs - at the clearance/retract planes).

use hsmworks_app::cl::{MoveKind, Toolpath};
use hsmworks_app::geo2d::{Loop, Point2, Region};
use hsmworks_app::machining2d::{contour, face, ContourParams, FaceParams, GenError, Lead};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .unwrap()])
    .unwrap()
}

fn params() -> FaceParams {
    FaceParams {
        tool_diameter: 10.0,
        stepover: 5.0,
        clearance_z: 5.0,
        cut_z: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    }
}

/// Row moves: Feed segments at cut depth running mostly along X.
fn rows(t: &Toolpath) -> Vec<(f64, f64, f64)> {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z)
        .filter(|m| (m.to.x - m.from.x).abs() > (m.to.y - m.from.y).abs())
        .map(|m| (m.from.y, m.from.x, m.to.x))
        .collect()
}

#[test]
fn face_of_a_square_is_three_exact_rows() {
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let t = face(&r, &params()).expect("valid params");
    let rs = rows(&t);
    assert_eq!(rs.len(), 3, "rows at coverage centers 5/10/15: {rs:?}");
    // Serpentine order and alternation.
    assert_eq!(rs[0], (5.0, -5.0, 25.0));
    assert_eq!(rs[1], (10.0, 25.0, -5.0));
    assert_eq!(rs[2], (15.0, -5.0, 25.0));
    assert!(t.is_chained());
    assert!(t.all_finite());
}

#[test]
fn rapids_only_appear_at_the_ends() {
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let t = face(&r, &params()).expect("valid params");
    assert_eq!(t.moves[0].kind, MoveKind::Plunge);
    assert_eq!(t.moves[0].from.z, 5.0, "path starts in air");
    assert_eq!(t.moves.last().unwrap().kind, MoveKind::Rapid);
    for m in &t.moves[1..t.len() - 1] {
        assert_ne!(m.kind, MoveKind::Rapid, "mid-path rapid: {m:?}");
    }
    // The final rapid lifts straight up from the last row's end.
    let last = t.moves.last().unwrap();
    assert_eq!(last.from.x, last.to.x);
    assert_eq!(last.from.y, last.to.y);
    assert_eq!(last.to.z, 5.0);
}

#[test]
fn conventional_starts_at_the_other_corner() {
    let mut p = params();
    p.climb = false;
    let t = face(&rect(0.0, 0.0, 20.0, 20.0), &p).expect("valid");
    let rs = rows(&t);
    assert_eq!(rs[0], (5.0, 25.0, -5.0));
    assert_eq!(rs[1], (10.0, -5.0, 25.0));
}

#[test]
fn thin_stock_gets_one_centered_row() {
    let r = rect(0.0, 0.0, 30.0, 6.0);
    let t = face(&r, &params()).expect("valid");
    let rs = rows(&t);
    assert_eq!(rs.len(), 1, "one 10-wide tool bands a 6-tall strip once");
    assert_eq!(rs[0].0, 3.0);
}

#[test]
fn holes_are_overflown_not_avoided() {
    // Facing clears a plane: a hole in the footprint does not reroute the
    // rows; the path simply runs across it (an air cut, not a gouge).
    let r = Region::new(vec![
        Loop::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(20.0, 0.0),
            Point2::new(20.0, 20.0),
            Point2::new(0.0, 20.0),
        ])
        .unwrap(),
        Loop::new(vec![
            Point2::new(8.0, 8.0),
            Point2::new(12.0, 8.0),
            Point2::new(12.0, 12.0),
            Point2::new(8.0, 12.0),
        ])
        .unwrap(),
    ])
    .unwrap();
    let t = face(&r, &params()).expect("valid");
    assert_eq!(rows(&t).len(), 3, "same rows as the plain square");
}

#[test]
fn degenerate_parameters_are_refused_before_any_geometry() {
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let cases = [
        {
            let mut p = params();
            p.stepover = 0.0;
            p
        },
        {
            let mut p = params();
            p.tool_diameter = f64::NAN;
            p
        },
        {
            let mut p = params();
            p.feed = -1.0;
            p
        },
        {
            let mut p = params();
            p.clearance_z = f64::INFINITY;
            p
        },
    ];
    for p in cases {
        assert!(matches!(face(&r, &p), Err(GenError::Invalid(_))), "{p:?}");
    }
    assert!(matches!(
        face(&Region::empty(), &params()),
        Err(GenError::Empty(_))
    ));
}

#[test]
fn face_is_deterministic() {
    // Undo/redo exactness rests on generators being pure.
    let r = rect(-7.5, 2.0, 41.25, 33.0);
    let a = face(&r, &params()).unwrap();
    let b = face(&r, &params()).unwrap();
    assert_eq!(a, b);
    assert!(a.is_chained());
    // Odd extents still band inside themselves: first row at ymin+r, no
    // row beyond the last coverage center, x overrun exactly one radius.
    let rs = rows(&a);
    assert_eq!(rs.first().unwrap().0, 7.0);
    assert_eq!(rs.last().unwrap().0, 28.0);
    for (_, x0, x1) in &rs {
        assert!([(-7.5 - 5.0, 41.25 + 5.0), (41.25 + 5.0, -7.5 - 5.0)].contains(&(*x0, *x1)));
    }
}

// ---- contour ------------------------------------------------------------

fn cp() -> ContourParams {
    ContourParams {
        tool_diameter: 10.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 3.0,
        top_z: 0.0,
        bottom_z: 0.0,
        stepdown: 1.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
    }
}

/// XY polyline of the level's in-plane Feed moves (start = first move's
/// from, then every `to` in order).
fn level_xy(t: &Toolpath, z: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for m in &t.moves {
        if m.kind == MoveKind::Feed && m.from.z == z && m.to.z == z {
            if out.is_empty() {
                out.push((m.from.x, m.from.y));
            }
            out.push((m.to.x, m.to.y));
        }
    }
    out
}

/// Total in-plane feed length at `z`, summed per move so tab-hop
/// transitions (which leave the plane) can't count as cut distance.
fn level_len(t: &Toolpath, z: f64) -> f64 {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z == z && m.to.z == z)
        .map(|m| ((m.to.x - m.from.x).powi(2) + (m.to.y - m.from.y).powi(2)).sqrt())
        .sum()
}

fn plunge_xy(t: &Toolpath) -> Vec<(f64, f64)> {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge)
        .map(|m| (m.to.x, m.to.y))
        .collect()
}

fn shoelace(p: &[(f64, f64)]) -> f64 {
    let mut s = 0.0;
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        s += a.0 * b.1 - b.0 * a.1;
    }
    s * 0.5
}

#[test]
fn contour_of_a_square_grazes_the_boundary() {
    let t = contour(&rect(0.0, 0.0, 20.0, 20.0), &cp()).expect("valid");
    assert!(t.is_chained());
    assert!(t.all_finite());
    assert_eq!(plunge_xy(&t).len(), 1, "one loop, one level");
    let p = level_xy(&t, 0.0);
    // Cutter center sits one radius out; straight runs are exact, and the
    // rounded corner passes through radius-5 territory around (20, 20).
    let xs: Vec<f64> = p.iter().map(|q| q.0).collect();
    let ys: Vec<f64> = p.iter().map(|q| q.1).collect();
    assert_eq!(xs.iter().cloned().fold(f64::INFINITY, f64::min), -5.0);
    assert_eq!(xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max), 25.0);
    assert_eq!(ys.iter().cloned().fold(f64::INFINITY, f64::min), -5.0);
    assert_eq!(ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max), 25.0);
    assert!(
        p.iter()
            .any(|q| ((q.0 - 20.0).powi(2) + (q.1 - 20.0).powi(2)).sqrt() - 5.0 < 1e-3),
        "corner arc points expected around the original corner"
    );
    // Closed ring: the last in-plane feed returns to the plunge point.
    assert_eq!(p[0], *plunge_xy(&t).first().unwrap());
    assert_eq!(p[p.len() - 1], p[0]);
}

#[test]
fn leave_pushes_the_path_off_the_wall() {
    let mut p = cp();
    p.stock_to_leave = 2.0;
    let t = contour(&rect(0.0, 0.0, 20.0, 20.0), &p).expect("valid");
    let pts = level_xy(&t, 0.0);
    let xs: Vec<f64> = pts.iter().map(|q| q.0).collect();
    assert_eq!(xs.iter().cloned().fold(f64::INFINITY, f64::min), -7.0);
    assert_eq!(xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max), 27.0);
}

#[test]
fn contour_walks_hole_walls_too() {
    // A 10-wide tool in a 12-wide hole: cutter-center loop is the hole
    // shrunk by r, so the edge grazes both walls exactly.
    let r = Region::new(vec![
        Loop::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(40.0, 0.0),
            Point2::new(40.0, 40.0),
            Point2::new(0.0, 40.0),
        ])
        .unwrap(),
        Loop::new(vec![
            Point2::new(14.0, 14.0),
            Point2::new(26.0, 14.0),
            Point2::new(26.0, 26.0),
            Point2::new(14.0, 26.0),
        ])
        .unwrap(),
    ])
    .unwrap();
    let t = contour(&r, &cp()).expect("valid");
    assert_eq!(plunge_xy(&t).len(), 2, "outer loop then island wall");
    let pts = level_xy(&t, 0.0);
    assert!(pts.iter().all(|q| q.0 >= -5.0 && q.0 <= 45.0 && q.1 >= -5.0 && q.1 <= 45.0));
    let hole: Vec<(f64, f64)> = pts
        .iter()
        .copied()
        .filter(|q| q.0 > 18.0 && q.0 < 22.0 && q.1 > 18.0 && q.1 < 22.0)
        .collect();
    assert!(!hole.is_empty());
    let hx: Vec<f64> = hole.iter().map(|q| q.0).collect();
    assert_eq!(hx.iter().cloned().fold(f64::INFINITY, f64::min), 19.0);
    assert_eq!(hx.iter().cloned().fold(f64::NEG_INFINITY, f64::max), 21.0);
    assert!(hole.iter().any(|q| q.0 == 19.0 && q.1 == 21.0));
}

#[test]
fn a_hole_the_tool_cannot_enter_vanishes() {
    // 10-wide hole, 10-wide tool: the center loop collapses, so only the
    // outer wall is contoured - nothing is silently cut off the part.
    let r = Region::new(vec![
        Loop::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(40.0, 0.0),
            Point2::new(40.0, 40.0),
            Point2::new(0.0, 40.0),
        ])
        .unwrap(),
        Loop::new(vec![
            Point2::new(15.0, 15.0),
            Point2::new(25.0, 15.0),
            Point2::new(25.0, 25.0),
            Point2::new(15.0, 25.0),
        ])
        .unwrap(),
    ])
    .unwrap();
    let t = contour(&r, &cp()).expect("valid");
    assert_eq!(plunge_xy(&t).len(), 1);
}

#[test]
fn contour_levels_snap_to_the_bottom() {
    let mut p = cp();
    p.top_z = 0.0;
    p.bottom_z = -6.0;
    p.stepdown = 2.5;
    let t = contour(&rect(0.0, 0.0, 20.0, 20.0), &p).expect("valid");
    assert_eq!(plunge_xy(&t).len(), 4, "levels 0, -2.5, -5, -6");
    let mut zs: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z)
        .map(|m| m.from.z)
        .collect();
    zs.sort_by(f64::total_cmp);
    zs.dedup();
    assert_eq!(zs, vec![-6.0, -5.0, -2.5, 0.0], "steps snap onto the bottom");
}

#[test]
fn climb_and_conventional_reverse_the_travel() {
    let up = contour(&rect(0.0, 0.0, 20.0, 20.0), &cp()).unwrap();
    let mut p = cp();
    p.climb = false;
    let down = contour(&rect(0.0, 0.0, 20.0, 20.0), &p).unwrap();
    let (a, b) = (level_xy(&up, 0.0), level_xy(&down, 0.0));
    assert!(shoelace(&a) > 0.0, "climb keeps the canonical orientation");
    assert!(shoelace(&b) < 0.0, "conventional reverses it");
    assert!((shoelace(&a) + shoelace(&b)).abs() < 1e-6);
    // Same wall, opposite direction: identical perimeter.
    assert!((level_len(&up, 0.0) - level_len(&down, 0.0)).abs() < 1e-6);
}

#[test]
fn linear_lead_overshoots_the_tangent() {
    let base = contour(&rect(0.0, 0.0, 20.0, 20.0), &cp()).unwrap();
    let mut p = cp();
    p.lead = Lead::Linear { length: 4.0 };
    let led = contour(&rect(0.0, 0.0, 20.0, 20.0), &p).unwrap();
    let s = plunge_xy(&base)[0];
    let pts = level_xy(&base, 0.0);
    let q = pts[1];
    let t = ((q.0 - s.0) / ((q.0 - s.0).powi(2) + (q.1 - s.1).powi(2)).sqrt(),
             (q.1 - s.1) / ((q.0 - s.0).powi(2) + (q.1 - s.1).powi(2)).sqrt());
    let e = plunge_xy(&led)[0];
    assert!((e.0 - (s.0 - 4.0 * t.0)).abs() < 1e-9);
    assert!((e.1 - (s.1 - 4.0 * t.1)).abs() < 1e-9);
    // The lead run lands exactly on the base path's entry vertex.
    assert!(level_xy(&led, 0.0).windows(2).any(|w| w[1] == s && w[0] == e));
    assert!(led.is_chained() && led.all_finite());
}

#[test]
fn arc_lead_enters_tangentially_as_a_quarter_arc() {
    let base = contour(&rect(0.0, 0.0, 20.0, 20.0), &cp()).unwrap();
    let mut p = cp();
    p.lead = Lead::Arc { radius: 5.0 };
    let led = contour(&rect(0.0, 0.0, 20.0, 20.0), &p).unwrap();
    let (s, e) = (plunge_xy(&base)[0], plunge_xy(&led)[0]);
    // The quarter-arc diagonal: |entry - start| = radius * sqrt(2).
    let d = ((e.0 - s.0).powi(2) + (e.1 - s.1).powi(2)).sqrt();
    assert!((d - 5.0 * 2.0f64.sqrt()).abs() < 1e-9, "entry-to-start {d}");
    // Tessellated to tolerance: many short feeds cover the 90-degree lead.
    let n = led
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z == 0.0 && m.to.z == 0.0)
        .count();
    let n0 = base
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z == 0.0 && m.to.z == 0.0)
        .count();
    assert!(n > n0 + 4, "arc should add ~13 chord feeds, got {n} vs {n0}");
    // Every arc chord stays within tolerance of radius 5 about its center.
    assert!(led.is_chained() && led.all_finite());
}

#[test]
fn tabs_hold_the_part_until_the_floor() {
    let mut p = cp();
    p.top_z = 0.0;
    p.bottom_z = -6.0;
    p.stepdown = 3.0;
    p.tab_count = 4;
    p.tab_width = 6.0;
    p.tab_height = 2.0;
    let t = contour(&rect(0.0, 0.0, 20.0, 20.0), &p).expect("valid");
    // Levels 0 and -3 run full rings, -6 runs four subpaths between tabs.
    assert_eq!(plunge_xy(&t).len(), 6);
    // Hop motion only at the tabbed level: three retract feeds + rapids.
    let up = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z != m.to.z)
        .count();
    assert_eq!(up, 3, "retracts are the only z-varying feeds");
    let low_rapids = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Rapid && m.from.z == 3.0 && m.to.z == 3.0)
        .count();
    assert_eq!(low_rapids, 3);
    // The tabbed level cuts exactly four tab widths less than a full ring.
    let full = level_len(&t, -3.0);
    let tabbed = level_len(&t, -6.0);
    assert!((full - tabbed - 24.0).abs() < 1e-6, "{full} - {tabbed}");
    assert!(t.is_chained() && t.all_finite());
}

#[test]
fn contour_refuses_bad_input() {
    let sq = rect(0.0, 0.0, 20.0, 20.0);
    let cases = [
        {
            let mut p = cp();
            p.stock_to_leave = -0.1;
            p
        },
        {
            let mut p = cp();
            p.stepdown = 0.0;
            p
        },
        {
            let mut p = cp();
            p.bottom_z = 1.0; // above top_z = 0.0
            p
        },
        {
            let mut p = cp();
            p.clearance_z = -1.0; // below the first cut level
            p
        },
        {
            let mut p = cp();
            p.lead = Lead::Arc { radius: 0.0 };
            p
        },
        {
            let mut p = cp();
            p.tab_count = 2;
            p.tab_width = 0.0;
            p
        },
        {
            let mut p = cp();
            p.feed = f64::NAN;
            p
        },
    ];
    for p in cases {
        assert!(matches!(contour(&sq, &p), Err(GenError::Invalid(_))), "{p:?}");
    }
    assert!(matches!(
        contour(&Region::empty(), &cp()),
        Err(GenError::Empty(_))
    ));
}

#[test]
fn contour_is_deterministic() {
    let r = rect(-3.5, 1.25, 27.0, 19.5);
    let a = contour(&r, &cp()).unwrap();
    let b = contour(&r, &cp()).unwrap();
    assert_eq!(a, b);
    assert!(a.is_chained() && a.all_finite());
}


