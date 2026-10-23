

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

/// The 1e6 lattice `push_loop` keys rims on, in one place so the
/// stitcher's endpoint matches and the dedupe agree to the nanometre.
pub(super) fn qkey(p: &Point2) -> (i64, i64) {
    (
        (p.x * 1e6).round().clamp(i64::MIN as f64, i64::MAX as f64) as i64,
        (p.y * 1e6).round().clamp(i64::MIN as f64, i64::MAX as f64) as i64,
    )
}

/// One distinct picked open run in the shape the stitcher works in:
/// world XY points with the entity's transform applied (where
/// `push_loop` expects every boundary to live), its quantized endpoints,
/// and every item index that picked it - the same chain twice is one
/// run picked twice.
pub(super) struct Run {
    items: Vec<usize>,
    entity: String,
    cid: u64,
    pts: Vec<Point2>,
    start: (i64, i64),
    end: (i64, i64),
}

/// What the stitcher hands back: rings composed from picked runs (label,
/// points, run count), and for every item the rings consumed, which ring
/// (index into that vec) consumed it - the set form answers "was this
/// stitched?", the map form lets the UI's chips say "→ ring k" from the
/// very walk the region takes.
type Stitched = (Vec<(String, Vec<Point2>, usize)>, HashMap<usize, usize>);

/// M1.5.1 issue C: tessellation - STEP adaptive chords especially - can
/// break a geometrically closed rim into several chains, each stopping
/// where a chord's turn exceeds the merge angle. The user picks them
/// all, and to them it is obviously one loop. So before routing, stitch
/// the picked open runs of each entity end to end and return every walk
/// that closes back onto its start as one ring, with its contributing
/// picks marked consumed so routing treats them as the closed chain
/// they describe.
///
/// Junctions match either endpoint of a candidate because a loop's runs
/// are stored with whatever direction their edge orientations allow -
/// across the rim of a split circle the two halves meet end-to-end on
/// one side and start-to-start on the other, so a flipped continuation
/// IS the ring, not a corruption of it. Committed rings are shoelace-
/// canonicalized to CCW so pick order can never flip the winding. A
/// doubled pick is deduped by chain id - the same segments twice close
/// no ring, they close a zero-area spike.
///
/// Deliberately conservative: only picked runs take part (nothing
/// unpicked is pulled in), cross-entity rings are not attempted (a
/// recorded limit), a greedy fork that dead-ends commits nothing, and
/// runs that do not close route exactly as they always did. Failed
/// lookups are not errors here; the routing loop owns those refusals
/// and will name them.
pub(super) fn stitch_open_runs(world: &World, items: &[GeoSel]) -> Stitched {
    let mut runs: Vec<Run> = Vec::new();
    for (i, g) in items.iter().enumerate() {
        let FeatureSel::Chain(cid) = g.feature else {
            continue;
        };
        if let Some(r) = runs.iter_mut().find(|r| r.entity == g.entity && r.cid == cid) {
            r.items.push(i);
            continue;
        }
        let Some(entity) = model::find_by_name(world, &g.entity) else {
            continue;
        };
        let Some(features) = model::features_of(world, &entity) else {
            continue;
        };
        let Some(c) = feature::chains(&features).into_iter().find(|c| c.id == cid) else {
            continue;
        };
        if c.closed {
            continue;
        }
        let loc = model::transform_of(world, &entity).unwrap_or_default();
        let pts: Vec<Point2> = feature::chain_points(&features, &c)
            .iter()
            .map(|p| {
                let w = transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
                Point2::new(w.x, w.y)
            })
            .collect();
        let (Some(first), Some(last)) = (pts.first().copied(), pts.last().copied()) else {
            continue;
        };
        runs.push(Run {
            items: vec![i],
            entity: g.entity.clone(),
            cid,
            pts,
            start: qkey(&first),
            end: qkey(&last),
        });
    }
    let used = |r: &Run, consumed: &HashMap<usize, usize>| {
        r.items.iter().any(|it| consumed.contains_key(it))
    };
    // Each committed ring carries the count of runs that spelled it, so
    // `selection_notes` can say what the stitching did.
    let mut rings: Vec<(String, Vec<Point2>, usize)> = Vec::new();
    let mut consumed: HashMap<usize, usize> = HashMap::new();
    for s in 0..runs.len() {
        if used(&runs[s], &consumed) {
            continue;
        }
        // Walk the cycle through run `s` forward: `anchor` is where we
        // started, `far` the junction to extend from. `in_seq` is why a
        // walk can never loop.
        let anchor = runs[s].start;
        let mut far = runs[s].end;
        let mut seq: Vec<(usize, bool)> = vec![(s, false)];
        let mut in_seq = vec![false; runs.len()];
        in_seq[s] = true;
        let closed = loop {
            if far == anchor {
                break true;
            }
            let mut next: Option<(usize, bool)> = None;
            for (j, r) in runs.iter().enumerate() {
                if in_seq[j] || used(r, &consumed) || r.entity != runs[s].entity {
                    continue;
                }
                if r.start == far {
                    next = Some((j, false));
                    break;
                }
                if r.end == far {
                    next = Some((j, true));
                    break;
                }
            }
            let Some((j, rev)) = next else { break false };
            in_seq[j] = true;
            seq.push((j, rev));
            far = if rev { runs[j].start } else { runs[j].end };
        };
        if !closed {
            continue;
        }
        // Concatenate the walk, flipping what the junctions asked for.
        // Shared seam vertices arrive as consecutive repeats and
        // closing wraps - `push_loop` and `Loop::new` drop exactly
        // those, so the ring needs no special handling here.
        let mut pts: Vec<Point2> = Vec::new();
        for (j, rev) in &seq {
            for &it in &runs[*j].items {
                consumed.insert(it, rings.len());
            }
            if *rev {
                pts.extend(runs[*j].pts.iter().rev().copied());
            } else {
                pts.extend(runs[*j].pts.iter().copied());
            }
        }
        let mut twice = 0.0;
        let n = pts.len();
        for i in 0..n.saturating_sub(1) {
            let (a, b) = (pts[i], pts[i + 1]);
            twice += a.x * b.y - a.y * b.x;
        }
        if twice < 0.0 {
            pts.reverse();
        }
        rings.push((
            format!("the stitched rim of '{}'", runs[s].entity),
            pts,
            seq.len(),
        ));
    }
    (rings, consumed)
}

