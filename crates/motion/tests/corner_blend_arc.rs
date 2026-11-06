//! M11.1 owed item - line-to-arc and arc-to-arc corner fillets: a
//! corner next to an arc rounds with ONE circle tangent to both
//! elements, of the user radius when it fits (checked numerically: the
//! radius, the tangent points on the original curves, the tangent
//! directions), always on the air side of the sharp path; a radius that
//! cannot fit leaves the corner sharp and counted; degenerate arcs never
//! panic or leak a NaN.
//!
//! Gate map (M11.1): covers camcraft_motion::machining2d (fillet, fillet_arc, fillet_tally).

use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{fillet_corners, Legal, Seg};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn room() -> Region {
    let l = Loop::new(vec![p(-100.0, -100.0), p(100.0, -100.0), p(100.0, 100.0), p(-100.0, 100.0)]).unwrap();
    Region::new(vec![l]).unwrap()
}

fn open(region: &Region) -> Legal<'_> {
    Legal { region, inside: true, air_left: None }
}

#[derive(Clone, Copy, Debug)]
enum E {
    L(Point2, Point2),
    A { from: Point2, c: Point2, ccw: bool, to: Point2 },
}

impl E {
    fn from(&self) -> Point2 {
        match *self {
            E::L(a, _) | E::A { from: a, .. } => a,
        }
    }
    fn to(&self) -> Point2 {
        match *self {
            E::L(_, b) | E::A { to: b, .. } => b,
        }
    }
    fn tangent(&self, at_end: bool) -> (f64, f64) {
        let v = match *self {
            E::L(a, b) => (b.x - a.x, b.y - a.y),
            E::A { from, c, ccw, to } => {
                let q = if at_end { to } else { from };
                let (dx, dy) = (q.x - c.x, q.y - c.y);
                if ccw { (-dy, dx) } else { (dy, -dx) }
            }
        };
        let l = v.0.hypot(v.1);
        (v.0 / l, v.1 / l)
    }
}

/// The walk as elements, one per line piece / arc.
fn els(walk: &[Seg], start: Point2) -> Vec<E> {
    let mut out = Vec::new();
    let mut at = start;
    for s in walk {
        match s {
            Seg::Line(qs) => {
                for &q in qs {
                    out.push(E::L(at, q));
                    at = q;
                }
            }
            Seg::Arc { center, ccw, to } => {
                out.push(E::A { from: at, c: *center, ccw: *ccw, to: *to });
                at = *to;
            }
        }
    }
    out
}

/// Signed side of `q` against an original element's full curve: > 0 on
/// the travel-left side.
fn left_of(e: &E, q: Point2) -> f64 {
    match *e {
        E::L(a, b) => {
            let l = (b.x - a.x).hypot(b.y - a.y);
            ((b.x - a.x) * (q.y - a.y) - (b.y - a.y) * (q.x - a.x)) / l
        }
        E::A { from, c, ccw, .. } => {
            let r = (from.x - c.x).hypot(from.y - c.y);
            let d = (q.x - c.x).hypot(q.y - c.y);
            if ccw { r - d } else { d - r }
        }
    }
}

fn on_curve(e: &E, q: Point2) -> f64 {
    left_of(e, q).abs()
}

