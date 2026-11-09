//! The lathe parity suite (M13.1 owed, the annular-sector sweep): a
//! turning program on the field engine against the closed-form
//! meridian law the turning branch judges with (`turning::revolve`'s
//! min-of-tips boxcar, here with the nose's own round caps) - the
//! revolved cut volume is a solid of revolution, so the remaining
//! radius R(z) and the column tops over it are exact numbers, and the
//! field must say them within `FIELD_VERDICT_EPS` at every azimuth.
//! The column arm has no revolved reading and must refuse by name.
//! Provenance: our own engines and the M5.2 meridian law; nothing from
//! vendor binaries.
//!
//! Gate map (M4.4): covers stock_sim::field_cut (Sweep::Lathe),
//! stock_sim::field_lathe (LatheAxis, the stated spindle - #19),
//! stock_sim::backend (simulate_lathe, simulate_lathe_about).

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::Point3;
use camcraft_sim::stock::StockShape;
use camcraft_sim::stock_sim::{Backend, LatheAxis, Stock, FIELD_BAND, FIELD_VERDICT_EPS, LATHE_COLUMN_REFUSAL};

const CELL: f64 = 0.5;
const BAR_R: f64 = 10.0;
const BAR_H: f64 = 30.0;
const NOSE: f64 = 1.0;

fn feed(from: Point3, to: Point3) -> Move {
    Move { rotary: None, kind: MoveKind::Feed, from, to, feed: Some(200.0), arc: None, axis: None }
}

/// The bar: diameter 20, standing 0..30 on the lathe axis +Z.
fn bar(b: Backend) -> Stock {
    Stock::from_shape(b, &StockShape::Cylinder { diameter: 2.0 * BAR_R, height: BAR_H }, CELL).expect("the bar")
}

/// The program, nose centers in the meridian at azimuth 0: an OD pass
/// at radius 7 from above the bar down to station 10, then a groove
/// plunge at station 20 from radius 7 in to radius 4 and back out.
fn program() -> Vec<Move> {
    vec![
        feed(Point3::new(7.0, 0.0, 35.0), Point3::new(7.0, 0.0, 10.0)),
        feed(Point3::new(7.0, 0.0, 10.0), Point3::new(7.0, 0.0, 35.0)),
        feed(Point3::new(7.0, 0.0, 20.0), Point3::new(4.0, 0.0, 20.0)),
        feed(Point3::new(4.0, 0.0, 20.0), Point3::new(7.0, 0.0, 20.0)),
    ]
}

/// The meridian law, in the test's own words: the nose circle swept
/// along each chord is a stadium in the (radius, station) half-plane
/// (the M5.2 E nose-center envelope), and a point is removed where
/// some stadium holds it. The signed half-plane distance to the
/// nearest stadium, written independently of the engine.
fn meridian_cut(rho: f64, z: f64) -> f64 {
    let seg = |ax: f64, az: f64, bx: f64, bz: f64| {
        let (dx, dz) = (bx - ax, bz - az);
        let t = (((rho - ax) * dx + (z - az) * dz) / (dx * dx + dz * dz)).clamp(0.0, 1.0);
        (rho - ax - t * dx).hypot(z - az - t * dz) - NOSE
    };
    seg(7.0, 35.0, 7.0, 10.0).min(seg(7.0, 20.0, 4.0, 20.0))
}

/// The column top over radius rho: the bar's end face (30) unless the
/// OD pass stands over it, then the cap surface of its lowest station.
fn expected_top(rho: f64) -> f64 {
    if rho < 7.0 - NOSE || rho > 7.0 + NOSE {
        BAR_H
    } else {
        10.0 - (NOSE * NOSE - (rho - 7.0) * (rho - 7.0)).sqrt()
    }
}

#[test]
fn the_column_arm_refuses_a_turning_program_by_name() {
    let mut c = bar(Backend::Column);
    let e = c.simulate_lathe(&program(), NOSE).expect_err("no revolved reading on the column");
    assert!(e.to_string().contains(LATHE_COLUMN_REFUSAL), "{e}");
    let mut f = bar(Backend::Field { band: FIELD_BAND });
    assert!(f.simulate_lathe(&program(), 0.0).is_err(), "a zero nose is refused by name");
    assert!(f.simulate_lathe(&program(), f64::NAN).is_err(), "a NaN nose is refused by name");
}