/// Accept one selected boundary: refuse what collapses to a line seen
/// from above, and count a rim already in the region once - two faces
/// meeting at an edge share it, and a coincident double loop would be a
/// zero-wide sliver.
pub(super) fn push_loop(
    label: &str,
    pts: Vec<Point2>,
    loops: &mut Vec<Loop>,
    rims: &mut Vec<Vec<(i64, i64)>>,
) -> Result<(), GenError> {
    let mut key: Vec<(i64, i64)> = pts.iter().map(qkey).collect();
    // Same drop Loop::new does - consecutive repeats and the closing
    // wrap - so the collapse check below sees the ring the loop would
    // actually be.
    key.dedup();
    if key.len() > 1 && key.first() == key.last() {
        key.pop();
    }
    if key.len() < 3 {
        // A vertical wall seen from above, or a chain that runs up one:
        // nothing encloses.
        return Err(GenError::Invalid(format!(
            "{label} projects to a line from above and bounds no area; select a horizontal face or a closed edge chain around the part"
        )));
    }
    let canon = canon_ring(&key);
    if rims.contains(&canon) {
        return Ok(());
    }
    let loop_ = Loop::new(pts)?;
    if loop_.area().abs() < DEGENERATE_AREA {
        return Err(GenError::Invalid(format!(
            "{label} projects to a line from above and bounds no area; select a horizontal face or a closed edge chain around the part"
        )));
    }
    rims.push(canon);
    loops.push(loop_);
    Ok(())
}

/// Rotate- and mirror-canonical form of a quantized ring: the
/// lexicographic minimum over its 2n walks. Compared exactly, never by
/// hash - a colliding dedupe key would silently drop a real boundary.
pub(super) fn canon_ring(ring: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let n = ring.len();
    if n == 0 {
        return Vec::new();
    }
    let mut best: Option<Vec<(i64, i64)>> = None;
    for start in 0..n {
        for step in [1usize, 2] {
            let walk: Vec<(i64, i64)> = (0..n).map(|k| ring[(start + k * step) % n]).collect();
            if best.as_ref().is_none_or(|b| walk < *b) {
                best = Some(walk);
            }
        }
    }
    best.unwrap_or_default()
}

/// Run one generated path after another. Both start and end on the
/// clearance plane, so the joint is a clearance rapid: append and re-anchor
/// the added path's opening `from` to the previous move's `to` - a stale
/// `(0, 0)` there would read as a teleport to the origin.
pub(super) fn stitch(mut a: Toolpath, mut b: Toolpath) -> Toolpath {
    if let (Some(to), Some(first)) = (a.moves.last().map(|m| m.to), b.moves.first_mut()) {
        first.from = to;
    }
    a.moves.append(&mut b.moves);
    a.cycles.append(&mut b.cycles);
    a
}
