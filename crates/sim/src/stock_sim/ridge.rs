//! The ridge rest law's reach (M13.8): which surface points a finish pass FLANKS, and which of those a ball can touch at all.
//!
//! The law it serves (`scan.rs`): a model point flanked by the
//! pass - centre-path feet on opposite sides of it in its tangent
//! plane, within one ball diameter - stands between two passes, so
//! whatever stands outside every ball there is scallop the pass's own
//! spacing left. The former predicate ("within `r + tol + floor` of
//! two chords") could never report above tolerance: a point inside
//! that band is by construction at most `tol` outside the nearest
//! ball, so a too-wide stepover verified silent. Flanking reads the
//! same geometry without capping the answer.
//!
//! A flanked point the ball cannot touch - a concave crease narrower
//! than the ball, where the tangent ball along the surface normal
//! would cut a neighbouring face - is not scallop: no spacing would
//! reach it, it is pencil/rest machining's jurisdiction, and it is
//! excused by the accessibility test below.
//!
//! The feet must come from two DISTINCT passes (M13.9): a closed ring
//! smaller than the reach surrounds its own inside, and the crown nub
//! it leaves standing there is the crown above the top ring - one
//! pass's graze, rest machining's jurisdiction by the scan's own law,
//! never the spacing's cusp. The slope-aware ladder's first crown ring
//! on the hill made the scan read exactly that nub as scallop (0.127 →
//! 0.189 on hill project); `closed_ring_ids` names each closed iso-Z
//! ring in the stream so `Flank` refuses a pair of feet from one ring.
//!
//! Provenance: the cusp between adjacent ball-end passes is the
//! scallop-height geometry of machining handbooks; the accessibility
//! criterion (a surface point is ball-machinable iff the ball tangent
//! there along the normal meets no other part of the surface) is the
//! offset-surface reading of public CAM literature; point-to-triangle
//! distance per Ericson, "Real-Time Collision Detection" 5.1 (via
//! `gouge::dist_pt_tri`). Our own law; no vendor material consulted.

use super::gouge::dist_pt_tri;
use crate::cl::{Move, MoveKind};
use crate::geometry::{Point3, Triangle};

type V3 = [f64; 3];

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The flank reach: the band plus three tolerances, so a flanked
/// point reads at most `4·tol` past the ball. Two passes farther apart
/// than that around a point leave a gap, not a cusp - a missing pass,
/// another law's word (never this one's silence: the band still
/// covers everything nearer). A full ball diameter was tried and read
/// the cone apex and the box corner as scallop - unreached by any
/// spacing, other laws' ground.
///
/// Validity range (#37): on flat ground the law reads a cusp only to
/// `s ≈ 2·sqrt(reach² − r²)` (2.9 on an r3 ball at tol 0.1, a reading
/// of `4·tol`); a wider spacing is NOT read here and no other scan law
/// claims it. The generator closes the nonsensical half - a stated
/// spacing at or past the ball diameter is refused by name
/// (`finishing3d::params::scallop_ceiling`) - while widening this
/// window to `2r` was measured on 2026-11-08 and convicts the cone
/// apex (1.68 mm), the box walls (0.42) and the dome waterline's crown
/// (1.10): one-pass ground the distinct-rings rule does not excuse.
/// It grows only behind a crown/corner exclusion (owed, RECOMMENDED
/// "#37").
pub(super) fn reach(band: f64, tol: f64) -> f64 {
    band + 3.0 * tol
}

/// The height field's upward normal at a column, by central
/// differences where both neighbours carry surface, one-sided where
/// only one does, flat where neither does.
pub(super) fn column_normal(part: &[f64], nx: usize, ny: usize, ix: usize, iy: usize, cell: f64) -> V3 {
    let at = |x: usize, y: usize| part.get(x + y * nx).copied().filter(|v| v.is_finite());
    let here = at(ix, iy).unwrap_or(0.0);
    let slope = |lo: Option<f64>, hi: Option<f64>| match (lo, hi) {
        (Some(a), Some(b)) => (b - a) / (2.0 * cell),
        (Some(a), None) => (here - a) / cell,
        (None, Some(b)) => (b - here) / cell,
        (None, None) => 0.0,
    };
    let gx = slope(
        ix.checked_sub(1).and_then(|x| at(x, iy)),
        if ix + 1 < nx { at(ix + 1, iy) } else { None },
    );
    let gy = slope(
        iy.checked_sub(1).and_then(|y| at(ix, y)),
        if iy + 1 < ny { at(ix, iy + 1) } else { None },
    );
    let l = (gx * gx + gy * gy + 1.0).sqrt();
    if !l.is_finite() {
        return [0.0, 0.0, 1.0];
    }
    [-gx / l, -gy / l, 1.0 / l]
}

