//! The generators' refusal type, and the place a refusal names (#261
//! part 2): a refusal that is about one spot of the part - a hole, a
//! strip between two holes, a plunge into solid - carries that spot, so
//! the viewport can show where the operation was refused instead of
//! leaving the user to read coordinates off the sentence.
//! Provenance: our own error model (split out of `face.rs`, where the
//! enum was born); no vendor material consulted.
//!
//! A located refusal reads exactly like an [`GenError::Invalid`] one -
//! same prefix, same sentence - so no message, log line or test that
//! reads the text changes when a site starts naming its place. The
//! point is in the frame the generator ran in; the caller that knows
//! the frame (the app's `run_job`) maps it to world before the UI sees
//! it ([`GenError::map_location`]).

use thiserror::Error;

use crate::geo2d::GeoError;
use crate::geometry::Point3;

#[derive(Error, Debug, PartialEq)]
pub enum GenError {
    #[error("invalid machining parameters: {0}")]
    Invalid(String),
    /// An [`Invalid`](GenError::Invalid) refusal about one spot: `at` is
    /// that spot in the generator's frame. Displays as `Invalid` does.
    #[error("invalid machining parameters: {msg}")]
    At { msg: String, at: Point3 },
    #[error("nothing to machine: {0}")]
    Empty(String),
    #[error(transparent)]
    Geo(#[from] GeoError),
}

impl GenError {
    /// An invalid-parameters refusal about the spot `at`. A non-finite
    /// spot is no place to show: the refusal stays, unlocated, so a bad
    /// coordinate can never move the camera to NaN.
    #[must_use]
    pub fn invalid_at(msg: impl Into<String>, at: Point3) -> Self {
        let msg = msg.into();
        if at.x.is_finite() && at.y.is_finite() && at.z.is_finite() {
            GenError::At { msg, at }
        } else {
            GenError::Invalid(msg)
        }
    }

    /// The spot this refusal names, if it names one.
    #[must_use]
    pub fn location(&self) -> Option<Point3> {
        match self {
            GenError::At { at, .. } => Some(*at),
            _ => None,
        }
    }

    /// The same refusal with its spot carried through `f` (a frame's
    /// local-to-world map). A spot `f` sends to a non-finite point is
    /// dropped, never shown; an unlocated refusal passes untouched.
    #[must_use]
    pub fn map_location(self, f: impl FnOnce(Point3) -> Point3) -> Self {
        match self {
            GenError::At { msg, at } => GenError::invalid_at(msg, f(at)),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_located_refusal_reads_exactly_like_an_invalid_one() {
        let at = GenError::invalid_at("hole at (1, 2) is too small", Point3::new(1.0, 2.0, -3.0));
        let plain = GenError::Invalid("hole at (1, 2) is too small".into());
        assert_eq!(at.to_string(), plain.to_string());
        assert_eq!(at.location(), Some(Point3::new(1.0, 2.0, -3.0)));
        assert_eq!(plain.location(), None);
    }

    #[test]
    fn a_non_finite_spot_is_dropped_never_shown() {
        let e = GenError::invalid_at("x", Point3::new(f64::NAN, 0.0, 0.0));
        assert_eq!(e, GenError::Invalid("x".into()));
        let mapped = GenError::invalid_at("y", Point3::new(1.0, 1.0, 1.0)).map_location(|_| Point3::new(0.0, f64::INFINITY, 0.0));
        assert_eq!(mapped, GenError::Invalid("y".into()));
    }

    #[test]
    fn the_spot_rides_the_frame_map_and_unlocated_refusals_pass() {
        let e = GenError::invalid_at("z", Point3::new(1.0, 2.0, 3.0)).map_location(|p| Point3::new(p.x + 10.0, p.y, p.z));
        assert_eq!(e.location(), Some(Point3::new(11.0, 2.0, 3.0)));
        let empty = GenError::Empty("nothing".into()).map_location(|_| Point3::new(9.0, 9.0, 9.0));
        assert_eq!(empty, GenError::Empty("nothing".into()));
    }
}
