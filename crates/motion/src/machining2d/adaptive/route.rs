//! A routed microlift traverse (M11.4 C round 4, `lift`): where the
//! straight chord between two pieces would sweep standing stock (the
//! strip the walls keep until their last lap, an island), the return
//! is routed through the cleared area instead - the free cells are the
//! cutter centers whose disc holds no stock and no wall, read off the
//! Euclidean distance transform of the planner's stock (Felzenszwalb &
//! Huttenlocher 2012, "Distance transforms of sampled functions",
//! Theory of Computing 8), the route the shortest 8-connected path
//! between them (A*, Hart, Nilsson & Raphael 1968), pulled taut to
//! straight legs each proved on the stock itself (the classical
//! string-pulling of grid paths). Ours; every bound is ours.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::geo2d::Point2;

use super::raster::Raster;

/// Cells further than this from an end (mm) are not tried as its door.
const DOOR: f64 = 6.0;

/// Squared distance, in cells, of every cell center to the nearest
/// stock or wall cell (`f64::INFINITY` with none on the grid).
pub(super) fn clearance(ras: &Raster) -> Vec<f64> {
    let (nx, ny) = ras.dims();
    let big = 1e30;
    let mut g: Vec<f64> = (0..nx * ny).map(|i| if ras.blocked(i) { 0.0 } else { big }).collect();
    let n = nx.max(ny);
    let (mut f, mut d, mut v, mut z) = (vec![0.0; n], vec![0.0; n], vec![0usize; n], vec![0.0; n + 1]);
    for x in 0..nx {
        for y in 0..ny {
            f[y] = g[x + y * nx];
        }
        edt_1d(&f[..ny], &mut d[..ny], &mut v, &mut z);
        for y in 0..ny {
            g[x + y * nx] = d[y];
        }
    }
    for y in 0..ny {
        f[..nx].copy_from_slice(&g[y * nx..(y + 1) * nx]);
        edt_1d(&f[..nx], &mut d[..nx], &mut v, &mut z);
        g[y * nx..(y + 1) * nx].copy_from_slice(&d[..nx]);
    }
    g.iter().map(|&q| if q >= 1e29 { f64::INFINITY } else { q }).collect()
}

/// The 1-D squared distance transform of `f` (lower envelope of
/// parabolas, Felzenszwalb & Huttenlocher).
fn edt_1d(f: &[f64], d: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        let fq = f[q] + (q * q) as f64;
        loop {
            let p = v[k];
            let s = (fq - (f[p] + (p * p) as f64)) / (2.0 * (q as f64 - p as f64));
            // z[0] is minus infinity: k never drops below 0.
            if s <= z[k] && k > 0 {
                k -= 1;
                continue;
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = f64::INFINITY;
            break;
        }
    }
    k = 0;
    for (q, dq) in d.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        *dq = (q as f64 - p as f64).powi(2) + f[p];
    }
}

