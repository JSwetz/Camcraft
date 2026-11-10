//! Issue #369: what a lathe holder body is judged against - the revolved part and stock read as an annulus per station on a fixed table (every vertex station of both, and a fine grid between), and the radial intervals the cuts before an instant removed (each cut a convex meridian footprint swept along its chord, cut by a station line exactly: the Minkowski sum of a convex polygon and a disc is the polygon, its edge bands and its vertex discs - textbook convex geometry); our own design, no vendor material consulted.

use std::cell::RefCell;

use super::turning_credit::Pilot;
use super::turning_outline::Revolved;

/// The table's grid pitch between vertex stations (mm).
pub(super) const PITCH: f64 = 0.25;
/// Most grid stations one table holds (a longer reach pitches coarser).
const MAX_GRID: f64 = 200_000.0;
/// Below this a left-over radial interval is float dust, not steel.
const DUST: f64 = 1e-6;

/// The material annuli at a station: the part's and the stock's, each
/// (hole radius, skin radius) unshrunk.
#[derive(Clone, Copy)]
pub(super) struct Annuli {
    pub part: (f64, f64),
    pub stock: Option<(f64, f64)>,
}

/// One body read just inside both ends of a table interval: the skin
/// at each end (it runs straight between them) and the narrower hole.
#[derive(Clone, Copy)]
struct Inside {
    skin: (f64, f64),
    hole: f64,
}

/// Both bodies inside one table interval.
#[derive(Clone, Copy)]
struct Interval {
    part: Inside,
    stock: Option<Inside>,
}

/// The revolved bodies read on a station table, lazily and once.
pub(super) struct Table<'a> {
    part: &'a Revolved<'a>,
    stock: Option<&'a Revolved<'a>>,
    stations: Vec<f64>,
    cache: RefCell<Vec<Option<Annuli>>>,
    inside: RefCell<Vec<Option<Interval>>>,
}

impl<'a> Table<'a> {
    /// The table over `[lo, hi]`: the grid and every station in `knots`
    /// inside it, ascending, deduplicated. The knots are where the
    /// bodies' skins bend and their hole walls can, so between two
    /// neighbours a skin runs straight.
    pub(super) fn new(part: &'a Revolved<'a>, stock: Option<&'a Revolved<'a>>, lo: f64, hi: f64, knots: &[f64]) -> Option<Self> {
        if !(lo.is_finite() && hi.is_finite() && lo <= hi) {
            return None;
        }
        let pitch = PITCH.max((hi - lo) / MAX_GRID);
        let n = ((hi - lo) / pitch).ceil().clamp(1.0, MAX_GRID + 1.0) as usize;
        let mut stations: Vec<f64> = (0..=n).map(|k| lo + (hi - lo) * k as f64 / n as f64).collect();
        stations.extend(knots.iter().copied().filter(|s| s.is_finite() && *s > lo && *s < hi));
        stations.sort_by(f64::total_cmp);
        stations.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);
        let len = stations.len();
        Some(Self { part, stock, stations, cache: RefCell::new(vec![None; len]), inside: RefCell::new(vec![None; len]) })
    }

    pub(super) fn station(&self, k: usize) -> f64 {
        self.stations.get(k).copied().unwrap_or(f64::NAN)
    }

    /// The first index whose station is at or above `s`.
    pub(super) fn first_at(&self, s: f64) -> usize {
        self.stations.partition_point(|&t| t < s)
    }

    /// The annuli at table index `k`, read once.
    pub(super) fn annuli(&self, k: usize) -> Annuli {
        if let Some(Some(a)) = self.cache.borrow().get(k) {
            return *a;
        }
        let s = self.station(k);
        let a = Annuli { part: self.part.annulus(s), stock: self.stock.map(|o| o.annulus(s)) };
        if let Some(slot) = self.cache.borrow_mut().get_mut(k) {
            *slot = Some(a);
        }
        a
    }

    /// The annuli at any station: a table station's own, else read
    /// inside its interval - each skin interpolated between its readings
    /// just inside both ends (straight there, so exact), each hole the
    /// narrower of its two (more material, never less). Off the table:
    /// nothing stands.
    pub(super) fn annuli_at(&self, s: f64) -> Annuli {
        let k = self.first_at(s);
        if (self.station(k) - s).abs() <= 1e-12 {
            return self.annuli(k);
        }
        let (Some(&t0), Some(&t1)) = (k.checked_sub(1).and_then(|j| self.stations.get(j)), self.stations.get(k)) else {
            return Annuli { part: (f64::INFINITY, 0.0), stock: self.stock.map(|_| (f64::INFINITY, 0.0)) };
        };
        let iv = self.interval(k - 1, t0, t1);
        let lam = ((s - t0) / (t1 - t0)).clamp(0.0, 1.0);
        let read = |b: Inside| (b.hole, b.skin.0 + (b.skin.1 - b.skin.0) * lam);
        Annuli { part: read(iv.part), stock: iv.stock.map(read) }
    }

    fn interval(&self, k: usize, t0: f64, t1: f64) -> Interval {
        if let Some(Some(iv)) = self.inside.borrow().get(k) {
            return *iv;
        }
        let d = (t1 - t0) * 1e-4;
        let inside = |o: &Revolved| {
            let (h0, r0) = o.annulus(t0 + d);
            let (h1, r1) = o.annulus(t1 - d);
            Inside { skin: (r0, r1), hole: h0.min(h1) }
        };
        let iv = Interval { part: inside(self.part), stock: self.stock.map(inside) };
        if let Some(slot) = self.inside.borrow_mut().get_mut(k) {
            *slot = Some(iv);
        }
        iv
    }
}

