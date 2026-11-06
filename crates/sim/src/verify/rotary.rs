//! The radial field law of `Mode::Rotary` (M13): rest material read in
//! the frame a turned work offers the spindle. Every height-field law
//! of [`super::engine`] stands its columns on world Z, which is the
//! right reading for a tool that comes down from above and the wrong
//! one for a tool lying RADIAL beside a bar: the ball clears the
//! radial column outward from its tip, not the vertical one above it.
//! Here the material is a field over the developed surface - azimuth
//! `θ` and station `v` about the fitted axis - holding, per ray out of
//! the axis, the outermost radius `M(θ, v)` that still stands (the
//! stock's, the part's where no stock is stated) and the part's own
//! `P(θ, v)`, which no cut ever lowers. Material is read SOLID from the
//! axis out to `M` along each ray: a bored or off-axis stock is taken
//! fuller than it is (an over-report, never a hide).
//!
//! The path is replayed in order, each move sampled along its length
//! with the pose the machine holds there - the start on the pose held
//! coming in (the post keeps the tilt modal; a move stating nothing
//! keeps it), the end on the one it states, the letters between lerped
//! as the controller lerps them ([`crate::cl::pose`]). A cutting
//! sample first CARVES: on every ray its flute (the ball, the flute
//! cylinder above it) enters below `M` and leaves at or beyond it,
//! `M` drops to where the flute entered - never below `P`. A flute
//! buried along its ray (cut deeper than its length) carves nothing,
//! so the shank behind it meets what stands. Then the shank and
//! holder bands are checked: a band entering below `M` (less `allow`)
//! is a [`Warning::ShankHit`] - `part` when below `P` too - and, on a
//! move that turns, the coordinated samples clean but a sample off
//! the diagonal of (path fraction x turn fraction) striking is the
//! [`Warning::SweepHit`] of law (c3): the head may arrive before or
//! after the table. A rapid carves nothing; any atom of the assembly
//! below `M` less `rapid_allow` (the op's own finish budget: a rapid
//! sliding along the skin its cuts left grazes the scallops, nothing
//! more) is a [`Warning::RapidInStock`]. One warning per move.
//!
//! Limits, stated: the field is sampled at cell centres (rays between
//! them unseen - the cell is the law's resolution), ray-mesh
//! intersection is exact, and the tool's tip shapes are read by their
//! enclosing solids when checking (a bull or point cone as its
//! cylinder) and their enclosed ones when carving (a bull's flat
//! core, a cone not at all) - both directions err toward a warning.
//!
//! Provenance: the developed (unwrapped) surface of a turned work and
//! radial rest-material reading from public 4-axis / mill-turn CAM
//! literature; the ray geometry is [`super::rotary_geom`]'s (sources
//! cited there). Our own design; no vendor source consulted.

use crate::cl::{angles_to_axis, axis_to_angles, MoveKind, Toolpath};
use crate::geometry::{Point3, Triangle};

use super::probe::{Atom, Cutter};
use super::rotary_geom::{add_s, cross, dot, p3, ray_tri, shapes, sub, unit, Shape, V3};
use super::types::Warning;

/// The fitted axis of the work, in the path's own (part) frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RotaryAxis {
    pub origin: Point3,
    pub dir: [f64; 3],
}

/// How the radial field law samples and what it excuses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RotaryLaw {
    /// Field cell edge on the material's widest skin, mm.
    pub cell: f64,
    /// Sample spacing along a move, mm.
    pub step: f64,
    /// Penetration excused for the bands, mm (the gouge tolerance).
    pub allow: f64,
    /// Penetration excused for a rapid's flute, mm (the finish budget).
    pub rapid_allow: f64,
}

/// Turn fractions sampled on a move that changes pose (endpoints
/// included - the head may finish its arc before the table arrives).
const TURN_FRACS: [f64; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];
/// Path fractions the off-diagonal (turn) grid samples.
const GRID_PATH: [f64; 5] = [0.2, 0.4, 0.6, 0.8, 1.0];
/// Field size cap: the cell grows until the grid fits.
const MAX_CELLS: usize = 2_000_000;
/// Samples per move cap (a move longer than this many steps is sampled
/// coarser, never in an unbounded loop).
const MAX_SAMPLES: usize = 2048;