/// The single corner `a -> b` rounded in `out`: the output has the shape
/// [trimmed a, fillet, trimmed b]. Checks the fillet radius, that its ends
/// sit on the original curves and are tangent in travel sense to both,
/// and that every point of it is on the turn's inside of both curves
/// (the air side - never past the sharp path's material side).
fn assert_one_fillet(a: E, b: E, out: &[Seg], rho: f64) {
    let got = els(out, a.from());
    assert_eq!(got.len(), 3, "{got:?}");
    let E::A { from: t1, c, ccw, to: t2 } = got[1] else { panic!("no fillet: {got:?}") };
    assert!(((t1.x - c.x).hypot(t1.y - c.y) - rho).abs() < 1e-9, "radius at T1");
    assert!(((t2.x - c.x).hypot(t2.y - c.y) - rho).abs() < 1e-9, "radius at T2");
    assert!(on_curve(&a, t1) < 1e-9 && on_curve(&b, t2) < 1e-9, "tangent points off the curves");
    for (u, w) in [(got[0].tangent(true), got[1].tangent(false)), (got[1].tangent(true), got[2].tangent(false))] {
        let cross = u.0 * w.1 - u.1 * w.0;
        let dot = u.0 * w.0 + u.1 * w.1;
        assert!(dot > 0.0 && cross.abs() < 1e-9, "kink {cross} at a fillet end");
    }
    // The trimmed neighbours are the originals' own curves.
    assert!(on_curve(&a, got[0].from()) < 1e-9 && on_curve(&b, got[2].to()) < 1e-9);
    assert!(got[2].to() == b.to());
    // Air side: the fillet turns the corner's way and stays inside both.
    let sign = if ccw { 1.0 } else { -1.0 };
    let (s, e) = ((t1.y - c.y).atan2(t1.x - c.x), (t2.y - c.y).atan2(t2.x - c.x));
    let sweep = (sign * (e - s)).rem_euclid(std::f64::consts::TAU);
    assert!(sweep > 0.0 && sweep < std::f64::consts::PI);
    for k in 1..64 {
        let t = s + sign * sweep * k as f64 / 64.0;
        let q = p(c.x + rho * t.cos(), c.y + rho * t.sin());
        assert!(sign * left_of(&a, q) > 0.0 && sign * left_of(&b, q) > 0.0, "fillet point {q:?} past the path");
    }
}

fn walk_of(a: E, b: E) -> Vec<Seg> {
    let seg = |e: E| match e {
        E::L(_, q) => Seg::Line(vec![q]),
        E::A { c, ccw, to, .. } => Seg::Arc { center: c, ccw, to },
    };
    vec![seg(a), seg(b)]
}

fn round_one(a: E, b: E, r: f64) {
    let room = room();
    let walk = walk_of(a, b);
    let (out, t) = fillet_corners(&walk, a.from(), r, 0.01, &open(&room));
    assert_eq!((t.corners, t.rounded, t.curved), (1, 1, 0), "{t:?}");
    assert!(t.max_rest > 0.0 && t.max_rest.is_finite());
    assert_one_fillet(a, b, &out, r);
}

// ---- line -> arc and arc -> line, convex and concave ---------------------

#[test]
fn a_line_into_a_convex_arc_rounds_tangent_to_both() {
    // Left turn of 90 deg onto a CW arc (bending away from the fillet:
    // the offset circle is R + rho).
    let a = E::L(p(0.0, 0.0), p(10.0, 0.0));
    let b = E::A { from: p(10.0, 0.0), c: p(20.0, 0.0), ccw: false, to: p(20.0, 10.0) };
    round_one(a, b, 2.0);
}

#[test]
fn a_line_into_a_concave_arc_rounds_tangent_to_both() {
    // Left turn of 90 deg onto a CCW arc (wrapping round the fillet: the
    // offset circle is R - rho).
    let a = E::L(p(0.0, 0.0), p(10.0, 0.0));
    let b = E::A { from: p(10.0, 0.0), c: p(5.0, 0.0), ccw: true, to: p(5.0, 5.0) };
    round_one(a, b, 1.0);
    // The mirror: a right turn onto a CW arc.
    let a = E::L(p(0.0, 0.0), p(10.0, 0.0));
    let b = E::A { from: p(10.0, 0.0), c: p(5.0, 0.0), ccw: false, to: p(5.0, -5.0) };
    round_one(a, b, 1.0);
}

#[test]
fn an_arc_into_a_line_rounds_convex_and_concave() {
    // Convex: a CW quarter about the origin ends at (10, 0) heading -y,
    // then turns left onto +x (the arc bends away from the fillet).
    let a = E::A { from: p(0.0, 10.0), c: p(0.0, 0.0), ccw: false, to: p(10.0, 0.0) };
    let b = E::L(p(10.0, 0.0), p(20.0, 0.0));
    round_one(a, b, 2.0);
    // Concave: a CCW arc ending at (10, 0) heading +y, turning left onto -x.
    let a = E::A { from: p(5.0, -5.0), c: p(5.0, 0.0), ccw: true, to: p(10.0, 0.0) };
    let b = E::L(p(10.0, 0.0), p(0.0, 10.0));
    round_one(a, b, 1.0);
}

