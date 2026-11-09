//! Separation of two convex shapes by the Gilbert-Johnson-Keerthi
//! search (#339): is there a plane holding them apart by more than a
//! margin? The narrow phase of the fixture mesh law - a swept tool band
//! against one triangle, or against one BVH node's box.
//!
//! Provenance: Gilbert, Johnson, Keerthi, "A fast procedure for
//! computing the distance between complex objects in three-dimensional
//! space", IEEE J. Robotics and Automation 4(2), 1988; the lower-bound
//! termination of van den Bergen, "A Fast and Robust GJK Implementation
//! for Collision Detection of Convex Objects", J. Graphics Tools 4(2),
//! 1999; the closest-point-on-simplex routines of Ericson, "Real-Time
//! Collision Detection", 2005, ch. 5.1. Our own code.
//!
//! ## Why the answer is conservative
//!
//! "Separated" is only ever returned on a PROOF: a direction `v` and a
//! support point `w` of the difference set A - B in direction `-v`, with
//! `v.w / |v| > margin`. That is the supporting-hyperplane bound - every
//! point x of A - B has `x.v >= w.v` - so every point of A sits more than
//! `margin` from every point of B. The proof needs nothing from the
//! simplex arithmetic: rounding or a degenerate simplex there can only
//! slow the search or end it early, and an unfinished search answers
//! "not separated" (a hit). The supports below are exact for the shapes
//! they name, so the bound is exact up to float dust far below
//! `FIXTURE_HIT_TOL`.

pub(super) type V = [f64; 3];

