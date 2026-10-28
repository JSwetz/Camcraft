//! Split from `stock_sim.rs` by M5.0.1; structure only.

use super::kernel::{MAX_CELLS_PER_AXIS, StockError, part_surface};
use super::dexel::fy0;

use crate::cl::{Move, MoveKind};
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};
/// The envelope verdict for a finishing path (M2.2a), on a stated
/// lattice, measured not assumed. Two truths, each read by the law
/// that is exact for it. Gouge is facet truth (M5.2 E): the exact
/// distance from each model triangle to each instant's ball center,
/// and `r` minus it where positive - a surface standing inside the
/// swept envelope. A column floor cannot say that honestly (it is
/// inflated by 1/cos on tilts, blind on walls, and convicts a pass
/// running legitimately under an overhang); two convex sets can.
/// Rest is distance truth at RIDGES: a
/// model surface point that stands within the pass's promise band
/// (`r + tol + floor` of at least two chords the center path ran
/// over), and how far outside every ball it stood. Two band contacts
/// is exactly what "this op rolled a stepdown ladder over you"
/// promises, so what stands outside both is scallop left on the
/// pass's own word. A point with one graze (a lone ring's annulus,
/// the box's footprint square) or none (a flat shelf, the crown
/// above the top ring, untouched walls) was never promised anything
/// by this pass: its leftover is rest machining's jurisdiction
/// (M2.3), law there, not a silence here. Gating rest by "this pass
/// cut this column" instead was tried and is dead code - a ball
/// reaching a column's surface is the same event as a center standing
/// within `r + floor` of that point (prove it: `cut <= t + floor`
/// rearranges to `dist^2 <= (r + floor)^2`), so it can never report
/// above tolerance. Proved, not observed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EnvelopeReport {
    /// Worst depth by which the model surface stood INSIDE the swept
    /// envelope (`r - distance`, and a path that buried its centers in
    /// material reads the saturation value `r` - as deep as a distance
    /// measurement can say). 0.0 when the envelope never reached the
    /// surface. Not a trench depth: the column's "how far below the
    /// wanted surface" phrasing died with the column law (M5.2 E).
    pub gouge_mm: f64,
    /// Worst distance the model surface stood OUTSIDE the swept
    /// envelope at a ridge the pass promised to roll over (two band
    /// contacts; single grazes and untouched columns are law, not
    /// this number). M2.2d: the union with the solid truth - material
    /// standing hidden under an occluding top grows this number too,
    /// measured by the same distance law. The union is in the FIELD
    /// only; the ridge predicate above is untouched by it.
    pub rest_mm: f64,
    /// The lattice's cell size, so a caller can state the
    /// quantization floor of these numbers honestly.
    pub cell: f64,
    /// XY of the largest single deviation.
    pub at: Point2,
}

/// The envelope verdict for a ball-mill path: the moves carry TIP
/// positions (the whole codebase's convention) and a ball's tip sits
/// `r` under its center, so the centers are the tips lifted by `r`.
pub fn envelope_scan(
    facets: &[Triangle],
    moves: &[Move],
    r: f64,
    tol: f64,
) -> Result<EnvelopeReport, StockError> {
    envelope_scan_lift(facets, moves, r, tol, r)
}

/// The same law for a path whose positions ARE the envelope centers
/// (M5.2 E): a turning generator walks its nose CENTER along the
/// offset, so the mill's `+r` tip lift would tilt every ball off the
/// skin it rode - exactly `r·(1 - 1/cos n̂_z)` INTO a face whose
/// material stands above it (the chamfer's phantom gouge) and the same
/// depth OUT of every face tilting the other way (a blindness no
/// passing test showed, because a vacuous clean passes). One
/// convention per kind, chosen by the caller that knows the kind.
pub fn envelope_scan_centers(
    facets: &[Triangle],
    moves: &[Move],
    r: f64,
    tol: f64,
) -> Result<EnvelopeReport, StockError> {
    envelope_scan_lift(facets, moves, r, tol, 0.0)
}

