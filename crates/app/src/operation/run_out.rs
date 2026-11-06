//! The run-out of an unclosed picked run to the stock edge (M1.5.1 issue
//! C): a facing pass bounded by an open chain machines from the run out
//! to the envelope, the run acting as the limit. Split out of `stitch`
//! at M13.11 along the seam between "runs that close" and "runs that do
//! not"; the law is unchanged.
//!
//! Provenance: our own design notes. No vendor material consulted.

use super::*;

/// Close one open run into a boundary by casting each free end straight
/// out to the envelope's outer ring and walking that ring between the
/// hits (M1.5.1 issue C: "an open loop should just let us generate to
/// the outside of the stock box - the open loop selected should act as
/// a limit"). The run is first oriented so the chord-closing shoelace is
/// positive, which pins the outward normal (a 90° clockwise turn of the
/// travel direction) to the material-free side at each end; the walk
/// direction that keeps the strip on that side is chosen by material
/// overlap when a footprint is known, shortest boundary otherwise, with
/// the forward walk winning ties - and candidates that cross into the
/// run's own chord lens are out of the race, because the run is a limit.
/// Every failure mode - no stock ahead of an end, a self-crossing strip,
/// a zero-area strip - is the named ambiguity, never a silent skip.
pub(super) fn extend_run_to_footprint(
    label: &str,
    run: &[Point2],
    envelope: &Region,
    material: Option<&Region>,
) -> Result<Loop, GenError> {
    let ambiguous = || {
        GenError::Invalid(format!(
            "{label} and the selection geometry is ambiguous; use a closed chain or a Face pick"
        ))
    };
    let Some(outer) = envelope
        .loops()
        .iter()
        .max_by(|a, b| total_cmp_signed(a.area(), b.area()))
    else {
        return Err(ambiguous());
    };
    let ring = outer.points();
    if ring.len() < 3 || run.len() < 2 {
        return Err(ambiguous());
    }
    // Orient: shoelace over run + chord. A straight run shoelaces to ~0
    // and keeps its order; the normal choice is still deterministic.
    let mut twice = 0.0f64;
    let m = run.len();
    for i in 0..m {
        let a = run[i];
        let b = run[(i + 1) % m];
        twice += a.x * b.y - b.x * a.y;
    }
    let pts: Vec<Point2> = if twice < 0.0 {
        run.iter().rev().copied().collect()
    } else {
        run.to_vec()
    };
    // Outward = clockwise turn of travel at each free end: (d.y, -d.x).
    let d0 = Point2::new(pts[1].x - pts[0].x, pts[1].y - pts[0].y);
    let d1 = Point2::new(pts[m - 1].x - pts[m - 2].x, pts[m - 1].y - pts[m - 2].y);
    let n0 = Point2::new(d0.y, -d0.x);
    let n1 = Point2::new(d1.y, -d1.x);
    // A straight run laces to nothing, so "away from the bulge" names
    // no side - both normals are tried and the overlap score picks the
    // material side. A curved run's chord-CCW orientation already fixed
    // one honest outward direction; trying the other would let a run
    // that rings material (a bore rim) claim the far half of itself.
    let straight = twice.abs() < DEGENERATE_AREA;
    // The lens the run closes against its own chord: material or air on
    // the run's concave side. A strip that reaches into it crosses the
    // run - and "the open loop acts as a limit" forbids crossing. One
    // probe per segment, a hair left of travel (the chord-CCW
    // orientation above puts lens interior on the left); any candidate
    // containing one is out of the race, so overlap can no longer be
    // gamed by wrapping the whole envelope behind the run's back. A
    // straight run closes no lens: pure overlap decides, as scribbled.
    let mut probes: Vec<Point2> = Vec::new();
    if !straight {
        for i in 0..m - 1 {
            let (a, b) = (pts[i], pts[i + 1]);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let len = (dx * dx + dy * dy).sqrt();
            if len <= f64::EPSILON {
                continue;
            }
            let eps = len * 1e-3;
            probes.push(Point2::new(
                (a.x + b.x) * 0.5 - dy / len * eps,
                (a.y + b.y) * 0.5 + dx / len * eps,
            ));
        }
    }
    let mut candidates: Vec<(f64, f64, usize, Loop)> = Vec::new();
    let sides: &[f64] = if straight { &[1.0, -1.0] } else { &[1.0] };
    for (si, &sgn) in sides.iter().enumerate() {
        let Some(hit_start) = ray_hit(ring, pts[0], Point2::new(n0.x * sgn, n0.y * sgn)) else {
            continue;
        };
        let Some(hit_end) = ray_hit(ring, pts[m - 1], Point2::new(n1.x * sgn, n1.y * sgn)) else {
            continue;
        };
        for &forward in [true, false].iter() {
            let Some(between) = boundary_path(
                ring,
                (hit_end.1, hit_end.0),
                (hit_start.1, hit_start.0),
                forward,
            ) else {
                continue;
            };
            let mut closed = pts.clone();
            closed.push(hit_end.0);
            closed.extend(between.iter().copied());
            let Ok(loop_) = Loop::new(closed) else {
                continue;
            };
            if loop_.area().abs() < DEGENERATE_AREA {
                continue;
            }
            // Self-crossing is the ambiguity guard: a crossing loop is
            // no boundary, and `Region::new` is the one that says so.
            let Ok(region) = Region::new(vec![loop_.clone()]) else {
                continue;
            };
            if probes.iter().any(|p| region.contains(*p)) {
                continue;
            }
            // More material inside the strip than less - the side that
            // machines metal, not air.
            let overlap = match material {
                Some(mat) => match region.intersect(mat) {
                    Ok(r) => r.area().abs(),
                    Err(_) => continue,
                },
                None => 0.0,
            };
            // Then the shorter boundary walk: the corridor hugging the
            // run's normal side beats the candidate that wraps the
            // whole envelope, and with no footprint to consult this is
            // the whole decision.
            let mut walk = 0.0;
            let mut prev = hit_end.0;
            for &p in &between {
                walk += ((p.x - prev.x).powi(2) + (p.y - prev.y).powi(2)).sqrt();
                prev = p;
            }
            candidates.push((overlap, -walk, si, loop_));
        }
    }
    // Highest score wins; ties break to the shorter walk, then to the
    // first side tried, so the answer is a pure function of the pick.
    match candidates
        .into_iter()
        .max_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then(a.1.total_cmp(&b.1))
                .then(a.2.cmp(&b.2))
        })
    {
        Some((_, _, _, loop_)) => Ok(loop_),
        None => Err(ambiguous()),
    }
}

/// Order two signed areas by magnitude - the outer loops of a region
/// are the big positive ones; a stray negative inside an `outer` pick
/// should never win.
pub(super) fn total_cmp_signed(a: f64, b: f64) -> std::cmp::Ordering {
    a.abs().total_cmp(&b.abs())
}
