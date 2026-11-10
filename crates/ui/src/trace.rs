//! The GPU eye (M7.2 B): the sim's preview march dispatched as a
//! compute pass over the packed lattice (`preview::pack`). Provenance:
//! Hart's sphere tracing (SIGGRAPH 1996), the laws ported term for
//! term from `camcraft_sim::stock_sim::preview` into `trace.wgsl`;
//! nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **One march, two eyes.** The kernel walks `PreviewCamera::ray`,
//!   the lattice blend, the hit law and the shade exactly as the CPU
//!   march does; the frame comes back as the sim's own `Preview`, so a
//!   consumer cannot tell (or care) which eye answered. The parity
//!   suite (`tests/trace_parity.rs`) holds the two to it.
//! - **One stated difference: the class oracle.** The CPU path classes
//!   by the target's EXACT facet distance; the GPU cannot afford a
//!   facet scan per pixel and reads the target's packed lattice, which
//!   is exact near the surface (the band) and answers `cap` with the
//!   right sign beyond it - the gate (`gate_tol`) is the same.
//! - **Refuse, never black-screen.** A device that cannot build the
//!   kernel, a lattice past the device's storage limit, a frame past
//!   the CPU path's own caps: each is an `Err` naming its reason, and
//!   the caller falls back to the CPU march. No `unwrap` on any of it.

pub mod composite;
pub mod cpu_eye;
pub mod eye;
mod eye_geom;

use egui_wgpu::wgpu;
use wgpu::util::DeviceExt;

use camcraft_app::stock_sim::preview::{
    gate_tol, step_eps, CutPatch, DistanceLattice, FieldLattice, LatticeMeta, Preview, PreviewCamera,
};

const SHADER: &str = include_str!("trace.wgsl");

/// Thirteen vec4s - `struct U` in trace.wgsl.
const UNIFORM_SIZE: u64 = 13 * 16;

/// The CPU march's frame cap (`preview()`): both eyes refuse alike.
const MAX_FRAME_PIXELS: u64 = 1 << 22;

/// One vec4<f32> per pixel out.
const OUT_STRIDE: u64 = 16;

/// The compute pipeline and whatever lattice is resident on the GPU.
pub struct TracePass {
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    scene: Option<Scene>,
}

/// The uploaded lattice: three storage layers and the metadata the
/// uniform needs to index them.
struct Scene {
    mesh: wgpu::Buffer,
    cut: wgpu::Buffer,
    target: wgpu::Buffer,
    meta: LatticeMeta,
    t_meta: Option<LatticeMeta>,
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn f32_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

impl TracePass {
    /// Build the kernel. A device that refuses it (no compute, a
    /// validation failure) answers `Err` by name: the CPU march is the
    /// caller's next move, never a panic.
    pub fn new(device: &wgpu::Device) -> Result<Self, String> {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("trace"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let uniform_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("trace-bgl"),
            entries: &[
                uniform_entry,
                storage_entry(1, true),
                storage_entry(2, true),
                storage_entry(3, true),
                storage_entry(4, false),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("trace-pl"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("trace-pipe"),
            layout: Some(&layout),
            module: &module,
            entry_point: Some("cs_trace"),
            compilation_options: Default::default(),
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("trace-uniform"),
            size: UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if let Some(err) = pollster::block_on(scope.pop()) {
            return Err(format!("trace pass: the device refused the compute kernel ({err})"));
        }
        Ok(TracePass { pipeline, bgl, uniform, scene: None })
    }

    /// Put a stock lattice (and optionally the deviation target's) on
    /// the GPU. Refuses a malformed pack or one past the device's
    /// storage binding limit by name; the resident scene is untouched
    /// on refusal.
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        stock: &FieldLattice,
        target: Option<&DistanceLattice>,
    ) -> Result<(), String> {
        self.upload_layers(device, &stock.meta, &stock.mesh, &stock.cut, target)
    }

    /// `upload` from loose layers (playback's pre-packed views share
    /// the mesh layer across frames and carry only their own cut).
    pub fn upload_layers(
        &mut self,
        device: &wgpu::Device,
        meta: &LatticeMeta,
        mesh: &[f32],
        cut: &[f32],
        target: Option<&DistanceLattice>,
    ) -> Result<(), String> {
        let limit = device.limits().max_storage_buffer_binding_size;
        let texels = |m: &LatticeMeta| {
            m.dims.iter().try_fold(1usize, |n, &d| n.checked_mul(d as usize))
        };
        let n = texels(meta).ok_or("trace pass: the stock lattice extent overflows")?;
        if mesh.len() != n || cut.len() != n {
            return Err("trace pass: the stock lattice layers disagree with its extent".into());
        }
        if (n as u64).saturating_mul(4) > limit {
            return Err(format!(
                "trace pass: the stock lattice needs {} bytes, the device binds at most {limit}",
                n as u64 * 4
            ));
        }
        if let Some(t) = target {
            let tn = texels(&t.meta).ok_or("trace pass: the target lattice extent overflows")?;
            if t.d.len() != tn {
                return Err("trace pass: the target lattice disagrees with its extent".into());
            }
            if (tn as u64).saturating_mul(4) > limit {
                return Err("trace pass: the target lattice exceeds the device's storage limit".into());
            }
        }
        let init = |label: &str, bytes: &[u8], extra: wgpu::BufferUsages| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytes,
                usage: wgpu::BufferUsages::STORAGE | extra,
            })
        };
        let none = wgpu::BufferUsages::empty();
        let mesh = init("trace-mesh", &f32_bytes(mesh), none);
        // The cut layer is the one playback rewrites in place.
        let cut = init("trace-cut", &f32_bytes(cut), wgpu::BufferUsages::COPY_DST);
        // A storage binding cannot be empty: no target binds one zero
        // texel the kernel never reads (has_target = 0).
        let target_buf = match target {
            Some(t) => init("trace-target", &f32_bytes(&t.d), none),
            None => init("trace-target", &f32_bytes(&[0.0]), none),
        };
        self.scene = Some(Scene {
            mesh,
            cut,
            target: target_buf,
            meta: meta.clone(),
            t_meta: target.map(|t| t.meta.clone()),
        });
        Ok(())
    }

