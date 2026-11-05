//! M7.3, both backends side by side: every committed golden program
//! runs through the column engine AND the field engine, and the two
//! rest/gouge/deviation verdicts must agree within the field's
//! declared epsilon (`FIELD_VERDICT_EPS`, the march's solver floor).
//! A disagreement is a located finding - which program, which part,
//! which column, by how much - collected and printed whole before the
//! suite fails; nothing is rounded away (the M4.2b dome-golden law).
//! Heavy lane: both engines at full size over 21 programs.
//! Provenance: our own golden programs, engines and parity law - the
//! M7.1 C parity suite (camcraft-sim) generalized to the goldens.
//!
//! Gate map (M4.4): covers app::stock_sim (both arms of the seam).

mod common;

use common::engine_fixtures::{programs, Program};

use camcraft_app::geometry::Triangle;
use camcraft_app::stock_sim::{Backend, Deviation, SimOp, Stock, DEV_TOL, FIELD_BAND, FIELD_VERDICT_EPS};

/// The lattice the heavy lanes judge on (the cost suite's too).
const CELL: f64 = 0.5;

/// A column's verdict word, as `deviate` draws its line.
fn class(v: f64, tol: f64) -> &'static str {
    if v.is_nan() {
        "no verdict"
    } else if v > tol {
        "rest"
    } else if v < -tol {
        "gouge"
    } else {
        "clean"
    }
}

/// Every way two verdicts can differ, said with its location.
fn compare(pg: &Program, label: &str, x0: f64, y0: f64, c: &Deviation, f: &Deviation) -> (Vec<String>, f64) {
    let at = format!("{} / {label}", pg.file);
    let mut out = Vec::new();
    if (c.cells_x, c.cells_y) != (f.cells_x, f.cells_y) || c.cell != f.cell {
        out.push(format!(
            "{at}: lattice {}x{} @ {} vs field {}x{} @ {}",
            c.cells_x, c.cells_y, c.cell, f.cells_x, f.cells_y, f.cell
        ));
        return (out, f64::NAN);
    }
    let mut worst = 0.0f64;
    for (k, (a, b)) in c.dev.iter().zip(&f.dev).enumerate() {
        let (ix, iy) = (k % c.cells_x, k / c.cells_x);
        let (x, y) = (x0 + (ix as f64 + 0.5) * c.cell, y0 + (iy as f64 + 0.5) * c.cell);
        let d = (a - b).abs();
        if a.is_finite() && b.is_finite() {
            worst = worst.max(d);
        }
        let (ca, cb) = (class(*a, c.tol), class(*b, f.tol));
        if a.is_nan() != b.is_nan() || (a.is_finite() && d > FIELD_VERDICT_EPS) || ca != cb {
            out.push(format!(
                "{at}: column ({x:.3}, {y:.3}): column dev {a:.6} ({ca}) vs field dev {b:.6} ({cb}), |delta| {d:.3e}"
            ));
        }
    }
    let a2 = c.cell * c.cell;
    let vol_eps = FIELD_VERDICT_EPS * a2 * c.dev.len() as f64;
    for (name, a, b, e) in [
        ("max_rest", c.max_rest, f.max_rest, FIELD_VERDICT_EPS),
        ("max_gouge", c.max_gouge, f.max_gouge, FIELD_VERDICT_EPS),
        ("rest_volume", c.rest_volume, f.rest_volume, vol_eps),
        ("gouge_volume", c.gouge_volume, f.gouge_volume, vol_eps),
    ] {
        // NaN on either side is a disagreement too, never a pass.
        if (a - b).abs().is_nan() || (a - b).abs() > e {
            out.push(format!("{at}: {name} column {a:.6} vs field {b:.6}, |delta| {:.3e} > {e:.3e}", (a - b).abs()));
        }
    }
    if (c.rest_cells, c.gouge_cells) != (f.rest_cells, f.gouge_cells) || c.is_clean() != f.is_clean() {
        out.push(format!(
            "{at}: counts rest {} / gouge {} / clean {} vs field rest {} / gouge {} / clean {}",
            c.rest_cells, c.gouge_cells, c.is_clean(), f.rest_cells, f.gouge_cells, f.is_clean()
        ));
    }
    (out, worst)
}

/// One engine's run; a refusal is a finding (the backend-independence
/// law: a program that runs on one arm must run on the other).
fn run(pg: &Program, b: Backend) -> Result<Stock, String> {
    let (x0, y0, x1, y1) = pg.stock_box(CELL);
    let mut s = Stock::block(b, x0, y0, x1, y1, pg.top, CELL).map_err(|e| format!("{b:?} block: {e}"))?;
    let ops = [SimOp { moves: &pg.moves, radius: pg.radius, ball: pg.ball, cone: None }];
    s.simulate(&ops).map_err(|e| format!("{b:?} simulate: {e}"))?;
    Ok(s)
}

#[test]
#[ignore = "heavy lane (M7.3 verdict agreement): both stock engines over every golden program, release ~minutes - run at milestone close with -- --ignored"]
fn both_engines_judge_every_golden_program_alike() {
    let progs = programs().expect("the goldens parse");
    let mut findings: Vec<String> = Vec::new();
    println!("program / target | judged columns | rest | gouge | clean | max |delta| | findings");
    for pg in &progs {
        let (x0, y0, _, _) = pg.stock_box(CELL);
        let (col, fld) = match (run(pg, Backend::Column), run(pg, Backend::Field { band: FIELD_BAND })) {
            (Ok(c), Ok(f)) => (c, f),
            (c, f) => {
                let say = |r: &Result<Stock, String>| match r {
                    Ok(_) => "ran".to_string(),
                    Err(e) => e.clone(),
                };
                let msg = format!("{}: the engines did not both run - column: {}; field: {}", pg.file, say(&c), say(&f));
                println!("{msg}");
                findings.push(msg);
                continue;
            }
        };
        let targets: Vec<(String, Vec<Triangle>)> = pg.targets(CELL);
        for (label, part) in &targets {
            let c = col.deviate(part, DEV_TOL).expect("column deviate");
            let f = fld.deviate(part, DEV_TOL).expect("field deviate");
            let (found, worst) = compare(pg, label, x0, y0, &c, &f);
            println!(
                "{} / {label} | {} | {} | {} | {} | {worst:.3e} | {}",
                pg.file,
                c.dev.iter().filter(|v| v.is_finite()).count(),
                c.rest_cells,
                c.gouge_cells,
                c.is_clean(),
                found.len()
            );
            findings.extend(found);
        }
    }
    for f in &findings {
        println!("FINDING {f}");
    }
    assert!(findings.is_empty(), "{} located disagreement(s) between the engines - see FINDING lines", findings.len());
}
