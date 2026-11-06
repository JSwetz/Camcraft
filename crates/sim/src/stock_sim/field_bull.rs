//! The bull-nose solid (M13.1): a corner-radius end mill read exactly
//! in its own (rho, along-axis) half-plane, and the two moves 2.5-D
//! paths are made of closed in form. Split beside `field_solid.rs`
//! by the 400-line law; the sweep walk that consumes it lives in
//! `field_cut.rs`.
//!
//! Provenance: the profile is the rounded rectangle of the published
//! 2-D distance catalogue (I. Quilez, "2D distance functions",
//! sdRoundedBox, iquilezles.org/articles/distfunctions2d) revolved by
//! the same author's revolution operator ("distance functions",
//! opRevolution / sdRoundedCylinder, iquilezles.org/articles/
//! distfunctions) - here with only the BOTTOM edge rounded and the
//! top left square, and every branch written out so the distance is
//! exact on both sides, not the lower-bound `max` of an intersection.
//! The sweep along a segment is the Minkowski sum of a convex solid
//! and a segment (the swept-volume SDF of the literature: the distance
//! to the sweep is the minimum over the trajectory parameter of the
//! distance to the moved solid, convex in that parameter for a convex
//! solid). Nothing from vendor binaries.
//!
//! ## The law here
//!
//! - **The profile.** Radius `r`, corner `c` (0 < c <= r), flank `h`
//!   (h >= c): the set `0 <= rho <= r, 0 <= z <= h` with the corner
//!   quadrant `rho > r - c, z < c` cut back to the quarter disc about
//!   `(r - c, c)`. It is convex and symmetric about the axis, so (the
//!   `field_solid` header's law) the half-plane distance IS the 3-D
//!   distance - outside exact, inside exact (min over the four
//!   boundary pieces: top, flank, floor, arc).
//! - **Level and along-axis moves are closed form.** At fixed height
//!   the distance to a convex, axis-symmetric section grows with the
//!   radial offset, so a move perpendicular to the axis is minimized
//!   at its foot; a move ALONG the axis sweeps the same profile with
//!   its flank lengthened by the move, anchored at the lower tip.
//!   Oblique moves go to the golden section on the convex parameter
//!   map, `field_cut`'s law.

/// The exact signed distance from the half-plane point `(rho, zl)`
/// (`rho >= 0`) to the bull-nose profile: negative inside.
pub(crate) fn bull_dist(rho: f64, zl: f64, r: f64, c: f64, h: f64) -> f64 {
    let k = r - c; // where the floor ends and the arc begins
    let in_corner = rho > k && zl < c;
    let arc = (rho - k).hypot(zl - c); // distance from the arc's center
    let inside = (0.0..=h).contains(&zl) && rho <= r && !(in_corner && arc > c);
    if !inside {
        if in_corner {
            return arc - c; // the quarter arc's radial projection
        }
        // Everywhere else the nearest point sits on the square part of
        // the box: the top, the flank above the arc, or the floor.
        return (rho - r).max(0.0).hypot((-zl).max(0.0)).hypot((zl - h).max(0.0));
    }
    // Inside: the nearest of the four boundary pieces.
    let top = h - zl;
    let flank = if zl >= c { r - rho } else { (r - rho).hypot(c - zl) };
    let floor = if rho <= k { zl } else { (rho - k).hypot(zl) };
    let curve = if rho >= k && zl <= c { c - arc } else { f64::INFINITY };
    -top.min(flank).min(floor).min(curve)
}

/// The anchor parameter and flank a closed-form move needs: `Some((t,
/// h))` where the swept distance is `bull_dist` at anchor `a + t d`
/// with flank `h`; None sends the move to the golden section. Upright
/// tools only (the 2.5-D vocabulary): a level move's foot, or a
/// plunge's lower tip with the flank lengthened by the plunge.
pub(crate) fn bull_closed(ax: [f64; 3], a: [f64; 3], d: [f64; 3], q: [f64; 3], h: f64) -> Option<(f64, f64)> {
    if ax != [0.0, 0.0, 1.0] {
        return None;
    }
    if d[2] == 0.0 {
        let dd = d[0] * d[0] + d[1] * d[1];
        let t = ((q[0] - a[0]) * d[0] + (q[1] - a[1]) * d[1]) / dd;
        return t.is_finite().then(|| (t.clamp(0.0, 1.0), h));
    }
    if d[0] == 0.0 && d[1] == 0.0 {
        // The lower tip anchors; the swept profile is the same rounded
        // bottom under a flank `|dz|` taller.
        let t = if d[2] < 0.0 { 1.0 } else { 0.0 };
        return Some((t, h + d[2].abs()));
    }
    None
}

