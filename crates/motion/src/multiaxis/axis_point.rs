//! The point-concurrency guard of the axis fit (issue #208). The
//! concurrency law of `axis.rs` asks whether each facet's normal line
//! meets the candidate axis, and that test cannot tell a pencil of
//! lines through ONE POINT from lines meeting a line: a two-triangle
//! face anchors its line at the face centre, so a box's faces all
//! pass through the box centre, and a single-band wall's lines all
//! pass through the bar's mid point. Lines through one point meet
//! EVERY line through it. On a plate or cube with a small turned boss
//! the boss lends its bearings to the coarseness gate and the plate's
//! faces "meet" a 45-degree line through their centre: a trusted
//! wrong axis.
//!
//! A pencil of lines through a point carries no direction of its own,
//! so it may certify an axis only when it is itself rotationally
//! symmetric about it - the Gauss map of a surface of revolution is a
//! union of circles about the axis: each ring's normals keep ONE angle
//! to the axis and bear every azimuth. So the inliers are peeled into
//! pencils (the point their lines meet best, least trimmed squares on
//! the area, then the next among the rest) and each pencil is judged
//! by itself. Its facets, sorted into latitude classes (`n . u`), must
//! each hold a ring (flats meeting the axis at a ring's latitude ride
//! with it: a D-flat, a square plate round a bore); a crossing rod
//! folds onto two bearings at many latitudes and holds none. And its
//! latitude must not run with the bearing: a skewed rod's ring, seen
//! from another axis, swings its latitude once per turn. A pencil
//! holding half the inliers must also pass the coarseness gate alone.
//! The coarseness gate itself reads only facets that meet the axis
//! (`axis.rs`), so an outlier boss lends nothing.
//!
//! Provenance: the Gauss map of a surface of revolution (do Carmo,
//! Differential Geometry of Curves and Surfaces) and the concurrency
//! of its normal lines with the axis (Pottmann & Randrup 1998);
//! least-squares point of nearest approach to a set of lines, least
//! trimmed squares (Rousseeuw 1984), and the trigonometric regression
//! of linear on circular data (Mardia & Jupp, Directional Statistics). Written from those sources and
//! our own design notes; no vendor material.

use crate::geometry::Point3;

use super::axis::{Axis, Datum, REL_TOL};
use super::axis_robust::dist;
use super::linalg::{solve3, unit};

/// Share of the inlier (non-pole) area meeting at one point past which
/// the point, not the axis, is what the lines agree on.
const POINT_SHARE: f64 = 0.5;
/// Trimming rounds that find the point a pencil of lines meets.
const ROUNDS: usize = 6;
/// Pencils peeled off the inliers at most (a banded wall is one per
/// band; past this the rest are too small to bias anything).
const MAX_PENCILS: usize = 12;
/// Smallest pencil, as a share of the inlier area, worth judging.
const MIN_PENCIL: f64 = 0.05;
/// The uneven area of all pencils together, as a share of the inlier
/// area: the same 10 % the concurrency share lets miss the axis.
const BIAS_SHARE: f64 = 0.1;
/// Harmonics of the bearing the latitude is regressed on: a ring about
/// another axis runs its latitude with the first.
const HARMONICS: usize = 2;
/// A class holds a ring when its widest bearing skip stays inside this
/// many sagitta budgets: a keyway's mouth in one band (a skip either
/// side of its floor) is still a ring, a fold onto two or four bearings
/// (a crossing rod, a box) never. The whole part still meets the
/// strict budget in `axis.rs`.
const RING_SKIP: f64 = 2.0;
/// Latitude jump that separates two classes (rings) of one pencil.
const LAT_GAP: f64 = 0.01;
/// Latitude range past which a class is a continuous run, not a ring.
const LAT_WIDE: f64 = 0.05;
/// Latitude swing (sine of the tilt) the bearing must explain before a
/// pencil counts as off its cones: far above a mesh's rounding.
const LAT_FLOOR: f64 = 2e-3;
/// Latitude bin width inside a wide class (#237): a fine dome's
/// latitudes run continuously, so the class is cut into bins this wide
/// and each bin's bearings are read for a fold.
const LAT_BIN: f64 = 0.02;
/// A bin is folded when an area-weighted bearing harmonic (1st to 4th)
/// keeps this share of its area: one flat (1st), two opposite faces
/// (2nd), three (3rd), or faces and caps a quarter turn apart (4th - a
/// flat bar's sides and ends about its width) - never a ring, a hex or a
/// fine wall, whose low harmonics cancel. (A square section inside a
/// dome is refused too: fail closed, the safe side.)
const FOLD: f64 = 0.5;
/// Folded area, as a share of the inlier area, past which a wide class
/// hides flat faces that do not revolve (#237): a rod through a ball.
const FOLD_SHARE: f64 = 0.02;

