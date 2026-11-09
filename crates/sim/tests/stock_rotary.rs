//! The rotary parity suite (M13.1, the rotary half of the field
//! verdict): a turning program on the field engine - a ball riding a
//! half-ring groove around a bar laid on the X axis, the pose radial
//! at every station - against the radial law written independently
//! here: the union of the balls the controller's lerp places (tip on
//! its chord, the letters each on their own line - the law
//! `verify::rotary` samples), read densely. The field must say the
//! groove's column tops within the chord sag the pre-pass declares
//! plus `FIELD_VERDICT_EPS`, and the steel/air sign at every cell
//! center off the groove's skin by more than that. The column arm
//! refuses by name. Provenance: our own engines and the M13 radial
//! law; nothing from vendor binaries.
//!
//! Gate map (M4.4): covers stock_sim::field_rotary (rotary_chords,
//! simulate_rotary), stock_sim::field_cut (the posed sweep it rides).

use camcraft_sim::cl::{angles_to_axis, axis_to_angles, Move, MoveKind};
use camcraft_sim::geometry::{Point3, Triangle};
use camcraft_sim::stock::{stock_mesh, StockShape};
use camcraft_sim::stock_sim::{
    rotary_chords, Backend, SimOp, Stock, FIELD_BAND, FIELD_VERDICT_EPS, MAX_SUBCHORDS, ROTARY_COLUMN_REFUSAL,
};
use camcraft_sim::verify::RotaryAxis;

const CELL: f64 = 0.5;
const BAR_R: f64 = 10.0;
const BAR_L: f64 = 30.0;
const R: f64 = 3.0;
/// The groove: tip radius 5 at station x 15, so the ball (center at
/// radius 8) cuts from radius 5 out through the skin.
const TIP_RHO: f64 = 5.0;
const STATION: f64 = 15.0;
/// Stations every 15 degrees from -75 to +75 off the crown (+Z): the
/// half the head reaches from above, no letter flip mid-ring.
const STEP_DEG: f64 = 15.0;
const SPAN_DEG: f64 = 75.0;
/// The chord sag allowed at the tool's farthest point.
const POSE_TOL: f64 = 1e-4;

fn axis() -> RotaryAxis {
    RotaryAxis { origin: Point3::ZERO, dir: [1.0, 0.0, 0.0] }
}

/// The bar laid along +X, x in 0..30: the standing cylinder's own
/// watertight mesh, cyclically permuted (a rotation, orientation kept).
fn bar(b: Backend) -> Stock {
    let standing = stock_mesh(&StockShape::Cylinder { diameter: 2.0 * BAR_R, height: BAR_L }).expect("the bar meshes");
    let s = |p: Point3| Point3::new(p.z, p.x, p.y);
    let laid: Vec<Triangle> = standing.iter().map(|t| Triangle::new(s(t.a), s(t.b), s(t.c))).collect();
    Stock::from_facets(b, &laid, CELL).expect("the laid bar")
}

/// Radial pose at `deg` off the crown, toward +Y.
fn radial(deg: f64) -> [f64; 3] {
    let (s, c) = deg.to_radians().sin_cos();
    [0.0, s, c]
}

fn tip(deg: f64) -> Point3 {
    let (s, c) = deg.to_radians().sin_cos();
    Point3::new(STATION, TIP_RHO * s, TIP_RHO * c)
}

/// The program: a posed rapid to the first station, then the ring's
/// chords station to station, each stamped with its own radial pose.
fn program() -> Vec<Move> {
    let mut out = vec![Move {
        rotary: None,
        kind: MoveKind::Rapid,
        from: tip(-SPAN_DEG),
        to: tip(-SPAN_DEG),
        feed: None,
        arc: None,
        axis: Some(radial(-SPAN_DEG)),
    }];
    let n = ((2.0 * SPAN_DEG) / STEP_DEG).round() as usize;
    for k in 0..n {
        let (a, b) = (-SPAN_DEG + k as f64 * STEP_DEG, -SPAN_DEG + (k + 1) as f64 * STEP_DEG);
        out.push(Move { rotary: None, kind: MoveKind::Feed, from: tip(a), to: tip(b), feed: Some(300.0), arc: None, axis: Some(radial(b)) });
    }
    out
}

fn op(moves: &[Move]) -> [SimOp<'_>; 1] {
    [SimOp { moves, radius: R, ball: true, cone: None, corner: None }]
}

/// The radial law's ball center `s` of the way along a station move:
/// the tip on its chord, the letters lerped - the lerp `verify::rotary`
/// samples, restated here.
fn center(m: &Move, from_axis: [f64; 3], s: f64) -> (Point3, [f64; 3]) {
    let (la, lb) = (axis_to_angles(from_axis), axis_to_angles(m.axis.expect("stamped")));
    let l = [la[0] + (lb[0] - la[0]) * s, la[1] + (lb[1] - la[1]) * s, 0.0];
    let a = angles_to_axis(l).expect("a finite pose");
    let t = Point3::new(m.from.x + (m.to.x - m.from.x) * s, m.from.y + (m.to.y - m.from.y) * s, m.from.z + (m.to.z - m.from.z) * s);
    (Point3::new(t.x + R * a[0], t.y + R * a[1], t.z + R * a[2]), a)
}

