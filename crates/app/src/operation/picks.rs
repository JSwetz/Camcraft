

use super::*;

/// Which stitched ring (0-based, the stitcher's own order) each pick took
/// part in: one entry per item, `None` for picks that closed nothing or
/// were themselves closed chains. The chips' `· → ring k` note reads
/// through the very stitch walk the region takes, so a row can never say
/// it joins a ring the preview does not build.
#[must_use]
pub fn stitched_rings(world: &World, items: &[GeoSel]) -> Vec<Option<usize>> {
    // The source selection's own walk is world XY (a picked source
    // under a tilt is refused upstream), so the chips read it there.
    let (_, consumed) = stitch_open_runs(world, items, None);
    let mut out: Vec<Option<usize>> = vec![None; items.len()];
    for (it, k) in consumed {
        if let Some(slot) = out.get_mut(it) {
            *slot = Some(k);
        }
    }
    out
}

/// A human-readable account of what a feature selection machines beyond
/// the bare pick list: runs stitched into rings, runs extended out to
/// the stock boundary, and a region taken on the far side of its rings
/// because the rings bound air. Computed on read from the very walk the
/// region takes - the account cannot disagree with the geometry, and a
/// selection that refuses says so by owning nothing.
///
/// Deliberately *not* a persisted `verify::Warning` variant: those names
/// are the v4 project-file contract, and this is derived state, not a
/// verification finding.
#[must_use]
pub fn selection_notes(
    world: &World,
    setup: Option<&str>,
    items: &[GeoSel],
    params: &OpParams,
) -> Vec<String> {
    selection_account(world, setup, items, params).0
}

/// The stable core of the "collapses to a line seen from above" refusal
/// (`push_loop`): what [`selection_refusal`] keys on, so the pick and
/// Apply doors and the page say the sentence generation would.
pub(super) const LINE_FROM_ABOVE: &str = "projects to a line from above and bounds no area";

/// `push_loop`'s refusal for one boundary named by `label`.
pub(super) fn line_from_above(label: &str) -> GenError {
    GenError::Invalid(format!(
        "{label} {LINE_FROM_ABOVE}; select a horizontal face or a closed edge chain around the part"
    ))
}

/// [`selection_notes`] plus the early-door refusal the same walk found,
/// in one pass (the page reads both every frame).
#[must_use]
pub fn selection_account(
    world: &World,
    setup: Option<&str>,
    items: &[GeoSel],
    params: &OpParams,
) -> (Vec<String>, Option<String>) {
    let mut notes = Vec::new();
    match selection_geometry_impl(world, setup, items, params, Some(&mut notes)) {
        Ok(_) => (notes, None),
        // Other refusals stay `region_of`'s and generation's to make,
        // with detail; notes stay as far as the walk got.
        Err(err) => (notes, door_refusal(params, err)),
    }
}

/// The pocket family's flat-area refusal, decided at the doors (pick,
/// links edit, creation) instead of only at Generate: a pocket, adaptive
/// or offset clearing whose picks include a vertical wall's rim or a
/// non-horizontal face bounds no area seen from above - that is a wrong
/// pick, and the user should hear it when they make it. `None` for any
/// other source, kind, or refusal (those stay generation's; an open run
/// mid-pick is the normal state of building a chain edge by edge).
#[must_use]
pub fn selection_refusal(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
    params: &OpParams,
) -> Option<String> {
    let SrcRef::Selection(items) = src else {
        return None;
    };
    if !pocket_family(params) || items.is_empty() {
        return None;
    }
    selection_geometry_impl(world, setup, items, params, None)
        .err()
        .and_then(|err| door_refusal(params, err))
}

fn pocket_family(params: &OpParams) -> bool {
    matches!(
        params,
        OpParams::Pocket { .. } | OpParams::Adaptive2D { .. } | OpParams::Offset2D { .. }
    )
}

