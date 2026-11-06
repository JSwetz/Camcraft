//! The swept area of a disc along open polylines (M11.4 C): the region a
//! flat cutter of radius `r` clears when its center walks each path -
//! the Minkowski sum of the polyline with the disc. Provenance: the
//! standard Minkowski-sum / offset-curve formulation of the published
//! polygon-offsetting literature, computed by the same `clipper2-rust`
//! inflate the region buffer already uses (open paths, round ends and
//! round joins); the millimeter bridge and the gates are ours. The
//! fine buffer below is the same inflate at a caller-chosen arc
//! flattening (M11.4 C round 3: the adaptive's growth rings are walked
//! as cut, so their fillets must turn by a few degrees a chord, not the
//! region buffer's 5 um flats).

use clipper2_rust::{EndType, JoinType, Path64, Paths64, Point64, inflate_paths_64};

use super::{ARC_TOL_UNITS, COORD_LIMIT, GeoError, MITER_LIMIT, Point2, Region, SCALE, quantize};

impl Region {
    /// The area swept by a disc of radius `r` whose center walks every
    /// path in `paths` (each an open polyline; a single point sweeps one
    /// disc). Round joins and round ends: exactly the cutter's footprint.
    /// Non-finite input or a non-positive radius is refused, never a
    /// guessed region (Prime Directive).
    pub fn sweep(paths: &[Vec<Point2>], r: f64) -> Result<Region, GeoError> {
        if !(r.is_finite() && r > 0.0) {
            return Err(GeoError::Invalid("sweep radius must be positive and finite".into()));
        }
        if r >= COORD_LIMIT {
            return Err(GeoError::OutOfRange);
        }
        let mut subj: Paths64 = Vec::with_capacity(paths.len());
        for path in paths {
            let mut q: Path64 = Path64::with_capacity(path.len());
            for p in path {
                if !(p.x.is_finite() && p.y.is_finite()) {
                    return Err(GeoError::Invalid("non-finite sweep vertex".into()));
                }
                let pt = Point64 { x: quantize(p.x)?, y: quantize(p.y)? };
                if q.last().is_none_or(|l: &Point64| l.x != pt.x || l.y != pt.y) {
                    q.push(pt);
                }
            }
            if !q.is_empty() {
                subj.push(q);
            }
        }
        if subj.is_empty() {
            return Ok(Region::empty());
        }
        Ok(Region::from_paths(inflate_paths_64(
            &subj,
            r * SCALE,
            JoinType::Round,
            EndType::Round,
            MITER_LIMIT,
            ARC_TOL_UNITS,
        )))
    }

    /// [`Region::offset`] with round joins flattened to at most `arc_tol`
    /// mm (clamped to [1 nm, 5 um]: never coarser than the plain buffer,
    /// never finer than the micron lattice can carry). Same refusals.
    pub fn offset_fine(&self, d: f64, arc_tol: f64) -> Result<Region, GeoError> {
        if !(d.is_finite() && arc_tol.is_finite()) {
            return Err(GeoError::Invalid("non-finite offset distance or tolerance".into()));
        }
        if d.abs() >= COORD_LIMIT {
            return Err(GeoError::OutOfRange);
        }
        let tol = (arc_tol * SCALE).clamp(1e-3, ARC_TOL_UNITS);
        Ok(Region::from_paths(inflate_paths_64(&self.to_paths()?, d * SCALE, JoinType::Round, EndType::Polygon, MITER_LIMIT, tol)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_of_a_segment_is_a_stadium() {
        let s = Region::sweep(&[vec![Point2::new(0.0, 0.0), Point2::new(10.0, 0.0)]], 1.0);
        let Ok(s) = s else { panic!("sweep refused a plain segment") };
        let want = 20.0 + std::f64::consts::PI;
        // Round ends are inscribed polygons at the 5 um arc tolerance.
        assert!((s.area() - want).abs() < 0.05, "area {}", s.area());
        assert!(s.contains(Point2::new(10.9, 0.0)));
        assert!(!s.contains(Point2::new(11.1, 0.0)));
    }

    #[test]
    fn a_fine_offset_turns_by_less_than_the_plain_one() {
        let Ok(sq) = Region::square(10.0) else { panic!("square") };
        let turn = |r: &Region| {
            let Some(l) = r.loops().first() else { return 0.0 };
            let p = l.points();
            let n = p.len();
            (0..n)
                .map(|i| {
                    let (a, b, c) = (p[i], p[(i + 1) % n], p[(i + 2) % n]);
                    let t0 = (b.y - a.y).atan2(b.x - a.x);
                    let t1 = (c.y - b.y).atan2(c.x - b.x);
                    (t1 - t0 + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
                })
                .fold(0.0f64, |m, t| m.max(t.abs()))
                .to_degrees()
        };
        let (Ok(plain), Ok(fine)) = (sq.offset(1.0), sq.offset_fine(1.0, 2.5e-4)) else { panic!("offset") };
        assert!(turn(&fine) < 3.0 && turn(&fine) < turn(&plain), "fine {} plain {}", turn(&fine), turn(&plain));
        assert!((fine.area() - (100.0 + 40.0 + std::f64::consts::PI)).abs() < 0.01, "area {}", fine.area());
        assert!(sq.offset_fine(f64::NAN, 1e-3).is_err());
    }

    #[test]
    fn sweep_refuses_bad_radius_and_points() {
        assert!(Region::sweep(&[vec![Point2::new(0.0, 0.0)]], 0.0).is_err());
        assert!(Region::sweep(&[vec![Point2::new(f64::NAN, 0.0)]], 1.0).is_err());
        assert!(Region::sweep(&[], 1.0).is_ok_and(|r| r.is_empty()));
    }
}