fn envelope_scan_lift(
    facets: &[Triangle],
    moves: &[Move],
    r: f64,
    tol: f64,
    center_lift: f64,
) -> Result<EnvelopeReport, StockError> {
    if !(r.is_finite() && r > 0.0) {
        return Err(StockError::Invalid(format!(
            "envelope ball radius {r} is not finite-positive"
        )));
    }
    if !tol.is_finite() {
        return Err(StockError::Invalid(format!(
            "envelope tolerance {tol} is not finite"
        )));
    }
    let tol = tol.max(0.0);
    let (mut bx0, mut by0, mut bx1, mut by1) =
        (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for t in facets {
        for v in [t.a, t.b, t.c] {
            if !(v.x.is_finite() && v.y.is_finite() && v.z.is_finite()) {
                return Err(StockError::Invalid("model has a non-finite vertex".into()));
            }
            bx0 = bx0.min(v.x);
            bx1 = bx1.max(v.x);
            by0 = by0.min(v.y);
            by1 = by1.max(v.y);
        }
    }
    if facets.is_empty() || bx1 <= bx0 || by1 <= by0 {
        return Ok(EnvelopeReport::default()); // no footprint to measure on
    }
    // One eighth of the tool radius sees a scallop the tolerance must
    // keep; the floor and the span growth keep the grid honest on
    // dust motes and stadiums alike (MAX_CELLS discipline, applied
    // before allocation, never after).
    let mut cell = (r * 0.125).max(1e-4);
    cell = cell.max((bx1 - bx0) / 1023.0).max((by1 - by0) / 1023.0);
    let margin = r + cell;
    let (x0, y0) = (bx0 - margin, by0 - margin);
    let nx = (((bx1 - bx0 + 2.0 * margin) / cell).ceil() as usize).max(1);
    let ny = (((by1 - by0 + 2.0 * margin) / cell).ceil() as usize).max(1);
    if nx > MAX_CELLS_PER_AXIS || ny > MAX_CELLS_PER_AXIS {
        return Err(StockError::Invalid(format!(
            "envelope grid would be {nx}x{ny} cells; at most {MAX_CELLS_PER_AXIS} per axis"
        )));
    }
    // The arc tessellation of the measurement: the generator's own
    // chord law - every XY shortcut keeps tolerance/4 (the march law,
    // the ring law) - so the scan's discretization error never
    // outweighs the promise it is measuring. A cell-derived budget
    // was tried: at the r/8 lattice it shortens an arc by a sagitta
    // of cell/4, which exceeds the tol + floor gate itself, and a
    // tangential ring's chords then dig a phantom trench the emitted
    // G2 arc never cut. This budget only ever tightens the yardstick:
    // the machine follows the true arc, chord or not.
    let chord_tol = (tol * 0.25).max(1e-9);
    let segs = center_segments(moves, center_lift, chord_tol);
    let part = part_surface(facets, x0, y0, cell, nx, ny);
    let floor = cell * cell / (2.0 * r); // the sagitta one cell can hide
    let mut rep = EnvelopeReport {
        cell,
        at: Point2::new(0.0, 0.0),
        ..Default::default()
    };
    let mut worst = 0.0f64;
    // Gouge: the facet truth (M5.2 E). Every model triangle against
    // every ball-center instant, by the exact distance of two convex
    // sets - no column, no floor, no height field in the verdict. A
    // flush ride reads 0 by construction (distance == r); a tangent
    // chamfer ride under its own top cap reads 0, because no ball
    // ever reached inside the cap's facets; and the r/2 mixup still
    // reads its ~3 mm, because distance is the whole story. The Dexel
    // sweep that answered this question before is dead in the scan -
    // it inflated tilts by 1/cos, saw nothing on walls, and convicted
    // legitimate under-passes, and no column reading can be fixed
    // into honesty from those.
    let (gouge, gouge_at) = super::gouge::facet_gouge(facets, &segs, r, x0, y0, cell, nx, ny)?;
    rep.gouge_mm = gouge;
    if gouge > worst {
        worst = gouge;
        rep.at = gouge_at;
    }
    // Rest: the distance truth at ridges - surface points the center
    // path passed by twice inside the promise band. The band is what
    // the stepdown law swore, so a point outside every ball there is
    // this path's own failure, while a once-grazed or never-touched
    // point is another op's job (the law above).
    let band = r + tol + floor;
    // A far segment's 3D distance cannot stand inside the band, so
    // its XY window rejects it before the distance is paid. A
    // column's dmin only feeds a report when two segments proved
    // near, and those are never the skipped ones - the filter is
    // exact, not an approximation.
    let boxes: Vec<(f64, f64, f64, f64)> = segs
        .iter()
        .map(|(a, b)| (a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y)))
        .collect();
    for iy in 0..ny {
        for ix in 0..nx {
            let t = part[ix + iy * nx];
            if !t.is_finite() {
                continue; // no model surface at this column
            }
            let q = Point2::new(x0 + (ix as f64 + 0.5) * cell, y0 + (iy as f64 + 0.5) * cell);
            let (mut dmin, mut near) = (f64::INFINITY, 0u8);
            for ((a, b), (x0, x1, y0, y1)) in segs.iter().zip(&boxes) {
                if q.x < x0 - band
                    || q.x > x1 + band
                    || q.y < y0 - band
                    || q.y > y1 + band
                {
                    continue;
                }
                let d = dist_pt_seg3(q.x, q.y, t, *a, *b);
                if d <= band {
                    near = near.saturating_add(1);
                }
                if d < dmin {
                    dmin = d;
                }
            }
            if near < 2 {
                continue; // one graze or none: no promise kept or broken here
            }
            let rest = dmin - r - floor;
            if rest > rep.rest_mm {
                rep.rest_mm = rest;
                if rest > worst {
                    worst = rest;
                    rep.at = q;
                }
            }
        }
    }
    // Solid truth behind the height field (M2.2d): material tops the
    // visible surface OCCLUDES - a ceiling's shadow over a floor -
    // judged by this same distance law. The union lives in the FIELD:
    // the ridge numbers above stand as computed, predicate untouched.
    let (hidden, hidden_at) = hidden_rest(facets, &segs, &part, x0, y0, cell, nx, ny, r, floor);
    if hidden > rep.rest_mm {
        rep.rest_mm = hidden;
        rep.at = hidden_at;
    }
    Ok(rep)
}

