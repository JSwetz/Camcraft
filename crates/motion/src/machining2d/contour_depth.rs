//! The contour's per-ring depth ladder against the part solid: a ring
//! sectioned off the model at the top plane is walked only at the levels
//! where the wall it trims still exists - a blind bore's ring ends at its
//! floor (plus the stock to leave), a through feature and an outer wall
//! keep every level. Specced from the 2.5D profiling rule of published
//! machining practice (a profile pass never descends below the floor of
//! the feature it finishes) read against ALL the part material standing
//! above each plane - its section plus the shadow of what hangs over it
//! (`slice::material_overhead`'s law, read in its two halves), so a ring
//! under an overhang or a part's underside is never walked through it.
//! No vendor material consulted.

use std::collections::HashMap;

use crate::geo2d::{Point2, Region};
use crate::geometry::Triangle;
use crate::geometry::slice::{material_above, shadow_above};

/// Radial slack of the ring-legality test, mm: the offset ring's round
/// joins sit a chord sag inside the exact offset; anything nearer the
/// part than `r + leave - LEGAL_SLACK` means the wall moved in.
const LEGAL_SLACK: f64 = 0.02;
/// Two heights closer than this are one level (a floor that lands on an
/// existing level never doubles it).
const SAME_LEVEL: f64 = 1e-6;
/// Probe heights per ring-level beyond which the flank check stops
/// sampling vertex planes (a dense curved mesh): bounded work, and the
/// verifier's model law still judges the posted path.
const MAX_FLANK_PROBES: usize = 256;

/// The ladder the contour walks: every level any ring is cut at
/// (descending), and per ring the levels it is cut at.
pub(super) struct Ladder {
    pub zs: Vec<f64>,
    pub per_ring: Vec<Vec<f64>>,
}

impl Ladder {
    /// Every ring at every level - the flat-region law, byte for byte.
    pub(super) fn uniform(zs: &[f64], rings: usize) -> Self {
        Ladder { zs: zs.to_vec(), per_ring: vec![zs.to_vec(); rings] }
    }

    /// Whether ring `li` is cut at level `z`.
    pub(super) fn walks(&self, li: usize, z: f64) -> bool {
        self.per_ring.get(li).is_some_and(|v| v.iter().any(|q| q.to_bits() == z.to_bits()))
    }

    /// Ring `li`'s own levels (descending).
    pub(super) fn ring(&self, li: usize) -> &[f64] {
        self.per_ring.get(li).map_or(&[], |v| v.as_slice())
    }
}

fn dist2_seg(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (qx, qy) = (a.x + dx * t - p.x, a.y + dy * t - p.y);
    qx * qx + qy * qy
}

/// The ring's samples: its vertices and segment midpoints.
fn samples(ring: &[Point2]) -> Vec<Point2> {
    let mut v = Vec::with_capacity(ring.len() * 2);
    for w in ring.windows(2) {
        v.push(w[0]);
        v.push(Point2::new((w[0].x + w[1].x) * 0.5, (w[0].y + w[1].y) * 0.5));
    }
    if let Some(&l) = ring.last() {
        v.push(l);
    }
    v
}

/// The part material standing above a plane, in its two halves: the
/// cross-section and the shadow of the down-facing faces above it.
/// Kept apart (not united) so a ring clear of a part solid down to the
/// plane reads exactly as before - the shadow only ever adds a veto.
struct Above {
    section: Region,
    shadow: Region,
}

/// Material above each plane, cached by plane; `None` = a slice failed,
/// which every reader treats as "not legal" (fail closed).
struct Sections<'a> {
    facets: &'a [Triangle],
    cache: HashMap<u64, Option<Above>>,
}

/// Every sample keeps `need2` (squared) off the region and out of it.
fn clear_of(s: &Region, pts: &[Point2], need2: f64) -> bool {
    if s.is_empty() {
        return true;
    }
    pts.iter().all(|&p| {
        !s.contains(p)
            && s.loops().iter().all(|l| {
                let q = l.points();
                (0..q.len()).all(|k| dist2_seg(p, q[k], q[(k + 1) % q.len()]) >= need2)
            })
    })
}

