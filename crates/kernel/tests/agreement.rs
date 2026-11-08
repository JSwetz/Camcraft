//! The dual-kernel agreement bench (M12.1 retirement gate): the same
//! boolean (subtract, union, intersect) and offset fixtures through truck and
//! solidcore, measured (volume + bounding box) and compared against each other
//! AND against the closed-form answer, so a disagreement is located to the
//! engine that left the analytic value - a finding against that engine, never
//! a tolerance to widen. Edge blends live in blend.rs (solidcore only).
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

mod common;

use std::f64::consts::PI;

use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::truck_impl::TruckKernel;
use camcraft_kernel::{Kernel, KernelError, Measure, P3};
use common::{REL_FLOOR, Want, verdict};

#[derive(Clone, Copy)]
enum Op {
    Subtract,
    Union,
    Intersect,
}

enum Tool {
    Box(P3, P3),
    Cyl(P3, f64, f64),
}

struct Fixture {
    want: Want,
    op: Op,
    stock: (P3, P3),
    tool: Tool,
    /// Curved surface area of the result (drives truck's volume budget).
    curved_area: f64,
}

fn fx(name: &'static str, op: Op, stock: (P3, P3), tool: Tool, volume: f64, bbox: (P3, P3), curved_area: f64) -> Fixture {
    Fixture {
        want: Want { name, volume, bbox },
        op,
        stock,
        tool,
        curved_area,
    }
}

fn fixtures() -> Vec<Fixture> {
    use Op::*;
    let cube = ([0.0; 3], [10.0; 3]);
    vec![
        fx("box-minus-box-corner", Subtract, cube, Tool::Box([5.0; 3], [10.0; 3]), 875.0, cube, 0.0),
        fx(
            "box-minus-box-slot",
            Subtract,
            ([0.0; 3], [20.0, 10.0, 10.0]),
            Tool::Box([5.0, -1.0, 5.0], [10.0, 12.0, 10.0]),
            1500.0,
            ([0.0; 3], [20.0, 10.0, 10.0]),
            0.0,
        ),
        fx(
            "cylinder-through-box",
            Subtract,
            cube,
            Tool::Cyl([5.0, 5.0, -1.0], 2.0, 12.0),
            1000.0 - 40.0 * PI,
            cube,
            2.0 * PI * 2.0 * 10.0,
        ),
        fx(
            "cylinder-blind-pocket",
            Subtract,
            ([0.0; 3], [20.0, 20.0, 10.0]),
            Tool::Cyl([10.0, 10.0, 5.0], 3.0, 10.0),
            4000.0 - 45.0 * PI,
            ([0.0; 3], [20.0, 20.0, 10.0]),
            2.0 * PI * 3.0 * 5.0,
        ),
        // Union: 2 * 1000 minus the shared 5-cube.
        fx("box-union-box-corner", Union, cube, Tool::Box([5.0; 3], [10.0; 3]), 1875.0, ([0.0; 3], [15.0; 3]), 0.0),
        // A r2 boss standing 5 above the top face.
        fx(
            "box-union-cylinder-boss",
            Union,
            cube,
            Tool::Cyl([5.0, 5.0, 5.0], 2.0, 10.0),
            1000.0 + 20.0 * PI,
            ([0.0; 3], [10.0, 10.0, 15.0]),
            2.0 * PI * 2.0 * 5.0,
        ),
        fx("box-intersect-box-corner", Intersect, cube, Tool::Box([5.0; 3], [10.0; 3]), 125.0, ([5.0; 3], [10.0; 3]), 0.0),
        // The cylinder clipped to the cube's height.
        fx(
            "box-intersect-cylinder",
            Intersect,
            cube,
            Tool::Cyl([5.0, 5.0, -1.0], 2.0, 12.0),
            40.0 * PI,
            ([3.0, 3.0, 0.0], [7.0, 7.0, 10.0]),
            2.0 * PI * 2.0 * 10.0,
        ),
    ]
}

fn run<K: Kernel>(k: &mut K, f: &Fixture) -> Result<Measure, KernelError> {
    let stock = k.make_box(f.stock.0, f.stock.1)?;
    let tool = match f.tool {
        Tool::Box(o, s) => k.make_box(o, s)?,
        Tool::Cyl(b, r, h) => k.make_cylinder(b, r, h)?,
    };
    let mut r = match f.op {
        Op::Subtract => k.boolean_subtract(stock, tool)?,
        Op::Union => k.boolean_union(stock, tool)?,
        Op::Intersect => k.boolean_intersect(stock, tool)?,
    };
    k.measure(&mut r)
}