/// Dense samples of the center curve over the whole program, each
/// with the pose its flute stands on.
fn centers() -> Vec<(Point3, [f64; 3])> {
    const N: usize = 1000;
    let p = program();
    let mut held = p[0].axis.expect("stamped");
    let mut out = Vec::new();
    for m in p.iter().filter(|m| m.kind != MoveKind::Rapid) {
        for k in 0..=N {
            out.push(center(m, held, k as f64 / N as f64));
        }
        held = m.axis.expect("stamped");
    }
    out
}

/// The flute above the ball: the column twin's law (everything up the
/// axis from the center is removed), long enough to leave the bar on
/// every pose here (the radial law reads the tool's own flute; beyond
/// the bar's skin the two agree on air).
const FLUTE: f64 = 20.0;

/// Signed distance to the swept tool - ball and flute cylinder at
/// every sample: negative where cut (air).
fn law_cut(cs: &[(Point3, [f64; 3])], p: Point3) -> f64 {
    let mut best = f64::INFINITY;
    for (c, a) in cs {
        let w = [p.x - c.x, p.y - c.y, p.z - c.z];
        let ww = w[0] * w[0] + w[1] * w[1] + w[2] * w[2];
        best = best.min(ww.sqrt() - R);
        let t = w[0] * a[0] + w[1] * a[1] + w[2] * a[2];
        let rho = (ww - t * t).max(0.0).sqrt();
        let (dr, dt) = (rho - R, (-t).max(t - FLUTE));
        let d = if dr > 0.0 || dt > 0.0 { dr.max(0.0).hypot(dt.max(0.0)) } else { dr.max(dt) };
        best = best.min(d);
    }
    best
}

/// The groove's column top at (x, y): the lowest point of the swept
/// tool over the column (the sphere's bottom; the tilted cylinder's
/// lowest point on the column, a quadratic in z); the skin where no
/// sample reaches it.
fn law_top(cs: &[(Point3, [f64; 3])], x: f64, y: f64) -> (f64, bool) {
    let skin = (BAR_R * BAR_R - y * y).max(0.0).sqrt();
    let mut top = skin;
    let mut sphere = false;
    for (c, a) in cs {
        let (wx, wy) = (x - c.x, y - c.y);
        let d2 = wx * wx + wy * wy;
        if d2 < R * R && c.z - (R * R - d2).sqrt() < top {
            top = c.z - (R * R - d2).sqrt();
            sphere = true;
        }
        // Points (x, y, c.z + zeta) inside the cylinder: along-axis
        // t = A + B zeta in [0, FLUTE], perpendicular distance <= R.
        let (aa, b) = (wx * a[0] + wy * a[1], a[2]);
        let (qa, qb, qc) = (1.0 - b * b, -2.0 * aa * b, d2 - aa * aa - R * R);
        if qa <= 1e-12 || b.abs() <= 1e-12 {
            continue; // a vertical or horizontal flute: not this ring's
        }
        let disc = qb * qb - 4.0 * qa * qc;
        if disc < 0.0 {
            continue;
        }
        let (z1, z2) = ((-qb - disc.sqrt()) / (2.0 * qa), (-qb + disc.sqrt()) / (2.0 * qa));
        let (s1, s2) = ((-aa) / b, (FLUTE - aa) / b);
        let (lo, hi) = (z1.max(s1.min(s2)), z2.min(s1.max(s2)));
        if lo <= hi && c.z + lo < top {
            top = c.z + lo;
            sphere = false;
        }
    }
    (top, sphere)
}

#[test]
fn the_column_arm_refuses_a_rotary_program_by_name() {
    let mut c = bar(Backend::Column);
    let e = c.simulate_rotary(&op(&program()), &axis(), POSE_TOL).expect_err("no turning sweep on the column");
    assert!(e.to_string().contains(ROTARY_COLUMN_REFUSAL), "{e}");
    let mut f = bar(Backend::Field { band: FIELD_BAND });
    assert!(f.simulate_rotary(&op(&program()), &axis(), 0.0).is_err(), "a zero tolerance is refused by name");
    let bad = RotaryAxis { origin: Point3::ZERO, dir: [0.0, 0.0, 0.0] };
    assert!(f.simulate_rotary(&op(&program()), &bad, POSE_TOL).is_err(), "a degenerate axis is refused by name");
    // A turn too fine to chord within the cap refuses, never coarsens.
    let e = rotary_chords(&program(), &axis(), R, 1.0e3, 1e-10, true).expect_err("the cap");
    assert!(e.to_string().contains(&MAX_SUBCHORDS.to_string()), "{e}");
    // Pass-through: a program that never turns is itself, stamped.
    let still: Vec<Move> = program().into_iter().map(|m| Move { axis: Some(radial(0.0)), ..m }).collect();
    let (same, st) = rotary_chords(&still, &axis(), R, 10.0, POSE_TOL, true).expect("no turn");
    assert_eq!(same.len(), still.len());
    assert_eq!(st.sub_chords, 0);
}

