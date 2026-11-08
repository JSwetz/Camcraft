//! The Hermite data dual contouring reads (M13.1, #23): per sign-changing
//! lattice edge, the zero crossing and the surface normal there, plus
//! whether the crossing can be trusted.
//! Provenance: the Hermite-data formulation of Ju, Losasso, Schaefer &
//! Warren, "Dual Contouring of Hermite Data" (SIGGRAPH 2002); the normal
//! and trust estimators are our own reading of two textbook properties
//! of a signed distance field - its gradient is the unit surface normal,
//! and |d| at a sample never exceeds the distance to the surface along
//! any path, a lattice edge included. Nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **A normal is read from its own face.** Central differences of the
//!   samples blend two faces' normals across the ridge an edge throws -
//!   the samples next to a corner straddle it. So a crossing's normal is
//!   read from the crossings of the parallel lattice edges beside it, the
//!   sheet as a height field over the edge's axis; a side whose parallel
//!   edge does not cross the same way is another face and is not read,
//!   and where the two sides disagree the flatter one is the edge's own
//!   face. With no neighbour on some axis the sample gradient answers,
//!   each endpoint weighted by how nearly it obeys |grad d| = 1.
//! - **A crossing next to a ridge is flagged, never trusted.** On a plane
//!   the two endpoint magnitudes sum to the edge length times |n_k|; a
//!   ridge endpoint reads the OTHER face's distance and the sum falls
//!   short, so the interpolated crossing is biased. Such an edge is
//!   `tight == false`: its crossing still places the cell's mass point,
//!   but its plane is replaced by trusted neighbours' (`dual`), and the
//!   normals are re-read from trusted neighbours only.
//! - **Total.** NaN reads as air; a normal that cannot be formed is
//!   `None`, never a made-up direction.

use std::collections::HashMap;

use super::iso::Grid;

type V3 = [f64; 3];

/// A lattice edge: its lower sample and its axis.
pub(crate) type EdgeKey = ([usize; 3], usize);

/// One sign-changing edge's Hermite datum.
pub(crate) struct Hermite {
    /// The zero crossing, world coordinates.
    pub(crate) p: V3,
    /// Unit normal, material to air.
    pub(crate) n: Option<V3>,
    /// The crossing is not biased by a ridge endpoint.
    pub(crate) tight: bool,
}

/// Slopes closer than this (cells per cell) are one smooth sheet.
const SLOPE_AGREE: f64 = 0.5;
/// A sheet steeper than this (cells per cell) reads as no slope at all.
const SLOPE_MAX: f64 = 1e3;
/// Slack in the eikonal weight of the gradient fallback.
const EIKONAL_EPS: f64 = 0.05;
/// How far the endpoint sum may fall short of a plane's before the
/// crossing counts as ridge-biased.
const TIGHT_TOL: f64 = 0.05;

pub(crate) fn inside(v: f64) -> bool {
    v < 0.0
}

/// Where d crosses zero on the edge from value `a` to `b`.
pub(crate) fn crossing(a: f64, b: f64) -> f64 {
    if (a - b).abs() > f64::EPSILON {
        (a / (a - b)).clamp(0.0, 1.0)
    } else {
        0.5
    }
}

fn unit(n: V3) -> Option<V3> {
    let len = n.iter().map(|c| c * c).sum::<f64>().sqrt();
    (len.is_finite() && len > 1e-12).then(|| n.map(|c| c / len))
}

/// Raw edge record: crossing parameter, lower end inside, |va| + |vb|.
#[derive(Clone, Copy)]
struct Raw {
    t: f64,
    lower_in: bool,
    sum: f64,
}