#[test]
fn an_arc_into_an_arc_rounds_tangent_to_both() {
    // Two CW quarters meeting at a 90 deg left turn, -y onto +x (both
    // bend away from the fillet).
    let a = E::A { from: p(0.0, 10.0), c: p(0.0, 0.0), ccw: false, to: p(10.0, 0.0) };
    let b = E::A { from: p(10.0, 0.0), c: p(10.0, -10.0), ccw: false, to: p(20.0, -10.0) };
    round_one(a, b, 2.0);
    // A CCW arc into a CCW arc, left turn: both wrap round the fillet.
    let a = E::A { from: p(10.0, -10.0), c: p(10.0, 0.0), ccw: true, to: p(20.0, 0.0) };
    let b = E::A { from: p(20.0, 0.0), c: p(10.0, 0.0), ccw: true, to: p(10.0, 10.0) };
    let room = room();
    let (out, t) = fillet_corners(&walk_of(a, b), a.from(), 1.0, 0.01, &open(&room));
    // Concentric arcs of one circle meet tangentially: no corner at all.
    assert_eq!((t.corners, out.len()), (0, 2), "{t:?}");
    let b = E::A { from: p(20.0, 0.0), c: p(20.0, 6.0), ccw: true, to: p(26.0, 6.0) };
    // CCW about (10, 0) at (20, 0) heads +y; CCW about (20, 6) heads +x
    // there: a right turn, the fillet outside both circles.
    round_one(a, b, 1.0);
}

// ---- refusals --------------------------------------------------------------

#[test]
fn a_radius_too_large_for_the_arc_stays_sharp_and_counted() {
    // A 0.2 mm arc behind a 90 deg corner: a tangent fillet of r = 5 (or
    // its four halvings, down to 0.3125) would need more sweep than half
    // the arc has.
    let a = E::L(p(0.0, 0.0), p(10.0, 0.0));
    let b = E::A { from: p(10.0, 0.0), c: p(20.0, 0.0), ccw: false, to: p(10.002, 0.2) };
    let room = room();
    let walk = walk_of(a, b);
    let (out, t) = fillet_corners(&walk, a.from(), 5.0, 0.01, &open(&room));
    assert_eq!((t.corners, t.rounded, t.curved), (1, 0, 1), "{t:?}");
    assert_eq!(out, walk, "a refused walk comes back verbatim");
    assert!(t.notes("walked", 0.01).iter().any(|n| n.contains("no tangent fillet fits")));
    // A smaller radius fits the same corner.
    let (_, t) = fillet_corners(&walk, a.from(), 0.05, 0.01, &open(&room));
    assert_eq!(t.rounded, 1, "{t:?}");
}

#[test]
fn a_curved_turn_toward_the_part_stays_sharp() {
    let room = room();
    let a = E::L(p(0.0, 0.0), p(10.0, 0.0));
    let b = E::A { from: p(10.0, 0.0), c: p(20.0, 0.0), ccw: false, to: p(20.0, 10.0) };
    let legal = Legal { region: &room, inside: true, air_left: Some(false) };
    let walk = walk_of(a, b);
    let (out, t) = fillet_corners(&walk, a.from(), 2.0, 0.01, &legal);
    assert_eq!((t.toward_part, t.rounded), (1, 0));
    assert_eq!(out, walk);
}

#[test]
fn a_boundary_across_the_fillet_blocks_it() {
    // The legal room's wall runs through the corner's inside, one tenth
    // of a millimetre off the line: every radius crosses it.
    let l = Loop::new(vec![p(-50.0, -50.0), p(50.0, -50.0), p(50.0, 0.1), p(-50.0, 0.1)]).unwrap();
    let room = Region::new(vec![l]).unwrap();
    let a = E::L(p(0.0, 0.0), p(10.0, 0.0));
    let b = E::A { from: p(10.0, 0.0), c: p(20.0, 0.0), ccw: false, to: p(20.0, 10.0) };
    let walk = walk_of(a, b);
    let (out, t) = fillet_corners(&walk, a.from(), 2.0, 0.01, &open(&room));
    assert_eq!((t.rounded, t.blocked), (0, 1), "{t:?}");
    assert_eq!(out, walk);
}

