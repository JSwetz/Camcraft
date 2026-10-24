//! Tool-pose conversion: the unit axis a move states and the RS-274
//! letter angles a stream commands.
//!
//! Provenance: RS-274 addresses A, B and C as rotations about X, Y and
//! Z (public standard); the composition order and the canonical form
//! below are ours, stated ONCE here so every consumer - the stream
//! dump, the checker's refusals, the sim's swept bands - rebuilds the
//! same vector from the same letters, never a private copy.
//!
//! Convention: `axis = Rz(C) · Ry(B) · Rx(A) · ẑ`, angles in degrees -
//! A tips toward −Y, B turns toward +X, C spins last. The canonical
//! form of a given axis keeps C at zero: `A = asin(−ay) ∈ [−90°, 90°]`
//! and `B = atan2(ax, az)`. Exact, not approximate: for a unit vector,
//! ax² + az² = cos²A, so Ry(B)·Rx(A)·ẑ reconstructs it. Commanded
//! letters are the truth once commanded: consumers rebuild the vector
//! FROM the letters and never round-trip floats back through them.

/// The unit vector along `d`, or None when `d` states no direction:
/// a non-finite component, or a length under 1e-12 (float dust must
/// not be amplified into a pose). Exact inputs keep their bits: an
/// axis of [0, 0, 1] divides by a length of exactly 1.0, so the
/// prismatic case never drifts off the vertical the laws branch on.
#[must_use]
pub fn normalize_axis(d: [f64; 3]) -> Option<[f64; 3]> {
    if !d.iter().all(|x| x.is_finite()) {
        return None;
    }
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len < 1e-12 {
        return None;
    }
    Some([d[0] / len, d[1] / len, d[2] / len])
}

/// True when the axis states no angle: straight up the Z column,
/// EXACTLY. The exactness is load-bearing: the dump and the sim's
/// bands take the bit-identical prismatic code path when this reads
/// true, so goldens never learn about float drift. A stated
/// [0, 0, −1] is NOT prismatic - that is the tool end-over-end, a
/// 180° B rotation, and it reads as one.
#[must_use]
pub fn is_vertical(axis: &[f64; 3]) -> bool {
    axis[0] == 0.0 && axis[1] == 0.0 && axis[2] > 0.0
}

/// The canonical letter angles (degrees) of a unit axis: [A, B, 0].
/// Meant for vectors that state a direction (from `with_axis`,
/// `set_axis` or the project lattice); two guards keep near-garbage
/// from becoming NaN: the asin clamps a vector a hair longer than
/// unit, and B reads 0 exactly where atan2 would take nothing over
/// nothing (the zero vector - every other input states something).
#[must_use]
pub fn axis_to_angles(axis: [f64; 3]) -> [f64; 3] {
    let a = (-axis[1]).clamp(-1.0, 1.0).asin().to_degrees();
    let b = if axis[0] == 0.0 && axis[2] == 0.0 {
        0.0
    } else {
        axis[0].atan2(axis[2]).to_degrees()
    };
    [a, b, 0.0]
}

/// The axis a commanded letter triple (degrees) holds, composed in the
/// convention above. Re-normalized - the rotation product is unit to
/// the last bit, and the vector contract says unit. None on a
/// non-finite letter (there is no direction in a NaN angle).
#[must_use]
pub fn angles_to_axis(deg: [f64; 3]) -> Option<[f64; 3]> {
    if !deg.iter().all(|x| x.is_finite()) {
        return None;
    }
    let (sa, ca) = deg[0].to_radians().sin_cos();
    let (sb, cb) = deg[1].to_radians().sin_cos();
    let (sc, cc) = deg[2].to_radians().sin_cos();
    normalize_axis([ca * sb * cc + sa * sc, ca * sb * sc - sa * cc, ca * cb])
}
