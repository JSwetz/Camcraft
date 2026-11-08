//! `camcraft-sim` - headless stock simulation (M13.3): cuts a box of
//! stock with a CLFile's moves on the dexel grid and prints the removed
//! volume and a rapid/depth verdict. Provenance: dexel (height-field)
//! stock removal is general CAM verification practice from published
//! literature, as the `stock_sim` module states; the CLI is ours.

use camcraft_sim::cl::{text, MoveKind};
use camcraft_sim::stock_sim::{auto_cell, simulate, Dexel, SimOp};
use std::process::ExitCode;

const USAGE: &str = "\
camcraft-sim - CamCraft stock simulation (headless)

USAGE:
    camcraft-sim [OPTIONS] <file.cl> <x0> <y0> <z0> <x1> <y1> <z1> <tool-diameter>

Cuts the stock box x0..x1, y0..y1, z0..z1 (mm) with the CLFile's moves
using a flat end mill (or --ball) of the given diameter, then prints
the removed volume and the verdict.

EXIT:
    0  simulated, no flags
    1  bad input (unreadable or malformed file, nonsense numbers)
    2  simulated, verdict flagged (a rapid that travels sideways or
       descends below the stock top inside the stock footprint, or a
       cut below the stock bottom)

OPTIONS:
    --ball            ball-nose tool instead of a flat end mill
    --cell <mm>       grid cell size (default: from the tool and box)
    -h, --help        print this text
    --version         print the version
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut ball = false;
    let mut cell: Option<String> = None;
    let mut pos: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--version" => {
                println!("camcraft-sim {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--ball" => ball = true,
            "--cell" => match it.next() {
                Some(v) => cell = Some(v.clone()),
                None => return die("--cell needs a value".into()),
            },
            o if o.starts_with("--") => return die(format!("unknown option {o:?} (try --help)")),
            o => pos.push(o.to_string()),
        }
    }
    let [path, rest @ ..] = pos.as_slice() else {
        return die("expected a CLFile path and seven numbers (try --help)".into());
    };
    if rest.len() != 7 {
        return die("expected a CLFile path and seven numbers (try --help)".into());
    }
    let mut n = [0.0f64; 7];
    for (slot, s) in n.iter_mut().zip(rest) {
        match s.parse::<f64>() {
            Ok(v) if v.is_finite() => *slot = v,
            _ => return die(format!("not a finite number: {s:?}")),
        }
    }
    let [x0, y0, z0, x1, y1, z1, d] = n;
    if !(x1 > x0 && y1 > y0 && z1 > z0) {
        return die("stock box needs x0 < x1, y0 < y1, z0 < z1".into());
    }
    if d <= 0.0 {
        return die("tool diameter must be positive".into());
    }
    let r = d / 2.0;
    let cell = match cell.as_deref().map(str::parse::<f64>) {
        None => auto_cell(r, x1 - x0, y1 - y0),
        Some(Ok(c)) if c.is_finite() && c > 0.0 => c,
        Some(_) => return die("--cell must be a positive number".into()),
    };
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => return die(format!("cannot read {path}: {e}")),
    };
    let t = match text::from_text(&src) {
        Ok(t) => t,
        Err(e) => return die(format!("cannot parse {path}: {e}")),
    };
    let mut dex = match Dexel::block(x0, y0, x1, y1, z1, cell) {
        Ok(d) => d,
        Err(e) => return die(e.to_string()),
    };
    let op = SimOp { moves: &t.moves, radius: r, ball, cone: None, corner: None };
    if let Err(e) = simulate(&mut dex, &[op]) {
        return die(e.to_string());
    }
    // Removed volume: per column, the stock surface down to what stands,
    // floored at the stock bottom (the grid has no floor of its own).
    let a2 = dex.cell * dex.cell;
    let (mut removed, mut cut_cells, mut lowest) = (0.0f64, 0usize, z1);
    for k in 0..dex.nx * dex.ny {
        let top = dex.top_at(k);
        if top.is_finite() && top < z1 {
            removed += (z1 - top.max(z0)) * a2;
            cut_cells += 1;
            lowest = lowest.min(top);
        }
    }
    let inside = |x: f64, y: f64| x > x0 - r && x < x1 + r && y > y0 - r && y < y1 + r;
    let bad_rapids = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Rapid)
        .filter(|m| {
            // A straight-up retract leaves through the air it cut; a
            // rapid that travels sideways or descends below the stock
            // top inside the footprint may meet material at rapid feed.
            let sideways = m.from.x != m.to.x || m.from.y != m.to.y;
            let below = m.from.z.min(m.to.z) < z1;
            below
                && (sideways || m.to.z < m.from.z)
                && (inside(m.from.x, m.from.y) || inside(m.to.x, m.to.y))
        })
        .count();
    let through = lowest < z0;
    println!("file: {path}");
    println!("moves: {}", t.len());
    println!("grid: {} x {} cells of {cell:.4} mm", dex.nx, dex.ny);
    println!("stock volume: {:.3} mm^3", (x1 - x0) * (y1 - y0) * (z1 - z0));
    println!("removed volume: {removed:.3} mm^3");
    println!("cut cells: {cut_cells}");
    println!("lowest cut: {lowest:.4}");
    println!("rapids into the stock: {bad_rapids}");
    println!("cut below stock bottom: {through}");
    if bad_rapids > 0 || through {
        println!("verdict: FLAGGED");
        return ExitCode::from(2);
    }
    println!("verdict: ok (rapid and depth check only - no part, no gouge verdict)");
    ExitCode::SUCCESS
}

fn die(msg: String) -> ExitCode {
    eprintln!("camcraft-sim: {msg}");
    ExitCode::FAILURE
}
