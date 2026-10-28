//! The hole's inner wall: what the MIN-fold says the wall IS.
//! The silhouette's edge sweep, sampled like the OD profile's,
//! but answering each station with its innermost material - the
//! mirror of the max-fold that carries every outer ride.
//!
//! Provenance: the chord-sampling frame and the sagitta recursion
//! are this repo's own (stage B's silhouette machinery, turned to
//! the inside); a boring bar's ball-center locus riding the hole
//! wall at the tool radius is machine-tool handbook material.

use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::turning::profile::Frame;

/// The wall's chord budget: a model whose every edge needs more
/// samples than this is coarser than any tolerance should ride.
const MAX_WALL_PTS: usize = 1 << 16;

/// The hole's inner wall: the silhouette's edge sweep folded with
/// MIN - the innermost material a station plane meets. Within one
/// interval of constant crossing-set a valid revolve's meridian
/// crosses no pair of edges twice, so one edge carries the minimum
/// there and it is convex in the station; the same sagitta law
/// therefore chords the wall to within `tol`, in the same safe
/// direction. Step faces of the wall answer as verticals with
/// their NEARER vertex (a shelf's inner edge is where the bar's
/// corner circle claim starts to matter, and offset_in has it);
/// floor and ceiling faces answer with their axis end, which is
/// what makes a solid station read 0.0 here and name itself in
/// the doors.
pub(super) fn inner_wall(
    facets: &[Triangle],
    f: &Frame,
    tol: f64,
) -> Result<Vec<(f64, f64)>, GenError> {
    let mut edges: Vec<(Point3, Point3, f64, f64)> = Vec::with_capacity(facets.len() * 3);
    let mut stations: Vec<f64> = Vec::with_capacity(facets.len() * 6);
    let mut verticals: Vec<(f64, f64)> = Vec::new();
    for t in facets {
        for (a, b) in [(t.a, t.b), (t.b, t.c), (t.c, t.a)] {
            if !a.is_finite() || !b.is_finite() {
                return Err(GenError::Invalid(
                    "the model carries a non-finite vertex - the mesh is corrupt".into(),
                ));
            }
            let (sa, ra) = f.sr(a);
            let (sb, rb) = f.sr(b);
            stations.push(sa);
            stations.push(sb);
            if sa == sb {
                verticals.push((sa, ra.min(rb)));
            } else if sa < sb {
                edges.push((a, b, sa, sb));
            } else {
                edges.push((b, a, sb, sa));
            }
        }
    }
    stations.sort_unstable_by(f64::total_cmp);
    stations.dedup();
    verticals.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));

    let rho_at = |a: Point3, b: Point3, lo: f64, hi: f64, s: f64| -> f64 {
        let t = ((s - lo) / (hi - lo)).clamp(0.0, 1.0);
        f.sr(Point3::new(
            a.x + (b.x - a.x) * t,
            a.y + (b.y - a.y) * t,
            a.z + (b.z - a.z) * t,
        ))
        .1
    };

    let n = stations.len();
    let mut crosses: Vec<Vec<usize>> = vec![Vec::new(); n - 1];
    for (k, &(_, _, lo, hi)) in edges.iter().enumerate() {
        let i0 = stations.partition_point(|&s| s < lo);
        let i1 = stations.partition_point(|&s| s <= hi);
        for slot in &mut crosses[i0..i1.saturating_sub(1)] {
            slot.push(k);
        }
    }
    for (i, c) in crosses.iter().enumerate() {
        if c.is_empty() {
            return Err(GenError::Invalid(format!(
                "no material crosses the station interval [{:.3}, {:.3}] - the model is \
                 not one continuous turned body, and boring machines one hole in one bar",
                stations[i], stations[i + 1]
            )));
        }
    }
    let env = |i: usize, s: f64| -> f64 {
        let mut r: f64 = f64::INFINITY;
        for &k in &crosses[i] {
            let (a, b, lo, hi) = edges[k];
            r = r.min(rho_at(a, b, lo, hi, s));
        }
        r
    };
    let v_at = |s: f64| -> f64 {
        let lo = verticals.partition_point(|&(sv, _)| sv < s);
        let hi = verticals.partition_point(|&(sv, _)| sv <= s);
        let mut r: f64 = f64::INFINITY;
        for &(_, rv) in &verticals[lo..hi] {
            r = r.min(rv);
        }
        r
    };

    let mut pts: Vec<(f64, f64)> = Vec::new();
    let mut budget = MAX_WALL_PTS;
    for i in 0..n {
        let s = stations[i];
        let l = if i > 0 { Some(env(i - 1, s)) } else { None };
        let r = if i + 1 < n { Some(env(i, s)) } else { None };
        let here = l.unwrap_or(f64::INFINITY).min(r.unwrap_or(f64::INFINITY)).min(v_at(s));
        if let Some(v) = l.filter(|&v| v > here) {
            pts.push((s, v));
        }
        pts.push((s, here));
        if let Some(v) = r.filter(|&v| v > here) {
            pts.push((s, v));
        }
        let Some(f0) = r else { break };
        wall_bisect(
            &crosses[i],
            &edges,
            f,
            s,
            stations[i + 1],
            f0,
            env(i, stations[i + 1]),
            tol,
            &mut pts,
            &mut budget,
        )?;
    }
    Ok(pts)
}

/// The interval's sagitta recursion on the MIN fold - the
/// silhouette's own, with the chord standing over the nearer wall.
#[allow(clippy::too_many_arguments)]
fn wall_bisect(
    set: &[usize],
    edges: &[(Point3, Point3, f64, f64)],
    f: &Frame,
    x0: f64,
    x1: f64,
    f0: f64,
    f1: f64,
    tol: f64,
    pts: &mut Vec<(f64, f64)>,
    budget: &mut usize,
) -> Result<(), GenError> {
    let m = x0 + (x1 - x0) * 0.5;
    if m <= x0 || m >= x1 {
        return Ok(());
    }
    if *budget == 0 {
        return Err(GenError::Invalid(format!(
            "the hole's wall does not settle its chord at tolerance {tol} - \
             coarsen the model or widen the tolerance"
        )));
    }
    let mut fm: f64 = f64::INFINITY;
    for &k in set {
        let (a, b, lo, hi) = edges[k];
        let t = ((m - lo) / (hi - lo)).clamp(0.0, 1.0);
        fm = fm.min(
            f.sr(Point3::new(
                a.x + (b.x - a.x) * t,
                a.y + (b.y - a.y) * t,
                a.z + (b.z - a.z) * t,
            ))
            .1,
        );
    }
    *budget -= 1;
    if (f0 + f1) * 0.5 - fm > tol {
        wall_bisect(set, edges, f, x0, m, f0, fm, tol, pts, budget)?;
        pts.push((m, fm));
        wall_bisect(set, edges, f, m, x1, fm, f1, tol, pts, budget)?;
    } else {
        pts.push((m, fm));
    }
    Ok(())
}