/// The radial field law over one path; ascending, one warning per move.
/// A degenerate axis fails closed: a [`Warning::Gouge`] at the first
/// cutting move, never a pass.
pub fn rotary_hits(
    path: &Toolpath,
    ax: &RotaryAxis,
    stock: Option<&[Triangle]>,
    part: &[Triangle],
    cutter: &Cutter,
    law: &RotaryLaw,
) -> Vec<Warning> {
    let first_cut = path.moves.iter().position(|m| m.kind != MoveKind::Rapid);
    let fail = || first_cut.map(|at| vec![Warning::Gouge { at }]).unwrap_or_default();
    let atoms = cutter.atoms();
    let Some(mut f) = Field::build(ax, stock, part, law.cell) else {
        return fail();
    };
    if atoms.is_empty() || !(law.step.is_finite() && law.step > 0.0) {
        return fail();
    }
    let fl = cutter.flute_len;
    let mut out = Vec::new();
    let mut at = path.moves.first().map_or(Point3::ZERO, |m| m.from);
    // Letters; the first move starts on its own pose - the path holds
    // no record of the approach that brought the head there (the
    // post's own positioning before the first block), so no turn is
    // invented for it.
    let mut held = path.moves.first().and_then(|m| m.axis).map_or([0.0, 0.0, 0.0f64], axis_to_angles);
    for (i, m) in path.moves.iter().enumerate() {
        if !(m.from.is_finite() && m.to.is_finite()) {
            continue; // the NonFinite law owns it
        }
        let from_pose = held;
        let to_pose = m.axis.map_or(held, axis_to_angles);
        let from = if i == 0 { m.from } else { at };
        let turns = from_pose != to_pose;
        let len = dist(from, m.to);
        let dth = (to_pose[0] - from_pose[0]).abs().max((to_pose[1] - from_pose[1]).abs());
        let n = ((len / law.step).ceil() as usize).max((dth / 2.0).ceil() as usize).clamp(1, MAX_SAMPLES);
        let air = m.kind == MoveKind::Rapid;
        let allow = if air { law.rapid_allow } else { law.allow };
        let mut hit: Option<bool> = None;
        let k0 = usize::from(i != 0);
        for k in k0..=n {
            let s = k as f64 / n as f64;
            let (tip, a) = (lerp(from, m.to, s), pose(from_pose, to_pose, s));
            if !air {
                f.carve(&atoms, tip, a, fl);
            }
            if let Some(p) = f.check(&atoms, tip, a, if air { 0.0 } else { fl }, allow) {
                hit = Some(hit.unwrap_or(false) || p);
            }
        }
        let mut swept: Option<bool> = None;
        if hit.is_none() && turns {
            for &s in &GRID_PATH {
                for &t in &TURN_FRACS {
                    let (tip, a) = (lerp(from, m.to, s), pose(from_pose, to_pose, t));
                    if let Some(p) = f.check(&atoms, tip, a, if air { 0.0 } else { fl }, allow) {
                        swept = Some(swept.unwrap_or(false) || p);
                    }
                }
            }
        }
        match (air, hit, swept) {
            (true, Some(_), _) | (true, None, Some(_)) => out.push(Warning::RapidInStock { at: i }),
            (false, Some(part), _) => out.push(Warning::ShankHit { at: i, part }),
            (false, None, Some(part)) => out.push(Warning::SweepHit { at: i, part }),
            _ => {}
        }
        at = m.to;
        held = to_pose;
    }
    out
}

/// The axis the letters `deg` hold, lerped `s` of the way from `a` to
/// `b` - each letter on its own straight line, as the controller runs
/// them. Upright when the letters say nothing a vector can hold.
fn pose(a: [f64; 3], b: [f64; 3], s: f64) -> [f64; 3] {
    let l = [a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s, a[2] + (b[2] - a[2]) * s];
    angles_to_axis(l).unwrap_or([0.0, 0.0, 1.0])
}

fn lerp(a: Point3, b: Point3, s: f64) -> Point3 {
    Point3::new(a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s, a.z + (b.z - a.z) * s)
}

fn dist(a: Point3, b: Point3) -> f64 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2) + (b.z - a.z).powi(2)).sqrt()
}

/// The developed field: rows of stations, columns of azimuth.
struct Field {
    o: V3,
    u: V3,
    e1: V3,
    e2: V3,
    v0: f64,
    dv: f64,
    nv: usize,
    dth: f64,
    nth: usize,
    cs: Vec<(f64, f64)>,
    m: Vec<f64>,
    p: Vec<f64>,
    top: f64,
}