/// The widest empty skip (radians) between the bearings about `u` of
/// the normals `ns`; `None` when too few bear an azimuth to call
/// anything round. `sqrt(8 * REL_TOL)` is the chord-sagitta budget in
/// angle: a skip past it is a polygon pretending to be a wall.
pub(super) fn widest_skip(ns: impl Iterator<Item = Point3>, u: Point3) -> Option<f64> {
    let (e1, e2) = frame(u)?;
    let mut angs: Vec<f64> = ns.filter_map(|n| bearing(n, u, e1, e2)).collect();
    if angs.len() <= 2 {
        return None;
    }
    angs.sort_by(|x, y| x.total_cmp(y));
    let (first, last) = (angs.first().copied()?, angs.last().copied()?);
    let gap = angs.windows(2).fold(std::f64::consts::TAU - last + first, |g, w| g.max(w[1] - w[0]));
    Some(gap)
}

/// The sagitta budget written in angle (see [`widest_skip`]).
pub(super) fn skip_budget() -> f64 {
    (8.0 * REL_TOL).sqrt()
}

/// True when the inliers of `ax` (normal lines within `tol`) bear the
/// axis out. The inliers are peeled into pencils - the lines through
/// one point, found by [`pencil_point`], then the next point among the
/// rest - and each pencil is judged ON ITS OWN (review of PR #223: two
/// crossed single-band cylinders are two pencils at one point, and the
/// union of a ring and a two-bearing pencil looked round). A pencil of
/// a revolve is a ring (or zone) about the axis, so its normals keep
/// one angle to the axis all round: the area off such cones
/// ([`off_cone`]) may total at most [`BIAS_SHARE`] of the inliers. A pencil holding half the
/// inliers must also pass the coarseness gate by itself.
pub(super) fn borne_out(data: &[Datum], ax: Axis, tol: f64) -> bool {
    let mut rest: Vec<&Datum> = data.iter().filter(|d| dist(d, ax).is_some_and(|e| e <= tol)).collect();
    let a_in: f64 = rest.iter().map(|d| d.a).sum();
    if a_in <= 0.0 || !a_in.is_finite() {
        return true; // judge's share gate speaks for an empty set
    }
    let mut biased = 0.0;
    for _ in 0..MAX_PENCILS {
        let Some(c) = pencil_point(&rest) else {
            break; // the lines fix no point: no pencil left to fear
        };
        let (near, far): (Vec<&Datum>, Vec<&Datum>) = rest.iter().partition(|d| miss(d, c) <= tol);
        let a_near: f64 = near.iter().map(|d| d.a).sum();
        if near.len() < 3 || a_near < MIN_PENCIL * a_in {
            break;
        }
        if a_near >= POINT_SHARE * a_in && widest_skip(near.iter().map(|d| d.n), ax.dir).is_none_or(|g| g > skip_budget()) {
            return false;
        }
        // #237: a wide class (a fine ball's continuous latitudes) is not
        // a ring check's business, but flats hiding in it are: a rod's
        // faces through a ball fold onto two bearings at one latitude.
        if folded(&near, ax.dir) > FOLD_SHARE * a_in {
            return false;
        }
        biased += off_cone(&near, ax.dir).unwrap_or(1.0) * a_near + ringless(&near, ax.dir);
        if biased > BIAS_SHARE * a_in {
            return false;
        }
        rest = far;
    }
    true
}

/// The bearing of `n` about `u` in the frame (e1, e2); `None` for a
/// pole (a normal along the axis bears no azimuth).
fn bearing(n: Point3, u: Point3, e1: Point3, e2: Point3) -> Option<f64> {
    let s = n.dot(u);
    let p = Point3::new(n.x - u.x * s, n.y - u.y * s, n.z - u.z * s);
    (p.dot(p) > 1e-4).then(|| p.dot(e2).atan2(p.dot(e1)).rem_euclid(std::f64::consts::TAU))
}

/// A frame perpendicular to `u`.
fn frame(u: Point3) -> Option<(Point3, Point3)> {
    let any = if u.z.abs() < 0.9 { Point3::new(0.0, 0.0, 1.0) } else { Point3::new(1.0, 0.0, 0.0) };
    let e1 = unit(u.cross(any))?;
    Some((e1, u.cross(e1)))
}

