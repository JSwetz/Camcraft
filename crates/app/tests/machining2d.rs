//! Face, contour, pocket, slot and trace generator goldens: row counts,
//! offsets and spacings are exact hand-computable numbers, and rapids may
//! only appear where the strategy says (face: at the path ends; contour:
//! between loops and at the hop planes; pocket/slot/trace: only at the
//! clearance plane).

use hsmworks_app::cl::{MoveKind, Toolpath};
use hsmworks_app::geo2d::{Loop, Point2, Region};
use hsmworks_app::machining2d::{
    chord_clears, contour, face, on_boundary, pocket, slot, trace, ContourParams, FaceParams,
    GenError, Lead, PocketParams, PocketStrategy, SlotParams, TraceParams,
};

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
        top_z: None,
        stepdown: None,
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
fn face_default_stays_single_level() {
    // The byte-shield of `face_square_40x40.cl`: with no staging the face
    // cuts exactly one level at `cut_z`. Naming the same height explicitly
    // (top == cut, one step) must produce the identical path - staging is
    // opt-in geometry, never a re-phrasing that moves the moves.
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let legacy = face(&r, &params()).expect("valid params");
    let mut p = params();
    p.top_z = Some(p.cut_z);
    p.stepdown = Some(1.0);
    let explicit = face(&r, &p).expect("single-level staging is valid");
    assert_eq!(legacy, explicit, "top_z == cut_z must not change the path");
    assert_eq!(
        explicit.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count(),
        1
    );
}

#[test]
fn face_steps_down_through_levels() {
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let mut p = params(); // cut_z 2.0, clearance 5.0
    p.top_z = Some(4.0);
    p.stepdown = Some(1.0);
    let t = face(&r, &p).expect("valid params");
    assert!(t.is_chained());
    assert!(t.all_finite());
    let zs: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge)
        .map(|m| m.to.z)
        .collect();
    assert_eq!(zs, [4.0, 3.0, 2.0], "three staged levels");
    // Same rows at every level: the band law is computed once and the
    // single-level sweep count repeats per staged level (3 rows, 20x20).
    for &z in &zs {
        let sweeps = t
            .moves
            .iter()
            .filter(|m| {
                m.kind == MoveKind::Feed
                    && m.from.z == z
                    && m.to.z == z
                    && (m.to.x - m.from.x).abs() > (m.to.y - m.from.y).abs()
            })
            .count();
        assert_eq!(sweeps, 3, "level {z} must repeat every row");
    }
    // Between levels the tool lifts to clearance and re-enters from the
    // same corner it plunges at: exactly two rapids before each extra
    // level's plunge, and nothing but rapids leave the level plane.
    let rapids = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Rapid)
        .count();
    assert_eq!(rapids, 2 * (zs.len() - 1) + 1, "hop rapids plus the final lift");
    for (i, m) in t.moves.iter().enumerate() {
        if m.kind == MoveKind::Plunge && i > 0 {
            assert_eq!(t.moves[i - 1].kind, MoveKind::Rapid);
            assert_eq!(m.from.z, 5.0, "plunge must start in air");
            assert_eq!(m.from.x, t.moves[i - 1].to.x);
            assert_eq!(m.from.y, t.moves[i - 1].to.y);
        }
    }
}