impl Field {
    fn build(ax: &RotaryAxis, stock: Option<&[Triangle]>, part: &[Triangle], cell: f64) -> Option<Self> {
        let u = unit(ax.dir)?;
        let o = p3(ax.origin);
        if !o.iter().all(|x| x.is_finite()) || !(cell.is_finite() && cell > 0.0) {
            return None;
        }
        let any = if u[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
        let e1 = unit(cross(u, any))?;
        let e2 = cross(u, e1);
        let all = stock.unwrap_or(&[]).iter().chain(part.iter());
        let (mut vlo, mut vhi, mut rmax) = (f64::INFINITY, f64::NEG_INFINITY, 0.0f64);
        for t in all {
            for q in [t.a, t.b, t.c] {
                let d = sub(p3(q), o);
                if !d.iter().all(|x| x.is_finite()) {
                    return None;
                }
                let v = dot(d, u);
                vlo = vlo.min(v);
                vhi = vhi.max(v);
                rmax = rmax.max(dot(add_s(d, u, -v), add_s(d, u, -v)).sqrt());
            }
        }
        let mut f = Field { o, u, e1, e2, v0: 0.0, dv: 1.0, nv: 0, dth: 1.0, nth: 0, cs: Vec::new(), m: Vec::new(), p: Vec::new(), top: 0.0 };
        if vhi < vlo || rmax <= 0.0 {
            return Some(f); // no material: nothing to meet
        }
        let mut cell = cell;
        let (nth, nv) = loop {
            let nth = ((std::f64::consts::TAU * rmax / cell).ceil() as usize).clamp(16, 8192);
            let nv = (((vhi - vlo) / cell).ceil() as usize).clamp(1, 8192);
            if nth.saturating_mul(nv) <= MAX_CELLS {
                break (nth, nv);
            }
            cell *= 1.5;
        };
        f.v0 = vlo;
        f.dv = (vhi - vlo) / nv as f64;
        f.nv = nv;
        f.nth = nth;
        f.dth = std::f64::consts::TAU / nth as f64;
        f.cs = (0..nth).map(|j| ((j as f64 + 0.5) * f.dth).sin_cos()).collect();
        let stock_m = stock.map(|s| f.cast(s));
        f.p = f.cast(part);
        f.m = match stock_m {
            Some(s) => s.iter().zip(&f.p).map(|(a, b)| a.max(*b)).collect(),
            None => f.p.clone(),
        };
        f.top = f.m.iter().copied().fold(0.0, f64::max);
        Some(f)
    }

    fn ray(&self, iv: usize, j: usize) -> (V3, V3) {
        let v = self.v0 + (iv as f64 + 0.5) * self.dv;
        let (s, c) = self.cs[j];
        let d = [self.e1[0] * c + self.e2[0] * s, self.e1[1] * c + self.e2[1] * s, self.e1[2] * c + self.e2[2] * s];
        (add_s(self.o, self.u, v), d)
    }

    /// The outermost radius each cell's ray meets the mesh at, 0 where
    /// it meets nothing. Facets bucket by station row.
    fn cast(&self, tris: &[Triangle]) -> Vec<f64> {
        let mut rows: Vec<Vec<usize>> = vec![Vec::new(); self.nv];
        for (k, t) in tris.iter().enumerate() {
            let vs = [t.a, t.b, t.c].map(|q| dot(sub(p3(q), self.o), self.u));
            let (lo, hi) = (vs[0].min(vs[1]).min(vs[2]), vs[0].max(vs[1]).max(vs[2]));
            let (a, b) = (self.row(lo), self.row(hi));
            for row in rows.iter_mut().take(b + 1).skip(a) {
                row.push(k);
            }
        }
        let mut out = vec![0.0; self.nv * self.nth];
        for (iv, row) in rows.iter().enumerate() {
            for j in 0..self.nth {
                let (o, d) = self.ray(iv, j);
                let mut best = 0.0f64;
                for &k in row {
                    if let Some(t) = ray_tri(o, d, &tris[k]) {
                        best = best.max(t);
                    }
                }
                out[iv * self.nth + j] = best;
            }
        }
        out
    }

    fn row(&self, v: f64) -> usize {
        let r = ((v - self.v0) / self.dv).floor();
        if r.is_finite() { r.clamp(0.0, (self.nv.max(1) - 1) as f64) as usize } else { 0 }
    }

    /// The cells whose rays may meet `s`, or none when its nearest
    /// point stands outside every standing radius.
    fn cells(&self, s: &Shape, mut each: impl FnMut(usize, usize)) {
        if self.nv == 0 || self.nth == 0 {
            return;
        }
        let (a, b, r) = s.ends();
        let (wa, wb) = (sub(a, self.o), sub(b, self.o));
        let (va, vb) = (dot(wa, self.u), dot(wb, self.u));
        let (pa, pb) = (add_s(wa, self.u, -va), add_s(wb, self.u, -vb));
        let dp = sub(pb, pa);
        let tt = if dot(dp, dp) > 1e-18 { (-dot(pa, dp) / dot(dp, dp)).clamp(0.0, 1.0) } else { 0.0 };
        let near = add_s(pa, dp, tt);
        let rho_min = dot(near, near).sqrt();
        if !rho_min.is_finite() || rho_min - r > self.top {
            return;
        }
        let vlo = va.min(vb) - r;
        let vhi = va.max(vb) + r;
        if vhi < self.v0 || vlo > self.v0 + self.dv * self.nv as f64 {
            return;
        }
        let (i0, i1) = (self.row(vlo), self.row(vhi));
        let full = rho_min <= r * 1.000_001 + 1e-9;
        let cols: Vec<usize> = if full {
            (0..self.nth).collect()
        } else {
            let ang = |p: V3| dot(p, self.e2).atan2(dot(p, self.e1));
            let (t0, t1) = (ang(pa), ang(pb));
            let mut d = t1 - t0;
            while d > std::f64::consts::PI {
                d -= std::f64::consts::TAU;
            }
            while d < -std::f64::consts::PI {
                d += std::f64::consts::TAU;
            }
            let half = (r / rho_min).clamp(0.0, 1.0).asin() + self.dth;
            let (lo, hi) = (t0.min(t0 + d) - half, t0.max(t0 + d) + half);
            if hi - lo >= std::f64::consts::TAU {
                (0..self.nth).collect()
            } else {
                let j0 = (lo / self.dth).floor() as i64;
                let j1 = (hi / self.dth).ceil() as i64;
                let n = self.nth as i64;
                (j0..=j1).map(|j| j.rem_euclid(n) as usize).collect()
            }
        };
        for iv in i0..=i1 {
            for &j in &cols {
                each(iv, j);
            }
        }
    }

    /// The flute's pieces (ball, flute cylinder) are one solid: per ray
    /// their intervals are merged before the carve rule reads them, so
    /// the ball's reach below the cylinder is never lost.
    fn carve(&mut self, atoms: &[Atom], tip: Point3, a: V3, fl: f64) {
        let mut per: std::collections::HashMap<usize, Vec<(f64, f64)>> = std::collections::HashMap::new();
        for (s, flute, carve) in shapes(atoms, tip, a, fl) {
            if !(flute && carve) {
                continue;
            }
            self.cells(&s, |iv, j| {
                let (o, d) = self.ray(iv, j);
                if let Some(iv_) = s.interval(o, d) {
                    per.entry(iv * self.nth + j).or_default().push(iv_);
                }
            });
        }
        for (k, mut ivs) in per {
            ivs.sort_by(|x, y| x.0.total_cmp(&y.0));
            let mut runs: Vec<(f64, f64)> = Vec::with_capacity(ivs.len());
            for (lo, hi) in ivs {
                match runs.last_mut() {
                    Some(l) if lo <= l.1 + 1e-9 => l.1 = l.1.max(hi),
                    _ => runs.push((lo, hi)),
                }
            }
            let m = self.m[k];
            if let Some(&(tin, _)) = runs.iter().find(|(lo, hi)| *lo < m && *hi >= m - 1e-9) {
                self.m[k] = m.min(tin.max(self.p[k]));
            }
        }
    }

    /// Whether the assembly above `from_z` (the bands for a cut, the
    /// whole tool for a rapid) enters standing material: Some(part).
    fn check(&self, atoms: &[Atom], tip: Point3, a: V3, from_z: f64, allow: f64) -> Option<bool> {
        let mut hit: Option<bool> = None;
        for (s, flute, _) in shapes(atoms, tip, a, from_z.max(1e-12)) {
            if from_z > 0.0 && flute {
                continue; // a cut's flute is the cut itself
            }
            self.cells(&s, |iv, j| {
                if hit == Some(true) {
                    return;
                }
                let k = iv * self.nth + j;
                let (o, d) = self.ray(iv, j);
                if let Some((tin, _)) = s.interval(o, d)
                    && tin < self.m[k] - allow
                {
                    hit = Some(hit.unwrap_or(false) || tin < self.p[k] - allow);
                }
            });
            if hit == Some(true) {
                break;
            }
        }
        hit
    }
}

