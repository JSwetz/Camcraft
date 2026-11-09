//! The rotary wrap's developed sheet: every facet of the work laid out
//! in wrap space (s = R_ref·θ, v the station, ρ the radius) about one
//! azimuth basis, twice (the second copy one period up), with the seam
//! edges and the seam straddlers' vertices [`super::rotary_seam`] reads.
//! The basis' ±π branch is where the development is split; turning the
//! basis (issue #226) moves the split, never where the part is cut -
//! the world exit maps s back through the same basis.
//!
//! Provenance: the development of a cylinder cut along one meridian,
//! from standard differential-geometry texts; composed on this crate's
//! own wrap map ([`super::rotary_offset`]). No vendor material.

use crate::geometry::{Point3, Triangle};

use super::axis;
use super::rotary_seam::DevEdge;

const TAU: f64 = std::f64::consts::TAU;
const PI: f64 = std::f64::consts::PI;

/// One development of the work about a basis.
pub(super) struct Sheet {
    /// The developed facets, both copies.
    pub(super) wrap: Vec<Triangle>,
    /// The first copy's developed edges, three per facet.
    pub(super) edges: Vec<DevEdge>,
    /// The off-axis vertices `(azimuth, station, radius)` of every facet
    /// straddling the basis' ±π branch.
    pub(super) seam_cols: Vec<(f64, f64, f64)>,
    pub(super) s_lo: f64,
    pub(super) s_hi: f64,
}

/// The azimuth basis: `e1` is azimuth zero, `e2 = u × e1` a quarter
/// turn on, the ±π branch opposite `e1`.
#[derive(Clone, Copy)]
pub(super) struct Basis {
    pub(super) e1: Point3,
    pub(super) e2: Point3,
}

impl Basis {
    /// The basis turned `delta` radians about the axis `u`: azimuth θ
    /// here reads θ - delta in the turned one, so its branch stands at
    /// this basis' azimuth π + delta.
    pub(super) fn turned(self, u: Point3, delta: f64) -> Basis {
        let (sn, cs) = delta.sin_cos();
        let e1 = Point3::new(
            self.e1.x * cs + self.e2.x * sn,
            self.e1.y * cs + self.e2.y * sn,
            self.e1.z * cs + self.e2.z * sn,
        );
        Basis { e1, e2: u.cross(e1) }
    }
}

/// A point's `(azimuth, station, radius)` about the axis in `b`; an
/// axis point (ρ ≤ `rho_eps`) reads azimuth 0.
pub(super) fn polar(ax: &axis::Axis, b: Basis, rho_eps: f64, q0: Point3) -> (f64, f64, f64) {
    let u = ax.dir;
    let d = q0 - ax.origin;
    let v = d.dot(u);
    let w = Point3::new(d.x - u.x * v, d.y - u.y * v, d.z - u.z * v);
    let rho = (w.dot(w)).sqrt();
    let th = if rho <= rho_eps { 0.0 } else { w.dot(b.e2).atan2(w.dot(b.e1)) };
    (th, v, rho)
}