/// The open radial interval of material an annulus holds, shrunk by
/// `margin` (the meridian laws' reading): `None` when it holds none.
pub(super) fn material((inner, outer): (f64, f64), margin: f64) -> Option<(f64, f64)> {
    let hi = outer - margin;
    // A hole that cannot be read (NaN) is no hole: material to the axis.
    let lo = if inner > 0.0 { inner + margin } else { f64::NEG_INFINITY };
    (hi > lo && hi > 0.0).then_some((lo, hi))
}

/// A cut as a convex meridian solid: the footprint's hull in (signed
/// radius, station) dilated by `round`, the stations it spans, and the
/// move that cut it (`None`: an op ahead, always before).
pub(super) struct Kerf {
    pub hull: Vec<[f64; 2]>,
    pub round: f64,
    pub s_lo: f64,
    pub s_hi: f64,
    pub at: Option<usize>,
}

impl Kerf {
    pub(super) fn new(hull: Vec<[f64; 2]>, round: f64, at: Option<usize>) -> Option<Self> {
        let r = if round.is_finite() { round.max(0.0) } else { return None };
        let (lo, hi) = hull.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), p| (l.min(p[1]), h.max(p[1])));
        if lo <= hi && hull.iter().flatten().all(|v| v.is_finite()) {
            Some(Self { hull, round: r, s_lo: lo - r, s_hi: hi + r, at })
        } else {
            None
        }
    }

    /// The radii (unsigned: the part spins) this cut removed at
    /// station `s`, as up to two intervals.
    pub(super) fn removed(&self, s: f64, out: &mut Vec<(f64, f64)>) {
        if s < self.s_lo || s > self.s_hi {
            return;
        }
        let Some((lo, hi)) = chord(&self.hull, self.round, s) else { return };
        if hi >= 0.0 {
            out.push((lo.max(0.0), hi));
        }
        if lo <= 0.0 {
            out.push(((-hi).max(0.0), -lo));
        }
    }
}