fn bench(f: &Fixture, truck: &mut TruckKernel) -> Vec<String> {
    let mut findings = Vec::new();
    let name = f.want.name;
    let mut sc = SolidcoreKernel::default();
    let sc_tol = REL_FLOOR * f.want.volume;
    let t = truck.mesh_tol + truck.boolean_tol;
    let tr_vol_tol = (f.curved_area * t).max(sc_tol);
    let tr_box_tol = if f.curved_area > 0.0 { t } else { 1e-6 };
    let sc_m = run(&mut sc, f);
    let tr_m = run(truck, f);
    match &sc_m {
        Ok(m) => findings.extend(verdict("solidcore", m, &f.want, sc_tol, 1e-6).err()),
        Err(e) => findings.push(format!("{name}: solidcore error: {e}")),
    }
    match &tr_m {
        Ok(m) => findings.extend(verdict("truck", m, &f.want, tr_vol_tol, tr_box_tol).err()),
        Err(e) => findings.push(format!("{name}: truck error: {e}")),
    }
    if let (Ok(a), Ok(b)) = (&sc_m, &tr_m) {
        // The cross-engine check: within the looser (truck) budget.
        let dv = (a.volume - b.volume).abs();
        println!("{name:<26} cross     |V_sc - V_truck| = {dv:.3e}");
        if dv > tr_vol_tol + sc_tol || a.bbox_deviation(b) > tr_box_tol + 1e-6 {
            findings.push(format!("{name}: engines disagree: solidcore {a:?} vs truck {b:?}"));
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
    let a = 10.0_f64;
    let cases = [
        // Steiner: V + S d + (pi d^2 / 4) * sum(edges) + 4/3 pi d^3
        (
            1.0,
            Want {
                name: "box-offset-out-1",
                volume: a.powi(3) + 6.0 * a * a + PI / 4.0 * 12.0 * a + 4.0 / 3.0 * PI,
                bbox: ([-1.0; 3], [11.0; 3]),
            },
        ),
        (
            -1.0,
            Want {
                name: "box-offset-in-1",
                volume: 8.0_f64.powi(3),
                bbox: ([1.0; 3], [9.0; 3]),
            },
        ),
    ];
    let mut truck = TruckKernel::default();
    let mut tb = truck.make_box([0.0; 3], [a; 3]).ok();
    if let Some(b) = tb.as_mut() {
        assert!(matches!(truck.offset(b, 1.0), Err(KernelError::Unsupported { .. })));
    }
    let mut findings = Vec::new();
    for (d, w) in cases {
        let mut sc = SolidcoreKernel::default();
        let m = sc
            .make_box([0.0; 3], [a; 3])
            .and_then(|mut b| sc.offset(&mut b, d))
            .and_then(|mut r| sc.measure(&mut r));
        match m {
            Ok(m) => findings.extend(verdict("solidcore", &m, &w, REL_FLOOR * w.volume, 1e-6).err()),
            Err(e) => findings.push(format!("{}: solidcore error: {e}", w.name)),
        }
    }
    assert!(findings.is_empty(), "offset findings:\n{}", findings.join("\n"));
}

/// Offset of a holed plate (the hole grows as the walls shrink).
#[test]
#[ignore = "heavy lane: offset of a boolean result (cylinder-through-box, inset 0.5)"]
fn holed_box_inset_matches_closed_form() {
    let mut sc = SolidcoreKernel::default();
    let w = Want {
        name: "holed-box-inset-0.5",
        volume: 9.0_f64.powi(3) - PI * 2.5 * 2.5 * 9.0,
        bbox: ([0.5; 3], [9.5; 3]),
    };
    let m = (|| {
        let s = sc.make_box([0.0; 3], [10.0; 3])?;
        let c = sc.make_cylinder([5.0, 5.0, -1.0], 2.0, 12.0)?;
        let mut holed = sc.boolean_subtract(s, c)?;
        let mut r = sc.offset(&mut holed, -0.5)?;
        sc.measure(&mut r)
    })();
    match m {
        Ok(m) => {
            let r = verdict("solidcore", &m, &w, REL_FLOOR * w.volume, 1e-6);
            assert!(r.is_ok(), "{r:?}");
        }
        Err(e) => panic!("holed-box-inset-0.5: solidcore error: {e}"),
    }
}