/// How much of a pencil is NOT on cones about `u`: a surface of
/// revolution meets its axis at one angle all round each ring, so the
/// latitude `l = n . u` of its facets does not depend on their bearing
/// theta. A ring about ANOTHER axis seen from `u` (a skewed or crossing
/// rod) has `l` running with theta. The share of the latitude's
/// variance that the first [`HARMONICS`] harmonics of theta explain,
/// `R^2 = sum_k 2 |sum a (l - mean) e^(ik theta)|^2 / (A^2 var)` (the
/// trigonometric regression of linear on circular data, Mardia & Jupp,
/// Directional Statistics), reads as the share of the pencil's area on
/// such a ring: 0 on any revolve, about the rod's share on a cross.
/// A pencil whose latitudes are one cone within rounding explains
/// nothing and gives 0. `None` when no facet bears an azimuth.
fn off_cone(ds: &[&Datum], u: Point3) -> Option<f64> {
    let (e1, e2) = frame(u)?;
    let pts: Vec<(f64, f64, f64)> = ds.iter().filter_map(|d| bearing(d.n, u, e1, e2).map(|t| (t, d.n.dot(u), d.a))).collect();
    let total: f64 = pts.iter().map(|p| p.2).sum();
    if pts.is_empty() || total <= 0.0 || !total.is_finite() {
        return None;
    }
    let mean = pts.iter().map(|p| p.1 * p.2).sum::<f64>() / total;
    // The latitude itself (a skewed ring: l runs with theta) and its
    // square deviation (a crossing ring at 90 degrees: its bearings
    // fold onto two, where l spreads both ways, so only the spread
    // runs with theta).
    let lin = explained(&pts, total, |l| l - mean, LAT_FLOOR);
    let sq = explained(&pts, total, |l| (l - mean) * (l - mean), LAT_FLOOR * LAT_FLOOR);
    Some(lin.max(sq))
}

/// The share (R^2) of the variance of `f(l)` that the first
/// [`HARMONICS`] harmonics of the bearing explain, counted only when
/// the explained part itself stands above `floor` (a mesh's rounding
/// noise is explained by chance, never by a millimetre of tilt).
fn explained(pts: &[(f64, f64, f64)], total: f64, f: impl Fn(f64) -> f64, floor: f64) -> f64 {
    let fm = pts.iter().map(|p| p.2 * f(p.1)).sum::<f64>() / total;
    let var = pts.iter().map(|p| p.2 * (f(p.1) - fm).powi(2)).sum::<f64>() / total;
    if var.is_nan() || var <= 0.0 {
        return 0.0;
    }
    let mut e = 0.0;
    for k in 1..=HARMONICS {
        let (mut c, mut sn) = (0.0, 0.0);
        for (t, l, a) in pts {
            let kt = k as f64 * t;
            c += a * (f(*l) - fm) * kt.cos();
            sn += a * (f(*l) - fm) * kt.sin();
        }
        e += 2.0 * (c * c + sn * sn) / (total * total);
    }
    if e.sqrt() <= floor {
        return 0.0;
    }
    (e / var).clamp(0.0, 1.0)
}

/// The area of the pencil's latitude classes that hold no ring: the
/// facets sorted by latitude `n . u` and split where it jumps by more
/// than [`LAT_GAP`]; a revolve's ring (or zone band) is one narrow
/// class bearing every azimuth (to [`RING_SKIP`]), and flats meeting the axis at its
/// latitude (a D-flat, a square plate's sides round a bore) ride with
/// it. A crossing rod's wall seen at 90 degrees folds onto two
/// bearings at many latitudes: narrow classes with no ring. Classes
/// wider than [`LAT_WIDE`] are a continuous run of latitudes (a fine
/// dome, a skewed ring) and are left to [`off_cone`].
fn ringless(ds: &[&Datum], u: Point3) -> f64 {
    let Some((e1, e2)) = frame(u) else {
        return 0.0;
    };
    let mut v: Vec<(f64, &Datum)> = ds.iter().filter(|d| bearing(d.n, u, e1, e2).is_some()).map(|d| (d.n.dot(u), *d)).collect();
    v.sort_by(|x, y| x.0.total_cmp(&y.0));
    let (mut lost, mut start) = (0.0, 0usize);
    for i in 1..=v.len() {
        let split = i == v.len() || v.get(i).zip(v.get(i - 1)).is_some_and(|(b, a)| b.0 - a.0 > LAT_GAP);
        if !split {
            continue;
        }
        let class = v.get(start..i).unwrap_or(&[]);
        let wide = class.first().zip(class.last()).is_some_and(|(a, b)| b.0 - a.0 > LAT_WIDE);
        if !wide && widest_skip(class.iter().map(|c| c.1.n), u).is_none_or(|g| g > RING_SKIP * skip_budget()) {
            lost += class.iter().map(|c| c.1.a).sum::<f64>();
        }
        start = i;
    }
    lost
}