    /// Rewrite the resident cut layer in place (playback: the mesh and
    /// target layers stand). Refuses a layer of the wrong extent, or no
    /// scene, by name - the caller then uploads whole.
    pub fn write_cut(&self, queue: &wgpu::Queue, cut: &[f32]) -> Result<(), String> {
        let scene = self.scene.as_ref().ok_or("trace pass: no lattice to rewrite")?;
        let n = scene.meta.dims.iter().try_fold(1usize, |n, &d| n.checked_mul(d as usize));
        if n != Some(cut.len()) {
            return Err("trace pass: the cut layer disagrees with the resident extent".into());
        }
        queue.write_buffer(&scene.cut, 0, &f32_bytes(cut));
        Ok(())
    }

    /// Rewrite only a patch's runs of the resident cut layer (M13.1),
    /// each at its texel offset. A patch that does not fit the
    /// resident extent is refused before anything is written.
    pub fn write_cut_patch(&self, queue: &wgpu::Queue, patch: &CutPatch) -> Result<(), String> {
        let scene = self.scene.as_ref().ok_or("trace pass: no lattice to patch")?;
        let n = scene.meta.dims.iter().try_fold(1usize, |n, &d| n.checked_mul(d as usize));
        if !n.is_some_and(|n| patch.fits(n)) {
            return Err("trace pass: the cut patch disagrees with the resident extent".into());
        }
        for (o, run) in patch.chunks() {
            queue.write_buffer(&scene.cut, (o as u64).saturating_mul(4), &f32_bytes(run));
        }
        Ok(())
    }

    /// Record the march for a `w` x `h` frame into `encoder`; the
    /// returned buffer holds one vec4 per pixel (depth, class, shade,
    /// status) once the encoder is submitted.
    #[allow(clippy::too_many_arguments)] // device, queue, encoder + the frame's own four
    pub fn encode(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        cam: &PreviewCamera,
        w: u32,
        h: u32,
        tol: f64,
    ) -> Result<wgpu::Buffer, String> {
        let scene = self.scene.as_ref().ok_or("trace pass: no lattice uploaded")?;
        if w == 0 || h == 0 {
            return Err("trace pass: frame size must be at least 1x1".into());
        }
        let px = u64::from(w) * u64::from(h);
        if px > MAX_FRAME_PIXELS {
            return Err("trace pass: frame exceeds 4M pixels".into());
        }
        let out_size = px * OUT_STRIDE;
        if out_size > device.limits().max_storage_buffer_binding_size {
            return Err("trace pass: the frame exceeds the device's storage limit".into());
        }
        queue.write_buffer(&self.uniform, 0, &uniform_bytes(scene, cam, w, h, tol)?);
        let out = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("trace-out"),
            size: out_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("trace-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: scene.mesh.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: scene.cut.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: scene.target.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: out.as_entire_binding() },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("trace-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
        drop(pass);
        Ok(out)
    }

    /// March a frame and read it back as the sim's `Preview` - the
    /// parity tests' entry and the CPU-comparable answer. NaN depth
    /// and class 3 wherever the kernel reported no hit; `unresolved`
    /// counts its budget bails, the CPU law exactly.
    pub fn trace(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cam: &PreviewCamera,
        w: u32,
        h: u32,
        tol: f64,
    ) -> Result<Preview, String> {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("trace-enc"),
        });
        let out = self.encode(device, queue, &mut enc, cam, w, h, tol)?;
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("trace-read"),
            size: out.size(),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        enc.copy_buffer_to_buffer(&out, 0, &read, 0, out.size());
        queue.submit([enc.finish()]);
        if let Some(err) = pollster::block_on(scope.pop()) {
            return Err(format!("trace pass: the frame failed validation ({err})"));
        }
        let (tx, rx) = std::sync::mpsc::channel();
        read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        if device.poll(wgpu::PollType::wait_indefinitely()).is_err() {
            return Err("trace pass: the device was lost while marching".into());
        }
        if !matches!(rx.recv(), Ok(Ok(()))) {
            return Err("trace pass: the frame could not be read back".into());
        }
        let raw = read
            .slice(..)
            .get_mapped_range()
            .map_err(|e| format!("trace pass: readback map failed ({e})"))?;
        let frame = decode(&raw, w, h);
        drop(raw);
        read.unmap();
        Ok(frame)
    }
}