#[test]
fn degenerate_arcs_never_panic_or_leak_nan() {
    let room = room();
    let l = open(&room);
    let o = p(0.0, 0.0);
    let cases: Vec<Vec<Seg>> = vec![
        // Arc centre on its own start (zero radius).
        vec![Seg::Line(vec![p(10.0, 0.0)]), Seg::Arc { center: p(10.0, 0.0), ccw: true, to: p(10.0, 0.0) }],
        vec![Seg::Line(vec![p(10.0, 0.0)]), Seg::Arc { center: p(10.0, 0.0), ccw: true, to: p(11.0, 1.0) }],
        // Non-finite centre and end.
        vec![Seg::Line(vec![p(10.0, 0.0)]), Seg::Arc { center: p(f64::NAN, 0.0), ccw: true, to: p(12.0, 2.0) }],
        vec![Seg::Line(vec![p(10.0, 0.0)]), Seg::Arc { center: p(20.0, 0.0), ccw: false, to: p(f64::INFINITY, 2.0) }],
        // A cusp: the arc doubles back on the line.
        vec![Seg::Line(vec![p(10.0, 0.0)]), Seg::Arc { center: p(10.0, 5.0), ccw: false, to: p(10.0, 10.0) }],
        // A huge arc, a micro arc.
        vec![Seg::Line(vec![p(10.0, 0.0)]), Seg::Arc { center: p(1e12, 0.0), ccw: false, to: p(10.0, 1e-3) }],
        vec![Seg::Line(vec![p(10.0, 0.0)]), Seg::Arc { center: p(10.0 + 1e-7, 0.0), ccw: false, to: p(10.0 + 1e-7, 1e-7) }],
    ];
    for walk in cases {
        for r in [3.0, 0.0, f64::NAN, 1e9] {
            let (out, t) = fillet_corners(&walk, o, r, 0.01, &l);
            assert!(t.max_rest.is_finite());
            if t.rounded == 0 {
                assert_eq!(format!("{out:?}"), format!("{walk:?}"), "nothing proved: the walk verbatim");
                continue;
            }
            // Whatever rounded is finite to the last coordinate.
            let fin = |q: &Point2| q.x.is_finite() && q.y.is_finite();
            for s in &out {
                match s {
                    Seg::Line(qs) => assert!(qs.iter().all(fin), "{out:?}"),
                    Seg::Arc { center, to, .. } => assert!(fin(center) && fin(to), "{out:?}"),
                }
            }
        }
    }
}

// ---- the counter on a mixed walk -----------------------------------------

#[test]
fn a_mixed_line_arc_walk_rounds_every_corner_and_counts_none_sharp() {
    // line, CW arc, line, CCW arc, CW arc, line: five 90 deg corners
    // (line-arc, arc-line, line-arc, arc-arc, arc-line), each with room
    // for a 1 mm fillet on both sides.
    let room = room();
    let walk = vec![
        Seg::Line(vec![p(10.0, 0.0)]),
        Seg::Arc { center: p(20.0, 0.0), ccw: false, to: p(20.0, 10.0) },
        Seg::Line(vec![p(20.0, 20.0)]),
        Seg::Arc { center: p(20.0, 25.0), ccw: true, to: p(25.0, 25.0) },
        Seg::Arc { center: p(25.0, 15.0), ccw: false, to: p(35.0, 15.0) },
        Seg::Line(vec![p(50.0, 15.0)]),
    ];
    let (out, t) = fillet_corners(&walk, p(0.0, 0.0), 1.0, 0.01, &open(&room));
    assert_eq!(t.corners, 5, "{t:?}");
    assert_eq!((t.rounded, t.curved, t.blocked, t.tiny), (5, 0, 0, 0), "{t:?}");
    // Every joint of the result is tangent, and the walk ends where it did.
    let got = els(&out, p(0.0, 0.0));
    assert_eq!(got.len(), 6 + 5);
    for w in got.windows(2) {
        assert!(w[0].to() == w[1].from());
        let (u, v) = (w[0].tangent(true), w[1].tangent(false));
        assert!(u.0 * v.0 + u.1 * v.1 > 0.0 && (u.0 * v.1 - u.1 * v.0).abs() < 1e-9, "{w:?}");
    }
    assert!(got.last().map(|e| e.to()) == Some(p(50.0, 15.0)));
}
