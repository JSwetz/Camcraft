//! The stock eye's pure geometry (split from `eye.rs` at the 400-line
//! seam, M13.1): the viewport camera as the march's eye, the frame
//! budget fit, and the CPU frame's encoding into the composite's
//! layout. Provenance: `Camera::mvp`'s 60° law and Hart's sphere
//! tracing via the sim's `preview`; nothing from vendor binaries.

use camcraft_app::geometry::Point3;
use camcraft_app::stock_sim::{Preview, PreviewCamera};

use crate::viewport::Camera;

/// The camera's vertical half field of view - `Camera::mvp`'s 60° law.
pub(crate) const TAN_HALF_FOV: f64 = 0.577_350_269_189_625_8; // tan(pi/6)

/// The viewport camera as the march's eye: the same basis and eye the
/// `mvp` is built from, the same 60° vertical field.
#[must_use]
pub fn preview_camera(cam: &Camera) -> Option<PreviewCamera> {
    let (r, u, f) = cam.view_basis();
    let e = cam.eye();
    let p = |t: (f64, f64, f64)| Point3::new(t.0, t.1, t.2);
    let pc = PreviewCamera {
        eye: Point3::new(e[0], e[1], e[2]),
        fwd: p(f),
        right: p(r),
        up: p(u),
        tan_half_fov: TAN_HALF_FOV,
    };
    let finite = [pc.eye, pc.fwd, pc.right, pc.up].iter().all(|q| q.is_finite());
    let unit = |q: Point3| ((q.x * q.x + q.y * q.y + q.z * q.z) - 1.0).abs() < 1e-6;
    (finite && unit(pc.fwd) && unit(pc.right) && unit(pc.up)).then_some(pc)
}

/// The camera's bits, for a frame key.
pub(crate) fn cam_bits(c: &PreviewCamera) -> [u64; 4] {
    let h = |p: Point3| p.x.to_bits() ^ p.y.to_bits().rotate_left(21) ^ p.z.to_bits().rotate_left(42);
    [h(c.eye), h(c.fwd), h(c.right), h(c.up)]
}

/// The smallest integer downscale `k` whose trace frame fits `budget`
/// pixels: (k, trace width, trace height), never zero-sized.
#[must_use]
pub fn fit(view: (u32, u32), budget: u64) -> (u32, u32, u32) {
    let (w, h) = (view.0.max(1), view.1.max(1));
    let mut k = 1u32;
    loop {
        let (tw, th) = (w.div_ceil(k), h.div_ceil(k));
        if u64::from(tw) * u64::from(th) <= budget.max(1) || tw == 1 && th == 1 {
            return (k, tw, th);
        }
        k = k.saturating_add(1);
    }
}

/// A CPU frame as the kernel's vec4s (depth, class, shade, status):
/// status 1 hit, 0 miss - the composite reads one layout from both eyes.
#[must_use]
pub fn encode(pv: &Preview) -> Vec<u8> {
    let mut b = Vec::with_capacity(pv.depth.len() * 16);
    for i in 0..pv.depth.len() {
        let d = pv.depth[i];
        let hit = d.is_finite();
        let cls = pv.class.get(i).copied().unwrap_or(3.0);
        let shade = pv.shade.get(i).copied().unwrap_or(0.0);
        for x in [if hit { d as f32 } else { 0.0 }, cls, shade, if hit { 1.0 } else { 0.0 }] {
            b.extend_from_slice(&x.to_le_bytes());
        }
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::eye::GPU_PIXELS;

    #[test]
    fn the_fit_keeps_small_frames_whole_and_halves_a_4k_one() {
        assert_eq!(fit((800, 600), GPU_PIXELS), (1, 800, 600));
        let (k, tw, th) = fit((3840, 2160), GPU_PIXELS);
        assert_eq!(k, 2);
        assert!(u64::from(tw) * u64::from(th) <= GPU_PIXELS);
        assert_eq!(fit((0, 0), 0), (1, 1, 1));
    }

    #[test]
    fn the_preview_camera_is_the_viewport_basis() {
        let cam = Camera::default();
        let pc = preview_camera(&cam).expect("default camera is sane");
        let (r, u, f) = cam.view_basis();
        assert!((pc.fwd.x - f.0).abs() < 1e-12 && (pc.right.y - r.1).abs() < 1e-12);
        assert!((pc.up.z - u.2).abs() < 1e-12);
        // The centre ray of any frame is the forward axis.
        let d = pc.ray_dir(50, 50, 101, 101);
        assert!((d.x - f.0).abs() < 1e-9 && (d.y - f.1).abs() < 1e-9 && (d.z - f.2).abs() < 1e-9);
    }

    #[test]
    fn a_cpu_miss_encodes_as_status_zero_never_a_hit() {
        let pv = Preview { w: 2, h: 1, depth: vec![f64::NAN, 3.5], class: vec![3.0, 1.0], shade: vec![0.0, 0.7], unresolved: 0 };
        let b = encode(&pv);
        let f = |o: usize| f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        assert_eq!(f(12), 0.0);
        assert_eq!((f(16), f(20), f(28)), (3.5, 1.0, 1.0));
    }
}
