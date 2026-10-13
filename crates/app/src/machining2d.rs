//! 2D machining generators: region + parameters in, CL path out.
//!
//! Provenance: serpentine facing, offset contouring, zigzag and waterline
//! pocketing are textbook CAM strategies as described in machine-tool
//! handbooks and published CAM literature; every number (parameter model,
//! entry/exit overruns, snap rules) is ours. Pure module: no flecs, no
//! world access - the command layer resolves geometry into regions, this
//! module only cuts them.
//!
//! Prime Directive: parameters are validated before any geometry is
//! touched; impossible shapes return `GenError` (or a clean empty result
//! where the plan says collapse means empty), never a partially linked
//! path. Every returned path is chained, finite, and posts real motion.

use thiserror::Error;

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{GeoError, Region};
use crate::geometry::Point3;

#[derive(Error, Debug, PartialEq)]
pub enum GenError {
    #[error("invalid machining parameters: {0}")]
    Invalid(String),
    #[error("nothing to machine: {0}")]
    Empty(String),
    #[error(transparent)]
    Geo(#[from] GeoError),
}

/// Facing: clear the top plane of a footprint in straight XY passes.
///
/// The path lives in world coordinates (regions arrive in the frame the
/// stock is drawn in). Rows band the footprint's Y extent at `stepover`,
/// centered a tool radius in from each edge so the full width is covered;
/// X runs overrun the bbox by a radius for clean entry and exit. Passes
/// snake back and forth at cut depth - rapids appear only at the two
/// ends, matching how facers actually run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceParams {
    pub tool_diameter: f64,
    /// Pass spacing in Y; the overlap 2r - stepover is what leaves the
    /// scallop height, which facing simply accepts.
    pub stepover: f64,
    /// Rapid plane above the material.
    pub clearance_z: f64,
    /// The finished surface height.
    pub cut_z: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Climb milling: the first pass runs -X to +X. Conventional starts
    /// the other way.
    pub climb: bool,
}

impl FaceParams {
    fn validate(&self) -> Result<(), GenError> {
        let finite = [
            self.tool_diameter,
            self.stepover,
            self.clearance_z,
            self.cut_z,
            self.feed,
            self.plunge_feed,
        ]
        .into_iter()
        .all(f64::is_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.tool_diameter <= 0.0 {
            return Err(GenError::Invalid("tool diameter must be positive".into()));
        }
        if self.stepover <= 0.0 {
            return Err(GenError::Invalid("stepover must be positive".into()));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        Ok(())
    }
}

/// Serpentine face pass over `region` at `p.cut_z`. A stepover wider than
/// the tool leaves an uncut band between rows - visible in the path, never
/// silently smoothed over, and never a crash: facing simply cuts where it
/// passes.
pub fn face(region: &Region, p: &FaceParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    let (lo, hi) = match region.bounds() {
        Some(b) => b,
        None => return Err(GenError::Empty("empty footprint".into())),
    };
    let r = p.tool_diameter * 0.5;
    let ys = band(lo.y, hi.y, r, p.stepover);
    let (x_in, x_out) = (lo.x - r, hi.x + r);

    let start_x = if p.climb { x_in } else { x_out };
    let mut b = PathBuilder::new(Point3::new(start_x, ys[0], p.clearance_z));
    b.push(
        MoveKind::Plunge,
        Point3::new(start_x, ys[0], p.cut_z),
        Some(p.plunge_feed),
    );
    for (i, y) in ys.iter().enumerate() {
        // Serpentine: even rows run the climb direction, odd rows return.
        let forward = p.climb == (i % 2 == 0);
        b.push(
            MoveKind::Feed,
            Point3::new(if forward { x_out } else { x_in }, *y, p.cut_z),
            Some(p.feed),
        );
        if i + 1 < ys.len() {
            let at = b.at();
            b.push(
                MoveKind::Feed,
                Point3::new(at.x, ys[i + 1], p.cut_z),
                Some(p.feed),
            );
        }
    }
    let at = b.at();
    b.push(MoveKind::Rapid, Point3::new(at.x, at.y, p.clearance_z), None);

    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// Row centers banding [a + r, b - r]: start at the low edge's coverage
/// center, stride by `step`, and always end exactly at the high edge's
/// coverage center (a stride that would overshoot is replaced by that
/// snap; a band thinner than one tool diameter gets the single center
/// row). Never empty, never unsorted.
fn band(a: f64, b: f64, r: f64, step: f64) -> Vec<f64> {
    let (first, last) = (a + r, b - r);
    let span = last - first;
    if span <= 0.0 {
        return vec![(a + b) * 0.5];
    }
    let tol = span * 1e-9;
    let mut ys = vec![first];
    let mut y = first;
    while y + step < last - tol {
        y += step;
        ys.push(y);
    }
    if last - *ys.last().unwrap_or(&first) > tol {
        ys.push(last);
    }
    ys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_snaps_to_both_coverage_centers() {
        assert_eq!(band(0.0, 20.0, 5.0, 5.0), vec![5.0, 10.0, 15.0]);
        assert_eq!(band(0.0, 20.0, 5.0, 12.0), vec![5.0, 15.0]);
        assert_eq!(band(0.0, 20.0, 5.0, 7.0), vec![5.0, 12.0, 15.0]);
        // Thinner than one tool: the single center row, still covering.
        assert_eq!(band(0.0, 6.0, 5.0, 3.0), vec![3.0]);
    }
}