/// The area folded onto a few bearings inside the pencil's WIDE latitude
/// classes (#237) - the classes [`ringless`] leaves to [`off_cone`]. A
/// fine ball meshed by UV quads puts every facet's line through its
/// centre, so a rod or bar through that centre shares its pencil, and the
/// ball's continuous latitudes swallow the rod's faces into one wide
/// class. Cut into [`LAT_BIN`] bins, a revolve's bin bears its area evenly
/// round the axis (the low bearing harmonics cancel); the rod's bin holds
/// two opposite faces, whose 2nd harmonic keeps nearly all of it. The
/// returned area is each such bin's area times its strongest of the 1st
/// to 4th harmonics, for bins past [`FOLD`].
fn folded(ds: &[&Datum], u: Point3) -> f64 {
    let Some((e1, e2)) = frame(u) else {
        return 0.0;
    };
    let mut v: Vec<(f64, f64, f64)> = ds
        .iter()
        .filter_map(|d| bearing(d.n, u, e1, e2).map(|b| (d.n.dot(u), b, d.a)))
        .filter(|(l, b, a)| l.is_finite() && b.is_finite() && a.is_finite() && *a > 0.0)
        .collect();
    v.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut out = 0.0;
    let mut start = 0usize;
    for i in 1..=v.len() {
        let split = i == v.len() || v.get(i).zip(v.get(i - 1)).is_some_and(|(b, a)| b.0 - a.0 > LAT_GAP);
        if !split {
            continue;
        }
        let class = v.get(start..i).unwrap_or(&[]);
        start = i;
        let wide = class.first().zip(class.last()).is_some_and(|(a, b)| b.0 - a.0 > LAT_WIDE);
        if !wide {
            continue;
        }
        let mut bins: std::collections::BTreeMap<i64, Vec<(f64, f64)>> = std::collections::BTreeMap::new();
        for (l, b, a) in class {
            bins.entry((l / LAT_BIN).floor() as i64).or_default().push((*b, *a));
        }
        for bin in bins.values() {
            let area: f64 = bin.iter().map(|p| p.1).sum();
            if !(area > 0.0) {
                continue;
            }
            let fold = (1..=4)
                .map(|k| {
                    let k = k as f64;
                    let (c, s) = bin.iter().fold((0.0, 0.0), |(c, s), (b, a)| (c + a * (k * b).cos(), s + a * (k * b).sin()));
                    c.hypot(s) / area
                })
                .fold(0.0, f64::max);
            if fold > FOLD {
                out += fold * area;
            }
        }
    }
    out
}

/// The point the lines meet best: the area-weighted least-squares
/// point of nearest approach, refitted on the area half that misses it
/// least until that half settles. `None` when the lines fix no point
/// (all parallel) or the solve fails.
fn pencil_point(inl: &[&Datum]) -> Option<Point3> {
    let mut c = point_of(inl.iter().copied())?;
    let total: f64 = inl.iter().map(|d| d.a).sum();
    for _ in 0..ROUNDS {
        let mut m: Vec<(f64, &Datum)> = inl.iter().map(|d| (miss(d, c), *d)).collect();
        m.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut acc = 0.0;
        let half: Vec<&Datum> = m
            .iter()
            .take_while(|(_, d)| {
                let before = acc;
                acc += d.a;
                before < 0.5 * total
            })
            .map(|(_, d)| *d)
            .collect();
        let next = point_of(half.into_iter())?;
        let step = next - c;
        c = next;
        if step.dot(step).sqrt() <= 1e-12 * (1.0 + c.dot(c).sqrt()) {
            break;
        }
    }
    c.is_finite().then_some(c)
}

/// Least-squares point nearest the lines (p, n) weighted by area:
/// `sum a (I - n n^T) c = sum a (I - n n^T) p`.
fn point_of<'a>(ds: impl Iterator<Item = &'a Datum>) -> Option<Point3> {
    let (mut m, mut b) = ([[0.0f64; 3]; 3], [0.0f64; 3]);
    let mut tr = 0.0;
    for d in ds {
        let n = [d.n.x, d.n.y, d.n.z];
        let p = [d.p.x, d.p.y, d.p.z];
        for i in 0..3 {
            for j in 0..3 {
                let pij = if i == j { 1.0 } else { 0.0 } - n[i] * n[j];
                m[i][j] += d.a * pij;
                b[i] += d.a * pij * p[j];
            }
        }
        tr += 2.0 * d.a;
    }
    if tr <= 0.0 {
        return None;
    }
    // A pencil of lines in one plane (a single-band wall) fixes its
    // point fully; parallel lines leave a direction free and fix none.
    let (vals, _) = super::linalg::sym_eigen(m)?;
    if vals[0] <= 1e-6 * tr {
        return None;
    }
    solve3(m, b).filter(|c| c.is_finite())
}

/// The distance from `c` to the facet's normal line.
fn miss(d: &Datum, c: Point3) -> f64 {
    let w = c - d.p;
    let s = w.dot(d.n);
    let r = Point3::new(w.x - d.n.x * s, w.y - d.n.y * s, w.z - d.n.z * s);
    r.dot(r).sqrt()
}

#[cfg(test)]
#[path = "axis_point_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "axis_wide_tests.rs"]
mod wide_tests;