/// The hidden-rest scan (M2.2d), paying M2.2b's open debt. Per
/// column the mesh's SOLID truth is paired (`solid_rest` law (a));
/// every material top below the height field's own top minus the
/// quantization floor is occluded material - invisible to a
/// Z-following measure by construction. Each occluded top is then
/// judged by the ridge law's own distance truth: nearest ball
/// center minus the ball minus the floor, so the two reports are
/// one yardstick reading two depths, never two predicates. An odd
/// column is an open shell - unknown, and unknown never reports
/// material (law (b)). Work is binned facet-outer (the
/// `part_surface` shape); over budget the answer is the stated
/// zero - the height field's debt stays on the books, never
/// guessed at.
#[allow(clippy::too_many_arguments)] // the scan's whole geometry frame, threaded once from envelope_scan
pub(super) fn hidden_rest(
    facets: &[Triangle],
    segs: &[(Point3, Point3)],
    part: &[f64],
    x0: f64,
    y0: f64,
    cell: f64,
    nx: usize,
    ny: usize,
    r: f64,
    floor: f64,
) -> (f64, Point2) {
    const BUDGET_CELLS: usize = 1_200_000; // ~30 MB of bin headers
    const BUDGET_CLAIMS: usize = 40_000_000; // plane claims within them
    if segs.is_empty() || facets.is_empty() || nx * ny > BUDGET_CELLS {
        return (0.0, Point2::new(0.0, 0.0));
    }
    let mut bins: Vec<Vec<crate::solid_rest::Claim>> = (0..nx * ny).map(|_| Vec::new()).collect();
    let mut claims = 0usize;
    for t in facets {
        let (a, b, c) = (t.a, t.b, t.c);
        if !(a.is_finite() && b.is_finite() && c.is_finite()) {
            continue;
        }
        let fx = |v: f64| ((v - x0) / cell).floor();
        let i0 = fx(a.x.min(b.x).min(c.x)).max(0.0) as usize;
        let i1 = fx(a.x.max(b.x).max(c.x)).clamp(0.0, nx as f64 - 1.0) as usize;
        let j0 = fy0(a.y.min(b.y).min(c.y), y0, cell).max(0.0) as usize;
        let j1 = fy0(a.y.max(b.y).max(c.y), y0, cell).clamp(0.0, ny as f64 - 1.0) as usize;
        if i0 >= nx || j0 >= ny {
            continue;
        }
        for iy in j0..=j1 {
            for ix in i0..=i1 {
                let (cx, cy) = (x0 + (ix as f64 + 0.5) * cell, y0 + (iy as f64 + 0.5) * cell);
                if let Some(cl) = crate::solid_rest::plane_claim(&a, &b, &c, cx, cy) {
                    bins[ix + iy * nx].push(cl);
                    claims += 1;
                    if claims > BUDGET_CLAIMS {
                        return (0.0, Point2::new(0.0, 0.0)); // over budget: the stated zero
                    }
                }
            }
        }
    }
    let mut best = 0.0;
    let mut at = Point2::new(0.0, 0.0);
    for iy in 0..ny {
        for ix in 0..nx {
            let bin = &bins[ix + iy * nx];
            if bin.is_empty() {
                continue;
            }
            let t = part[ix + iy * nx];
            if !t.is_finite() {
                continue; // the field sees nothing here; nor has this scan's ground
            }
            let crate::solid_rest::Crossings::Paired(z) =
                crate::solid_rest::crossings_from_claims(bin)
            else {
                continue; // open shell: unknown never reports material
            };
            // Interval tops, high to low: the highest is the visible
            // surface; every lower one hides under a top above it.
            for c in z.as_chunks::<2>().0 {
                let hi = c[1];
                if hi >= t - floor {
                    continue; // not occluded: the ridge law already owns this top
                }
                let (cx, cy) = (x0 + (ix as f64 + 0.5) * cell, y0 + (iy as f64 + 0.5) * cell);
                let mut dmin = f64::INFINITY;
                for (sa, sb) in segs {
                    let d = dist_pt_seg3(cx, cy, hi, *sa, *sb);
                    if d < dmin {
                        dmin = d;
                    }
                }
                let rest = (dmin - r - floor).max(0.0);
                if rest > best {
                    best = rest;
                    at = Point2::new(cx, cy);
                }
            }
        }
    }
    (best, at)
}