/// The via points of a traverse from `a` to `b` whose every leg passes
/// `leg_ok`, through cells clear by `rad` (mm) on `clear` ([`clearance`])
/// and more than `gap` off the `orbit` circle; `None` where no route is
/// found.
pub(super) fn route(
    ras: &Raster,
    clear: &[f64],
    a: Point2,
    b: Point2,
    (rad, orbit, gap): (f64, (Point2, f64), f64),
    leg_ok: &dyn Fn(Point2, Point2) -> bool,
) -> Option<Vec<Point2>> {
    let (nx, ny) = ras.dims();
    let need = (rad / ras.h + 0.75).powi(2);
    // Free: clear of stock and wall, and off the entry orbit's circle.
    let off = |q: Point2| orbit.1 <= 1e-6 || ((q.x - orbit.0.x).hypot(q.y - orbit.0.y) - orbit.1).abs() > gap + ras.h;
    let free = |i: usize| clear.get(i).is_some_and(|&c| c >= need) && off(ras.center(i));
    let door = |p: Point2| -> Option<usize> {
        let (cx, cy) = ras.cell_of(p)?;
        let reach = (DOOR / ras.h).ceil() as isize;
        let mut best: Option<(f64, usize)> = None;
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let (x, y) = (cx as isize + dx, cy as isize + dy);
                if x < 0 || y < 0 || x >= nx as isize || y >= ny as isize {
                    continue;
                }
                let i = x as usize + y as usize * nx;
                let d = ((dx * dx + dy * dy) as f64).sqrt();
                if free(i) && best.is_none_or(|b| d < b.0) && d * ras.h <= DOOR && leg_ok(p, ras.center(i)) {
                    best = Some((d, i));
                }
            }
        }
        best.map(|b| b.1)
    };
    let (s, t) = (door(a)?, door(b)?);
    let cells = astar(nx, ny, s, t, &free)?;
    let mut path: Vec<Point2> = vec![a];
    path.extend(cells.iter().map(|&i| ras.center(i)));
    path.push(b);
    // Pulled taut: from each kept point, the farthest point a legal leg
    // reaches.
    let mut via = Vec::new();
    let mut i = 0usize;
    while i + 1 < path.len() {
        let mut j = i + 1;
        let mut k = path.len() - 1;
        while k > j {
            if leg_ok(path[i], path[k]) {
                j = k;
                break;
            }
            k = j + (k - j) / 2;
        }
        if !leg_ok(path[i], path[j]) {
            return None;
        }
        if j + 1 < path.len() {
            via.push(path[j]);
        }
        i = j;
    }
    Some(via)
}

/// Shortest 8-connected path of free cells from `s` to `t` (both ends
/// included), A* on the Euclidean metric; capped at one visit a cell.
fn astar(nx: usize, ny: usize, s: usize, t: usize, free: &dyn Fn(usize) -> bool) -> Option<Vec<usize>> {
    let n = nx * ny;
    let (tx, ty) = ((t % nx) as f64, (t / nx) as f64);
    let h = |i: usize| ((i % nx) as f64 - tx).hypot((i / nx) as f64 - ty);
    let key = |c: f64| Reverse((c * 1024.0) as u64);
    let mut g = vec![f64::INFINITY; n];
    let mut from = vec![usize::MAX; n];
    let mut heap = BinaryHeap::new();
    g[s] = 0.0;
    heap.push((key(h(s)), s));
    let mut visits = 0usize;
    while let Some((_, i)) = heap.pop() {
        if i == t {
            let mut out = vec![t];
            let mut c = t;
            while c != s && from[c] != usize::MAX {
                c = from[c];
                out.push(c);
            }
            out.reverse();
            return Some(out);
        }
        visits += 1;
        if visits > n {
            return None;
        }
        let (x, y) = ((i % nx) as isize, (i / nx) as isize);
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let (qx, qy) = (x + dx, y + dy);
            if qx < 0 || qy < 0 || qx >= nx as isize || qy >= ny as isize {
                continue;
            }
            let q = qx as usize + qy as usize * nx;
            if !free(q) {
                continue;
            }
            let ng = g[i] + if dx != 0 && dy != 0 { std::f64::consts::SQRT_2 } else { 1.0 };
            if ng < g[q] {
                g[q] = ng;
                from[q] = i;
                heap.push((key(ng + h(q)), q));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_distance_transform_is_the_brute_force_one() {
        let f: Vec<f64> = (0..40).map(|i| if i % 13 == 3 || i == 37 { 0.0 } else { 1e30 }).collect();
        let (mut d, mut v, mut z) = (vec![0.0; 40], vec![0usize; 40], vec![0.0; 41]);
        edt_1d(&f, &mut d, &mut v, &mut z);
        for (q, &dq) in d.iter().enumerate() {
            let brute = (0..40).filter(|&p| f[p] == 0.0).map(|p| (q as f64 - p as f64).powi(2)).fold(f64::INFINITY, f64::min);
            assert!((dq - brute).abs() < 1e-9, "cell {q}: {dq} vs {brute}");
        }
    }
}