#[test]
fn the_field_reads_the_groove_the_radial_law_sweeps() {
    let mut f = bar(Backend::Field { band: FIELD_BAND });
    let t0 = std::time::Instant::now();
    let stats = f.simulate_rotary(&op(&program()), &axis(), POSE_TOL).expect("the field chords the turn");
    let wall = t0.elapsed();
    let bound = stats.center_sag + FIELD_VERDICT_EPS;
    println!(
        "rotary pre-pass: {} turning moves -> {} sub-chords, half-angle {:.3e} rad, center sag {:.3e} mm, wall {:.2?}",
        stats.turning_moves, stats.sub_chords, stats.half_angle, stats.center_sag, wall
    );
    assert!(stats.sub_chords > 0 && stats.center_sag > 0.0 && stats.center_sag <= POSE_TOL);
    let cs = centers();

    // Height parity on the verdict lattice over the groove: every
    // column whose law top sits under the skin by more than the
    // groove-edge step (where the top jumps from the band to the
    // skin and a hair of position is a mm of height).
    // Two classes of column: the groove FLOOR, cut by the sphere whose
    // center the pre-pass keeps exact (parity at the center sag, the
    // FIELD_VERDICT_EPS class), and the band's flanks cut by the flute
    // cylinder standing on each sub-chord's mid pose - off the true
    // pose by the half-angle, a displacement of (reach into the bar)
    // times the angle: the chord law's own class, stated, not hidden.
    let flute_bound = (R + 5.0) * stats.half_angle * 2.5;
    let (mut worst, mut tops) = (0.0f64, 0usize);
    let (mut worst_flute, mut flutes) = (0.0f64, 0usize);
    let n = (BAR_R / CELL) as i32 + 1;
    for j in -n..=n {
        for i in 0..((BAR_L / CELL) as i32) {
            let (x, y) = ((i as f64 + 0.5) * CELL, (j as f64 + 0.5) * CELL);
            if y.abs() >= BAR_R - 2.0 * CELL {
                continue; // the bar's tessellated flank is the mesh's business
            }
            let (want, sphere) = law_top(&cs, x, y);
            let skin = (BAR_R * BAR_R - y * y).sqrt();
            // Off the band's walls in x (the sphere's flank, where a
            // hair of position is a mm of height): the floor and the
            // gentle slopes only, which is where a verdict is read.
            if skin - want < 0.25 || (x - STATION).abs() > R - 1.0 {
                continue;
            }
            let got = f.top(x, y);
            assert!(got.is_finite(), "no top at ({x}, {y})");
            let d = (got - want).abs();
            if sphere {
                worst = worst.max(d);
                tops += 1;
                assert!(d <= bound, "floor top at ({x}, {y}): field {got:.7} vs radial law {want:.7}, |delta| {d:.3e} > {bound:.3e}");
            } else {
                worst_flute = worst_flute.max(d);
                flutes += 1;
                assert!(d <= flute_bound, "flute top at ({x}, {y}): field {got:.7} vs radial law {want:.7}, |delta| {d:.3e} > {flute_bound:.3e}");
            }
        }
    }
    println!("rotary height parity: {tops} floor tops, worst |delta| {worst:.3e} (bound {bound:.3e}); {flutes} flute tops, worst |delta| {worst_flute:.3e} (chord-law bound {flute_bound:.3e})");
    assert!(tops > 100, "fixture: the groove floor must be read");

    // Sign parity at cell centers inside the bar, off the band's skin
    // by more than the bound.
    let (mut checked, mut worst_sign) = (0usize, 0.0f64);
    for k in -n..=n {
        for j in -n..=n {
            for i in ((STATION - R - 1.0) / CELL) as i32..((STATION + R + 1.0) / CELL) as i32 {
                let (x, y, z) = ((i as f64 + 0.5) * CELL, (j as f64 + 0.5) * CELL, (k as f64 + 0.5) * CELL);
                if y.hypot(z) >= BAR_R - CELL {
                    continue;
                }
                let law = law_cut(&cs, Point3::new(x, y, z));
                // Off the swept skin by the flute's own chord-law
                // displacement (the ring's end stations are cut by
                // the flute's flank at a few 1e-4 of depth).
                if law.abs() <= (R + 5.0) * stats.half_angle {
                    continue;
                }
                let d = f.distance(Point3::new(x, y, z));
                checked += 1;
                if (d > 0.0) != (law < 0.0) {
                    worst_sign = worst_sign.max(law.abs());
                    println!("({x}, {y}, {z}): field {d:.6}, law {law:.6}");
                }
            }
        }
    }
    println!("rotary sign parity: {checked} cell centers, worst sign miss {worst_sign:.3e}");
    assert_eq!(worst_sign, 0.0);
    assert!(checked > 5_000, "fixture: the scan must cover the groove");
}