/// The kernel's per-pixel vec4s as the sim's frame type.
fn decode(raw: &[u8], w: u32, h: u32) -> Preview {
    let n = (w as usize) * (h as usize);
    let mut pv = Preview {
        w,
        h,
        depth: vec![f64::NAN; n],
        class: vec![3.0; n],
        shade: vec![0.0; n],
        unresolved: 0,
    };
    let word = |o: usize| {
        raw.get(o..o + 4)
            .and_then(|b| b.try_into().ok())
            .map_or(f32::NAN, f32::from_le_bytes)
    };
    for i in 0..n {
        let o = i * OUT_STRIDE as usize;
        let status = word(o + 12);
        if status == 1.0 {
            pv.depth[i] = f64::from(word(o));
            pv.class[i] = word(o + 4);
            pv.shade[i] = word(o + 8);
        } else if status == 2.0 {
            pv.unresolved += 1;
        }
    }
    pv
}

/// `struct U` in trace.wgsl, field by field.
fn uniform_bytes(s: &Scene, cam: &PreviewCamera, w: u32, h: u32, tol: f64) -> Result<Vec<u8>, String> {
    let m = &s.meta;
    let (world_lo, world_hi) = m.world();
    let mut b = Vec::with_capacity(UNIFORM_SIZE as usize);
    let mut v4 = |x: f64, y: f64, z: f64, w: f64| {
        for c in [x, y, z, w] {
            b.extend_from_slice(&(c as f32).to_le_bytes());
        }
    };
    let (e, f, r, u) = (cam.eye, cam.fwd, cam.right, cam.up);
    v4(e.x, e.y, e.z, cam.tan_half_fov);
    v4(f.x, f.y, f.z, step_eps(m.cell));
    v4(r.x, r.y, r.z, m.cell);
    v4(u.x, u.y, u.z, m.cap);
    v4(m.solid.0[0], m.solid.0[1], m.solid.0[2], gate_tol(tol, m.cell));
    v4(m.solid.1[0], m.solid.1[1], m.solid.1[2], if s.t_meta.is_some() { 1.0 } else { 0.0 });
    v4(world_lo[0], world_lo[1], world_lo[2], cam.ortho_half_height.unwrap_or(0.0));
    v4(world_hi[0], world_hi[1], world_hi[2], 0.0);
    let i = |x: u32| i32::try_from(x).map_err(|_| String::from("trace pass: a lattice extent overflows i32"));
    let fw = i(w)?;
    let fh = i(h)?;
    let t = s.t_meta.as_ref().unwrap_or(m);
    let words: [[i32; 4]; 4] = [
        [m.origin[0], m.origin[1], m.origin[2], fw],
        [i(m.dims[0])?, i(m.dims[1])?, i(m.dims[2])?, fh],
        [t.origin[0], t.origin[1], t.origin[2], 0],
        [i(t.dims[0])?, i(t.dims[1])?, i(t.dims[2])?, 0],
    ];
    for row in words {
        for x in row {
            b.extend_from_slice(&x.to_le_bytes());
        }
    }
    for c in [t.cell, 0.0, 0.0, 0.0] {
        b.extend_from_slice(&(c as f32).to_le_bytes());
    }
    Ok(b)
}