impl Sections<'_> {
    fn at(&mut self, z: f64) -> Option<&Above> {
        let f = self.facets;
        self.cache
            .entry(z.to_bits())
            .or_insert_with(|| {
                let section = material_above(f, z).ok()?;
                let shadow = shadow_above(f, z).ok()?;
                Some(Above { section, shadow })
            })
            .as_ref()
    }

    /// The ring keeps `need` off the part's material strictly above `z`:
    /// out of its section and out of the shadow of what hangs above.
    /// (Clear of both halves is clear of their union: outside each, the
    /// union's boundary distance is the nearer of the two.)
    fn clear(&mut self, pts: &[Point2], z: f64, need: f64) -> bool {
        let Some(a) = self.at(z) else { return false };
        let need2 = need.max(0.0) * need.max(0.0);
        clear_of(&a.section, pts, need2) && clear_of(&a.shadow, pts, need2)
    }
}

/// The part's distinct vertex heights, ascending.
fn vertex_heights(facets: &[Triangle]) -> Vec<f64> {
    let mut v: Vec<f64> = facets
        .iter()
        .flat_map(|t| [t.a.z, t.b.z, t.c.z])
        .filter(|z| z.is_finite())
        .collect();
    v.sort_by(f64::total_cmp);
    v.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);
    v
}

/// Is the ring legal with its tip at `z` (the floor probed `leave` below
/// it) and its flank up to `above` (every vertex plane in between)?
fn legal_at(
    sec: &mut Sections,
    pts: &[Point2],
    heights: &[f64],
    z: f64,
    above: f64,
    leave: f64,
    need: f64,
) -> bool {
    if !sec.clear(pts, z - leave, need) {
        return false;
    }
    let flank: Vec<f64> = heights
        .iter()
        .copied()
        .filter(|&v| v > z - leave && v < above)
        .take(MAX_FLANK_PROBES)
        .collect();
    flank.into_iter().all(|v| sec.clear(pts, v, need))
}

/// Build the ladder for `rings` (closed center polylines, contour order)
/// over the flat ladder `zs` against the part `facets`. A ring stops at
/// the first level it is not legal at; the deepest legal vertex plane
/// above that level (plus `leave`) becomes its floor level. Each dropped
/// range is named in `notes`.
pub(super) fn ladder(
    rings: &[Vec<Point2>],
    facets: &[Triangle],
    zs: &[f64],
    r: f64,
    leave: f64,
    notes: &mut Vec<String>,
) -> Ladder {
    let need = r + leave - LEGAL_SLACK;
    let heights = vertex_heights(facets);
    let mut sec = Sections { facets, cache: HashMap::new() };
    let mut per_ring: Vec<Vec<f64>> = Vec::with_capacity(rings.len());
    for ring in rings {
        let pts = samples(ring);
        let mut mine: Vec<f64> = Vec::new();
        let mut above = f64::INFINITY;
        let mut stop: Option<usize> = None;
        for (k, &z) in zs.iter().enumerate() {
            if legal_at(&mut sec, &pts, &heights, z, above, leave, need) {
                mine.push(z);
                above = z;
            } else {
                stop = Some(k);
                break;
            }
        }
        if let Some(k) = stop {
            let z_bad = zs[k];
            // The floor: the deepest legal vertex plane + leave strictly
            // between the failed level and the last walked one.
            let top = if mine.is_empty() { f64::NEG_INFINITY } else { above };
            let mut floor: Option<f64> = None;
            if !mine.is_empty() {
                for &v in heights.iter().rev() {
                    let c = v + leave;
                    if c >= top - SAME_LEVEL || c <= z_bad + SAME_LEVEL {
                        continue;
                    }
                    if legal_at(&mut sec, &pts, &heights, c, top, leave, need) {
                        floor = Some(c);
                    } else {
                        break;
                    }
                }
            }
            if let Some(c) = floor {
                mine.push(c);
            }
            let skipped: Vec<String> = zs[k..].iter().map(|z| format!("{z:.3}")).collect();
            let at = ring.first().copied().unwrap_or(Point2::new(0.0, 0.0));
            notes.push(match mine.last() {
                Some(end) => format!(
                    "depth: the ring at ({:.3}, {:.3}) ends at its floor Z {end:.3} - the part is \
                     solid below it; levels below skipped ({})",
                    at.x,
                    at.y,
                    skipped.join(", ")
                ),
                None => format!(
                    "depth: the ring at ({:.3}, {:.3}) meets the part at its first level - not \
                     walked at any level ({})",
                    at.x,
                    at.y,
                    skipped.join(", ")
                ),
            });
        }
        per_ring.push(mine);
    }
    let mut all: Vec<f64> = per_ring.iter().flatten().copied().collect();
    all.sort_by(|a, b| b.total_cmp(a));
    all.dedup_by(|a, b| a.to_bits() == b.to_bits());
    Ladder { zs: all, per_ring }
}
