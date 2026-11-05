//! The dual-kernel agreement bench (M12.1 retirement gate, stage 1): the same
//! boolean/offset fixtures through truck and solidcore, measured (volume +
//! bounding box) and compared against each other AND against the closed-form
//! answer, so a disagreement is located to the engine that left the analytic
//! value - a finding against that engine, never a tolerance to widen.
//! Provenance: our own fixtures; closed forms are elementary solid geometry
//! (box/cylinder volumes, Steiner's formula for the rounded box offset).
//!
//! Gate map (M12.1): covers camcraft_kernel::{solidcore_impl, truck_impl,
//! measure}.
//!
//! Stated tolerance: a tessellation whose every point lies within `t` of the
//! true surface moves the volume by at most `curved_area * t`; truck's error
//! budget is `t = mesh_tol + boolean_tol` over the fixture's curved area
//! (planar faces tessellate exactly), with a 1e-6 relative floor. solidcore
//! integrates exactly at 1e-7 relative, so it is held to 1e-6 relative.

use std::f64::consts::PI;

use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::truck_impl::TruckKernel;
use camcraft_kernel::{Kernel, KernelError, Measure, P3};

const REL_FLOOR: f64 = 1e-6;

enum Tool {
    Box(P3, P3),
    Cyl(P3, f64, f64),
}

struct Fixture {
    name: &'static str,
    stock: (P3, P3),
    tool: Tool,
    volume: f64,
    bbox: (P3, P3),
    /// Curved surface area of the result (drives truck's volume budget).
    curved_area: f64,
}

fn fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            name: "box-minus-box-corner",
            stock: ([0.0; 3], [10.0; 3]),
            tool: Tool::Box([5.0; 3], [10.0; 3]),
            volume: 875.0,
            bbox: ([0.0; 3], [10.0; 3]),
            curved_area: 0.0,
        },
        Fixture {
            name: "box-minus-box-slot",
            stock: ([0.0; 3], [20.0, 10.0, 10.0]),
            tool: Tool::Box([5.0, -1.0, 5.0], [10.0, 12.0, 10.0]),
            volume: 1500.0,
            bbox: ([0.0; 3], [20.0, 10.0, 10.0]),
            curved_area: 0.0,
        },
        Fixture {
            name: "cylinder-through-box",
            stock: ([0.0; 3], [10.0; 3]),
            tool: Tool::Cyl([5.0, 5.0, -1.0], 2.0, 12.0),
            volume: 1000.0 - 40.0 * PI,
            bbox: ([0.0; 3], [10.0; 3]),
            curved_area: 2.0 * PI * 2.0 * 10.0,
        },
        Fixture {
            name: "cylinder-blind-pocket",
            stock: ([0.0; 3], [20.0, 20.0, 10.0]),
            tool: Tool::Cyl([10.0, 10.0, 5.0], 3.0, 10.0),
            volume: 4000.0 - 45.0 * PI,
            bbox: ([0.0; 3], [20.0, 20.0, 10.0]),
            curved_area: 2.0 * PI * 3.0 * 5.0,
        },
    ]
}

fn run<K: Kernel>(k: &mut K, f: &Fixture) -> Result<Measure, KernelError> {
    let stock = k.make_box(f.stock.0, f.stock.1)?;
    let tool = match f.tool {
        Tool::Box(o, s) => k.make_box(o, s)?,
        Tool::Cyl(b, r, h) => k.make_cylinder(b, r, h)?,
    };
    let mut r = k.boolean_subtract(stock, tool)?;
    k.measure(&mut r)
}

fn want(f: &Fixture) -> Measure {
    Measure {
        volume: f.volume,
        min: f.bbox.0,
        max: f.bbox.1,
    }
}

/// One engine's verdict on one fixture, `Err` = a located finding.
fn verdict(engine: &str, m: &Measure, f: &Fixture, vol_tol: f64, box_tol: f64) -> Result<(), String> {
    let w = want(f);
    let dv = (m.volume - w.volume).abs();
    let db = m.bbox_deviation(&w);
    println!(
        "{:<24} {:<9} V={:.9} (analytic {:.9}, |dV|={:.3e} tol {:.3e}) bbox dev {:.3e} tol {:.3e}",
        f.name, engine, m.volume, w.volume, dv, vol_tol, db, box_tol
    );
    if dv > vol_tol || db > box_tol {
        Err(format!(
            "{}: {engine} left the closed form (|dV|={dv:.3e} > {vol_tol:.3e} or bbox {db:.3e} > {box_tol:.3e}); measured {m:?}",
            f.name
        ))
    } else {
        Ok(())
    }
}