/// Lay the work out about `b`. A facet is read by which of its
/// vertices stand on the axis: exactly one (the triangle closing a pole
/// or a cap fan) develops as the whole RECTANGLE it sweeps between its
/// rim vertices' columns, never as an affine blend - an axis vertex's
/// own azimuth is meaningless and a blended one would shear every
/// radial edge off its rim's column, slicing the fan open. Each facet
/// also picks its lap: measured from its lowest column unless those
/// straddle the ±π branch, where the high seam column rides one turn
/// below instead, so a mesh vertex keeps ONE developed coordinate in
/// every facet sharing it. The mesh is then duplicated a full period
/// so the seam's chords meet their own copy's ends.
pub(super) fn lay(facets: &[Triangle], ax: &axis::Axis, b: Basis, rho_ref: f64, rho_eps: f64) -> Sheet {
    let period = TAU * rho_ref;
    let mut wrap: Vec<Triangle> = Vec::with_capacity(facets.len() * 3);
    let (mut s_lo, mut s_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut edges: Vec<DevEdge> = Vec::with_capacity(facets.len() * 3);
    let mut seam_cols: Vec<(f64, f64, f64)> = Vec::new(); // straddlers' vertices
    let dev = |q0: Point3| polar(ax, b, rho_eps, q0);
    for shift in [0.0, period] {
        for t in facets {
            let (ta, va, ra) = dev(t.a);
            let (tb, vb, rb) = dev(t.b);
            let (tc, vc, rc) = dev(t.c);
            let mn = ta.min(tb).min(tc);
            let mx = ta.max(tb).max(tc);
            let base = if mx - mn > PI { mx } else { mn };
            if shift == 0.0 && mx - mn > PI {
                for (th, v, rho) in [(ta, va, ra), (tb, vb, rb), (tc, vc, rc)] {
                    if rho > rho_eps {
                        seam_cols.push((th, v, rho));
                    }
                }
            }
            let s = |th: f64| -> f64 {
                // Measured (Pi - x).rem_euclid so a column exactly
                // half a turn off the base rides +Pi, never its -Pi
                // alias: the two spellings must agree across every
                // facet that shares the vertex, or the weld splits.
                rho_ref * (base + PI - (PI - (th - base)).rem_euclid(TAU)) + shift
            };
            let n_off = usize::from(ra > rho_eps) + usize::from(rb > rho_eps) + usize::from(rc > rho_eps);
            if n_off == 2 {
                // The wedge: axis row running under both rim columns,
                // rim row the meridian segment itself.
                let (axv, b, c) = if ra <= rho_eps {
                    ((va, ra), (s(tb), vb, rb), (s(tc), vc, rc))
                } else if rb <= rho_eps {
                    ((vb, rb), (s(ta), va, ra), (s(tc), vc, rc))
                } else {
                    ((vc, rc), (s(ta), va, ra), (s(tb), vb, rb))
                };
                let (a0, a1) = (Point3::new(b.0, axv.0, axv.1), Point3::new(c.0, axv.0, axv.1));
                let (pb, pc) = (Point3::new(b.0, b.1, b.2), Point3::new(c.0, c.1, c.2));
                wrap.push(Triangle::new(a0, a1, pc));
                wrap.push(Triangle::new(a0, pc, pb));
                if shift == 0.0 {
                    // The axis vertex stands at a0 under b, at a1 under c.
                    let (w0, wb, wc) = if ra <= rho_eps {
                        (t.a, t.b, t.c)
                    } else if rb <= rho_eps {
                        (t.b, t.a, t.c)
                    } else {
                        (t.c, t.a, t.b)
                    };
                    edges.push(DevEdge::new(w0, wb, a0, pb));
                    edges.push(DevEdge::new(wb, wc, pb, pc));
                    edges.push(DevEdge::new(wc, w0, pc, a1));
                }
                s_lo = s_lo.min(b.0).min(c.0);
                s_hi = s_hi.max(b.0).max(c.0);
            } else {
                let pt = |th: f64, v: f64, rho: f64| Point3::new(s(th), v, rho);
                let (da, db, dc) = (pt(ta, va, ra), pt(tb, vb, rb), pt(tc, vc, rc));
                wrap.push(Triangle::new(da, db, dc));
                if shift == 0.0 {
                    edges.push(DevEdge::new(t.a, t.b, da, db));
                    edges.push(DevEdge::new(t.b, t.c, db, dc));
                    edges.push(DevEdge::new(t.c, t.a, dc, da));
                }
                s_lo = s_lo.min(s(ta)).min(s(tb)).min(s(tc));
                s_hi = s_hi.max(s(ta)).max(s(tb)).max(s(tc));
            }
        }
    }
    Sheet { wrap, edges, seam_cols, s_lo, s_hi }
}
