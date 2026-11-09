//! The ridge rest pass of the envelope scan (M2.2a / M13.8 / #212): the band's two-contact rest and the flank's scallop, read within one ball diameter between two passes.
//!
//! Split from `scan.rs` by #212; the verdict's law text stays there,
//! the reach and the accessibility test in `ridge.rs`. The flank law
//! (#212) reads a column past the band as scallop when it stands
//! BETWEEN two passes within one ball diameter (`ridge::reach`) -
//! the cusp's own geometry, `flank` - so a too-wide spacing is read
//! all the way to the generator's `2r` ceiling, while a crown above
//! every pass or an edge the passes run beside stays rest's ground.
//!
//! Provenance: the scallop (cusp) of adjacent ball-end passes is the
//! ridge where the two balls meet, `h = r - sqrt(r² - (s/2)²)` on flat
//! ground (machining handbooks; Vickers & Quan, "Ball-mills versus
//! end-mills for curved surface machining", ASME J. Eng. Ind. 1989);
//! the foot of a point on a curve is its nearest point (classical
//! differential geometry). Our own law; no vendor material consulted.

use super::ridge::Access;
use super::scan_arc::{seg_dist, ScanArc};
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};

/// The lattice the scan reads the part on.
pub(super) struct Lattice<'a> {
    pub part: &'a [f64],
    pub x0: f64,
    pub y0: f64,
    pub cell: f64,
    pub nx: usize,
    pub ny: usize,
}

/// What the ridge pass read: the band's two-contact rest and the
/// flanked scallop, each with its place.
#[derive(Default)]
pub(super) struct RidgeRead {
    pub rest_mm: f64,
    pub rest_at: Option<Point2>,
    pub scallop_mm: f64,
    pub scallop_at: Point2,
}

type Seg = (Point3, Point3);
type Box2 = (f64, f64, f64, f64);

/// One column's walk over the segments within the band: the band
/// contacts, and the nearest centre distance when one is inside it
/// (past the band the number only says "past", and `flank` reads the
/// true one).
fn walk(q3: Point3, path: &Path, band: f64) -> (f64, u8) {
    let (mut dmin, mut near) = (f64::INFINITY, 0u8);
    for (i, ((a, b), (x0, x1, y0, y1))) in path.segs.iter().zip(&path.boxes).enumerate() {
        if q3.x < x0 - band || q3.x > x1 + band || q3.y < y0 - band || q3.y > y1 + band {
            continue;
        }
        let d = seg_dist(q3, &(*a, *b), path.arc(i));
        if d <= band {
            near = near.saturating_add(1);
        }
        dmin = dmin.min(d);
    }
    (dmin, near)
}

/// The centre path as the flank reads it: segments in stream
/// order, their XY boxes and closed-ring ids.
struct Path<'a> {
    segs: &'a [Seg],
    arcs: &'a [Option<ScanArc>],
    rings: &'a [u32],
    boxes: Vec<Box2>,
}

impl<'a> Path<'a> {
    /// Each box holds its piece: a chord's, grown by its arc's bulge
    /// (#413, `scan_arc`).
    fn new(segs: &'a [Seg], arcs: &'a [Option<ScanArc>], rings: &'a [u32]) -> Self {
        let boxes = segs
            .iter()
            .enumerate()
            .map(|(i, (a, b))| {
                let g = arcs.get(i).copied().flatten().map_or(0.0, |p| p.bulge());
                (a.x.min(b.x) - g, a.x.max(b.x) + g, a.y.min(b.y) - g, a.y.max(b.y) + g)
            })
            .collect();
        Path { segs, arcs, rings, boxes }
    }

    /// Segment `i`'s XY arc piece, when it stands for one.
    fn arc(&self, i: usize) -> Option<&ScanArc> {
        self.arcs.get(i).and_then(Option::as_ref)
    }

    /// Whether segment `i` starts where segment `i - 1` ended (a rapid
    /// between them leaves a gap: it is never a centre segment).
    fn joined(&self, i: usize) -> bool {
        match (i.checked_sub(1).and_then(|j| self.segs.get(j)), self.segs.get(i)) {
            (Some(p), Some(c)) => dist(p.1, c.0) <= 1e-9,
            _ => false,
        }
    }

