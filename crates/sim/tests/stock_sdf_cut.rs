//! M7.1 B: swept-tool subtraction on the brick SDF field - the max
//! layer and the closed-form solids it folds in. The laws here are
//! Phase 7's promises: a cut is the running max of the negated tool
//! distances (moves commute; order changes nothing, by construction),
//! the tool solid is evaluated exactly along the move - never
//! voxelized - and removal is claimed only where the solid
//! demonstrably reached, so material is never hidden. Rapids touch
//! nothing; nonsense refuses by name before a brick is written. The
//! oracle probes stand on CELL CENTERS and recompute the law from
//! formulas written independently of `field_cut`'s.
//! Provenance: SDF boolean subtraction (the published implicit-
//! surface CSG), the capsule/cylinder/cone solids of RS-274 world
//! machining, and this codebase's own chord and parity kernels - per
//! the Clean-Room Protocol.
//!
//! Gate map (M4.4): covers camcraft_sim::stock_sim::field_cut and the
//! half-plane kernels of camcraft_sim::stock_sim::field_solid.

use camcraft_sim::cl::{arc_chords, ArcMove, Move, MoveKind};
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::{Point3, Triangle};
use camcraft_sim::stock_sim::{SdfField, SdfTool, StockError};

type V3 = [f64; 3];

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}
fn subv(a: Point3, b: Point3) -> V3 {
    [a.x - b.x, a.y - b.y, a.z - b.z]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn addv(a: Point3, v: V3) -> Point3 {
    p(a.x + v[0], a.y + v[1], a.z + v[2])
}
fn scale(v: V3, s: f64) -> V3 {
    [v[0] * s, v[1] * s, v[2] * s]
}
fn seg3(q: Point3, a: Point3, b: Point3) -> f64 {
    let d = subv(b, a);
    let dd = dot(d, d);
    let t = if dd > 0.0 {
        (dot(subv(q, a), d) / dd).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (subv(q, addv(a, scale(d, t)))).iter().fold(0.0, |s, &x| s + x * x).sqrt()
}
fn seg2(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let (dx, dy) = (bx - ax, by - ay);
    let dd = dx * dx + dy * dy;
    let t = if dd > 0.0 {
        (((px - ax) * dx + (py - ay) * dy) / dd).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (px - (ax + dx * t)).hypot(py - (ay + dy * t))
}

/// The closed box `[lo, hi]^3`: 12 facets (the stock_sdf fixture).
fn box_mesh(lo: f64, hi: f64) -> Vec<Triangle> {
    let v = |i: usize| p([lo, hi][i & 1], [lo, hi][(i >> 1) & 1], [lo, hi][(i >> 2) & 1]);
    let quad = |a: usize, b: usize, c: usize, d: usize| {
        [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))]
    };
    let mut t = Vec::with_capacity(12);
    for q in [
        quad(4, 5, 7, 6),
        quad(0, 2, 3, 1),
        quad(0, 1, 5, 4),
        quad(2, 6, 7, 3),
        quad(1, 3, 7, 5),
        quad(2, 0, 4, 6),
    ] {
        t.extend(q);
    }
    t
}

fn field() -> SdfField {
    SdfField::from_mesh(&box_mesh(0.0, 10.0), 1.0, 4).unwrap() // cap 4.0
}

fn mv(kind: MoveKind, from: Point3, to: Point3, axis: Option<V3>) -> Move {
    Move {
        kind,
        from,
        to,
        feed: if kind == MoveKind::Rapid { None } else { Some(500.0) },
        arc: None,
        axis,
    }
}
fn feed_mv(from: Point3, to: Point3) -> Move {
    mv(MoveKind::Feed, from, to, None)
}

/// Every cell center near the box, on the unit lattice.
fn centers() -> Vec<Point3> {
    let mut c = Vec::new();
    for i in -1..=11i32 {
        for j in -1..=11 {
            for k in -1..=11 {
                c.push(p(i as f64 + 0.5, j as f64 + 0.5, k as f64 + 0.5));
            }
        }
    }
    c
}

/// The law, measured: `after == max(before, claim)` at every center,
/// where `claim(q)` is the test's OWN negated, cap-clamped tool
/// distance. `guard` skips the cap's ambiguous border (the walk's
/// store-skip at `phi >= cap`); `tol` covers the scan's coarseness.
fn check(
    f: &mut SdfField,
    ms: &[(&Move, &SdfTool)],
    claim: impl Fn(Point3, f64) -> f64,
    guard: f64,
    tol: f64,
    label: &str,
) {
    let pts = centers();
    let before: Vec<f64> = pts.iter().map(|q| f.distance(*q)).collect();
    for (m, t) in ms {
        f.subtract(m, t, 0.01).unwrap();
    }
    let cap = f.cap();
    let mut moved = 0;
    for (q, b) in pts.iter().zip(&before) {
        let d = claim(*q, cap); // the test's tool distance phi
        if (d - cap).abs() < guard {
            continue;
        }
        let want = (*b).max((-d).min(cap));
        let got = f.distance(*q);
        assert!((got - want).abs() <= tol, "{label}: at {q:?} field {got}, law {want} (before {b}, phi {d})");
        if got != *b {
            moved += 1;
        }
    }
    assert!(moved > 0, "{label}: the cut changed nothing anywhere");
}

/// The flat's own SDF, and the tapered cone's, at one pose in the
/// axis frame - formulas written from the solids' definitions.
fn flat_pose(y: V3, ax: V3, r: f64, flank: f64) -> f64 {
    let zl = dot(y, ax);
    let rho = (dot(y, y) - zl * zl).max(0.0).sqrt();
    let (e1, e2, e3) = (rho - r, -zl, zl - flank);
    if e1.max(e2).max(e3) > 0.0 {
        e1.max(0.0).hypot(e2.max(0.0)).hypot(e3.max(0.0))
    } else {
        e1.max(e2).max(e3)
    }
}
fn taper_pose(y: V3, ax: V3, r: f64, cot: f64) -> f64 {
    let zl = dot(y, ax);
    let rho = (dot(y, y) - zl * zl).max(0.0).sqrt();
    let h = r * cot;
    let inside = zl >= 0.0 && zl <= h && rho * cot <= zl;
    let d1 = seg2(rho, zl, 0.0, 0.0, r, h); // the flank
    let d2 = seg2(rho, zl, 0.0, h, r, h); // the top face
    let d3 = seg2(rho, zl, 0.0, 0.0, 0.0, h); // the axis
    if inside {
        -d1.min(d2)
    } else {
        d1.min(d2).min(d3)
    }
}
/// The sweep's distance by dense parameter scan - independent of the
/// field's golden section; 1-Lipschitz poses bound its slack.
fn scan_phi(q: Point3, a: Point3, b: Point3, ax: V3, pose: impl Fn(V3, V3) -> f64) -> f64 {
    let d = subv(b, a);
    if dot(d, d) == 0.0 {
        return pose(subv(q, a), ax);
    }
    let n = 4000;
    (0..=n).fold(f64::INFINITY, |m, i| {
        let t = i as f64 / n as f64;
        m.min(pose(subv(q, addv(a, scale(d, t))), ax))
    })
}

#[test]
fn the_ball_sweep_is_a_capsule_and_the_law_holds_exactly() {
    let mut f = field();
    let m = feed_mv(p(5.0, 5.0, 6.0), p(5.0, 5.0, 9.0));
    let t = SdfTool::Ball { r: 1.0 };
    // Tip path lifted by r along +Z: the centers, in the test's own words.
    let (c0, c1) = (p(5.0, 5.0, 7.0), p(5.0, 5.0, 10.0));
    check(
        &mut f,
        &[(&m, &t)],
        |q, _cap| seg3(q, c0, c1) - 1.0,
        0.0, // closed form on both sides: no ambiguous border
        1e-9,
        "ball capsule",
    );
}

#[test]
fn the_flat_sweep_says_cylinder_not_column() {
    // r 1.5, flank 2, a short vertical ride: above the flank the
    // field must show material where the dexel's infinite column
    // shows air - the documented divergence, stated as a probe.
    let mut f = field();
    let m = feed_mv(p(0.5, 5.0, 5.5), p(0.5, 5.0, 6.5));
    let t = SdfTool::Flat { r: 1.5, flank: 2.0 };
    let ax = [0.0, 0.0, 1.0];
    check(
        &mut f,
        &[(&m, &t)],
        |q, _cap| scan_phi(q, m.from, m.to, ax, |y, a| flat_pose(y, a, 1.5, 2.0)),
        1e-2,
        1e-3,
        "flat cylinder",
    );
    // The divergence itself: a point above every pose's flank top
    // (8.5) but below the box lid is untouched material for the
    // field, and would read air in the dexel's column.
    let high = p(0.5, 5.5, 8.8);
    assert!(f.distance(high) < 0.0, "over the flank the field keeps its material");
}

#[test]
fn the_cone_sweep_reads_its_profile_in_the_axis_frame() {
    let mut f = field();
    let m = feed_mv(p(5.5, 5.5, 7.5), p(5.5, 5.5, 8.5));
    let t = SdfTool::Taper { r: 2.0, cot: 1.0 };
    let ax = [0.0, 0.0, 1.0];
    check(
        &mut f,
        &[(&m, &t)],
        |q, _cap| scan_phi(q, m.from, m.to, ax, |y, a| taper_pose(y, a, 2.0, 1.0)),
        1e-2,
        1e-3,
        "45-degree cone",
    );
}

#[test]
fn tilted_poses_ride_their_stated_axis() {
    // M5.1's stamp IS the input: lean the tool 45 degrees and both a
    // cone sweep and a stationary ball follow the axis, not +Z.
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let ax = [s, 0.0, s];
    let mut f = field();
    let m1 = mv(MoveKind::Feed, p(5.0, 5.0, 6.0), p(5.0, 5.0, 6.5), Some(ax));
    let m2 = mv(MoveKind::Plunge, p(5.0, 5.0, 9.0), p(5.0, 5.0, 9.0), Some(ax));
    let t1 = SdfTool::Taper { r: 2.0, cot: 1.0 };
    let t2 = SdfTool::Ball { r: 1.0 };
    let bc = addv(m2.from, scale(ax, 1.0)); // the lifted center
    // The moves' claims combine by max, which for the distances is
    // the min: the tool that reached deeper here is the one that is
    // closer, so the composite phi is just the minimum.
    check(
        &mut f,
        &[(&m1, &t1), (&m2, &t2)],
        |q, _cap| {
            let c1 = scan_phi(q, m1.from, m1.to, ax, |y, a| taper_pose(y, a, 2.0, 1.0));
            let c2 = seg3(q, bc, bc) - 1.0;
            c1.min(c2)
        },
        1e-2,
        1e-3,
        "tilted poses",
    );
}

#[test]
fn rapids_touch_nothing() {
    let mut f = field();
    let pts = centers();
    let before: Vec<f64> = pts.iter().map(|q| f.distance(*q)).collect();
    let m = mv(MoveKind::Rapid, p(5.0, 5.0, 12.0), p(5.0, 5.0, 2.0), None);
    f.subtract(&m, &SdfTool::Flat { r: 3.0, flank: 10.0 }, 0.01).unwrap();
    for (q, b) in pts.iter().zip(&before) {
        assert_eq!(f.distance(*q), *b, "a rapid moved material at {q:?}");
    }
    assert_eq!(f.cut_brick_count(), 0, "a rapid must allocate no cut brick");
}

#[test]
fn the_order_of_moves_changes_nothing() {
    // The max law's headline promise, literal: replay in any order.
    let a = feed_mv(p(3.0, 5.0, 6.0), p(3.0, 5.0, 8.5));
    let b = feed_mv(p(7.0, 5.0, 4.0), p(7.0, 5.0, 7.0));
    let ta = SdfTool::Ball { r: 1.0 };
    let tb = SdfTool::Flat { r: 1.2, flank: 6.0 };
    let mut f1 = field();
    let mut f2 = field();
    f1.subtract(&a, &ta, 0.01).unwrap();
    f1.subtract(&b, &tb, 0.01).unwrap();
    f2.subtract(&b, &tb, 0.01).unwrap();
    f2.subtract(&a, &ta, 0.01).unwrap();
    for q in centers() {
        assert_eq!(f1.distance(q), f2.distance(q), "order changed the answer at {q:?}");
    }
}

#[test]
fn material_the_never_reached_is_never_hidden() {
    let mut f = field();
    let m = feed_mv(p(5.0, 5.0, 6.0), p(5.0, 5.0, 9.0));
    let untouched = [p(5.5, 5.5, 0.5), p(-5.5, 5.5, 5.5), p(5.5, 5.5, -0.5)];
    let before: Vec<f64> = untouched.iter().map(|q| f.distance(*q)).collect();
    f.subtract(&m, &SdfTool::Ball { r: 1.0 }, 0.01).unwrap();
    for (q, b) in untouched.iter().zip(&before) {
        assert_eq!(f.distance(*q), *b, "a far cell changed at {q:?}");
    }
}

#[test]
fn a_huge_claim_clamps_at_the_cap() {
    // A cylinder that swallows the box: every reachable interior
    // cell answers exactly +cap - the magnitude never pretends to
    // know a depth past the band.
    let mut f = field();
    let m = feed_mv(p(-5.0, -5.0, -25.0), p(-5.0, -5.0, -25.0));
    f.subtract(&m, &SdfTool::Flat { r: 20.0, flank: 40.0 }, 0.01).unwrap();
    let cap = f.cap();
    assert_eq!(f.distance(p(5.5, 5.5, 5.5)), cap);
    assert_eq!(f.distance(p(9.5, 0.5, 9.5)), cap);
}

#[test]
fn an_arcs_sweep_rides_its_chords_within_tolerance() {
    // The chord law, measured: the field's arc answer differs from
    // the true arc sweep by at most the sagitta budget it was given.
    let mut f = field();
    let arc = ArcMove { center: Point2::new(5.0, 5.0), ccw: true };
    let mut m = feed_mv(p(5.0, 3.0, 5.0), p(7.0, 5.0, 5.0));
    m.arc = Some(arc);
    let t = SdfTool::Ball { r: 1.0 };
    let (chords, pts) = (arc_chords(Point2::new(5.0, 3.0), Point2::new(7.0, 5.0), arc, 0.01), centers());
    assert!(chords.len() > 3, "a 0.01-tolerance quarter must chord up");
    let before: Vec<f64> = pts.iter().map(|q| f.distance(*q)).collect();
    f.subtract(&m, &t, 0.01).unwrap();
    for (q, b) in pts.iter().zip(&before) {
        // True arc: the exact center path is an arc lifted by r.
        let (th0, th1) = (-std::f64::consts::FRAC_PI_2, 0.0);
        let phi = (0..=4000).fold(f64::INFINITY, |m2, i| {
            let t2 = i as f64 / 4000.0;
            let th = th0 + (th1 - th0) * t2;
            let c = p(5.0 + 2.0 * th.cos(), 5.0 + 2.0 * th.sin(), 6.0);
            m2.min(seg3(*q, c, c) - 1.0)
        });
        if (phi - f.cap()).abs() < 0.2 {
            continue; // the chords' window vs the arc's, near the cap
        }
        let want = (*b).max((-phi).min(f.cap()));
        assert!((f.distance(*q) - want).abs() <= 0.05, "arc at {q:?}: field {} vs arc-law {want}", f.distance(*q));
    }
}

#[test]
fn the_cut_footprint_is_sparse() {
    let mut f = field();
    let m = feed_mv(p(5.0, 5.0, 6.0), p(5.0, 5.0, 9.0));
    f.subtract(&m, &SdfTool::Ball { r: 1.0 }, 0.01).unwrap();
    let n = f.cut_brick_count();
    assert!(n > 0 && n <= 8, "a small plunge took {n} cut bricks");
}

fn refuse(m: &Move, tool: &SdfTool) -> StockError {
    let mut f = field();
    match f.subtract(m, tool, 0.01) {
        Ok(()) => panic!("this input must refuse, never cut"),
        Err(e) => e,
    }
}

#[test]
fn nonsense_inputs_refuse_by_name() {
    let s = |e: &StockError| format!("{e}");
    let m = feed_mv(p(5.0, 5.0, 6.0), p(5.0, 5.0, 9.0));
    assert!(s(&refuse(&m, &SdfTool::Ball { r: 0.0 })).contains("radius"));
    assert!(s(&refuse(&m, &SdfTool::Ball { r: f64::NAN })).contains("radius"));
    assert!(s(&refuse(&m, &SdfTool::Flat { r: -1.0, flank: 5.0 })).contains("radius"));
    assert!(s(&refuse(&m, &SdfTool::Flat { r: 1.0, flank: f64::INFINITY })).contains("flank"));
    assert!(s(&refuse(&m, &SdfTool::Taper { r: 1.0, cot: 0.0 })).contains("slope"));
    let mut f = field();
    assert!(s(&f.subtract(&m, &SdfTool::Ball { r: 1.0 }, -1.0).unwrap_err()).contains("tolerance"));
    assert!(s(&f.subtract(&m, &SdfTool::Ball { r: 1.0 }, f64::NAN).unwrap_err()).contains("tolerance"));
    let bad = feed_mv(p(f64::NAN, 5.0, 6.0), p(5.0, 5.0, 9.0));
    assert!(s(&refuse(&bad, &SdfTool::Ball { r: 1.0 })).contains("non-finite"));
    let bad_rapid = mv(MoveKind::Rapid, p(f64::INFINITY, 0.0, 0.0), p(1.0, 1.0, 1.0), None);
    assert!(s(&refuse(&bad_rapid, &SdfTool::Ball { r: 1.0 })).contains("non-finite"));
    let zero_ax = mv(MoveKind::Feed, m.from, m.to, Some([0.0, 0.0, 0.0]));
    assert!(s(&refuse(&zero_ax, &SdfTool::Ball { r: 1.0 })).contains("axis"));
}

#[test]
fn an_unpayable_window_refuses_and_writes_nothing() {
    // Atomicity: a move whose work is too big for one update refuses
    // BEFORE any brick is touched, so the field answers exactly as it
    // did. Since M7.3 the ceiling bounds the window AS CLIPPED to the
    // region a cut can matter in (a long move over a small stock pays
    // only its in-stock part - `stock_field_cost`), so the unpayable
    // move here is a wide flat crossing the whole fine box.
    let mut f = SdfField::from_mesh(&box_mesh(0.0, 10.0), 0.05, 4).unwrap();
    let pts: Vec<Point3> = vec![p(5.0, 5.0, 5.0), p(0.5, 5.5, 9.5), p(9.5, 0.5, 0.5)];
    let before: Vec<f64> = pts.iter().map(|q| f.distance(*q)).collect();
    let m = feed_mv(p(0.0, 0.0, 5.0), p(10.0, 10.0, 5.0));
    let e = match f.subtract(&m, &SdfTool::Flat { r: 5.0, flank: 10.0 }, 0.01) {
        Ok(()) => panic!("a ~10M-cell window at cell 0.05 must refuse, not chew"),
        Err(e) => e,
    };
    assert!(format!("{e}").contains("segment"), "{e}");
    for (q, b) in pts.iter().zip(&before) {
        assert_eq!(f.distance(*q), *b, "a refused write still reached {q:?}");
    }
    assert_eq!(f.cut_brick_count(), 0, "refusal allocated nothing");
}
