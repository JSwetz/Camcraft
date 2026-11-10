//! A lathe body's insert zone (#396): a box about the tip, in the
//! turning tool frame, holding the insert plate a vendor mesh models
//! with its holder. The clearance bands leave out whatever of the body
//! lies inside it, so the plate's edges beyond the modelled nose never
//! read as rubbing the chip the insert is cutting. Specced from the ISO
//! 1832 insert size range (the largest turning insert's inscribed circle
//! is 25.4 mm) and textbook Sutherland-Hodgman clipping against the
//! box's six faces. Our own design; no vendor source or file consulted.
//!
//! ## Never a blind spot for the holder
//!
//! - The box must hold the tip: the plate carries the cutting edge.
//! - It may reach no farther from the tip, along any axis, than one
//!   insert can: [`MAX_INSERT_REACH`] at most, and less where the tool's
//!   own record states a smaller insert (the app's `tool_body_lathe`).
//! - It may not hold the whole body: some holder must stay judged.
//! - Only geometry strictly inside the box leaves the bands: a facet
//!   crossing a face keeps every part of it outside, exactly.

/// The farthest an insert zone may reach from the tip along any axis
/// (mm): the largest ISO turning insert's inscribed circle.
pub const MAX_INSERT_REACH: f64 = 25.4;

/// A box in the turning tool frame (tip at the origin): `lo[k] <= 0 <=
/// hi[k]` on x (radial), y (normal) and z (along the spindle).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InsertZone {
    pub lo: [f64; 3],
    pub hi: [f64; 3],
}

/// Why a box is no insert zone. Each names itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InsertZoneError {
    NonFinite,
    MissesTip,
    /// The box reaches `reach` mm along `axis`, past the `cap` allowed.
    TooFar { axis: char, reach: f64, cap: f64 },
}

impl core::fmt::Display for InsertZoneError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NonFinite => f.write_str("the insert zone's corners must be finite numbers"),
            Self::MissesTip => f.write_str("the insert zone must hold the tip - the insert plate carries the cutting edge"),
            Self::TooFar { axis, reach, cap } => write!(
                f,
                "the insert zone reaches {reach:.3} mm from the tip along {axis}, past the {cap:.3} mm one insert can - a zone that size would hide the holder"
            ),
        }
    }
}

impl std::error::Error for InsertZoneError {}

impl InsertZone {
    /// Refuse a box that is not finite, misses the tip, or reaches
    /// along axis k farther than `cap[k]` (itself held to
    /// [`MAX_INSERT_REACH`]).
    pub fn check(&self, cap: [f64; 3]) -> Result<(), InsertZoneError> {
        if !self.lo.iter().chain(self.hi.iter()).all(|v| v.is_finite()) {
            return Err(InsertZoneError::NonFinite);
        }
        if (0..3).any(|k| self.lo[k] > 0.0 || self.hi[k] < 0.0) {
            return Err(InsertZoneError::MissesTip);
        }
        for (k, axis) in ['x', 'y', 'z'].into_iter().enumerate() {
            let reach = self.lo[k].abs().max(self.hi[k]);
            let cap = if cap[k].is_finite() { cap[k].clamp(0.0, MAX_INSERT_REACH) } else { MAX_INSERT_REACH };
            if reach > cap {
                return Err(InsertZoneError::TooFar { axis, reach, cap });
            }
        }
        Ok(())
    }

    /// The parts of polygon `poly` outside the box: one clip per face's
    /// outer half space. They may overlap (a corner region shows in two),
    /// which only repeats points; together they cover every point of the
    /// polygon not inside the box. Slivers of fewer than three vertices
    /// lie on a face of the box and are dropped with it.
    #[must_use]
    pub fn outside(&self, poly: &[[f64; 3]]) -> Vec<Vec<[f64; 3]>> {
        let mut out = Vec::new();
        for k in 0..3 {
            for (c, keep_above) in [(self.lo[k], false), (self.hi[k], true)] {
                let piece = clip_axis(poly, k, c, keep_above);
                if piece.len() >= 3 {
                    out.push(piece);
                }
            }
        }
        out
    }
}

/// One Sutherland-Hodgman pass on axis `k`: keep `p[k] >= c`
/// (`keep_above`) or `p[k] <= c`.
fn clip_axis(poly: &[[f64; 3]], k: usize, c: f64, keep_above: bool) -> Vec<[f64; 3]> {
    let inside = |p: &[f64; 3]| if keep_above { p[k] >= c } else { p[k] <= c };
    let mut out = Vec::with_capacity(poly.len() + 2);
    for (i, p) in poly.iter().enumerate() {
        let q = &poly[(i + 1) % poly.len()];
        let (pi, qi) = (inside(p), inside(q));
        if pi {
            out.push(*p);
        }
        if pi != qi {
            let d = q[k] - p[k];
            let s = if d == 0.0 { 0.0 } else { ((c - p[k]) / d).clamp(0.0, 1.0) };
            let mut x = [p[0] + (q[0] - p[0]) * s, p[1] + (q[1] - p[1]) * s, p[2] + (q[2] - p[2]) * s];
            x[k] = c;
            out.push(x);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const Z: InsertZone = InsertZone { lo: [-1.0, -5.0, -1.0], hi: [6.0, 1.0, 6.0] };

    #[test]
    fn a_box_about_the_tip_within_one_insert_passes_and_others_refuse_by_name() {
        assert_eq!(Z.check([16.0; 3]), Ok(()));
        let far = InsertZone { hi: [6.0, 1.0, 20.0], ..Z };
        assert!(matches!(far.check([16.0; 3]), Err(InsertZoneError::TooFar { axis: 'z', .. })));
        // The absolute cap holds whatever the tool states.
        let huge = InsertZone { lo: [-30.0, -1.0, -1.0], ..Z };
        assert!(matches!(huge.check([100.0; 3]), Err(InsertZoneError::TooFar { axis: 'x', cap, .. }) if cap == MAX_INSERT_REACH));
        assert_eq!(InsertZone { lo: [1.0, -1.0, -1.0], ..Z }.check([16.0; 3]), Err(InsertZoneError::MissesTip));
        assert_eq!(InsertZone { hi: [f64::NAN, 1.0, 1.0], ..Z }.check([16.0; 3]), Err(InsertZoneError::NonFinite));
        assert!(InsertZoneError::TooFar { axis: 'z', reach: 20.0, cap: 16.0 }.to_string().contains("hide the holder"));
    }

    #[test]
    fn the_outside_pieces_cover_exactly_what_lies_outside_the_box() {
        // A triangle crossing the box's +x face: its outside part is the
        // x >= 6 piece, and nothing inside the box survives.
        let t = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 0.0, 4.0]];
        let pieces = Z.outside(&t);
        assert_eq!(pieces.len(), 1, "{pieces:?}");
        assert!(pieces[0].iter().all(|p| p[0] >= 6.0 - 1e-12), "{pieces:?}");
        assert!(pieces[0].iter().any(|p| (p[0] - 10.0).abs() < 1e-12));
        // A triangle wholly inside leaves nothing; one wholly outside is whole.
        assert!(Z.outside(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]).is_empty());
        let away = [[20.0, 0.0, 0.0], [30.0, 0.0, 0.0], [20.0, 0.0, 4.0]];
        assert_eq!(Z.outside(&away), vec![away.to_vec()]);
    }
}