    /// The foot of `q` on segment `i` when `q` stands BESIDE the pass
    /// there, None when it stands past a free end of it - the stream's
    /// first or last point, or one a rapid leaves: a cusp lies between
    /// two passes, never beyond where one of them begins or stops.
    fn side_foot(&self, q: Point3, i: usize) -> Option<Point3> {
        let (a, b) = *self.segs.get(i)?;
        let f = super::ridge::foot(q, a, b);
        if (!self.joined(i) && dist(f, a) <= 1e-9) || (!self.joined(i + 1) && dist(f, b) <= 1e-9) {
            return None;
        }
        Some(f)
    }
}

fn dist(a: Point3, b: Point3) -> f64 {
    (a.x - b.x).hypot(a.y - b.y).hypot(a.z - b.z)
}

/// The flank (#212): every centre-path segment within `reach`, the
/// nearest distance among them, and whether the point stands BETWEEN
/// two passes. Two readings make "between" a cusp's own geometry:
///
/// - **Equidistant.** Only the nearest segments count - those within
///   `slack` of the nearest distance. A scallop is the ridge where two
///   adjacent balls meet, so the column under it stands (to the
///   lattice's slack) equally far from both passes; a column much
///   nearer one pass than the other stands where that pass alone
///   decides - a crown or an edge bulging past it, never a cusp.
/// - **One foot per pass.** The nearest segments group into passes:
///   runs of consecutive, joined segments (the path never leaving the
///   nearest distance between them), merged by closed iso-Z ring
///   (`ridge::closed_ring_ids`, M13.9). Each pass stands on ONE side,
///   at its nearest point - the classical foot of a point on a curve -
///   so a pass curving round the point (a ring or helix lap round a
///   crown, a contour bending past an edge) is one side, not a fan of
///   chord feet that a 120-degree test would read as two. A pass whose
///   nearest point is a free end of the path stands past the point's
///   reach, not beside it, and gives no side.
/// - **In contact.** A cusp lies between two CONTACT paths: a pass
///   whose ball at its foot stands clear of the model by more than the
///   band (`Access::touches`) rides air or skims stock there, and the
///   material under it is the ladder's leftover - rest's ground, not
///   the spacing's.
///
/// Flanked when two passes' feet stand more than 120 degrees apart in
/// the tangent plane (M13.8's opposition: a cusp's two feet are
/// opposite, a pass's edge has all its feet on one side). Returns the
/// nearest distance when flanked.
fn flank(q3: Point3, n: [f64; 3], path: &Path, access: &Access, (band, reach, slack): (f64, f64, f64)) -> Option<f64> {
    let mut segs_in: Vec<(usize, f64)> = Vec::new();
    for (i, ((a, b), (x0, x1, y0, y1))) in path.segs.iter().zip(&path.boxes).enumerate() {
        if q3.x < x0 - reach || q3.x > x1 + reach || q3.y < y0 - reach || q3.y > y1 + reach {
            continue;
        }
        let d = seg_dist(q3, &(*a, *b), path.arc(i));
        if d <= reach {
            segs_in.push((i, d));
        }
    }
    let dmin = segs_in.iter().map(|f| f.1).fold(f64::INFINITY, f64::min);
    segs_in.retain(|f| f.1 <= dmin + slack);
    // Passes: (closed ring id or 0, nearest segment, its distance).
    let mut passes: Vec<(u32, usize, f64)> = Vec::new();
    // The previous nearest segment and the pass it joined.
    let mut prev: Option<(usize, usize)> = None;
    for &(i, d) in &segs_in {
        let ring = path.rings.get(i).copied().unwrap_or(0);
        // The run goes on when this segment follows the previous one
        // with no gap - unless it would carry one closed ring into
        // another: two rings are two passes (M13.9).
        let run = prev
            .filter(|&(p, _)| p + 1 == i && path.joined(i))
            .map(|(_, k)| k)
            .filter(|&k| passes.get(k).is_some_and(|p| p.0 == 0 || ring == 0 || p.0 == ring));
        let ring_slot = if ring != 0 { passes.iter().position(|p| p.0 == ring) } else { None };
        let slot = match ring_slot.or(run) {
            Some(k) => k,
            None => {
                if passes.len() >= MAX_PASSES {
                    return None; // a tangle, not two passes: never a verdict by default
                }
                passes.push((ring, i, d));
                passes.len() - 1
            }
        };
        if let Some(p) = passes.get_mut(slot) {
            if p.0 == 0 {
                p.0 = ring; // a run that reaches a ring is that ring
            }
            if d < p.2 {
                p.1 = i;
                p.2 = d;
            }
        }
        prev = Some((i, slot));
    }
    // Each pass's side in the tangent plane (a foot straight up the
    // normal, past a free end, or with no contact says no side).
    let sides: Vec<[f64; 3]> = passes
        .iter()
        .filter_map(|&(_, i, _)| {
            let f = path.side_foot(q3, i).filter(|f| access.touches(*f, band))?;
            let v = [f.x - q3.x, f.y - q3.y, f.z - q3.z];
            let k = v[0] * n[0] + v[1] * n[1] + v[2] * n[2];
            let t = [v[0] - k * n[0], v[1] - k * n[1], v[2] - k * n[2]];
            let l = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
            (l.is_finite() && l > 1e-9).then(|| [t[0] / l, t[1] / l, t[2] / l])
        })
        .collect();
    for (k, u) in sides.iter().enumerate() {
        for w in sides.iter().skip(k + 1) {
            if u[0] * w[0] + u[1] * w[1] + u[2] * w[2] < -0.5 {
                return Some(dmin);
            }
        }
    }
    None
}

