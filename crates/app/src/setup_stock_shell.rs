//! The incoming stock's bottom and walls (#406, hand check of #519): the
//! body's display sheet is its TOP only, so on its own it lay buried in
//! the part (a part spanning the stock's height hid it entirely). This
//! closes it into a block: a floor quad under every material column and
//! a wall wherever a column's [floor, top] is not covered by its
//! neighbour's - the outline and every step of a cut.
//!
//! Provenance: our own geometry over the column body (`carry::Body`).

use crate::geometry::{Point3, Triangle};
use crate::stock_sim::Body;

/// The floor and wall triangles of `body`, in the body's own frame.
#[must_use]
pub fn floor_and_walls(body: &Body) -> Vec<Triangle> {
    let d = &body.dex;
    let (nx, ny, h) = (d.nx, d.ny, d.cell * 0.5);
    let span = |k: usize| -> Option<(f64, f64)> {
        if body.empty_at(k) {
            return None;
        }
        let (t, f) = (d.top_at(k), body.floor_at(k));
        (t.is_finite() && f.is_finite() && t > f).then_some((f, t))
    };
    let mut out = Vec::new();
    for iy in 0..ny {
        for ix in 0..nx {
            let k = ix + iy * nx;
            let Some((f, t)) = span(k) else { continue };
            let c = d.center(ix, iy);
            let (x0, x1, y0, y1) = (c.x - h, c.x + h, c.y - h, c.y + h);
            // Floor, facing down.
            quad(&mut out, [x0, y0, f], [x0, y1, f], [x1, y1, f], [x1, y0, f]);
            // Walls toward each neighbour: the part of [f, t] the
            // neighbour's material does not cover.
            let n = |dx: isize, dy: isize| -> Option<(f64, f64)> {
                let (jx, jy) = (ix.checked_add_signed(dx)?, iy.checked_add_signed(dy)?);
                (jx < nx && jy < ny).then(|| span(jx + jy * nx)).flatten()
            };
            for (dir, (a, b)) in [
                (n(-1, 0), ([x0, y1], [x0, y0])),
                (n(1, 0), ([x1, y0], [x1, y1])),
                (n(0, -1), ([x0, y0], [x1, y0])),
                (n(0, 1), ([x1, y1], [x0, y1])),
            ] {
                for (lo, hi) in uncovered((f, t), dir) {
                    quad(&mut out, [a[0], a[1], lo], [b[0], b[1], lo], [b[0], b[1], hi], [a[0], a[1], hi]);
                }
            }
        }
    }
    out
}

/// The parts of `own` = [f, t] outside the neighbour's span.
fn uncovered(own: (f64, f64), other: Option<(f64, f64)>) -> Vec<(f64, f64)> {
    let (f, t) = own;
    let Some((nf, nt)) = other else { return vec![(f, t)] };
    let mut v = Vec::new();
    if nf > f {
        v.push((f, nf.min(t)));
    }
    if nt < t {
        v.push((nt.max(f), t));
    }
    v.retain(|(a, b)| b > a);
    v
}

fn quad(out: &mut Vec<Triangle>, a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) {
    let p = |q: [f64; 3]| Point3::new(q[0], q[1], q[2]);
    out.push(Triangle::new(p(a), p(b), p(c)));
    out.push(Triangle::new(p(a), p(c), p(d)));
}