fn door_refusal(params: &OpParams, err: GenError) -> Option<String> {
    match err {
        GenError::Invalid(msg) if pocket_family(params) && msg.contains(LINE_FROM_ABOVE) => Some(msg),
        _ => None,
    }
}

/// The footprint a piece of material projects to, XY: the same
/// `slice::project` the Face arm of `region_for` and the generation
/// verifier run, so a preview that consults it and the path it previews
/// agree by construction (M1.5.1: a run-out strip is clipped to this).
/// A model with height sweeps to a silhouette with its through-holes
/// intact; a zero-thickness sheet has no Z span to section, so its
/// facets' planar union is its footprint instead.
pub(super) fn silhouette(facets: &[Triangle]) -> Option<Region> {
    if let Ok(r) = slice::project(facets, FOOTPRINT_SAMPLES)
        && !r.is_empty()
    {
        return Some(r);
    }
    let loops: Vec<Loop> = facets
        .iter()
        .filter_map(|t| {
            Loop::new(vec![
                Point2::new(t.a.x, t.a.y),
                Point2::new(t.b.x, t.b.y),
                Point2::new(t.c.x, t.c.y),
            ])
            .ok()
            .filter(|l| l.area().abs() > DEGENERATE_AREA)
        })
        .collect();
    if loops.is_empty() {
        return None;
    }
    Region::new(loops)
        .ok()
        .filter(|r| !r.is_empty())
}

/// The first point and edge index where ray `o + t·d` (t > 0) hits a
/// polygon ring; `None` when it misses (or runs along the ring without
/// reaching it). The run-out's geometric question, verbatim.
pub(super) fn ray_hit(ring: &[Point2], o: Point2, d: Point2) -> Option<(Point2, usize)> {
    let mut best: Option<(f64, usize)> = None;
    let n = ring.len();
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        let e = Point2::new(b.x - a.x, b.y - a.y);
        let den = d.x * e.y - d.y * e.x;
        if den.abs() < 1e-12 {
            continue; // parallel; an overlapping collinear hit is the ambiguity's own job to name
        }
        let oa = Point2::new(a.x - o.x, a.y - o.y);
        let t = (oa.x * e.y - oa.y * e.x) / den;
        let s = (oa.x * d.y - oa.y * d.x) / den;
        if t > 1e-9 && (0.0..=1.0).contains(&s) && best.is_none_or(|(bt, _)| t < bt) {
            best = Some((t, i));
        }
    }
    let (t, i) = best?;
    Some((Point2::new(o.x + t * d.x, o.y + t * d.y), i))
}

/// Walk `ring` from the edge index+hit of one point to that of another,
/// forward along the ring's own order or back against it. Returns the
/// strict interior vertices, then the destination point - what a
/// closure needs between the two ray hits. `None` on a degenerate ring.
pub(super) fn boundary_path(
    ring: &[Point2],
    from: (usize, Point2),
    to: (usize, Point2),
    forward: bool,
) -> Option<Vec<Point2>> {
    let n = ring.len();
    if n < 3 {
        return None;
    }
    let mut out = Vec::new();
    if from.0 == to.0 {
        // Same edge: forward = the whole ring from here and back to the
        // destination; reverse = nothing between (the direct hop).
        if forward {
            for k in 1..=n {
                out.push(ring[(from.0 + k) % n]);
            }
        }
        out.push(to.1);
        return Some(out);
    }
    if forward {
        let mut i = (from.0 + 1) % n;
        loop {
            out.push(ring[i]);
            if i == to.0 {
                break;
            }
            i = (i + 1) % n;
        }
    } else {
        // Backward: the destination sits mid-edge, entered against the
        // ring's order from that edge's END vertex - walking to the one
        // before it would step past the hit and force the closing edge
        // to jump back over the interior.
        let mut i = from.0;
        loop {
            out.push(ring[i]);
            if i == (to.0 + 1) % n {
                break;
            }
            i = (i + n - 1) % n;
        }
    }
    out.push(to.1);
    Some(out)
}