fn bench(f: &Fixture, truck: &mut TruckKernel) -> Vec<String> {
    let mut findings = Vec::new();
    let mut sc = SolidcoreKernel::default();
    let sc_tol = REL_FLOOR * f.volume;
    let t = truck.mesh_tol + truck.boolean_tol;
    let tr_vol_tol = (f.curved_area * t).max(sc_tol);
    let tr_box_tol = if f.curved_area > 0.0 { t } else { 1e-6 };
    let sc_m = run(&mut sc, f);
    let tr_m = run(truck, f);
    match &sc_m {
        Ok(m) => findings.extend(verdict("solidcore", m, f, sc_tol, 1e-6).err()),
        Err(e) => findings.push(format!("{}: solidcore error: {e}", f.name)),
    }
    match &tr_m {
        Ok(m) => findings.extend(verdict("truck", m, f, tr_vol_tol, tr_box_tol).err()),
        Err(e) => findings.push(format!("{}: truck error: {e}", f.name)),
    }
    if let (Ok(a), Ok(b)) = (&sc_m, &tr_m) {
        // The cross-engine check: within the looser (truck) budget.
        let dv = (a.volume - b.volume).abs();
        println!("{:<24} cross     |V_sc - V_truck| = {dv:.3e}", f.name);
        if dv > tr_vol_tol + sc_tol || a.bbox_deviation(b) > tr_box_tol + 1e-6 {
            findings.push(format!("{}: engines disagree: solidcore {a:?} vs truck {b:?}", f.name));
        }
    }
    findings
}

#[test]
fn boolean_fixtures_agree_across_kernels() {
    let mut truck = TruckKernel::default();
    let findings: Vec<String> = fixtures().iter().flat_map(|f| bench(f, &mut truck)).collect();
    assert!(findings.is_empty(), "agreement findings:\n{}", findings.join("\n"));
}

/// The convergence leg: truck at 10x finer tessellation must close on the
/// same answer (a truck number that only agrees at one tolerance is luck).
#[test]
#[ignore = "heavy lane: truck tessellation at 1e-4 chord tolerance over all boolean fixtures"]
fn boolean_fixtures_agree_at_fine_tessellation() {
    let mut truck = TruckKernel {
        boolean_tol: 0.001,
        mesh_tol: 0.0001,
    };
    let findings: Vec<String> = fixtures().iter().flat_map(|f| bench(f, &mut truck)).collect();
    assert!(findings.is_empty(), "agreement findings:\n{}", findings.join("\n"));
}

/// Offset fixtures: truck offers no solid offset (Unsupported, asserted), so
/// solidcore is benched against the closed form alone.
#[test]
fn offset_fixtures_match_closed_form() {
    // (name, distance, volume, bbox)
    let a = 10.0_f64;
    let cases: [(&str, f64, f64, (P3, P3)); 2] = [
        (
            // Steiner: V + S d + (pi d^2 / 4) * sum(edges) + 4/3 pi d^3
            "box-offset-out-1",
            1.0,
            a.powi(3) + 6.0 * a * a + PI / 4.0 * 12.0 * a + 4.0 / 3.0 * PI,
            ([-1.0; 3], [11.0; 3]),
        ),
        ("box-offset-in-1", -1.0, 8.0_f64.powi(3), ([1.0; 3], [9.0; 3])),
    ];
    let mut truck = TruckKernel::default();
    let mut tb = truck.make_box([0.0; 3], [a; 3]).ok();
    if let Some(b) = tb.as_mut() {
        assert!(matches!(truck.offset(b, 1.0), Err(KernelError::Unsupported { .. })));
    }
    let mut findings = Vec::new();
    for (name, d, volume, bbox) in cases {
        let mut sc = SolidcoreKernel::default();
        let f = Fixture {
            name,
            stock: ([0.0; 3], [a; 3]),
            tool: Tool::Box([0.0; 3], [1.0; 3]),
            volume,
            bbox,
            curved_area: 0.0,
        };
        let m = sc
            .make_box([0.0; 3], [a; 3])
            .and_then(|mut b| sc.offset(&mut b, d))
            .and_then(|mut r| sc.measure(&mut r));
        match m {
            Ok(m) => findings.extend(verdict("solidcore", &m, &f, REL_FLOOR * volume, 1e-6).err()),
            Err(e) => findings.push(format!("{name}: solidcore error: {e}")),
        }
    }
    assert!(findings.is_empty(), "offset findings:\n{}", findings.join("\n"));
}

/// Offset of a holed plate (the hole grows as the walls shrink).
#[test]
#[ignore = "heavy lane: offset of a boolean result (cylinder-through-box, inset 0.5)"]
fn holed_box_inset_matches_closed_form() {
    let mut sc = SolidcoreKernel::default();
    let want_v = 9.0_f64.powi(3) - PI * 2.5 * 2.5 * 9.0;
    let m = (|| {
        let s = sc.make_box([0.0; 3], [10.0; 3])?;
        let c = sc.make_cylinder([5.0, 5.0, -1.0], 2.0, 12.0)?;
        let mut holed = sc.boolean_subtract(s, c)?;
        let mut r = sc.offset(&mut holed, -0.5)?;
        sc.measure(&mut r)
    })();
    let f = Fixture {
        name: "holed-box-inset-0.5",
        stock: ([0.0; 3], [10.0; 3]),
        tool: Tool::Box([0.0; 3], [1.0; 3]),
        volume: want_v,
        bbox: ([0.5; 3], [9.5; 3]),
        curved_area: 0.0,
    };
    match m {
        Ok(m) => {
            let r = verdict("solidcore", &m, &f, REL_FLOOR * want_v, 1e-6);
            assert!(r.is_ok(), "{r:?}");
        }
        Err(e) => panic!("holed-box-inset-0.5: solidcore error: {e}"),
    }
}