/// The foot of a point on a segment: its nearest point there.
pub(super) fn foot(q: Point3, a: Point3, b: Point3) -> Point3 {
    let e = [b.x - a.x, b.y - a.y, b.z - a.z];
    let len2 = dot(e, e);
    let t = if len2 > 0.0 {
        (dot([q.x - a.x, q.y - a.y, q.z - a.z], e) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    Point3::new(a.x + e[0] * t, a.y + e[1] * t, a.z + e[2] * t)
}

/// The closed iso-Z rings of a move stream, one id per move: a maximal
/// run of consecutive cutting moves (no rapid, no plunge) at one tip
/// height that comes back to its own start is a ring, numbered from 1;
/// every other move reads 0 - no ring, every foot its own pass. A
/// raster's serpentine never closes, a contour's loops do (and a
/// contour's innermost loop is under a stepover wide by construction,
/// so excusing its inside excuses nothing over tolerance).
pub(super) fn closed_ring_ids(moves: &[Move]) -> Vec<u32> {
    let mut ids = vec![0u32; moves.len()];
    let mut next = 1u32;
    let mut start = 0usize;
    while start < moves.len() {
        let z = moves[start].from.z;
        let iso = |m: &Move| {
            !matches!(m.kind, MoveKind::Rapid | MoveKind::Plunge)
                && m.is_finite()
                && (m.from.z - z).abs() <= 1e-9
                && (m.to.z - z).abs() <= 1e-9
        };
        if !iso(&moves[start]) {
            start += 1;
            continue;
        }
        let mut end = start;
        while end + 1 < moves.len()
            && iso(&moves[end + 1])
            && (moves[end + 1].from.x - moves[end].to.x).abs() <= 1e-9
            && (moves[end + 1].from.y - moves[end].to.y).abs() <= 1e-9
        {
            end += 1;
        }
        let (a, b) = (moves[start].from, moves[end].to);
        if end > start && (a.x - b.x).abs() <= 1e-6 && (a.y - b.y).abs() <= 1e-6 {
            for id in ids.iter_mut().take(end + 1).skip(start) {
                *id = next;
            }
            // The ring's own vertical entry and exit - the plunge onto
            // its start, the retract off its end - have no side of
            // their own: their feet ARE ring points, so they ride the
            // ring's id rather than stand as a second pass beside it.
            let vertical = |m: &Move, q: Point3| {
                (m.from.x - m.to.x).abs() <= 1e-9
                    && (m.from.y - m.to.y).abs() <= 1e-9
                    && (m.from.x - q.x).abs() <= 1e-9
                    && (m.from.y - q.y).abs() <= 1e-9
            };
            if start > 0 && vertical(&moves[start - 1], a) {
                ids[start - 1] = next;
            }
            if end + 1 < moves.len() && vertical(&moves[end + 1], b) {
                ids[end + 1] = next;
            }
            next = next.saturating_add(1);
        }
        start = end + 1;
    }
    ids
}

/// The tangent-plane directions from one surface point to the feet
/// within reach; flanked once two of them stand more than 120 degrees
/// apart (a cusp's two feet are opposite, a pass's edge has all its
/// feet on one side) AND come from distinct passes: two feet of one
/// closed ring (`closed_ring_ids`) surround the ring's inside, they
/// never flank it.
pub(super) struct Flank {
    n: V3,
    dirs: Vec<(V3, u32)>,
    flanked: bool,
}

impl Flank {
    // Room for one direction per ring in reach: a crown column sees
    // every ring of the ladder above the reach line, each its own.
    const MAX_DIRS: usize = 64;

    pub(super) fn new(n: V3) -> Self {
        Flank {
            n,
            dirs: Vec::new(),
            flanked: false,
        }
    }

    /// A foot `f` of the surface point `q`, from the pass `ring` (0
    /// for a move on no closed ring: every such foot is its own pass).
    pub(super) fn add(&mut self, q: Point3, f: Point3, ring: u32) {
        if self.flanked {
            return;
        }
        let v = [f.x - q.x, f.y - q.y, f.z - q.z];
        let k = dot(v, self.n);
        let t = [v[0] - k * self.n[0], v[1] - k * self.n[1], v[2] - k * self.n[2]];
        let l = dot(t, t).sqrt();
        if !(l.is_finite() && l > 1e-9) {
            return; // a foot straight up the normal says no side
        }
        let u = [t[0] / l, t[1] / l, t[2] / l];
        let mut dup = false;
        for (d, p) in &self.dirs {
            let same_ring = ring != 0 && *p == ring;
            let c = dot(*d, u);
            if c < -0.5 && !same_ring {
                self.flanked = true;
                return;
            }
            // A direction is only redundant within its own pass: the
            // same side seen from another ring is the other ring's
            // word, and it must stay on the record for ITS opposite.
            dup |= c > 0.95 && *p == ring;
        }
        if !dup && self.dirs.len() < Self::MAX_DIRS {
            self.dirs.push((u, ring));
        }
    }

    pub(super) fn flanked(&self) -> bool {
        self.flanked
    }
}

/// The ball-accessibility reader: the facets binned by XY bounds, so a
/// tangent ball asks only the facets within its own reach.
pub(super) struct Access<'a> {
    facets: &'a [Triangle],
    bins: Vec<Vec<usize>>,
    x0: f64,
    y0: f64,
    bs: f64,
    bx: usize,
    by: usize,
}

impl<'a> Access<'a> {
    const MAX_BINS: usize = 256;

    pub(super) fn new(facets: &'a [Triangle], r: f64) -> Self {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for t in facets {
            for v in [t.a, t.b, t.c] {
                x0 = x0.min(v.x);
                x1 = x1.max(v.x);
                y0 = y0.min(v.y);
                y1 = y1.max(v.y);
            }
        }
        let span = (x1 - x0).max(y1 - y0);
        let bs = r.max(span / Self::MAX_BINS as f64).max(1e-6);
        let ok = x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite();
        let bx = if ok { (((x1 - x0) / bs) as usize + 1).min(Self::MAX_BINS + 1) } else { 1 };
        let by = if ok { (((y1 - y0) / bs) as usize + 1).min(Self::MAX_BINS + 1) } else { 1 };
        let mut a = Access {
            facets,
            bins: vec![Vec::new(); bx * by],
            x0: if ok { x0 } else { 0.0 },
            y0: if ok { y0 } else { 0.0 },
            bs,
            bx,
            by,
        };
        for (i, t) in facets.iter().enumerate() {
            let (lx, hx) = (t.a.x.min(t.b.x).min(t.c.x), t.a.x.max(t.b.x).max(t.c.x));
            let (ly, hy) = (t.a.y.min(t.b.y).min(t.c.y), t.a.y.max(t.b.y).max(t.c.y));
            let (i0, i1, j0, j1) = a.span(lx, hx, ly, hy);
            for j in j0..=j1 {
                for k in i0..=i1 {
                    a.bins[k + j * a.bx].push(i);
                }
            }
        }
        a
    }

    fn span(&self, lx: f64, hx: f64, ly: f64, hy: f64) -> (usize, usize, usize, usize) {
        let ix = |x: f64| (((x - self.x0) / self.bs).floor().max(0.0) as usize).min(self.bx - 1);
        let iy = |y: f64| (((y - self.y0) / self.bs).floor().max(0.0) as usize).min(self.by - 1);
        (ix(lx), ix(hx), iy(ly), iy(hy))
    }

    /// Whether a ball of radius `r` tangent at the surface point `q`
    /// along the surface's normal there stands clear of every facet
    /// (within `slack`). The normal is the facet's own under `q`
    /// (upward), the height field's `n_fd` where no facet claims the
    /// point to the lattice's precision.
    pub(super) fn reachable(&self, q: Point3, n_fd: V3, r: f64, slack: f64) -> bool {
        let mut n = n_fd;
        let (i0, i1, j0, j1) = self.span(q.x, q.x, q.y, q.y);
        'find: for j in j0..=j1 {
            for k in i0..=i1 {
                for &fi in &self.bins[k + j * self.bx] {
                    if let Some(m) = self.facets.get(fi).and_then(|t| top_normal(t, q)) {
                        n = m;
                        break 'find;
                    }
                }
            }
        }
        let c = Point3::new(q.x + r * n[0], q.y + r * n[1], q.z + r * n[2]);
        let (i0, i1, j0, j1) = self.span(c.x - r, c.x + r, c.y - r, c.y + r);
        for j in j0..=j1 {
            for k in i0..=i1 {
                for &fi in &self.bins[k + j * self.bx] {
                    if self.facets.get(fi).is_some_and(|t| dist_pt_tri(c, t) < r - slack) {
                        return false;
                    }
                }
            }
        }
        true
    }
}