pub(super) fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(super) fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(super) fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(super) fn cross(a: V, b: V) -> V {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub(super) fn scale(a: V, s: f64) -> V {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn neg(a: V) -> V {
    [-a[0], -a[1], -a[2]]
}

/// A convex shape known by its support mapping: the point of the shape
/// farthest along `d`.
pub(super) trait Support {
    fn support(&self, d: V) -> V;
}

/// A tool band swept along a straight tip segment: the cylinder of
/// radius `r` about the unit axis `a`, standing `lo..hi` along it from
/// the tip, carried from `p0` to `p1` with its axis held. Exactly the
/// Minkowski sum segment + axial segment + disc normal to `a` - convex,
/// so its support is the sum of the three supports. `a` = +Z is the
/// upright band of the prismatic laws (#339 slice 1); any other unit
/// axis is a posed band (slice 3).
#[derive(Clone, Copy, Debug)]
pub(super) struct SweptBand {
    pub p0: V,
    pub p1: V,
    pub a: V,
    pub lo: f64,
    pub hi: f64,
    pub r: f64,
}

/// The upright axis.
pub(super) const UP: V = [0.0, 0.0, 1.0];

impl Support for SweptBand {
    fn support(&self, d: V) -> V {
        let run = sub(self.p1, self.p0);
        let mut p = if dot(d, run) > 0.0 { self.p1 } else { self.p0 };
        if self.a == UP {
            // The upright band, float for float as slice 1 wrote it.
            p[2] += if d[2] > 0.0 { self.hi } else { self.lo };
            let h = d[0].hypot(d[1]);
            if h > 0.0 && h.is_finite() {
                p[0] += self.r * d[0] / h;
                p[1] += self.r * d[1] / h;
            }
            return p;
        }
        let da = dot(d, self.a);
        p = add(p, scale(self.a, if da > 0.0 { self.hi } else { self.lo }));
        // The disc's support: `d` with its axial part removed, scaled
        // to the radius (none when `d` runs along the axis).
        let perp = sub(d, scale(self.a, da));
        let h = dot(perp, perp).sqrt();
        if h > 0.0 && h.is_finite() {
            p = add(p, scale(perp, self.r / h));
        }
        p
    }
}

impl SweptBand {
    /// The band's axis-aligned bounds: the parallelogram's corners
    /// (segment + axial segment), grown per axis by the disc's own
    /// extent `r * sqrt(1 - a_k^2)`.
    pub(super) fn bounds(&self) -> (V, V) {
        let mut lo = [0.0; 3];
        let mut hi = [0.0; 3];
        for k in 0..3 {
            let (s0, s1) = (self.a[k] * self.lo, self.a[k] * self.hi);
            let disc = self.r * (1.0 - self.a[k] * self.a[k]).max(0.0).sqrt();
            lo[k] = self.p0[k].min(self.p1[k]) + s0.min(s1) - disc;
            hi[k] = self.p0[k].max(self.p1[k]) + s0.max(s1) + disc;
        }
        (lo, hi)
    }
}

/// A triangle (or a degenerate one: the support is still its hull's).
pub(super) struct Tri(pub [V; 3]);

impl Support for Tri {
    fn support(&self, d: V) -> V {
        let [a, b, c] = self.0;
        let (da, db, dc) = (dot(a, d), dot(b, d), dot(c, d));
        if da >= db && da >= dc {
            a
        } else if db >= dc {
            b
        } else {
            c
        }
    }
}

/// An axis-aligned box.
pub(super) struct Boxed {
    pub lo: V,
    pub hi: V,
}

impl Support for Boxed {
    fn support(&self, d: V) -> V {
        [0, 1, 2].map(|k| if d[k] > 0.0 { self.hi[k] } else { self.lo[k] })
    }
}

/// Search cap: a search this long without a proof answers "hit".
const MAX_ITERS: usize = 48;
/// Relative progress under which the search has converged.
const REL_EPS: f64 = 1e-12;

/// Is there a plane holding `a` and `b` more than `margin` apart?
/// `true` only with a proof (module header); anything else - touching,
/// overlapping, non-finite input, an exhausted search - is `false`.
pub(super) fn separated(a: &impl Support, b: &impl Support, margin: f64) -> bool {
    let cso = |d: V| sub(a.support(d), b.support(neg(d)));
    let mut v = cso([1.0, 0.0, 0.0]);
    let mut simplex: Vec<V> = Vec::with_capacity(4);
    for _ in 0..MAX_ITERS {
        let vv = dot(v, v);
        if !vv.is_finite() || vv <= 1e-30 {
            return false;
        }
        let w = cso(neg(v));
        let vw = dot(v, w);
        if !vw.is_finite() {
            return false;
        }
        // The proof: the whole difference set lies past the plane
        // x.v = w.v, which stands vw/|v| from the origin.
        if vw > 0.0 && vw > margin * vv.sqrt() {
            return true;
        }
        if vv - vw <= REL_EPS * vv {
            return false; // converged within the margin: contact
        }
        simplex.push(w);
        match closest(&mut simplex) {
            Some(nv) => v = nv,
            None => return false,
        }
    }
    false
}

/// The point of the simplex's hull closest to the origin, reducing the
/// simplex to the smallest face holding it. None: the origin is inside
/// a solid tetrahedron (overlap).
fn closest(s: &mut Vec<V>) -> Option<V> {
    match s.len() {
        1 => Some(s[0]),
        2 => {
            let (p, keep) = on_segment(s[0], s[1]);
            *s = keep;
            Some(p)
        }
        3 => {
            let (p, keep) = on_triangle(s[0], s[1], s[2]);
            *s = keep;
            Some(p)
        }
        _ => on_tetra(s),
    }
}

fn on_segment(a: V, b: V) -> (V, Vec<V>) {
    let ab = sub(b, a);
    let den = dot(ab, ab);
    let t = if den > 0.0 { (-dot(a, ab) / den).clamp(0.0, 1.0) } else { 0.0 };
    if t <= 0.0 {
        (a, vec![a])
    } else if t >= 1.0 {
        (b, vec![b])
    } else {
        (add(a, scale(ab, t)), vec![a, b])
    }
}

/// Ericson's ClosestPtPointTriangle (Voronoi regions), with the origin
/// as the query point, returning the feature that holds the answer.
fn on_triangle(a: V, b: V, c: V) -> (V, Vec<V>) {
    let (ab, ac, ap) = (sub(b, a), sub(c, a), neg(a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return (a, vec![a]);
    }
    let bp = neg(b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return (b, vec![b]);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let t = d1 / (d1 - d3);
        return (add(a, scale(ab, t)), vec![a, b]);
    }
    let cp = neg(c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return (c, vec![c]);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let t = d2 / (d2 - d6);
        return (add(a, scale(ac, t)), vec![a, c]);
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let t = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (add(b, scale(sub(c, b), t)), vec![b, c]);
    }
    let den = va + vb + vc;
    if den.abs() <= 0.0 || !den.is_finite() {
        // A collinear simplex: the best of its edges.
        return best_edge(a, b, c);
    }
    let (v, w) = (vb / den, vc / den);
    (add(a, add(scale(ab, v), scale(ac, w))), vec![a, b, c])
}

fn best_edge(a: V, b: V, c: V) -> (V, Vec<V>) {
    let cands = [on_segment(a, b), on_segment(b, c), on_segment(a, c)];
    let mut best = cands[0].clone();
    for cand in cands.into_iter().skip(1) {
        if dot(cand.0, cand.0) < dot(best.0, best.0) {
            best = cand;
        }
    }
    best
}

/// The closest point of a tetrahedron: the origin strictly inside all
/// four face planes is overlap (None); otherwise the nearest of the
/// faces the origin lies outside of - every face when the tetrahedron
/// is flat, so a degenerate simplex still answers its hull's point.
fn on_tetra(s: &mut Vec<V>) -> Option<V> {
    let (a, b, c, d) = (s[0], s[1], s[2], s[3]);
    let faces = [(a, b, c, d), (a, c, d, b), (a, d, b, c), (b, d, c, a)];
    let mut outside_any = false;
    let mut best: Option<(V, Vec<V>)> = None;
    for &(p, q, r, opp) in &faces {
        let n = cross(sub(q, p), sub(r, p));
        let side_o = dot(neg(p), n);
        let side_opp = dot(sub(opp, p), n);
        let flat = side_opp.abs() <= 1e-18 * (dot(n, n).sqrt() + 1.0);
        if flat || side_o * side_opp < 0.0 {
            outside_any = true;
            let cand = on_triangle(p, q, r);
            if best.as_ref().is_none_or(|b| dot(cand.0, cand.0) < dot(b.0, b.0)) {
                best = Some(cand);
            }
        }
    }
    if !outside_any {
        return None;
    }
    let (p, keep) = best?;
    *s = keep;
    Some(p)
}