/// The ball-center polyline the path sweeps: rapids excluded, arcs
/// reduced to their sagitta chords with Z interpolated, everything
/// lifted by `center_lift` - `r` for a tip-stored ball-mill path (the
/// exact discretization `subtract_ball` cuts with), 0.0 for a turning
/// path whose positions are already nose centers (M5.2 E).
pub(super) fn center_segments(
    moves: &[Move],
    center_lift: f64,
    chord_tol: f64,
) -> Vec<(Point3, Point3)> {
    let mut segs = Vec::new();
    for m in moves {
        if m.kind == MoveKind::Rapid || !m.is_finite() {
            continue;
        }
        match m.arc {
            None => segs.push((
                Point3::new(m.from.x, m.from.y, m.from.z + center_lift),
                Point3::new(m.to.x, m.to.y, m.to.z + center_lift),
            )),
            Some(arc) => {
                let a2 = Point2::new(m.from.x, m.from.y);
                let b2 = Point2::new(m.to.x, m.to.y);
                let cs = crate::cl::arc_chords(a2, b2, arc, chord_tol);
                let n = cs.len().max(1) as f64;
                let dz = m.to.z - m.from.z;
                for (j, (ca, cb)) in cs.into_iter().enumerate() {
                    segs.push((
                        Point3::new(ca.x, ca.y, m.from.z + dz * (j as f64 / n) + center_lift),
                        Point3::new(
                            cb.x,
                            cb.y,
                            m.from.z + dz * ((j + 1) as f64 / n) + center_lift,
                        ),
                    ));
                }
            }
        }
    }
    segs
}

/// 3D point-to-segment distance - the honest "did any ball of the
/// sweep reach this surface point" test a per-column floor cannot
/// answer on tilted walls.
pub(super) fn dist_pt_seg3(px: f64, py: f64, pz: f64, a: Point3, b: Point3) -> f64 {
    let (ex, ey, ez) = (b.x - a.x, b.y - a.y, b.z - a.z);
    let len2 = ex * ex + ey * ey + ez * ez;
    let t = if len2 > 0.0 {
        ((px - a.x) * ex + (py - a.y) * ey + (pz - a.z) * ez) / len2
    } else {
        0.0
    }
    .clamp(0.0, 1.0);
    let (dx, dy, dz) = (px - a.x - ex * t, py - a.y - ey * t, pz - a.z - ez * t);
    dx.hypot(dy).hypot(dz)
}

// ---- M2.3 stage A: the program-level sim ------------------------------
