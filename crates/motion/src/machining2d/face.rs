//! Split from `machining2d.rs` by M5.0.1; structure only.

use thiserror::Error;
use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{GeoError, Region};
use crate::geometry::Point3;

use super::levels::cut_levels;
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
/// snake back and forth at each cut depth - in the single-level default
/// rapids appear only at the two ends, matching how facers actually run;
/// staged facing adds the level-to-level lift through clearance.
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
    /// First cut level for staged facing. `None` - the default, and what
    /// every pre-stepdown file carries - faces in one level at `cut_z`.
    pub top_z: Option<f64>,
    /// Level spacing while staging; must be positive when `top_z` is
    /// set, ignored otherwise.
    pub stepdown: Option<f64>,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Climb milling: the first pass runs -X to +X. Conventional starts
    /// the other way.
    pub climb: bool,
}

impl FaceParams {
    pub(super) fn validate(&self) -> Result<(), GenError> {
        let mut vals = vec![
            self.tool_diameter,
            self.stepover,
            self.clearance_z,
            self.cut_z,
            self.feed,
            self.plunge_feed,
        ];
        vals.extend([self.top_z, self.stepdown].into_iter().flatten());
        if !vals.into_iter().all(f64::is_finite) {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if let Some(t) = self.top_z {
            if t < self.cut_z {
                return Err(GenError::Invalid("staged top below the finished height".into()));
            }
            if self.clearance_z <= t {
                // Equality is refused too: a level sitting ON the rapid
                // plane would plunge zero-depth, and the builder drops
                // zero-length moves - a silently missing plunge.
                return Err(GenError::Invalid("clearance plane not above the first cut level".into()));
            }
            if !(self.stepdown.is_some_and(|s| s > 0.0)) && t > self.cut_z {
                return Err(GenError::Invalid("staged facing needs a positive stepdown".into()));
            }
        }
        if self.stepdown.is_some_and(|s| s <= 0.0) {
            return Err(GenError::Invalid("stepdown must be positive".into()));
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

/// Serpentine face passes over `region`. One level at `p.cut_z` when
/// `p.top_z` is `None` - the legacy facing, unchanged to the byte - or a
/// staged descent through `cut_levels(top_z, cut_z, stepdown)` when it
/// is set: the rows are computed once and run at every level, each level
/// entered from the clearance plane the same way the single level was.
/// A stepover wider than the tool leaves an uncut band between rows -
/// visible in the path, never silently smoothed over, and never a crash:
/// facing simply cuts where it passes.
pub fn face(region: &Region, p: &FaceParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    let (lo, hi) = match region.bounds() {
        Some(b) => b,
        None => return Err(GenError::Empty("empty footprint".into())),
    };
    let r = p.tool_diameter * 0.5;
    let ys = band(lo.y, hi.y, r, p.stepover);
    let (x_in, x_out) = (lo.x - r, hi.x + r);
    let zs = match (p.top_z, p.stepdown) {
        (Some(t), Some(s)) => cut_levels(t, p.cut_z, s),
        _ => vec![p.cut_z],
    };

    let start_x = if p.climb { x_in } else { x_out };
    let mut b = PathBuilder::new(Point3::new(start_x, ys[0], p.clearance_z));
    for (k, &z) in zs.iter().enumerate() {
        if k > 0 {
            // New material is engaged from the air, never from under:
            // lift to clearance and re-enter above the row's start.
            let at = b.at();
            b.push(MoveKind::Rapid, Point3::new(at.x, at.y, p.clearance_z), None);
            b.push(MoveKind::Rapid, Point3::new(start_x, ys[0], p.clearance_z), None);
        }
        b.push(MoveKind::Plunge, Point3::new(start_x, ys[0], z), Some(p.plunge_feed));
        for (i, y) in ys.iter().enumerate() {
            // Serpentine: even rows run the climb direction, odd rows return.
            let forward = p.climb == (i % 2 == 0);
            b.push(
                MoveKind::Feed,
                Point3::new(if forward { x_out } else { x_in }, *y, z),
                Some(p.feed),
            );
            if i + 1 < ys.len() {
                let at = b.at();
                b.push(MoveKind::Feed, Point3::new(at.x, ys[i + 1], z), Some(p.feed));
            }
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
pub(super) fn band(a: f64, b: f64, r: f64, step: f64) -> Vec<f64> {
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