#[test]
fn the_field_cuts_the_revolved_stadium_at_every_azimuth() {
    let mut f = bar(Backend::Field { band: FIELD_BAND });
    f.simulate_lathe(&program(), NOSE).expect("the field cuts the lathe program");
    // Sign parity at cell centers: the cut layer is closed form there,
    // so every center off the surface by more than a hair must read
    // the meridian law's sign - at every azimuth, the revolution made
    // literal.
    let mut checked = 0usize;
    let mut worst_sign = 0.0f64;
    let n = (2.0 * BAR_R / CELL) as i32 + 2;
    for k in 0..((BAR_H / CELL) as i32) {
        let z = (k as f64 + 0.5) * CELL;
        for j in -n..=n {
            for i in -n..=n {
                let (x, y) = ((i as f64 + 0.5) * CELL, (j as f64 + 0.5) * CELL);
                let rho = x.hypot(y);
                let law = meridian_cut(rho, z);
                if rho >= BAR_R - CELL || law.abs() < 1e-6 {
                    continue; // the bar's tessellated skin is the mesh's business
                }
                let d = f.distance(Point3::new(x, y, z));
                let want_air = law < 0.0;
                checked += 1;
                if (d > 0.0) != want_air {
                    worst_sign = worst_sign.max(law.abs());
                    println!("({x}, {y}, {z}): rho {rho:.4}: field {d:.6}, meridian law {law:.4} says {}", if want_air { "air" } else { "steel" });
                }
            }
        }
    }
    println!("lathe sign parity: {checked} cell centers checked, worst sign miss {worst_sign:.3e}");
    assert_eq!(worst_sign, 0.0);
    assert!(checked > 10_000, "fixture: the scan must cover the bar");

    // Height parity: the verdict vocabulary. Column tops over the
    // OD pass's cap and over the untouched bar must read the meridian
    // law's numbers within the field's declared epsilon.
    // Read on the verdict lattice (cell centers, where the ray entry
    // is recorded - the law every parity suite reads on), every
    // azimuth the lattice has, off the cap's two edges where the top
    // steps from the cap to the bar's end face.
    let mut worst = 0.0f64;
    let (mut tops, mut caps) = (0usize, 0usize);
    for j in -n..=n {
        for i in -n..=n {
            let (x, y) = ((i as f64 + 0.5) * CELL, (j as f64 + 0.5) * CELL);
            let rho = x.hypot(y);
            if rho >= BAR_R - CELL || (rho - 6.0).abs() < 0.05 || (rho - 8.0).abs() < 0.05 {
                continue;
            }
            let got = f.top(x, y);
            let want = expected_top(rho);
            assert!(got.is_finite(), "no top at ({x}, {y}): {got}");
            let d = (got - want).abs();
            worst = worst.max(d);
            tops += 1;
            caps += usize::from(want < BAR_H);
            assert!(d <= FIELD_VERDICT_EPS, "top at ({x}, {y}) rho {rho:.4}: field {got:.7} vs meridian law {want:.7}, |delta| {d:.3e}");
        }
    }
    println!("lathe height parity: {tops} tops ({caps} on the cap), worst |delta| {worst:.3e} (eps {FIELD_VERDICT_EPS:.0e})");
    assert!(caps > 100, "fixture: the cap must be read");
}

#[test]
fn a_parting_chord_through_the_axis_is_read_from_its_mirror_too() {
    // A plunge from radius 7 through the axis to radius -1 (past
    // center): the revolved stadium removes the whole disc of the
    // kerf, radius 7 + nose at station 15, both caps included.
    let mut f = bar(Backend::Field { band: FIELD_BAND });
    let m = [feed(Point3::new(7.0, 0.0, 15.0), Point3::new(0.0, 0.0, 15.0))];
    f.simulate_lathe(&m, NOSE).expect("the parting cut runs");
    for az_deg in [0.0f64, 123.0, 270.0] {
        let az = az_deg.to_radians();
        for rho in [0.25, 0.75, 3.25, 6.75, 7.75] {
            let (x, y) = (rho * az.cos(), rho * az.sin());
            let d = f.distance(Point3::new(x, y, 15.25));
            assert!(d > 0.0, "rho {rho} az {az_deg} inside the kerf must be air: {d}");
        }
        let d = f.distance(Point3::new(8.25 * az.cos(), 8.25 * az.sin(), 15.25));
        assert!(d < 0.0, "rho 8.25 az {az_deg} beyond the kerf stays steel: {d}");
        let d = f.distance(Point3::new(3.25 * az.cos(), 3.25 * az.sin(), 13.25));
        assert!(d < 0.0, "under the kerf's cap stays steel: {d}");
    }
}

