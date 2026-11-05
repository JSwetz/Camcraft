//! The viewport's stock eye (M7.2 B3b): owns the trace pass and the
//! composite, keeps the document's field view resident on the GPU, and
//! re-marches only when the camera, the extent or the field changed.
//! Provenance: Hart's sphere tracing (SIGGRAPH 1996) via `TracePass`
//! and the sim's CPU `preview`; frame caching is our own design.
//!
//! ## The laws here
//!
//! - **The GPU is a speedup, never a dependency.** A device that
//!   refuses the kernel, or a lattice past its storage limit, marches
//!   on the CPU instead - at a thumbnail resolution, upsampled by the
//!   same composite. Only a device that cannot composite at all keeps
//!   the sheet, and the shell says so.
//! - **A refusal is said once.** `prepare` answers `Err` the first time
//!   a frame fails, then quietly keeps the sheet for that same state -
//!   the status line is not spammed every frame, and the log names it.
//! - **Nothing stale.** The field the GPU holds is the `Arc` the state
//!   holds; a different (or absent) field view clears the resident
//!   lattice and the cached frame before anything else happens.

use std::sync::Arc;

use egui_wgpu::wgpu;
use wgpu::util::DeviceExt;

use camcraft_app::stock_sim::preview::{pack_field, pack_target};
use camcraft_app::stock_sim::{preview, Preview, PreviewCamera, DEV_TOL};
use camcraft_app::stock_view::FieldView;
use camcraft_app::geometry::Point3;

use super::composite::{Composite, FrameGeom};
use super::TracePass;
use crate::viewport::Camera;

/// GPU trace budget per frame: past it the trace runs at an integer
/// fraction of the viewport (a 4K panel traces at half resolution).
const GPU_PIXELS: u64 = 1 << 21;

/// CPU fallback budget: a thumbnail the march answers interactively.
const CPU_PIXELS: u64 = 1 << 15;

/// The camera's vertical half field of view - `Camera::mvp`'s 60° law.
const TAN_HALF_FOV: f64 = 0.577_350_269_189_625_8; // tan(pi/6)

/// What a cached frame was marched for.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Key {
    cam: [u64; 4],
    view: (u32, u32),
}

struct Frame {
    key: Key,
    bind: wgpu::BindGroup,
    // The bind group keeps the buffer alive; held for clarity of ownership.
    _buf: wgpu::Buffer,
    k: u32,
    trace: (u32, u32),
}

/// The trace + composite pair and whatever they hold resident.
pub struct StockEye {
    trace: Option<TracePass>,
    composite: Composite,
    source: Option<Arc<FieldView>>,
    /// The resident field is on the GPU (false: the CPU marches it).
    gpu_scene: bool,
    frame: Option<Frame>,
    /// The state a refusal was last said for (not re-said, not retried).
    refused: Option<Key>,
    /// The verdict palette the composite paints with (M13.4).
    verdict: [[f32; 3]; 4],
}

