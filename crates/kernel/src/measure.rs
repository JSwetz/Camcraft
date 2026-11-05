//! The bench's common currency: solid volume + axis-aligned bounding box, and
//! the signed mesh volume truck's tessellation is measured with.
//! Provenance: divergence theorem over a closed triangle mesh (sum of signed
//! tetrahedra against the origin) - standard computational-geometry result.

use crate::P3;

/// Volume and bounding box of a closed solid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measure {
    /// Enclosed volume (positive for an outward-oriented solid).
    pub volume: f64,
    /// Minimum corner of the bounding box.
    pub min: P3,
    /// Maximum corner of the bounding box.
    pub max: P3,
}

impl Measure {
    /// Largest per-axis distance between the two boxes' corners.
    pub fn bbox_deviation(&self, other: &Measure) -> f64 {
        let mut worst = 0.0_f64;
        for i in 0..3 {
            worst = worst
                .max((self.min[i] - other.min[i]).abs())
                .max((self.max[i] - other.max[i]).abs());
        }
        worst
    }
}

/// Signed volume of a triangle soup (`tris` index `positions`). `None` when
/// an index is out of range or a coordinate is non-finite - a corrupt mesh is
/// reported, never read past.
pub fn mesh_signed_volume(positions: &[P3], tris: &[[usize; 3]]) -> Option<f64> {
    let mut six_v = 0.0_f64;
    for t in tris {
        let a = positions.get(t[0])?;
        let b = positions.get(t[1])?;
        let c = positions.get(t[2])?;
        // a . (b x c)
        let cx = b[1] * c[2] - b[2] * c[1];
        let cy = b[2] * c[0] - b[0] * c[2];
        let cz = b[0] * c[1] - b[1] * c[0];
        six_v += a[0] * cx + a[1] * cy + a[2] * cz;
    }
    let v = six_v / 6.0;
    v.is_finite().then_some(v)
}
