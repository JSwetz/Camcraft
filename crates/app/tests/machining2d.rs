//! Face generator goldens: serpentine row counts and spacings are exact
//! hand-computable numbers, and rapids may only appear at the path ends.

use hsmworks_app::cl::MoveKind;
use hsmworks_app::geo2d::{Loop, Point2, Region};
use hsmworks_app::machining2d::{face, FaceParams, GenError};

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
fn rows(t: &hsmworks_app::cl::Toolpath) -> Vec<(f64, f64, f64)> {
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