/// The upward unit normal of `t` when `q` lies on it (XY inside, Z on
/// its plane to 1e-6 mm), else None.
fn top_normal(t: &Triangle, q: Point3) -> Option<V3> {
    let (a, b, c) = (t.a, t.b, t.c);
    let det = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    if det.abs() <= f64::EPSILON {
        return None;
    }
    let u = ((q.x - a.x) * (c.y - a.y) - (q.y - a.y) * (c.x - a.x)) / det;
    let v = ((b.x - a.x) * (q.y - a.y) - (b.y - a.y) * (q.x - a.x)) / det;
    if u < -1e-9 || v < -1e-9 || u + v > 1.0 + 1e-9 {
        return None;
    }
    let z = a.z + u * (b.z - a.z) + v * (c.z - a.z);
    if (z - q.z).abs() > 1e-6 {
        return None;
    }
    let (e0, e1) = ([b.x - a.x, b.y - a.y, b.z - a.z], [c.x - a.x, c.y - a.y, c.z - a.z]);
    let mut n = [
        e0[1] * e1[2] - e0[2] * e1[1],
        e0[2] * e1[0] - e0[0] * e1[2],
        e0[0] * e1[1] - e0[1] * e1[0],
    ];
    let l = dot(n, n).sqrt();
    if !(l.is_finite() && l > 0.0) {
        return None;
    }
    if n[2] < 0.0 {
        n = [-n[0], -n[1], -n[2]];
    }
    Some([n[0] / l, n[1] / l, n[2] / l])
}