impl StockEye {
    /// Build the eye for the viewport's color `format`. The trace pass
    /// may refuse (the CPU then marches); the composite may not - `Err`
    /// means this device keeps the sheet.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Result<Self, String> {
        let composite = Composite::new(device, format)?;
        let trace = match TracePass::new(device) {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!("stock eye: {e} - the CPU march stands in");
                None
            }
        };
        let verdict = Self::verdict_of(&crate::egui_app::colorvision::Semantic::OFF);
        Ok(StockEye { trace, composite, source: None, gpu_scene: false, frame: None, refused: None, verdict })
    }

    fn verdict_of(p: &crate::egui_app::colorvision::Semantic) -> [[f32; 3]; 4] {
        [p.clean, p.rest, p.gouge, p.unjudged]
    }

    /// The verdict palette the next `prepare` stages (M13.4's colour-
    /// vision remap): clean, rest, gouge, unjudged.
    pub fn set_palette(&mut self, p: &crate::egui_app::colorvision::Semantic) {
        self.verdict = Self::verdict_of(p);
    }

    /// The same eye with its GPU march dropped: the CPU fallback a
    /// refusing device lands on, reachable on purpose for the tests.
    #[must_use]
    pub fn cpu_only(mut self) -> Self {
        self.trace = None;
        self.clear();
        self
    }

    /// Forget the resident field and frame (deviate off, document switch).
    pub fn clear(&mut self) {
        self.source = None;
        self.gpu_scene = false;
        self.frame = None;
        self.refused = None;
    }

    /// Ready a traced frame of `fv` for this camera and extent,
    /// recording any GPU march into `encoder`. `Ok(true)`: `draw` will
    /// paint it this frame. `Ok(false)`: keep the sheet (no field, or a
    /// refusal already said). `Err`: a NEW refusal, said once.
    #[allow(clippy::too_many_arguments)] // device, queue, encoder + the frame's own four
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        fv: Option<&Arc<FieldView>>,
        cam: &Camera,
        mvp: &[[f32; 4]; 4],
        view: (u32, u32),
    ) -> Result<bool, String> {
        let Some(fv) = fv else {
            if self.source.is_some() {
                self.clear();
            }
            return Ok(false);
        };
        if !self.source.as_ref().is_some_and(|s| Arc::ptr_eq(s, fv)) {
            self.clear();
            self.source = Some(Arc::clone(fv));
            self.gpu_scene = self.upload(device, fv);
        }
        let pcam = preview_camera(cam).ok_or("the camera basis is degenerate")?;
        let key = Key { cam: cam_bits(&pcam), view };
        if self.refused == Some(key) {
            return Ok(false);
        }
        if self.frame.as_ref().is_none_or(|f| f.key != key) {
            self.frame = None;
            match self.march(device, queue, encoder, fv, &pcam, key) {
                Ok(f) => self.frame = Some(f),
                Err(e) => {
                    self.refused = Some(key);
                    eprintln!("stock eye: {e}");
                    return Err(e);
                }
            }
        }
        let Some(f) = &self.frame else { return Ok(false) };
        let v = |p: Point3| [p.x as f32, p.y as f32, p.z as f32];
        let geom = FrameGeom {
            eye: v(pcam.eye),
            fwd: v(pcam.fwd),
            right: v(pcam.right),
            up: v(pcam.up),
            tan_half_fov: TAN_HALF_FOV as f32,
            view,
            k: f.k,
            trace: f.trace,
            verdict: self.verdict,
        };
        self.composite.write(queue, mvp, &geom);
        Ok(true)
    }

    /// Paint the prepared frame into the open 3D pass (a no-op when
    /// `prepare` did not answer `Ok(true)` this frame).
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if let Some(f) = &self.frame {
            self.composite.draw(pass, &f.bind);
        }
    }

    /// Put `fv`'s lattices on the GPU; false (and a log line) when the
    /// pass is absent or refuses them - the CPU marches instead.
    fn upload(&mut self, device: &wgpu::Device, fv: &FieldView) -> bool {
        let Some(trace) = self.trace.as_mut() else { return false };
        let Some(stock) = fv.stock.field().and_then(pack_field) else {
            eprintln!("stock eye: the stock field would not pack - the CPU march stands in");
            return false;
        };
        let target = fv.target.as_ref().and_then(|t| t.field()).and_then(pack_target);
        match trace.upload(device, &stock, target.as_ref()) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("stock eye: {e} - the CPU march stands in");
                false
            }
        }
    }

    fn march(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        fv: &FieldView,
        pcam: &PreviewCamera,
        key: Key,
    ) -> Result<Frame, String> {
        let gpu = self.trace.as_ref().filter(|_| self.gpu_scene);
        let budget = if gpu.is_some() { GPU_PIXELS } else { CPU_PIXELS };
        let (k, tw, th) = fit(key.view, budget);
        let buf = match gpu {
            Some(t) => t.encode(device, queue, encoder, pcam, tw, th, DEV_TOL)?,
            None => {
                let stock = fv.stock.field().ok_or("the field view holds no field to march")?;
                let target = fv.target.as_ref().and_then(|t| t.field());
                let pv = preview(stock, target, pcam, tw, th, DEV_TOL)
                    .map_err(|e| format!("the CPU march refused the frame: {e}"))?;
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("stock-eye-cpu-frame"),
                    contents: &encode(&pv),
                    usage: wgpu::BufferUsages::STORAGE,
                })
            }
        };
        let bind = self.composite.bind(device, &buf);
        Ok(Frame { key, bind, _buf: buf, k, trace: (tw, th) })
    }
}

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

fn cam_bits(c: &PreviewCamera) -> [u64; 4] {
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