/// The signed-radius interval where the station line `s` meets the
/// convex hull `h` (points (radius, station)) dilated by `round`: the
/// hull of the line's meetings with the polygon, each vertex disc and
/// each edge band - the three pieces of the dilated polygon, whose union
/// is convex, so the hull of the pieces' intervals is exact.
pub(super) fn chord(h: &[[f64; 2]], round: f64, s: f64) -> Option<(f64, f64)> {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut take = |a: f64, b: f64| {
        lo = lo.min(a);
        hi = hi.max(b);
    };
    for v in h {
        let d = s - v[1];
        if d.abs() <= round {
            let w = (round * round - d * d).max(0.0).sqrt();
            take(v[0] - w, v[0] + w);
        }
    }
    let n = h.len();
    let edges = match n {
        0 | 1 => 0,
        2 => 1,
        _ => n,
    };
    for i in 0..edges {
        let (a, b) = (h[i], h[(i + 1) % n]);
        // The polygon's own boundary on the line.
        if s >= a[1].min(b[1]) && s <= a[1].max(b[1]) {
            let ds = b[1] - a[1];
            if ds.abs() <= 1e-15 {
                take(a[0].min(b[0]), a[0].max(b[0]));
            } else {
                let r = a[0] + (b[0] - a[0]) * ((s - a[1]) / ds);
                take(r, r);
            }
        }
        // The edge's band, `round` either side of it.
        let (er, es) = (b[0] - a[0], b[1] - a[1]);
        let len = er.hypot(es);
        if round <= 0.0 || len.is_nan() || len <= 1e-15 {
            continue;
        }
        let (ur, us) = (er / len, es / len);
        // along = (r - a0) ur + (s - a1) us in [0, len];
        // across = (r - a0) us - (s - a1) ur in [-round, round].
        let along = solve(ur, (s - a[1]) * us - a[0] * ur, 0.0, len);
        let across = solve(us, -(s - a[1]) * ur - a[0] * us, -round, round);
        if let (Some(p), Some(q)) = (along, across) {
            let (l, u) = (p.0.max(q.0), p.1.min(q.1));
            if l <= u && l.is_finite() && u.is_finite() {
                take(l, u);
            }
        }
    }
    (lo <= hi).then_some((lo, hi))
}

/// The r with `c r + d` in `[lo, hi]`: an interval, everything
/// (`(-inf, inf)`), or `None`.
fn solve(c: f64, d: f64, lo: f64, hi: f64) -> Option<(f64, f64)> {
    if c.abs() <= 1e-15 {
        return (d >= lo && d <= hi).then_some((f64::NEG_INFINITY, f64::INFINITY));
    }
    let (a, b) = ((lo - d) / c, (hi - d) / c);
    Some((a.min(b), a.max(b)))
}

/// What a pilot drilled ahead removed at station `s`: radii below its
/// width there.
pub(super) fn pilot_removed(p: &Pilot, s: f64, out: &mut Vec<(f64, f64)>) {
    if s < p.lo || s > p.hi {
        return;
    }
    let wide = match p.cot {
        Some(k) if k > 0.0 => ((s - p.lo) / k).min(p.r),
        _ => p.r,
    };
    if wide > 0.0 {
        out.push((0.0, wide));
    }
}