#[test]
fn face_levels_snap_to_bottom_exactly() {
    // A step that would cut below the finished height is replaced by the
    // finished height itself - never skipped, never overshot.
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let mut p = params(); // cut_z 2.0, clearance 5.0
    p.top_z = Some(4.5);
    p.stepdown = Some(1.5); // would next cut at 1.5, below the finish
    let t = face(&r, &p).expect("valid params");
    let zs: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge)
        .map(|m| m.to.z)
        .collect();
    assert_eq!(zs, [4.5, 3.0, 2.0], "last level lands on cut_z, not below");

    let mut p = params();
    p.top_z = Some(4.5);
    p.stepdown = Some(2.0);
    let t = face(&r, &p).expect("valid params");
    let zs: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge)
        .map(|m| m.to.z)
        .collect();
    assert_eq!(zs, [4.5, 2.5, 2.0], "odd arithmetic still snaps exactly");
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
        {
            // Staging fields refuse the same way scalars do: a named
            // error before any geometry runs, never a silent one-level cut.
            let mut p = params();
            p.top_z = Some(1.0); // params cut_z is 2.0: top below the finish
            p.stepdown = Some(1.0);
            p
        },
        {
            let mut p = params();
            p.top_z = Some(4.0); // staged descent with no stepdown: the
            p.stepdown = None; //  while-loop of cut_levels must never spin
            p
        },
        {
            let mut p = params();
            p.top_z = Some(4.0);
            p.stepdown = Some(0.0);
            p
        },
        {
            let mut p = params();
            p.top_z = Some(6.0); // first level above the clearance plane
            p.stepdown = Some(1.0);
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

/// Chord budget for flattening arc moves into polylines: the same sagitta
/// allowance the fitter promised, so the sampled polyline never lies more
/// than tol off the path that will actually be cut.
const FLATTEN_TOL: f64 = 0.01;

/// XY polyline of the level's in-plane Feed moves (start = first move's
/// from, then every chord end in order; arcs flattened at FLATTEN_TOL).
fn level_xy(t: &Toolpath, z: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for m in &t.moves {
        if m.kind == MoveKind::Feed && m.from.z == z && m.to.z == z {
            if out.is_empty() {
                out.push((m.from.x, m.from.y));
            }
            for (_, b) in m.chords(FLATTEN_TOL) {
                out.push((b.x, b.y));
            }
        }
    }
    out
}

/// Total in-plane feed length at `z`, summed per move so tab-hop
/// transitions (which leave the plane) can't count as cut distance.
/// Arc moves carry their true circle length - the metal the rim actually
/// walks - not the chord across them.
fn level_len(t: &Toolpath, z: f64) -> f64 {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z == z && m.to.z == z)
        .map(|m| match m.arc {
            None => ((m.to.x - m.from.x).powi(2) + (m.to.y - m.from.y).powi(2)).sqrt(),
            Some(a) => {
                let (rx, ry) = (m.from.x - a.center.x, m.from.y - a.center.y);
                let (ux, uy) = (m.to.x - a.center.x, m.to.y - a.center.y);
                let r = (rx * rx + ry * ry).sqrt();
                let sweep = if a.ccw {
                    (uy.atan2(ux) - ry.atan2(rx)).rem_euclid(std::f64::consts::TAU)
                } else {
                    (ry.atan2(rx) - uy.atan2(ux)).rem_euclid(std::f64::consts::TAU)
                };
                r * sweep
            }
        })
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
    // The chorded polygons agree to the flattening's own word: chord
    // endpoints ride the fitted circle measured from each run's start,
    // so the mirror images differ by sub-step chords, not by geometry.
    assert!((shoelace(&a) + shoelace(&b)).abs() < 1e-3);
    // Same wall, opposite direction: the same perimeter. The fitter
    // walks each ring from its own end, so the seam corner splits
    // differently in the two runs - one direction fits it to arcs, the
    // other leaves a chorded remainder, which measures short by the
    // sagittas it actually travels. Both are honest descriptions of
    // the identical rounded square, so the meaningful law is each
    // path against the wall's true perimeter: straights plus full
    // quarter circles, r = 5 at each corner.
    let true_perimeter = 80.0 + std::f64::consts::TAU * 5.0;
    for l in [level_len(&up, 0.0), level_len(&down, 0.0)] {
        assert!((l - true_perimeter).abs() < 2e-3, "{l} vs wall {true_perimeter}");
    }
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
    let tl = ((q.0 - s.0) / ((q.0 - s.0).powi(2) + (q.1 - s.1).powi(2)).sqrt(),
              (q.1 - s.1) / ((q.0 - s.0).powi(2) + (q.1 - s.1).powi(2)).sqrt());
    let e = plunge_xy(&led)[0];
    let (ex, ey) = (e.0 - s.0, e.1 - s.1);
    let d = (ex * ex + ey * ey).sqrt();
    // Overshoot is exactly the lead's length behind the rim start, and
    // it lies along the wall's own tangent there - measured to the next
    // sampled wall vertex, so a chord-vs-tangent half-step, not a
    // right-angle slam into the wall, is all that may show.
    assert!((d - 4.0).abs() < 1e-9, "overshoot distance {d}");
    let cos = -(ex * tl.0 + ey * tl.1) / d;
    assert!(cos > 0.998, "lead leaves the wall at {cos} rad-equivalent");
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
    // The lead is one arc move landing exactly at the rim's start - not
    // thirteen chords. Two passes per loop (in and out), and nothing
    // else about the rim changed: its arcs are the same fitted circles
    // the Lead::None path carries.
    let arcs = |t: &Toolpath| t.moves.iter().filter(|m| m.arc.is_some()).count();
    assert_eq!(arcs(&led), arcs(&base) + 2, "one arc lands, one arc leaves");
    let land = led
        .moves
        .iter()
        .find(|m| m.arc.is_some() && m.to.x == s.0 && m.to.y == s.1)
        .expect("the landing arc hands off at the rim start, vertex-exact");
    // Tangent continuity: at the join the lead circle and the wall's
    // first arc share their tangent line - centers and contact point
    // collinear, which (radii 5 + 5) means |o - c| = 10. To the
    // fitter's own word, though: the wall's circle is fitted to the
    // tessellated rim, so the contact point sits on the rim's vertex,
    // a residual off the fitted circle - tangency inherits the fit.
    // When the wall starts on a straight, the join is tangent by
    // construction and there is no rim center to test.
    if let Some(wall) = base.moves.iter().find(|m| m.arc.is_some() && m.from.x == s.0 && m.from.y == s.1) {
        let (oc, wc) = (land.arc.unwrap().center, wall.arc.unwrap().center);
        let cd = ((oc.x - wc.x).powi(2) + (oc.y - wc.y).powi(2)).sqrt();
        assert!((cd - 10.0).abs() < 1e-4, "lead and wall circles tangent at s: {cd}");
    }
    // Every arc move is a planar feed; chords chain and stay finite.
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
    // The tabbed level cuts four tab widths less than a full ring.
    // Tab windows are placed along the ring's polyline parameter, so a
    // window boundary that falls inside a fitted corner arc removes an
    // arc length off 6.0 by the chord-to-arc drift of one tessellation
    // step - sub-micron, and the four boundaries share the bias.
    let full = level_len(&t, -3.0);
    let tabbed = level_len(&t, -6.0);
    assert!((full - tabbed - 24.0).abs() < 1e-3, "{full} - {tabbed}");
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

// ---- pocket -------------------------------------------------------------

fn pp() -> PocketParams {
    PocketParams {
        tool_diameter: 10.0,
        stepover: 5.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        top_z: 0.0,
        bottom_z: -6.0,
        stepdown: 3.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
    }
}

fn in_closed(r: &Region, q: (f64, f64)) -> bool {
    let p = Point2::new(q.0, q.1);
    r.contains(p) || on_boundary(r, p)
}

/// The level's cutter-center polyline in path order, with the flag saying
/// whether each point was reached by a plunge (a subpath start after a
/// hop) rather than by feeding from the previous point. Area and engrave
/// paths only (pocket, slot, trace): the one non-feed move at cut depth
/// they make is an entry plunge.
fn polyline_at(t: &Toolpath, z: f64) -> (Vec<(f64, f64)>, Vec<bool>) {
    let mut pts = Vec::new();
    let mut lifted = Vec::new();
    for m in &t.moves {
        match m.kind {
            MoveKind::Feed if m.from.z == z && m.to.z == z => {
                if pts.is_empty() {
                    pts.push((m.from.x, m.from.y));
                    lifted.push(true);
                }
                // An arc is walked as the chords the machine can read,
                // never as the long chord across it.
                for (_, b) in m.chords(FLATTEN_TOL) {
                    pts.push((b.x, b.y));
                    lifted.push(false);
                }
            }
            MoveKind::Plunge if m.to.z == z => {
                pts.push((m.to.x, m.to.y));
                lifted.push(true);
            }
            _ => {}
        }
    }
    (pts, lifted)
}

fn levels_3_zs() -> [f64; 3] {
    [0.0, -3.0, -6.0]
}

#[test]
fn zigzag_fills_a_square_with_one_plunge_per_level() {
    let region = rect(0.0, 0.0, 30.0, 30.0);
    let center = region.offset(-5.0).expect("offset");
    let t = pocket(&region, &pp()).expect("valid");
    assert!(t.is_chained() && t.all_finite());
    assert_eq!(plunge_xy(&t).len(), 3, "one entry per level, rows stay engaged");

    let pts = level_xy(&t, -6.0);
    let ys: Vec<f64> = {
        let mut v: Vec<f64> = pts.iter().map(|q| q.1).collect();
        v.sort_by(f64::total_cmp);
        v.dedup();
        v
    };
    assert_eq!(ys, vec![5.0, 10.0, 15.0, 20.0, 25.0]);
    for q in &pts {
        assert!(in_closed(&center, *q), "feed point {q:?} outside the center area");
    }
    // Five 20 mm rows plus four 5 mm connectors, all at cut depth.
    assert!((level_len(&t, -6.0) - 120.0).abs() < 1e-9);
}

#[test]
fn zigzag_hops_over_islands_and_leaves_them_standing() {
    let region = Region::new(vec![
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
    // Where the tool center may not go because of the island (rounded
    // corners): `center` is the exact grown area, so `in_closed` below is
    // the gouge check; the distance test is a loose physical cross-check.
    let center = region.offset(-5.0).expect("offset");
    let t = pocket(&region, &pp()).expect("valid");
    // Four subpaths per level: one entry plus three island-crossing hops.
    assert_eq!(plunge_xy(&t).len(), 12);

    for z in levels_3_zs() {
        let (pts, lifted) = polyline_at(&t, z);
        for q in &pts {
            assert!(in_closed(&center, *q), "point {q:?} outside the center area");
            // Distance from the point to the island rect [15,25]^2.
            let dx = (15.0 - q.0).max(q.0 - 25.0).max(0.0);
            let dy = (15.0 - q.1).max(q.1 - 25.0).max(0.0);
            assert!((dx * dx + dy * dy).sqrt() > 5.0 - 1e-3, "point {q:?} in the island");
        }
        // Consecutive centers never cross the island while feeding: every
        // chord either provably clears the center area or is a subpath
        // start the tool plunged into.
        for i in 1..pts.len() {
            let a = Point2::new(pts[i - 1].0, pts[i - 1].1);
            let b = Point2::new(pts[i].0, pts[i].1);
            if !chord_clears(&center, a, b) {
                assert!(lifted[i], "gouging feed chord {:?} -> {:?}", pts[i - 1], pts[i]);
            }
        }
    }
}

#[test]
fn pocket_stock_to_leave_insets_the_whole_path() {
    let mut p = pp();
    p.stock_to_leave = 2.0;
    let region = rect(0.0, 0.0, 30.0, 30.0);
    let t = pocket(&region, &p).expect("valid");
    let pts = level_xy(&t, -6.0);
    let min = |v: &[f64]| v.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = |v: &[f64]| v.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let xs: Vec<f64> = pts.iter().map(|q| q.0).collect();
    let ys: Vec<f64> = pts.iter().map(|q| q.1).collect();
    assert_eq!((min(&xs), max(&xs), min(&ys), max(&ys)), (7.0, 23.0, 7.0, 23.0));
}

#[test]
fn waterline_cuts_concentric_rings_until_collapse() {
    let mut p = pp();
    p.strategy = PocketStrategy::Waterline;
    p.tool_diameter = 8.0;
    p.stepover = 2.0;
    p.top_z = 0.0;
    p.bottom_z = 0.0;
    let region = rect(0.0, 0.0, 30.0, 30.0);
    let center = region.offset(-4.0).expect("offset");
    let t = pocket(&region, &p).expect("valid");
    assert!(t.is_chained() && t.all_finite());
    let (pts, lifted) = polyline_at(&t, 0.0);
    for q in &pts {
        assert!(in_closed(&center, *q), "ring point {q:?} outside the center area");
    }
    for i in 1..pts.len() {
        let a = Point2::new(pts[i - 1].0, pts[i - 1].1);
        let b = Point2::new(pts[i].0, pts[i].1);
        if !chord_clears(&center, a, b) {
            assert!(lifted[i], "gouging ring chord {:?} -> {:?}", pts[i - 1], pts[i]);
        }
    }
    // Ring k is [4+2k, 26-2k]^2 for k = 0..5; k = 6 is empty.
    let min = |v: &[f64]| v.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = |v: &[f64]| v.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let xs: Vec<f64> = pts.iter().map(|q| q.0).collect();
    let ys: Vec<f64> = pts.iter().map(|q| q.1).collect();
    assert_eq!((min(&xs), max(&xs), min(&ys), max(&ys)), (4.0, 26.0, 4.0, 26.0));
    // The innermost ring: the 2 mm box [14,16]^2 was reached and closed.
    assert!(pts.iter().any(|q| (q.0 - 14.0).abs() < 1e-9 && (q.1 - 16.0).abs() < 1e-9));
    let n_rings = plunge_xy(&t).len();
    assert!((1..=6).contains(&n_rings), "rings: {n_rings}");
}

#[test]
fn waterline_stays_clear_of_an_island() {
    let region = Region::new(vec![
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
    let mut p = pp();
    p.strategy = PocketStrategy::Waterline;
    p.stepover = 2.5;
    // The test's own chord flattening of a fitted arc cuts a sagitta
    // INSIDE the arc - the ring's true geometry stays on the exact
    // center boundary, but its flattened image dips FLATTEN_TOL into
    // the hole, where the exact polygon would report a gouge. So the
    // clearance question is measured against the center grown by twice
    // that sagitta; a real gouge (a ring reaching INTO the island) is
    // millimeters, and still fails this by a mile.
    let center = region.offset(-5.0 + 2.0 * FLATTEN_TOL).expect("offset");
    let t = pocket(&region, &p).expect("valid");
    for z in levels_3_zs() {
        let (pts, lifted) = polyline_at(&t, z);
        for q in &pts {
            assert!(in_closed(&center, *q), "ring point {q:?} outside the center area");
            let dx = (15.0 - q.0).max(q.0 - 25.0).max(0.0);
            let dy = (15.0 - q.1).max(q.1 - 25.0).max(0.0);
            assert!((dx * dx + dy * dy).sqrt() > 5.0 - 2.0 * FLATTEN_TOL, "point {q:?} in the island");
        }
        for i in 1..pts.len() {
            let a = Point2::new(pts[i - 1].0, pts[i - 1].1);
            let b = Point2::new(pts[i].0, pts[i].1);
            if !chord_clears(&center, a, b) {
                assert!(lifted[i], "gouging ring chord {:?} -> {:?}", pts[i - 1], pts[i]);
            }
        }
    }
}

#[test]
fn pocket_refuses_bad_input() {
    let sq = rect(0.0, 0.0, 20.0, 20.0);
    let cases = [
        {
            let mut p = pp();
            p.stepover = 0.0;
            p
        },
        {
            let mut p = pp();
            p.tool_diameter = f64::NAN;
            p
        },
        {
            let mut p = pp();
            p.stepover = 11.0; // beyond tool reach for zigzag
            p
        },
        {
            let mut p = pp();
            p.feed = 0.0;
            p
        },
        {
            let mut p = pp();
            p.stock_to_leave = -1.0;
            p
        },
        {
            let mut p = pp();
            p.bottom_z = 1.0; // bottom above top
            p
        },
        {
            let mut p = pp();
            p.clearance_z = -1.0; // under the first cut level
            p
        },
        {
            let mut p = pp();
            p.stepdown = 0.0;
            p
        },
        {
            // Waterline's tighter coverage bound: inside zigzag's limit.
            let mut p = pp();
            p.strategy = PocketStrategy::Waterline;
            p.tool_diameter = 8.0;
            p.stepover = 5.0; // > r = 4
            p
        },
    ];
    for p in cases {
        assert!(matches!(pocket(&sq, &p), Err(GenError::Invalid(_))), "{p:?}");
    }
    // The same numbers zigzag accepts.
    let mut ok = pp();
    ok.tool_diameter = 8.0;
    ok.stepover = 5.0;
    assert!(pocket(&sq, &ok).is_ok());
    // Narrower than the tool: material no legal position can reach.
    assert!(matches!(
        pocket(&rect(0.0, 0.0, 8.0, 8.0), &pp()),
        Err(GenError::Invalid(_))
    ));
    assert!(matches!(
        pocket(&Region::empty(), &pp()),
        Err(GenError::Empty(_))
    ));
}

#[test]
fn pocket_is_deterministic() {
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
    for s in [PocketStrategy::Zigzag, PocketStrategy::Waterline] {
        let mut p = pp();
        p.strategy = s;
        if s == PocketStrategy::Waterline {
            p.stepover = 2.0;
        }
        let a = pocket(&r, &p).unwrap();
        let b = pocket(&r, &p).unwrap();
        assert_eq!(a, b, "{s:?}");
    }
}

// ---- slot and trace -----------------------------------------------------

fn sp() -> SlotParams {
    SlotParams {
        tool_diameter: 10.0,
        width: 10.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        top_z: 0.0,
        bottom_z: -6.0,
        stepdown: 3.0,
        stepover: 5.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    }
}

fn tp() -> TraceParams {
    TraceParams {
        clearance_z: 5.0,
        top_z: 0.0,
        bottom_z: -2.0,
        stepdown: 1.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    }
}

/// Distance from p to the segment a -> b: the stadium membership test (a
/// point is in the stadium of half-width h iff it is within h of the axis
/// segment).
fn seg_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let dd = dx * dx + dy * dy;
    let t = if dd == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / dd).clamp(0.0, 1.0)
    };
    (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy)
}

#[test]
fn slot_the_tool_fills_the_channel_and_runs_the_centerline() {
    let (a, b) = (Point2::new(0.0, 0.0), Point2::new(40.0, 0.0));
    let t = slot(a, b, &sp()).expect("valid"); // h = 0: one row
    assert!(t.is_chained() && t.all_finite());
    for &z in &levels_3_zs() {
        let (pts, lifted) = polyline_at(&t, z);
        assert_eq!(pts, vec![(0.0, 0.0), (40.0, 0.0)]);
        assert!(lifted[0] && lifted[1..].iter().all(|&l| !l));
        assert!((level_len(&t, z) - 40.0).abs() < 1e-9);
    }
    assert_eq!(plunge_xy(&t).len(), 3, "one plunge per level, no in-cut lift");
    for m in t.moves.iter().filter(|m| m.kind == MoveKind::Rapid) {
        assert!(m.from.z == 5.0 || m.to.z == 5.0, "rapid inside the cut: {m:?}");
    }
}

#[test]
fn slot_offsets_rows_across_a_wide_channel() {
    let (a, b) = (Point2::new(0.0, 0.0), Point2::new(40.0, 0.0));
    let p = SlotParams {
        tool_diameter: 6.0,
        width: 20.0,
        stepover: 4.0,
        bottom_z: 0.0,
        ..sp()
    };
    let t = slot(a, b, &p).expect("valid"); // h = 7: rows -7, -3, 1, 5, 7
    let (s40, s48, s24) = (40f64.sqrt(), 48f64.sqrt(), 24f64.sqrt());
    let want = vec![
        (0.0, -7.0),
        (40.0, -7.0),
        (40.0 + s40, -3.0),
        (-s40, -3.0),
        (-s48, 1.0),
        (40.0 + s48, 1.0),
        (40.0 + s24, 5.0),
        (-s24, 5.0),
        (0.0, 7.0),
        (40.0, 7.0),
    ];
    let (pts, _) = polyline_at(&t, 0.0);
    assert_eq!(pts, want, "serpentine rows, each reaching its round end by sqrt(h^2 - d^2)");
    assert_eq!(
        t.moves.iter().filter(|m| m.kind == MoveKind::Feed).count(),
        want.len() - 1,
        "joins are feeds: the center area is convex"
    );
    assert_eq!(plunge_xy(&t).len(), 1);
    for q in &pts {
        assert!(
            seg_dist(*q, (0.0, 0.0), (40.0, 0.0)) <= 7.0 + 1e-9,
            "center left the area: {q:?}"
        );
    }
    let max_x = pts.iter().map(|q| q.0).fold(f64::NEG_INFINITY, f64::max);
    let min_x = pts.iter().map(|q| q.0).fold(f64::INFINITY, f64::min);
    assert_eq!(max_x, 40.0 + s48);
    assert_eq!(min_x, -s48);
}

#[test]
fn conventional_slot_starts_at_the_far_end() {
    let p = SlotParams {
        tool_diameter: 6.0,
        width: 20.0,
        stepover: 4.0,
        bottom_z: 0.0,
        climb: false,
        ..sp()
    };
    let t = slot(Point2::new(0.0, 0.0), Point2::new(40.0, 0.0), &p).unwrap();
    let (pts, _) = polyline_at(&t, 0.0);
    assert_eq!(pts[0], (40.0, -7.0), "conventional enters at +x");
    assert_eq!(pts[1], (0.0, -7.0));
    let mut ys: Vec<f64> = pts.iter().map(|q| q.1).collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    assert_eq!(ys, vec![-7.0, -3.0, 1.0, 5.0, 7.0], "same rows, reversed travel");
}

#[test]
fn slot_stock_to_leave_narrows_the_center_area() {
    let p = SlotParams {
        tool_diameter: 6.0,
        width: 20.0,
        stock_to_leave: 2.0,
        stepover: 2.0,
        bottom_z: 0.0,
        ..sp()
    };
    let t = slot(Point2::new(0.0, 0.0), Point2::new(40.0, 0.0), &p).unwrap();
    let (pts, _) = polyline_at(&t, 0.0);
    // h = 10 - 3 - 2: rows reach +-5 lateral, sqrt(24) past the ends.
    for q in &pts {
        assert!(
            seg_dist(*q, (0.0, 0.0), (40.0, 0.0)) <= 5.0 + 1e-9,
            "cut into the allowance: {q:?}"
        );
    }
    let mut ys: Vec<f64> = pts.iter().map(|q| q.1).collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    assert_eq!(ys, vec![-5.0, -3.0, -1.0, 1.0, 3.0, 5.0]);
    let max_x = pts.iter().map(|q| q.0).fold(f64::NEG_INFINITY, f64::max);
    assert_eq!(max_x, 40.0 + 24f64.sqrt());
}

#[test]
fn slot_refuses_bad_input() {
    let (a, b) = (Point2::new(0.0, 0.0), Point2::new(40.0, 0.0));
    let cases: Vec<(&str, SlotParams)> = vec![
        ("tool wider than the slot", SlotParams { tool_diameter: 12.0, ..sp() }),
        ("stepover beyond the tool", SlotParams { stepover: 11.0, ..sp() }),
        ("zero width", SlotParams { width: 0.0, ..sp() }),
        ("non-finite width", SlotParams { width: f64::NAN, ..sp() }),
        ("leave eats the slot", SlotParams { stock_to_leave: 6.0, ..sp() }),
        ("negative leave", SlotParams { stock_to_leave: -1.0, ..sp() }),
        ("top below bottom", SlotParams { bottom_z: 1.0, ..sp() }),
        ("clearance below the cut", SlotParams { clearance_z: -2.0, ..sp() }),
        ("zero feed", SlotParams { feed: 0.0, ..sp() }),
        ("zero stepdown", SlotParams { stepdown: 0.0, ..sp() }),
    ];
    for (name, p) in cases {
        assert!(
            matches!(slot(a, b, &p), Err(GenError::Invalid(_))),
            "{name}: expected a clean refusal"
        );
    }
    assert!(
        matches!(slot(a, a, &sp()), Err(GenError::Invalid(_))),
        "coincident endpoints"
    );
    assert!(
        matches!(slot(a, Point2::new(f64::INFINITY, 0.0), &sp()), Err(GenError::Invalid(_))),
        "non-finite endpoints"
    );
}

#[test]
fn slot_is_deterministic() {
    let p = SlotParams {
        tool_diameter: 6.0,
        width: 20.0,
        stepover: 4.0,
        ..sp()
    };
    let (a, b) = (Point2::new(-3.0, 2.0), Point2::new(21.0, 8.0)); // any old angle
    let x = slot(a, b, &p).unwrap();
    let y = slot(a, b, &p).unwrap();
    assert_eq!(x, y);
    assert!(x.is_chained() && x.all_finite());
}

#[test]
fn trace_runs_exactly_on_the_boundary() {
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let p = TraceParams { bottom_z: 0.0, ..tp() };
    let t = trace(&r, &p).expect("valid");
    let (pts, lifted) = polyline_at(&t, 0.0);
    assert_eq!(pts.len(), 5, "closed walk of a 4-vertex loop");
    assert_eq!(pts[0], pts[4], "the traversal returns to its plunge point");
    assert!(lifted[0] && lifted[1..].iter().all(|&l| !l));
    let corners = [(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)];
    for q in &pts[..4] {
        assert!(corners.iter().any(|c| c == q), "left the boundary: {q:?}");
    }
    assert!((level_len(&t, 0.0) - 80.0).abs() < 1e-9);
    assert!(shoelace(&pts[..4]) > 0.0, "climb keeps the interior on the left");
    assert_eq!(plunge_xy(&t).len(), 1);
}

#[test]
fn trace_conventional_reverses_the_walk() {
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let p = TraceParams { bottom_z: 0.0, climb: false, ..tp() };
    let t = trace(&r, &p).unwrap();
    let (pts, _) = polyline_at(&t, 0.0);
    assert!(shoelace(&pts[..4]) < 0.0, "conventional runs the loop backwards");
    assert_eq!(pts[0], pts[4]);
}

#[test]
fn trace_cuts_every_loop_at_every_level() {
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
    let t = trace(&r, &tp()).expect("valid"); // zs 0, -1, -2
    assert_eq!(plunge_xy(&t).len(), 6, "two loops at three levels");
    for m in t.moves.iter().filter(|m| m.kind == MoveKind::Rapid) {
        assert!(m.from.z == 5.0 || m.to.z == 5.0, "rapid inside the cut: {m:?}");
    }
    let verts: Vec<(f64, f64)> = r
        .loops()
        .iter()
        .flat_map(|l| l.points().iter().map(|q| (q.x, q.y)))
        .collect();
    for &z in &[0.0, -1.0, -2.0] {
        let (pts, _) = polyline_at(&t, z);
        assert_eq!(pts.len(), 10, "both rings closed at {z}: {pts:?}");
        for q in &pts {
            assert!(verts.iter().any(|v| v == q), "point off the sketch: {q:?}");
        }
        assert!((level_len(&t, z) - 96.0).abs() < 1e-9, "80 outer + 16 hole");
    }
}

#[test]
fn trace_refuses_bad_input() {
    assert!(
        matches!(trace(&Region::empty(), &tp()), Err(GenError::Empty(_))),
        "empty region"
    );
    let r = rect(0.0, 0.0, 10.0, 10.0);
    let cases: Vec<(&str, TraceParams)> = vec![
        ("non-finite clearance", TraceParams { clearance_z: f64::NAN, ..tp() }),
        ("zero stepdown", TraceParams { stepdown: 0.0, ..tp() }),
        ("top below bottom", TraceParams { bottom_z: 1.0, ..tp() }),
        ("clearance below the cut", TraceParams { clearance_z: -3.0, ..tp() }),
        ("zero feed", TraceParams { feed: 0.0, ..tp() }),
        ("zero plunge feed", TraceParams { plunge_feed: 0.0, ..tp() }),
    ];
    for (name, p) in cases {
        assert!(matches!(trace(&r, &p), Err(GenError::Invalid(_))), "{name}");
    }
}

#[test]
fn trace_is_deterministic() {
    let r = rect(0.0, 0.0, 10.0, 10.0);
    let a = trace(&r, &tp()).unwrap();
    let b = trace(&r, &tp()).unwrap();
    assert_eq!(a, b);
    assert!(a.is_chained() && a.all_finite());
}