/// The upright bull's exact distance `v` with its ties settled the
/// column engine's way (the `flat_tie` law, M7.3): a point ON the
/// flank above the arc, or strictly above the flat floor inside its
/// disc, is removed - the column's reach is inclusive - while a point
/// ON the floor or ON the arc is not (the column's top IS that
/// surface). Elsewhere the conservative hair: no other tie claims.
pub(crate) fn bull_tie(rho: f64, zl: f64, r: f64, c: f64, h: f64, v: f64) -> f64 {
    let flank_band = rho <= r && zl > c && zl < h;
    let floor_disc = rho <= r - c && zl > 0.0 && zl < h;
    if flank_band || floor_disc {
        return v.min(-f64::MIN_POSITIVE);
    }
    v + 1e-12 + 1e-15 * v.abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The brute-force reference: the profile's boundary sampled
    /// densely (top, flank, floor, arc), the nearest sample taken, the
    /// sign from an independent membership test. Exact to the sample
    /// spacing, which the asserts allow for.
    fn reference(rho: f64, zl: f64, r: f64, c: f64, h: f64) -> f64 {
        let k = r - c;
        let n = 4000;
        let mut best = f64::INFINITY;
        let mut eat = |x: f64, y: f64| best = best.min((rho - x).hypot(zl - y));
        for i in 0..=n {
            let s = i as f64 / n as f64;
            eat(s * r, h); // top
            eat(r, c + s * (h - c)); // flank
            eat(s * k, 0.0); // floor
            let th = -std::f64::consts::FRAC_PI_2 * (1.0 - s); // -90..0 degrees
            eat(k + c * th.cos(), c + c * th.sin()); // arc
        }
        let member = (0.0..=h).contains(&zl)
            && rho <= r
            && (rho <= k || zl >= c || (rho - k).hypot(zl - c) <= c);
        if member { -best } else { best }
    }

    #[test]
    fn the_profile_distance_matches_a_dense_boundary_reference() {
        // Three tools, a lattice of half-plane points around each,
        // inside and out, corner zone included.
        let mut n = 0;
        for (r, c, h) in [(5.0, 1.0, 12.0), (3.0, 2.5, 8.0), (4.0, 4.0, 10.0)] {
            for i in 0..=60 {
                for j in 0..=70 {
                    let rho = i as f64 * 0.13;
                    let zl = -2.0 + j as f64 * 0.21;
                    let got = bull_dist(rho, zl, r, c, h);
                    let want = reference(rho, zl, r, c, h);
                    // sample spacing: the longest piece / 4000, halved
                    assert!((got - want).abs() < 2e-3, "r {r} c {c} h {h} at ({rho}, {zl}): {got} vs {want}");
                    n += 1;
                }
            }
        }
        assert!(n > 12_000, "{n} samples");
    }

    #[test]
    fn the_profile_reads_its_landmarks_exactly() {
        let (r, c, h) = (5.0, 1.0, 12.0);
        assert!((bull_dist(0.0, -2.0, r, c, h) - 2.0).abs() < 1e-12, "under the floor");
        assert!((bull_dist(7.0, 6.0, r, c, h) - 2.0).abs() < 1e-12, "beside the flank");
        // Diagonally off the corner: the arc's center (4, 1) is
        // sqrt(2) * 2 away along 45 degrees, so the gap is 2v2 - 1.
        let d = bull_dist(6.0, -1.0, r, c, h);
        assert!((d - (8.0f64.sqrt() - 1.0)).abs() < 1e-12, "{d}");
        // A square end mill would claim that point's neighbour on the
        // corner's bisector; the bull leaves it standing.
        let gap = bull_dist(4.9, 0.1, r, c, h);
        assert!(gap > 0.0, "the corner leaves material a flat removes: {gap}");
        assert!((bull_dist(1.0, 0.5, r, c, h) + 0.5).abs() < 1e-12, "half a mm over the floor");
        assert!((bull_dist(4.0, 1.0, r, c, h) + 1.0).abs() < 1e-12, "the arc's center is one corner deep");
    }

    #[test]
    fn a_plunge_sweeps_a_taller_profile_at_its_lower_tip() {
        // Closed form vs a dense scan of the moved profile.
        let (r, c, h) = (3.0, 1.0, 6.0);
        let a = [0.0, 0.0, 10.0];
        let d = [0.0, 0.0, -4.0];
        for (x, z) in [(2.7, 6.3), (3.4, 5.0), (0.5, 5.8), (2.9, 9.0), (1.0, 20.5)] {
            let q = [x, 0.0, z];
            let (t, hh) = bull_closed([0.0, 0.0, 1.0], a, d, q, h).unwrap();
            let tip = a[2] + d[2] * t;
            let got = bull_dist(x, z - tip, r, c, hh);
            let mut want = f64::INFINITY;
            for s in 0..=20_000 {
                let tz = a[2] + d[2] * (s as f64 / 20_000.0);
                want = want.min(bull_dist(x, z - tz, r, c, h));
            }
            assert!((got - want).abs() < 1e-3, "({x}, {z}): {got} vs {want}");
        }
    }
}