/// The most passes one column weighs among its nearest segments.
const MAX_PASSES: usize = 64;

/// The ridge pass: every lattice column with model surface, judged
/// by the band (two contacts: rest) and, past the band, by the flank
/// (`flank`, within `ridge::reach`): the worst flanked column a
/// tangent ball can reach is the scallop; an unreachable one is a
/// crease no spacing would reach, excused by name in `ridge`.
pub(super) fn ridge_rest(
    facets: &[Triangle],
    segs: &[Seg],
    arcs: &[Option<ScanArc>],
    rings: &[u32],
    lat: &Lattice,
    (r, tol, floor): (f64, f64, f64),
) -> RidgeRead {
    let band = r + tol + floor;
    let reach = super::ridge::reach(r, band, tol);
    // The lattice's own slack: a column up to half a cell off the
    // cusp's ridge stands up to one cell nearer one pass than the
    // other - never narrower than the M13.8 window's `3·tol` past the
    // band, so no flank the old window read is lost.
    let slack = lat.cell.max(3.0 * tol);
    let path = Path::new(segs, arcs, rings);
    let access = Access::new(facets, r);
    let mut out = RidgeRead::default();
    let mut flanked: Vec<(f64, Point3, [f64; 3])> = Vec::new();
    for iy in 0..lat.ny {
        for ix in 0..lat.nx {
            let Some(&t) = lat.part.get(ix + iy * lat.nx) else {
                continue;
            };
            if !t.is_finite() {
                continue; // no model surface at this column
            }
            let q3 = Point3::new(
                lat.x0 + (ix as f64 + 0.5) * lat.cell,
                lat.y0 + (iy as f64 + 0.5) * lat.cell,
                t,
            );
            let (dmin, near) = walk(q3, &path, band);
            if near < 2 {
                if dmin > band {
                    let n = super::ridge::column_normal(lat.part, lat.nx, lat.ny, ix, iy, lat.cell);
                    if let Some(d) = flank(q3, n, &path, &access, (band, reach, slack))
                        && d > band
                    {
                        flanked.push((d - r - floor, q3, n)); // judged below, worst first
                    }
                }
                continue; // one graze or none: no promise kept or broken here
            }
            let rest = dmin - r - floor;
            if rest > out.rest_mm {
                out.rest_mm = rest;
                out.rest_at = Some(Point2::new(q3.x, q3.y));
            }
        }
    }
    if !flanked.is_empty() {
        flanked.sort_by(|a, b| b.0.total_cmp(&a.0));
        if let Some((rest, q3, _)) = flanked.iter().find(|(_, q3, n)| access.reachable(*q3, *n, r, tol)) {
            out.scallop_mm = *rest;
            out.scallop_at = Point2::new(q3.x, q3.y);
        }
    }
    out
}