/// Every sign-changing lattice edge's Hermite datum.
pub(crate) fn edges(g: &Grid) -> HashMap<EdgeKey, Hermite> {
    let mut raw: HashMap<EdgeKey, Raw> = HashMap::new();
    for k in 0..=g.n[2] {
        for j in 0..=g.n[1] {
            for i in 0..=g.n[0] {
                let s = [i, j, k];
                for a in 0..3 {
                    if s[a] >= g.n[a] {
                        continue;
                    }
                    let mut e = s;
                    e[a] += 1;
                    let (va, vb) = (g.val(s), g.val(e));
                    if inside(va) != inside(vb) {
                        let sum = va.abs() + vb.abs();
                        raw.insert((s, a), Raw { t: crossing(va, vb), lower_in: inside(va), sum });
                    }
                }
            }
        }
    }
    let tight_of = |key: &EdgeKey, r: &Raw, n: Option<V3>| match n {
        Some(n) => r.sum.is_finite() && r.sum >= (1.0 - TIGHT_TOL) * g.step * n[key.1].abs(),
        None => false,
    };
    // Pass one: normals from every neighbour, and the trust they imply.
    let mut first: HashMap<EdgeKey, (Option<V3>, bool)> = HashMap::with_capacity(raw.len());
    for (key, r) in &raw {
        let n = sheet_normal(&raw, *key, r, |_| true).or_else(|| gradient_normal(g, *key, r.t));
        first.insert(*key, (n, tight_of(key, r, n)));
    }
    // Pass two: normals re-read from trusted neighbours only.
    let mut out = HashMap::with_capacity(raw.len());
    for (key, r) in &raw {
        let trusted = |nb: &EdgeKey| first.get(nb).is_some_and(|f| f.1);
        let prior = first.get(key).and_then(|f| f.0);
        let n = sheet_normal(&raw, *key, r, trusted).or(prior);
        let (s, k) = *key;
        let pa = g.at(s);
        let mut p = [pa.x, pa.y, pa.z];
        p[k] += r.t * g.step;
        out.insert(*key, Hermite { p, n, tight: tight_of(key, r, n) });
    }
    out
}

/// The sheet-slope normal of edge `key`; `None` when some cross axis
/// has no accepted neighbour crossing the same way.
fn sheet_normal(raw: &HashMap<EdgeKey, Raw>, key: EdgeKey, r: &Raw, accept: impl Fn(&EdgeKey) -> bool) -> Option<V3> {
    let (s, k) = key;
    let out = if r.lower_in { 1.0 } else { -1.0 };
    let mut n = [0.0; 3];
    n[k] = out;
    let read = |nb: EdgeKey| raw.get(&nb).filter(|q| q.lower_in == r.lower_in && accept(&nb)).map(|q| q.t);
    for j in (0..3).filter(|&j| j != k) {
        let mut up = s;
        up[j] += 1;
        let fwd = read((up, k)).map(|u| u - r.t);
        let back = s[j].checked_sub(1).and_then(|d| {
            let mut dn = s;
            dn[j] = d;
            read((dn, k)).map(|u| r.t - u)
        });
        let slope = match (fwd, back) {
            (Some(a), Some(b)) if (a - b).abs() < SLOPE_AGREE => 0.5 * (a + b),
            (Some(a), Some(b)) => {
                if a.abs() <= b.abs() {
                    a
                } else {
                    b
                }
            }
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => return None,
        };
        n[j] = -out * slope.clamp(-SLOPE_MAX, SLOPE_MAX);
    }
    unit(n)
}

/// The central-difference gradient at sample `s` (one-sided on the
/// lattice's faces); `None` when any read is non-finite.
fn gradient(g: &Grid, s: [usize; 3]) -> Option<V3> {
    let mut out = [0.0; 3];
    for (k, o) in out.iter_mut().enumerate() {
        let (mut up, mut dn) = (s, s);
        if s[k] < g.n[k] {
            up[k] += 1;
        }
        if s[k] > 0 {
            dn[k] -= 1;
        }
        let span = up[k].saturating_sub(dn[k]) as f64 * g.step;
        if span.is_nan() || span <= 0.0 {
            return None;
        }
        *o = (g.val(up) - g.val(dn)) / span;
    }
    out.iter().all(|c| c.is_finite()).then_some(out)
}

/// The fallback normal: both endpoints' gradient directions, weighted
/// by position and by how nearly each obeys |grad d| = 1.
fn gradient_normal(g: &Grid, key: EdgeKey, t: f64) -> Option<V3> {
    let (a, k) = key;
    let mut b = a;
    b[k] += 1;
    let mut n = [0.0; 3];
    for (s, w) in [(a, 1.0 - t), (b, t)] {
        let Some(gr) = gradient(g, s) else { continue };
        let len = gr.iter().map(|c| c * c).sum::<f64>().sqrt();
        if len.is_nan() || len <= f64::MIN_POSITIVE {
            continue;
        }
        let w = w / (EIKONAL_EPS + (len - 1.0).abs());
        for (o, c) in n.iter_mut().zip(gr) {
            *o += w * c / len;
        }
    }
    unit(n)
}