#[test]
fn a_stated_axis_off_the_origin_cuts_the_same_meridian() {
    // #19: the document's turning kernels write moves in the plane of
    // the part's FITTED axis, which need not be world Z through the
    // origin. The same program written about a spindle through
    // (1, 0.5) - its meridian plane leaning off world XZ - must cut
    // the same revolved stadium about THAT line: every cell centre
    // inside the bar reads the meridian law's sign measured from it.
    let (ox, oy) = (1.0, 0.5);
    let axis = LatheAxis::new(Point3::new(ox, oy, 0.0), [0.0, 0.0, 2.0]).expect("a finite line");
    // The meridian plane at azimuth 30 degrees about the stated axis.
    let (c, s) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
    let at = |rho: f64, z: f64| Point3::new(ox + rho * c, oy + rho * s, z);
    let m: Vec<Move> = program().iter().map(|mv| feed(at(mv.from.x, mv.from.z), at(mv.to.x, mv.to.z))).collect();
    let mut f = bar(Backend::Field { band: FIELD_BAND });
    f.simulate_lathe_about(&m, NOSE, &axis).expect("the field cuts about the stated axis");
    let (mut checked, mut misses) = (0usize, 0usize);
    let n = (2.0 * BAR_R / CELL) as i32 + 2;
    for k in 0..((BAR_H / CELL) as i32) {
        let z = (k as f64 + 0.5) * CELL;
        for j in -n..=n {
            for i in -n..=n {
                let (x, y) = ((i as f64 + 0.5) * CELL, (j as f64 + 0.5) * CELL);
                let law = meridian_cut((x - ox).hypot(y - oy), z);
                if x.hypot(y) >= BAR_R - CELL || law.abs() < 1e-6 {
                    continue;
                }
                checked += 1;
                misses += usize::from((f.distance(Point3::new(x, y, z)) > 0.0) != (law < 0.0));
            }
        }
    }
    println!("stated-axis sign parity: {checked} cell centers, {misses} misses");
    assert_eq!(misses, 0);
    assert!(checked > 10_000, "fixture: the scan must cover the bar");
    // Degenerate input refuses by name and leaves the field as it
    // stood: a non-finite move, a spindle that is not a line. An
    // empty program cuts nothing and says so by succeeding.
    let before = f.distance(Point3::new(ox + 7.0, oy, 20.0));
    let bad = [feed(Point3::new(f64::NAN, 0.0, 20.0), Point3::new(4.0, 0.0, 20.0))];
    assert!(f.simulate_lathe_about(&bad, NOSE, &axis).is_err(), "a NaN move is refused");
    assert!(f.simulate_lathe_about(&[], NOSE, &axis).is_ok(), "an empty program is no error");
    assert_eq!(f.distance(Point3::new(ox + 7.0, oy, 20.0)), before, "a refusal writes nothing");
    assert!(LatheAxis::new(Point3::new(0.0, 0.0, 0.0), [0.0, 0.0, 0.0]).is_err());
    assert!(LatheAxis::new(Point3::new(f64::NAN, 0.0, 0.0), [0.0, 0.0, 1.0]).is_err());
}

