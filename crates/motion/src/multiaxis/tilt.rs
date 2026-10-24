//! The tool-axis control law of the simultaneous-axis finishing kinds
//! (M5.1 stage C). Specced from public multi-axis milling literature
//! (lead/lag angle definitions as in the ISO 10791-10 test-part
//! tradition and standard CAM manuals): at a surface contact point
//! with unit normal `n` and unit path tangent `t`, the commanded tool
//! axis starts at `n` (tip toward spindle), leans FORWARD along the
//! feed by the lead angle - a rotation about the binormal `n x t` -
//! and SIDEWAYS by the lag (side tilt) about the tangent `t`. The
//! `max_tilt` clamp then folds any axis that would lean further than
//! the allowed angle from the surface normal back onto that cone,
//! in the plane the rotation produced (never past it: an axis the
//! machine cannot hold is worse than an axis a finishing pass will
//! re-cut).
//!
//! The plan doc named the two rotation axes in the opposite pairing;
//! the industry-standard pairing is implemented here and the swap is
//! recorded in RECOMMENDED.md - the labels are what a machinist
//! reads, so they ride their standard meaning.
//!
//! The `stamp` module is where this law gets APPLIED: it walks a
//! ball-center path and asks, at every move, which normal and tangent
//! frame the command belongs to.

use crate::geometry::Point3;

/// The axis-control fields shared by the 5-axis kinds. Angles in
/// degrees; `max_tilt_deg` None says "no clamp" (the machine's own
/// reachability check, at generation time, is then the only wall).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisControl {
    /// Forward lean along the feed, degrees.
    pub lead_deg: f64,
    /// Sideways lean against the feed direction, degrees.
    pub lag_deg: f64,
    /// Largest allowed angle between the command and the surface
    /// normal, degrees. `None` runs unclamped.
    pub max_tilt_deg: Option<f64>,
}

impl Default for AxisControl {
    fn default() -> Self {
        Self {
            lead_deg: 0.0,
            lag_deg: 0.0,
            max_tilt_deg: Some(90.0),
        }
    }
}

/// Rotation of `v` about the unit axis `k` by `ang` (Rodrigues, f64).
/// `v` and `k` are assumed unit and finite; the caller's frame came
/// from facet geometry.
fn rot(v: Point3, k: Point3, ang: f64) -> Point3 {
    let (c, s) = (ang.cos(), ang.sin());
    let kv = k.x * v.x + k.y * v.y + k.z * v.z;
    let kxv = Point3::new(
        k.y * v.z - k.z * v.y,
        k.z * v.x - k.x * v.z,
        k.x * v.y - k.y * v.x,
    );
    Point3::new(
        v.x * c + kxv.x * s + k.x * kv * (1.0 - c),
        v.y * c + kxv.y * s + k.y * kv * (1.0 - c),
        v.z * c + kxv.z * s + k.z * kv * (1.0 - c),
    )
}

/// The angle between two directions, radians (0..=pi), from the
/// clamped dot and the cross norm - atan2 for the wide, stable form.
pub(super) fn angle_between(a: Point3, b: Point3) -> f64 {
    let d = (a.x * b.x + a.y * b.y + a.z * b.z).clamp(-1.0, 1.0);
    let cx = Point3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    );
    (cx.x * cx.x + cx.y * cx.y + cx.z * cx.z).sqrt().atan2(d)
}

