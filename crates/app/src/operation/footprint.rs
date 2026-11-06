//! The area an operation really machines, for the viewport (the user's
//! hand-test: "unclear what boundary actually does"): the clearing
//! kinds and the contour work their region as it is, but Face clears
//! the whole bounding band of its region - rows across its full span,
//! each run overrunning both ends by a tool radius - laid along an open
//! boundary's chain. Showing the region alone would promise a shape the
//! facing law never follows, so Face's footprint is that band, built by
//! the same bounds and turn generation uses.
//!
//! Provenance: our own design notes; the band is `machining2d::face`'s
//! own row law (bounds, radius overrun) and `boundary_turn`'s turn. No
//! vendor material consulted.

use super::boundary_turn::turn_region;
use super::*;

/// The tool-swept footprint of `op` as the world stands now, at the
/// region's plane: `region_of` for every kind but Face, Face's laid band
/// for Face. `None` exactly when `region_of` is `None` (nothing honest
/// to draw), or when the band cannot be built.
#[must_use]
pub fn machined_footprint(world: &World, op: &Operation) -> Option<Region> {
    let (region, turn) = region_turn_of(world, op)?;
    if !matches!(op.params, OpParams::Face { .. }) {
        return Some(region);
    }
    let r = model::find_by_name(world, &op.tool)
        .and_then(|e| model::tool_of(world, &e))
        .map(|t| t.diameter * 0.5)
        .filter(|r| r.is_finite() && *r > 0.0)
        .unwrap_or(0.0);
    face_band(&region, turn, r).ok()
}

/// The band Face sweeps over `region` with tool radius `r`, its rows laid
/// at `turn`: in the laid frame the bounds, widened in X by the overrun
/// (centre a radius past each end, so the edge reaches two) and in Y to
/// at least one tool width; turned back into the world frame.
pub(super) fn face_band(region: &Region, turn: f64, r: f64) -> Result<Region, GenError> {
    let laid = turn_region(region, turn)?;
    let Some((lo, hi)) = laid.bounds() else {
        return Err(GenError::Empty("empty footprint".into()));
    };
    let mid = (lo.y + hi.y) * 0.5;
    let (y0, y1) = (lo.y.min(mid - r), hi.y.max(mid + r));
    let (x0, x1) = (lo.x - 2.0 * r, hi.x + 2.0 * r);
    let band = Region::new(vec![Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])?])?;
    turn_region(&band, -turn)
}

#[cfg(test)]
mod law {
    use super::*;

    fn tri() -> Region {
        Region::new(vec![
            Loop::new(vec![Point2::new(0.0, 0.0), Point2::new(10.0, 0.0), Point2::new(0.0, 6.0)]).expect("loop"),
        ])
        .expect("region")
    }

    #[test]
    fn face_sweeps_the_bounding_band_with_its_radius_overrun() {
        let band = face_band(&tri(), 0.0, 2.0).expect("band");
        let (lo, hi) = band.bounds().expect("bounds");
        assert!((lo.x + 4.0).abs() < 1e-9 && (hi.x - 14.0).abs() < 1e-9, "{lo:?} {hi:?}");
        assert!(lo.y.abs() < 1e-9 && (hi.y - 6.0).abs() < 1e-9);
        assert!(band.contains(Point2::new(9.0, 5.0)), "the corner the triangle leaves is swept too");
    }

    #[test]
    fn a_quarter_turn_lays_the_band_along_y() {
        let band = face_band(&tri(), std::f64::consts::FRAC_PI_2, 1.0).expect("band");
        let (lo, hi) = band.bounds().expect("bounds");
        // Rows along Y: the overrun lands on the Y ends.
        assert!((lo.y + 2.0).abs() < 1e-6 && (hi.y - 8.0).abs() < 1e-6, "{lo:?} {hi:?}");
        assert!(lo.x.abs() < 1e-6 && (hi.x - 10.0).abs() < 1e-6);
    }

    #[test]
    fn a_thin_area_still_sweeps_one_tool_width() {
        let thin = Region::new(vec![
            Loop::new(vec![Point2::new(0.0, 0.0), Point2::new(10.0, 0.0), Point2::new(10.0, 1.0), Point2::new(0.0, 1.0)])
                .expect("loop"),
        ])
        .expect("region");
        let (lo, hi) = face_band(&thin, 0.0, 3.0).expect("band").bounds().expect("bounds");
        assert!((hi.y - lo.y - 6.0).abs() < 1e-9);
    }
}