#[test]
fn a_leaned_axis_cuts_the_same_meridian_about_its_own_line() {
    // #19 review: a spindle leaned off world Z (10 degrees about Y,
    // through (-1.5, 0, 0)) exercises the general window reach
    // (`rr * sqrt(1 - d_k^2)` in every world axis, none of them zero)
    // and the leaned meridian split. The program is written in the
    // meridian plane at azimuth 40 degrees about that line; the
    // reference (radius, station) is the test's own projection.
    let th = 10f64.to_radians();
    let o = [-1.5, 0.0, 0.0];
    let d = [th.sin(), 0.0, th.cos()];
    let u = [th.cos(), 0.0, -th.sin()]; // unit, normal to d
    let v = [0.0, 1.0, 0.0]; // d x u
    let axis = LatheAxis::new(Point3::new(o[0], o[1], o[2]), d).expect("a finite line");
    let (c, s) = (40f64.to_radians().cos(), 40f64.to_radians().sin());
    let e = [c * u[0] + s * v[0], c * u[1] + s * v[1], c * u[2] + s * v[2]];
    let at = |rho: f64, z: f64| Point3::new(o[0] + rho * e[0] + z * d[0], o[1] + rho * e[1] + z * d[1], o[2] + rho * e[2] + z * d[2]);
    let m: Vec<Move> = program().iter().map(|mv| feed(at(mv.from.x, mv.from.z), at(mv.to.x, mv.to.z))).collect();
    let mut f = bar(Backend::Field { band: FIELD_BAND });
    f.simulate_lathe_about(&m, NOSE, &axis).expect("the field cuts about the leaned axis");
    let reference = |q: [f64; 3]| {
        let w = [q[0] - o[0], q[1] - o[1], q[2] - o[2]];
        let st = w[0] * d[0] + w[1] * d[1] + w[2] * d[2];
        let r = [w[0] - st * d[0], w[1] - st * d[1], w[2] - st * d[2]];
        meridian_cut((r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt(), st)
    };
    let (mut checked, mut air, mut misses) = (0usize, 0usize, 0usize);
    let n = (2.0 * BAR_R / CELL) as i32 + 2;
    for k in 0..((BAR_H / CELL) as i32) {
        let z = (k as f64 + 0.5) * CELL;
        for j in -n..=n {
            for i in -n..=n {
                let (x, y) = ((i as f64 + 0.5) * CELL, (j as f64 + 0.5) * CELL);
                let law = reference([x, y, z]);
                if x.hypot(y) >= BAR_R - CELL || law.abs() < 1e-6 {
                    continue;
                }
                checked += 1;
                air += usize::from(law < 0.0);
                misses += usize::from((f.distance(Point3::new(x, y, z)) > 0.0) != (law < 0.0));
            }
        }
    }
    println!("leaned-axis sign parity: {checked} cell centers ({air} air), {misses} misses");
    assert_eq!(misses, 0);
    assert!(checked > 10_000 && air > 1_000, "fixture: the scan must cover the bar and the cut");
}

#[test]
fn a_chord_skewed_off_one_meridian_is_refused_and_writes_nothing() {
    // #19 review: a chord whose ends stand in different planes through
    // the axis sweeps a hyperboloid with a waist nearer the axis than
    // the flat meridian segment - the sim refuses it by name, before
    // a cell is written, even after a good chord in the same program.
    let mut f = bar(Backend::Field { band: FIELD_BAND });
    let probe = Point3::new(3.5, 3.5, 20.25);
    let before = f.distance(probe);
    let good = feed(Point3::new(7.0, 0.0, 5.0), Point3::new(4.0, 0.0, 5.0));
    let skew = feed(Point3::new(7.0, 0.0, 20.0), Point3::new(0.0, 7.0, 20.0));
    let e = f.simulate_lathe(&[good, skew], NOSE).expect_err("a skewed chord is refused");
    assert!(e.to_string().contains("skewed"), "{e}");
    assert_eq!(f.distance(probe), before, "a refusal writes nothing");
    assert_eq!(f.distance(Point3::new(5.25, 0.0, 5.25)), bar(Backend::Field { band: FIELD_BAND }).distance(Point3::new(5.25, 0.0, 5.25)), "not even the good chord before it");
    // A chord inside the tolerance (float-noise skew) still cuts.
    let near = feed(Point3::new(7.0, 0.0, 20.0), Point3::new(4.0, 1e-9, 20.0));
    f.simulate_lathe(&[near], NOSE).expect("noise-level skew is one meridian");
    assert!(f.distance(Point3::new(5.25, 0.0, 20.25)) > 0.0, "the near-meridian groove cut");
}

#[test]
fn a_parting_move_past_centre_still_cuts_the_whole_kerf() {
    // The meridian check must not refuse parting: a plunge from radius
    // 7 through the axis to radius -1 has antiparallel radial vectors
    // (one meridian), and cuts the full kerf disc at every azimuth.
    let mut f = bar(Backend::Field { band: FIELD_BAND });
    let m = [feed(Point3::new(7.0, 0.0, 15.0), Point3::new(-1.0, 0.0, 15.0))];
    f.simulate_lathe(&m, NOSE).expect("parting past centre runs");
    for az_deg in [0.0f64, 90.0, 180.0, 300.0] {
        let az = az_deg.to_radians();
        for rho in [0.25, 3.25, 7.75] {
            let d = f.distance(Point3::new(rho * az.cos(), rho * az.sin(), 15.25));
            assert!(d > 0.0, "rho {rho} az {az_deg} inside the kerf must be air: {d}");
        }
        let d = f.distance(Point3::new(8.25 * az.cos(), 8.25 * az.sin(), 15.25));
        assert!(d < 0.0, "rho 8.25 az {az_deg} beyond the kerf stays steel: {d}");
    }
}
