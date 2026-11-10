//! The column engine's bull-nose atom (M13.1): a corner-radius end
//! mill swept along a chord, the lowest steel above every column
//! found exactly - the dexel twin of `field_bull`'s solid, so the two
//! engines keep agreeing on a bull nose the way they agree on the
//! ball, the cone and the disc. Split beside `dexel.rs` by the
//! 400-line law.
//!
//! Provenance: the torus-cornered cutter's bottom profile of the
//! machine-tool handbooks (flat disc of `r - c`, then the quarter
//! circle of the corner radius `c` out to `r`), the convex
//! composition law of elementary convex analysis (a convex
//! nondecreasing map of a convex function is convex), and this
//! module's own reach and chord laws. Nothing from vendor binaries.
//!
//! ## The law here
//!
//! - **The bottom is convex in the move parameter.** Above a cell
//!   center at horizontal distance `rho(t)` from the axis, the bull's
//!   lowest steel sits at `z(t) + c - sqrt(c^2 - (rho(t) - (r - c))_+^2)`.
//!   `rho(t)` is convex (a distance to a moving point on a line), the
//!   profile is convex and nondecreasing in `rho`, and `z(t)` is
//!   linear, so the bottom is convex on the reach window and its
//!   minimum is a window end, a flat-disc window end, or one interior
//!   stationary point - the window ends and the XY-nearest instant
//!   are evaluated exactly, the stationary point by a golden section
//!   run to the float floor (64 rounds: the bracket ends at 4e-14 of
//!   the window). Level moves and plunges need no search at all.
//! - **Never an over-claim.** A search can only stop ABOVE the true
//!   minimum, so the column is lowered to at most the true bottom:
//!   conservative, the law `cut_ball` keeps.

use super::dexel::{reach, Dexel};
use super::dexel_arc::Atom;

use crate::cl::{Move, MoveKind};
use crate::geometry::Point3;

/// Golden-section rounds for the oblique stationary point.
const ROUNDS: u32 = 64;

impl Dexel {
    /// The bull-nose atom: a cutter of radius `r` with corner radius
    /// `c` (`0 < c <= r`) whose TIP travels the straight, Z-linear
    /// chord `a -> b`. Non-finite or nonsense numbers cut nothing
    /// (the `cut_chord` gate: validation lives upstream).
    pub fn cut_bull(&mut self, a: Point3, b: Point3, r: f64, c: f64) {
        if !(a.is_finite() && b.is_finite()) || !(r.is_finite() && r >= 0.0) || !(c.is_finite() && c > 0.0 && c <= r) {
            return;
        }
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let dd = dx * dx + dy * dy;
        let k = b.z - a.z;
        let flat = r - c;
        let fx = |v: f64| ((v - self.x0) / self.cell).floor();
        let fy = |v: f64| ((v - self.y0) / self.cell).floor();
        let lo_x = fx(a.x.min(b.x) - r).max(0.0) as usize;
        let hi_x = fx(a.x.max(b.x) + r).clamp(0.0, self.nx as f64 - 1.0) as usize;
        let lo_y = fy(a.y.min(b.y) - r).max(0.0) as usize;
        let hi_y = fy(a.y.max(b.y) + r).clamp(0.0, self.ny as f64 - 1.0) as usize;
        if lo_x >= self.nx || lo_y >= self.ny {
            return;
        }
        for iy in lo_y..=hi_y {
            for ix in lo_x..=hi_x {
                let q = self.center(ix, iy);
                let (wx, wy) = (q.x - a.x, q.y - a.y);
                let Some((t0, t1)) = reach(wx, wy, dx, dy, r) else {
                    continue;
                };
                let f = |t: f64| {
                    let rho = (wx - dx * t).hypot(wy - dy * t);
                    let e = (rho - flat).max(0.0);
                    a.z + k * t + c - (c * c - e * e).max(0.0).sqrt()
                };
                let t_c = if dd > f64::EPSILON { (wx * dx + wy * dy) / dd } else { 0.0 };
                let mut cut = f(t0).min(f(t1)).min(f(t_c.clamp(t0, t1)));
                // A plunge's bottom is linear (ends suffice); a level
                // move's is minimized where rho is (t_c, above).
                if dd > f64::EPSILON && k != 0.0 {
                    if let Some((f0, f1)) = reach(wx, wy, dx, dy, flat) {
                        cut = cut.min(f(f0.clamp(t0, t1))).min(f(f1.clamp(t0, t1)));
                    }
                    cut = cut.min(golden(&f, t0, t1));
                }
                self.lower(ix + iy * self.nx, cut);
            }
        }
    }

    /// Bull-nose version of `subtract`: moves carry TIP positions and
    /// the tip IS the profile's anchor (no lift, the cone's
    /// convention); XY arcs sweep the true arc (`dexel_arc`).
    pub fn subtract_bull(&mut self, m: &Move, r: f64, c: f64, chord_tol: f64) {
        if m.kind == MoveKind::Rapid || !m.is_finite() {
            return;
        }
        match m.arc {
            None => self.cut_bull(m.from, m.to, r, c),
            // XZ / YZ arcs (M11.2 A): their own 3-D chords.
            Some(arc) if !arc.plane.is_xy() => {
                for (a, b) in m.segments3(chord_tol) {
                    self.cut_bull(a, b, r, c);
                }
            }
            // XY arcs sweep the true arc (#402), not chords.
            Some(arc) => self.cut_xy_arc(m, arc, r, Atom::Bull(c)),
        }
    }
}