/// The longest piece of `(lo, hi)` no interval of `cuts` covers; 0 when
/// they cover it all (to float dust).
pub(super) fn standing(lo: f64, hi: f64, cuts: &mut [(f64, f64)]) -> f64 {
    let span = hi - lo;
    if span.is_nan() || span <= DUST {
        return 0.0;
    }
    cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (mut at, mut best) = (lo, 0.0f64);
    for &(a, b) in cuts.iter() {
        if b <= at {
            continue;
        }
        if a >= hi {
            break;
        }
        if a > at {
            best = best.max(a - at);
        }
        at = at.max(b);
        if at >= hi {
            break;
        }
    }
    if at < hi {
        best = best.max(hi - at);
    }
    if best > DUST { best } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Distance from `(r, s)` to the convex counter-clockwise polygon
    /// `h` (0 inside), brute force.
    fn dist(h: &[[f64; 2]], r: f64, s: f64) -> f64 {
        let seg = |a: [f64; 2], b: [f64; 2]| {
            let (vx, vy) = (b[0] - a[0], b[1] - a[1]);
            let l2 = vx * vx + vy * vy;
            let t = if l2 > 0.0 { (((r - a[0]) * vx + (s - a[1]) * vy) / l2).clamp(0.0, 1.0) } else { 0.0 };
            (r - a[0] - vx * t).hypot(s - a[1] - vy * t)
        };
        let n = h.len();
        let inside = n >= 3 && (0..n).all(|i| {
            let (a, b) = (h[i], h[(i + 1) % n]);
            (b[0] - a[0]) * (s - a[1]) - (b[1] - a[1]) * (r - a[0]) >= 0.0
        });
        if inside {
            return 0.0;
        }
        (0..n.max(1)).map(|i| seg(h[i], h[(i + 1) % n])).fold(f64::INFINITY, f64::min)
    }

    #[test]
    fn a_station_line_meets_a_dilated_footprint_exactly() {
        // A capsule (the nose's sweep), a point disc, and a swept blade
        // (a convex hexagon), each cut by station lines: the chord's
        // ends sit on the boundary, its inside is inside, past it out.
        let shapes: Vec<(Vec<[f64; 2]>, f64)> = vec![
            (vec![[6.8, 2.0], [6.8, 20.0]], 0.81),
            (vec![[3.0, -1.0]], 0.5),
            (vec![[5.0, -1.0], [9.0, -1.0], [11.0, 3.0], [11.0, 5.0], [7.0, 5.0], [5.0, 1.0]], 0.2),
            (vec![[2.0, 0.0], [-4.0, 0.0]], 0.4),
        ];
        for (h, round) in &shapes {
            for k in -40..=260 {
                let s = f64::from(k) * 0.1;
                match chord(h, *round, s) {
                    None => assert!((0..=200).all(|j| dist(h, -10.0 + 0.1 * f64::from(j), s) > round - 1e-9), "{h:?} s {s}: missed"),
                    Some((lo, hi)) => {
                        assert!((dist(h, lo, s) - round).abs() < 1e-6 && (dist(h, hi, s) - round).abs() < 1e-6, "{h:?} s {s}: [{lo}, {hi}]");
                        assert!(dist(h, 0.5 * (lo + hi), s) <= round + 1e-9);
                        assert!(dist(h, lo - 1e-3, s) > *round && dist(h, hi + 1e-3, s) > *round);
                    }
                }
            }
        }
    }

    #[test]
    fn a_kerf_credits_both_sides_of_the_axis_and_standing_measures_the_gap() {
        // A facing chord across the axis, r 7 to -1 at station 0, nose 1.
        let k = Kerf::new(vec![[7.0, 0.0], [-1.0, 0.0]], 1.0, None).expect("a kerf");
        let mut cuts = Vec::new();
        k.removed(0.0, &mut cuts);
        assert!(cuts.iter().any(|c| c.0 <= 0.0 && c.1 >= 8.0 - 1e-12), "{cuts:?}");
        cuts.clear();
        k.removed(5.0, &mut cuts);
        assert!(cuts.is_empty(), "{cuts:?}");
        // (2, 10) less (0, 5) and (6, 7): the longest standing piece is 7..10.
        let mut c = vec![(6.0, 7.0), (0.0, 5.0)];
        assert!((standing(2.0, 10.0, &mut c) - 3.0).abs() < 1e-12);
        let mut all = vec![(0.0, 4.0), (3.9, 11.0)];
        assert_eq!(standing(2.0, 10.0, &mut all), 0.0);
        assert_eq!(material((0.0, 10.0), 0.01), Some((f64::NEG_INFINITY, 9.99)));
        assert_eq!(material((f64::INFINITY, 0.0), 0.01), None);
        assert_eq!(material((6.0, 10.0), 0.01), Some((6.01, 9.99)));
    }
}