/// The commanded axis at one contact point: `n` unit facet normal
/// (material side), `t` unit path tangent (feed direction). None when
/// the inputs cannot frame a direction (zero tangent, non-finite
/// angle) - the generator refuses by name rather than guess an axis.
#[must_use]
pub fn command_axis(n: Point3, t: Point3, c: &AxisControl) -> Option<Point3> {
    let nn = (n.x * n.x + n.y * n.y + n.z * n.z).sqrt();
    let tn = (t.x * t.x + t.y * t.y + t.z * t.z).sqrt();
    if nn <= 1e-12
        || tn <= 1e-12
        || !n.is_finite()
        || !t.is_finite()
        || !c.lead_deg.is_finite()
        || !c.lag_deg.is_finite()
    {
        return None;
    }
    // The law is written for unit inputs; a degenerate facet normal
    // reaching here (zero-area triangle from a corrupt mesh) is
    // renormalized, never silently trusted.
    let n = Point3::new(n.x / nn, n.y / nn, n.z / nn);
    let t = Point3::new(t.x / tn, t.y / tn, t.z / tn);
    // Binormal of the contact frame: n x t.
    let b = Point3::new(
        n.y * t.z - n.z * t.y,
        n.z * t.x - n.x * t.z,
        n.x * t.y - n.y * t.x,
    );
    let bn = (b.x * b.x + b.y * b.y + b.z * b.z).sqrt();
    // A tangent parallel to the normal has no binormal: lead cannot
    // lean anywhere. Fall back to the side rotation only - the law,
    // not a guess, and the vertical-drill degenerate this frame can
    // meet on a wall-straddling contact.
    let a = if bn > 1e-9 {
        rot(n, Point3::new(b.x / bn, b.y / bn, b.z / bn), c.lead_deg.to_radians())
    } else {
        n
    };
    // Lag: the same rotation is well-defined wherever t is (it is).
    let a = rot(a, t, c.lag_deg.to_radians());
    let lim = match c.max_tilt_deg {
        None => return Some(a),
        Some(d) if d.is_finite() && d >= 0.0 => d.to_radians(),
        Some(_) => return None, // a non-finite clamp is a broken param
    };
    let off = angle_between(n, a);
    if off <= lim || off <= 1e-12 {
        return Some(a);
    }
    // Fold back onto the cone: rotate n toward a about n x a by the
    // clamp. n and a differ (off > 0) so the hinge exists; if it did
    // not, a == -n and the fold is undefined - keep a and let the
    // machine check refuse.
    let h = Point3::new(
        n.y * a.z - n.z * a.y,
        n.z * a.x - n.x * a.z,
        n.x * a.y - n.y * a.x,
    );
    let hn = (h.x * h.x + h.y * h.y + h.z * h.z).sqrt();
    if hn <= 1e-12 {
        return Some(a);
    }
    Some(rot(
        n,
        Point3::new(h.x / hn, h.y / hn, h.z / hn),
        lim,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    fn len(v: Point3) -> f64 {
        (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
    }

    fn unit(v: Point3) -> Point3 {
        let l = len(v);
        p(v.x / l, v.y / l, v.z / l)
    }

    fn near(a: Point3, b: Point3) -> bool {
        len(a - b) < 1e-12
    }

    #[test]
    fn no_angles_and_no_clamp_is_the_plain_normal() {
        let n = p(0.0, 0.0, 1.0);
        let c = AxisControl {
            lead_deg: 0.0,
            lag_deg: 0.0,
            max_tilt_deg: None,
        };
        assert!(near(
            command_axis(n, p(1.0, 0.0, 0.0), &c).unwrap(),
            n
        ));
    }

    #[test]
    fn lead_leans_forward_in_the_feed_plane() {
        // Floor normal +z, feed +x: a positive lead tips the axis
        // toward +x (forward), never sideways, and keeps it unit.
        let c = AxisControl {
            lead_deg: 30.0,
            lag_deg: 0.0,
            max_tilt_deg: None,
        };
        let a = command_axis(p(0.0, 0.0, 1.0), p(1.0, 0.0, 0.0), &c).unwrap();
        assert!(near(a, p(0.5, 0.0, 3.0f64.sqrt() * 0.5)), "{a:?}");
        assert!((len(a) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn lag_leans_sideways_against_the_feed() {
        // Positive lag about the tangent tips the axis toward -y on
        // the floor/+x-feed frame: the side lean is its own rotation,
        // composed after the lead.
        let c = AxisControl {
            lead_deg: 0.0,
            lag_deg: 30.0,
            max_tilt_deg: None,
        };
        let a = command_axis(p(0.0, 0.0, 1.0), p(1.0, 0.0, 0.0), &c).unwrap();
        assert!(near(a, p(0.0, -0.5, 3.0f64.sqrt() * 0.5)), "{a:?}");
    }

    #[test]
    fn lead_then_lag_equals_both_rotations_composed() {
        // A wall-straddling frame where the two do not commute: the
        // composition order is the documented one (lead about n x t,
        // then lag about t).
        let n = p(0.0, 0.0, 1.0);
        let t = unit(p(3.0f64.sqrt() * 0.5, 0.0, 0.5));
        let both = command_axis(
            n,
            t,
            &AxisControl {
                lead_deg: 20.0,
                lag_deg: 15.0,
                max_tilt_deg: None,
            },
        )
        .unwrap();
        let step = rot(rot(n, unit(n.cross(t)), 20f64.to_radians()), t, 15f64.to_radians());
        assert!(near(both, step));
    }

    #[test]
    fn the_clamp_folds_to_the_cone_never_past_it() {
        // 60 degrees of lead against a 30-degree clamp: the result
        // sits ON the cone, in the lead's own plane.
        let c = AxisControl {
            lead_deg: 60.0,
            lag_deg: 0.0,
            max_tilt_deg: Some(30.0),
        };
        let a = command_axis(p(0.0, 0.0, 1.0), p(1.0, 0.0, 0.0), &c).unwrap();
        let ang = angle_between(p(0.0, 0.0, 1.0), a);
        assert!((ang - 30f64.to_radians()).abs() < 1e-12, "{ang}");
        assert!(a.y.abs() < 1e-15);
        assert!(a.x > 0.0);
    }

    #[test]
    fn a_clamp_above_the_needed_lean_changes_nothing() {
        let c = AxisControl {
            lead_deg: 10.0,
            lag_deg: 0.0,
            max_tilt_deg: Some(30.0),
        };
        let a = command_axis(p(0.0, 0.0, 1.0), p(1.0, 0.0, 0.0), &c).unwrap();
        assert!(near(
            a,
            p(10f64.to_radians().sin(), 0.0, 10f64.to_radians().cos())
        ));
    }

    #[test]
    fn garbage_angles_and_a_zero_tangent_refuse() {
        let c = AxisControl {
            lead_deg: f64::NAN,
            lag_deg: 0.0,
            max_tilt_deg: None,
        };
        assert!(command_axis(p(0.0, 0.0, 1.0), p(1.0, 0.0, 0.0), &c).is_none());
        let ok = AxisControl::default();
        assert!(command_axis(p(0.0, 0.0, 1.0), p(0.0, 0.0, 0.0), &ok).is_none());
    }

    #[test]
    fn a_tangent_parallel_to_the_normal_drops_only_the_lead() {
        // The degenerate contact frame: no binormal exists, so the
        // side rotation still runs about t.
        let c = AxisControl {
            lead_deg: 25.0,
            lag_deg: 10.0,
            max_tilt_deg: None,
        };
        let a = command_axis(p(0.0, 0.0, 1.0), p(0.0, 0.0, 1.0), &c).unwrap();
        let want = rot(p(0.0, 0.0, 1.0), p(0.0, 0.0, 1.0), 10f64.to_radians());
        assert!(near(a, want));
    }

}