/// The minimum of a convex `f` on `[lo, hi]` by golden section: every
/// value it returns is a real `f` evaluation, so it can only sit at or
/// above the true minimum.
pub(super) fn golden(f: &impl Fn(f64) -> f64, mut lo: f64, mut hi: f64) -> f64 {
    const PHI: f64 = 0.6180339887498949;
    let mut c = hi - PHI * (hi - lo);
    let mut d = lo + PHI * (hi - lo);
    let (mut fc, mut fd) = (f(c), f(d));
    for _ in 0..ROUNDS {
        if fc < fd {
            hi = d;
            d = c;
            fd = fc;
            c = hi - PHI * (hi - lo);
            fc = f(c);
        } else {
            lo = c;
            c = d;
            fc = fd;
            d = lo + PHI * (hi - lo);
            fd = f(d);
        }
    }
    fc.min(fd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo2d::Point2;

    fn p3(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    /// The bull's bottom at horizontal offset `rho` above its tip.
    fn bottom(rho: f64, r: f64, c: f64) -> f64 {
        let e = (rho - (r - c)).max(0.0);
        c - (c * c - e * e).max(0.0).sqrt()
    }

    #[test]
    fn a_static_bull_is_its_profile_exactly() {
        let (r, c) = (3.0, 1.0);
        let mut dex = Dexel::block(-4.0, -4.0, 4.0, 4.0, 100.0, 0.25).unwrap();
        dex.cut_bull(p3(0.0, 0.0, 5.0), p3(0.0, 0.0, 5.0), r, c);
        for iy in 0..dex.ny {
            for ix in 0..dex.nx {
                let q = dex.center(ix, iy);
                let d = q.x.hypot(q.y);
                let want = if d <= r { 5.0 + bottom(d, r, c) } else { 100.0 };
                let got = dex.top_at(ix + iy * dex.nx);
                assert!((got - want).abs() <= 1e-12, "{q:?}: {got} vs {want}");
            }
        }
    }

    #[test]
    fn an_oblique_sweep_matches_a_dense_parameter_scan() {
        // A climbing, slanted chord: the column's cut must be the min
        // over t of the bottom, as a brute-force scan finds it.
        let (r, c) = (4.0, 1.5);
        let (a, b) = (p3(-6.0, -1.0, 2.0), p3(7.0, 2.0, 9.0));
        let mut dex = Dexel::block(-11.0, -6.0, 12.0, 7.0, 100.0, 0.37).unwrap();
        dex.cut_bull(a, b, r, c);
        let (mut n, mut tight) = (0, 0);
        for iy in 0..dex.ny {
            for ix in 0..dex.nx {
                let q = dex.center(ix, iy);
                let g = |t: f64| {
                    let (px, py) = (a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                    let rho = (q.x - px).hypot(q.y - py);
                    if rho <= r { a.z + (b.z - a.z) * t + bottom(rho, r, c) } else { f64::INFINITY }
                };
                // coarse scan, then a fine one around the coarse best
                // (the bottom is convex: the minimum is beside it)
                let (mut want, mut ts) = (f64::INFINITY, 0.0);
                for s in 0..=4000 {
                    let t = s as f64 / 4000.0;
                    if g(t) < want {
                        (want, ts) = (g(t), t);
                    }
                }
                for s in 0..=4000 {
                    let t = (ts - 2.0 / 4000.0 + s as f64 / 1e6).clamp(0.0, 1.0);
                    want = want.min(g(t));
                }
                let got = dex.top_at(ix + iy * dex.nx);
                if want.is_finite() {
                    // the atom is exact, the scan an upper bound: at or
                    // below it, by no more than the scan's step can hide
                    // at a window end's vertical tangent
                    assert!(got <= want + 1e-9 && got >= want - 1e-2, "{q:?}: {got} vs scan {want}");
                    n += 1;
                    if (got - want).abs() < 1e-6 {
                        tight += 1;
                    }
                } else if got != 100.0 {
                    // a column the scan never caught inside the disc:
                    // only a window-end graze may cut it
                    assert!(got >= 2.0 + c - 0.1, "phantom cut at {q:?}: {got}");
                }
            }
        }
        assert!(tight * 10 >= n * 8, "{tight} of {n} columns within 1e-6 of the scan");
        assert!(n > 300, "{n} columns compared");
    }

    #[test]
    fn the_corner_leaves_what_a_flat_removes() {
        // A level slot: beside the flat disc the bull stands higher
        // by exactly its arc, where the disc atom cuts to the floor.
        let (r, c) = (3.0, 1.0);
        let mut bull = Dexel::block(-10.0, -4.0, 10.0, 4.0, 100.0, 0.5).unwrap();
        let mut flat = bull.clone();
        bull.cut_bull(p3(-5.0, 0.0, 2.0), p3(5.0, 0.0, 2.0), r, c);
        flat.cut_chord(p3(-5.0, 0.0, 2.0), p3(5.0, 0.0, 2.0), r);
        let k = bull.col(Point2::new(0.0, 2.75)).unwrap();
        assert!((bull.top_at(k) - (2.0 + bottom(2.75, r, c))).abs() < 1e-12);
        assert_eq!(flat.top_at(k), 2.0);
        assert!(bull.top_at(k) > flat.top_at(k) + 0.3, "the fillet is rest");
    }

    #[test]
    fn nonsense_numbers_cut_nothing() {
        let mut dex = Dexel::block(-5.0, -5.0, 5.0, 5.0, 9.0, 1.0).unwrap();
        dex.cut_bull(p3(0.0, 0.0, 1.0), p3(1.0, 0.0, 1.0), 2.0, 0.0);
        dex.cut_bull(p3(0.0, 0.0, 1.0), p3(1.0, 0.0, 1.0), 2.0, 3.0);
        dex.cut_bull(p3(0.0, 0.0, f64::NAN), p3(1.0, 0.0, 1.0), 2.0, 1.0);
        assert!(dex.tops().all(|v| v == 9.0), "garbage cut something");
    }
}
