//! M1.4.1 spike binary (branch ui-egui-spike; see ROADMAP.md).
//!
//! S1: an eframe-less egui shell - winit 0.30 loop, our own wgpu instance/
//! surface (mirroring app.rs, no unwrap), egui-wgpu's low-level Renderer,
//! and an egui_dock dockspace whose one tab hosts the existing View3D: its
//! offscreen color texture is registered as a native egui texture once and
//! rebound on resize. Camera orbit/pan/zoom ride egui's own pointer
//! responses (the imgui shell needed raw-winit drag tracking precisely
//! because imgui's docked Image proved unreliable; egui's response model
//! is the fix we are spiking).
//! Provenance: our own spike harness, specced from the egui/egui-wgpu/
//! egui_dock docs and vendored sources; no HSMWorks material involved.

use std::sync::Arc;

use egui_dock::{DockArea, DockState, Style as DockStyle, TabViewer};
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

// The unified-wgpu guarantee from the S0 resolution test: every wgpu type in
// this file names egui_wgpu's re-export, so Renderer and View3D cannot drift
// apart onto different wgpu majors.
use egui_wgpu::wgpu;
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};

use hsmworks_ui::viewport::{Camera, GpuPart, View3D};

const VIEW3D_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--headless") {
        std::process::exit(match headless::run() {
            Ok(()) => {
                println!("egui spike S1 headless: offscreen View3D + native-texture path green");
                0
            }
            Err(err) => {
                eprintln!("egui spike S1 headless FAILED: {err}");
                1
            }
        });
    }
    std::process::exit(run_windowed());
}

fn run_windowed() -> i32 {
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(err) => {
            eprintln!("cannot create event loop: {err}");
            return 1;
        }
    };
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut shell = Shell::new();
    let code = match event_loop.run_app(&mut shell) {
        Ok(()) => shell.exit_code,
        Err(err) => {
            eprintln!("event loop failed: {err}");
            1
        }
    };
    shell.shutdown();
    code
}

/// Everything the loop owns. Field order matters at shutdown: the surface
/// borrows the window's raw handle, so it must drop before `window`
/// (same SAFETY argument as app.rs).
struct Shell {
    // Set on first resumed(); None until GPU init succeeds.
    window: Option<Arc<Window>>,
    device: Option<wgpu::Device>,
    queue: Option<wgpu::Queue>,
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    renderer: Option<Renderer>,
    winit_state: Option<egui_winit::State>,
    egui_ctx: egui::Context,
    dock_state: DockState<Tab>,
    view: Option<View3D>,
    view_tex: Option<egui::TextureId>,
    cam: Camera,
    /// Last laid-out viewport size in logical points; drives the View3D
    /// extent for the NEXT frame (imgui's shell uses the same deferral).
    viewport_points: [f32; 2],
    parts: Vec<GpuPart>,
    exit_code: i32,
    failed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Tab {
    Viewport,
}

impl Tab {
    fn title(&self) -> &'static str {
        match self {
            Tab::Viewport => "viewport",
        }
    }
}

impl Shell {
    fn new() -> Self {
        // One fresh dockspace per process; layout persistence (save/load) is
        // deliberately off for the spike - nothing to port or version yet.
        let dock_state = DockState::new(vec![Tab::Viewport]);
        Self {
            window: None,
            device: None,
            queue: None,
            surface: None,
            config: None,
            renderer: None,
            winit_state: None,
            egui_ctx: egui::Context::default(),
            dock_state,
            view: None,
            view_tex: None,
            cam: Camera::frame([0.0, 0.0, 20.0], 70.0),
            viewport_points: [600.0, 400.0],
            parts: sample_parts(),
            exit_code: 0,
            failed: false,
        }
    }

    fn fail(&mut self, el: &ActiveEventLoop, what: &str, err: impl std::fmt::Display) {
        eprintln!("egui spike: {what}: {err}");
        self.failed = true;
        self.exit_code = 1;
        el.exit();
    }

    fn init_gpu(&mut self, el: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title("HSMWorks (egui spike S1)")
            .with_inner_size(LogicalSize::new(1280.0, 800.0));
        let window = match el.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(err) => {
                self.fail(el, "window creation", err);
                return;
            }
        };
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        // SAFETY: `window` lives in self.window and the surface drops before
        // it (field order); the raw handle stays valid meanwhile.
        let target = match unsafe {
            wgpu::SurfaceTargetUnsafe::from_display_and_window(&window, &window)
        } {
            Ok(t) => t,
            Err(err) => {
                self.fail(el, "surface handle", err);
                return;
            }
        };
        // SAFETY: the window outlives the surface (same argument as above).
        let surface = match unsafe { instance.create_surface_unsafe(target) } {
            Ok(s) => s,
            Err(err) => {
                self.fail(el, "surface creation", err);
                return;
            }
        };
        let options = wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        };
        let adapter = match pollster::block_on(instance.request_adapter(&options)) {
            Ok(a) => a,
            Err(err) => {
                self.fail(el, "no suitable GPU adapter", err);
                return;
            }
        };
        let (device, queue) = match pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor::default(),
        )) {
            Ok(dq) => dq,
            Err(err) => {
                self.fail(el, "device request", err);
                return;
            }
        };
        let Some(mut config) =
            surface.get_default_config(&adapter, size.width.max(1), size.height.max(1))
        else {
            self.fail(el, "surface config", "no compatible configuration");
            return;
        };
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);

        let mut renderer = Renderer::new(&device, config.format, RendererOptions::default());
        let scale = window.scale_factor().clamp(0.1, 8.0) as f32;
        let mut view = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // View3D renders into its OWN texture (Rgba8Unorm, the format
            // egui's native-texture registration demands), decoupled from
            // the surface format - that is the registered-texture plan.
            View3D::new(
                &device,
                VIEW3D_FORMAT,
                (self.viewport_points[0] * scale).max(1.0) as u32,
                (self.viewport_points[1] * scale).max(1.0) as u32,
            )
        })) {
            Ok(v) => v,
            Err(_) => {
                self.fail(el, "viewport3d pipeline", "GPU validation failed (see log)");
                return;
            }
        };
        view.set_document(&device, &queue, &self.parts);
        let tex = renderer.register_native_texture(
            &device,
            view.color_view(),
            wgpu::FilterMode::Linear,
        );
        let winit_state = egui_winit::State::new(
            self.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &*window,
            Some(scale),
            window.theme(),
            Some(device.limits().max_texture_dimension_2d as usize),
        );

        self.window = Some(window);
        self.device = Some(device);
        self.queue = Some(queue);
        self.surface = Some(surface);
        self.config = Some(config);
        self.renderer = Some(renderer);
        self.winit_state = Some(winit_state);
        self.view = Some(view);
        self.view_tex = Some(tex);
    }

    fn on_resize(&mut self, size: PhysicalSize<u32>) {
        let (w, h) = (size.width.max(1), size.height.max(1));
        let (Some(surface), Some(device), Some(config)) =
            (self.surface.as_ref(), self.device.as_ref(), self.config.as_mut())
        else {
            return;
        };
        if config.width == w && config.height == h {
            return;
        }
        config.width = w;
        config.height = h;
        surface.configure(device, config);
    }

    fn draw(&mut self) {
        if self.failed {
            return;
        }
        let Self {
            window,
            device,
            queue,
            surface,
            config,
            renderer,
            winit_state,
            egui_ctx,
            dock_state,
            view,
            view_tex,
            cam,
            viewport_points,
            ..
        } = self;
        let (
            Some(window),
            Some(device),
            Some(queue),
            Some(surface),
            Some(conf),
            Some(renderer),
            Some(winit_state),
            Some(view),
            Some(tex_id),
        ) = (
            window.as_ref(),
            device.as_ref(),
            queue.as_ref(),
            surface.as_ref(),
            config.as_ref(),
            renderer.as_mut(),
            winit_state.as_mut(),
            view.as_mut(),
            view_tex.as_ref().copied(),
        ) else {
            return;
        };

        let scale = window.scale_factor().clamp(0.1, 8.0);
        let raw_input = winit_state.take_egui_input(window);

        // Size the 3D target from last frame's layout, like the imgui shell.
        let want = (
            (f64::from(viewport_points[0]) * scale).max(1.0) as u32,
            (f64::from(viewport_points[1]) * scale).max(1.0) as u32,
        );
        if view.extent() != want {
            view.resize(device, VIEW3D_FORMAT, want.0, want.1);
            // Rebind the SAME egui texture id to the recreated view.
            renderer.update_egui_texture_from_wgpu_texture(
                device,
                view.color_view(),
                wgpu::FilterMode::Linear,
                tex_id,
            );
        }

        // UI logic first: the dock reports the viewport tab's size, the GPU
        // passes follow (same ordering law as app.rs - panels drive extent).
        let mut viewer = ShellTabs {
            tex: tex_id,
            cam,
            viewport_points,
        };
        let mut full_output = egui_ctx.run_ui(raw_input, |ui| {
            DockArea::new(&mut *dock_state)
                .style(DockStyle::from_egui(ui.style().as_ref()))
                .show_inside(ui, &mut viewer);
        });
        // CPU textures egui produced this pass (font atlas first of all):
        // without this feed the shell renders with no text. 0.36's
        // TexturesDelta asserts on drop unless drained - consume, don't peek.
        for (id, deltas) in full_output.textures_delta.set.drain() {
            for delta in deltas {
                renderer.update_texture(device, queue, id, &delta);
            }
        }
        for id in full_output.textures_delta.free.drain() {
            renderer.free_texture(&id);
        }
        winit_state.handle_platform_output(window, full_output.platform_output);

        let clipped = egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);

        let st = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(st)
            | wgpu::CurrentSurfaceTexture::Suboptimal(st) => st,
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                surface.configure(device, conf);
                return;
            }
        };
        let target = st.texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Pass 1: the 3D offscreen target (its own bind group, mvp from the
        // camera the viewer just moved).
        let (ew, eh) = view.extent();
        let mvp = cam.mvp(f64::from(ew.max(1)) / f64::from(eh.max(1)));
        let eye = cam.eye_f32();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("egui-spike-frame"),
        });
        view.render(queue, &mut encoder, &mvp, eye);

        // Pass 2: egui over the surface.
        let sd = ScreenDescriptor {
            size_in_pixels: [conf.width, conf.height],
            pixels_per_point: scale as f32,
        };
        let mut buffer_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("egui-spike-buffers"),
        });
        let extra = renderer.update_buffers(
            device,
            queue,
            &mut buffer_encoder,
            &clipped,
            &sd,
        );
        // SAFETY of forget_lifetime: everything the pass encodes against
        // (target view, egui textures, buffer binds) outlives this block -
        // the pass is finished when the encoder closes the scope.
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            })
            .forget_lifetime();
        renderer.render(&mut pass, &clipped, &sd);
        drop(pass);
        let mut submits = extra;
        submits.push(buffer_encoder.finish());
        submits.push(encoder.finish());
        queue.submit(submits);
        queue.present(st);
    }
}

impl winit::application::ApplicationHandler for Shell {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_none() && !self.failed {
            self.init_gpu(el);
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // Feed egui first (pointer/keyboard/text/scale), like the imgui
        // shell's platform.prepare_frame; the EventResponse (child-viewport
        // control flow) is inert for our single root viewport.
        if let (Some(winit_state), Some(window)) = (self.winit_state.as_mut(), self.window.as_ref())
        {
            let _response = winit_state.on_window_event(window, &event);
        }
        match &event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(size) => self.on_resize(*size),
            WindowEvent::RedrawRequested => self.draw(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

impl Shell {
    fn shutdown(&mut self) {
        // Surface/renderer first, window last (see the field-order note).
        self.renderer = None;
        self.view = None;
        self.surface = None;
        self.winit_state = None;
        self.queue = None;
        self.device = None;
        self.config = None;
        self.window = None;
    }
}

struct ShellTabs<'a> {
    tex: egui::TextureId,
    cam: &'a mut Camera,
    viewport_points: &'a mut [f32; 2],
}

impl TabViewer for ShellTabs<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        // Title-derived: stable across frames, collision-free while the
        // dockspace holds one tab per title (true for the whole spike).
        egui::Id::new(tab.title())
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            Tab::Viewport => {
                let avail = ui.available_size();
                self.viewport_points[0] = avail.x.max(1.0);
                self.viewport_points[1] = avail.y.max(1.0);
                let resp = ui.add(
                    egui::Image::from_texture(egui::load::SizedTexture::new(self.tex, avail))
                        .max_size(avail)
                        .sense(egui::Sense::click_and_drag()),
                );
                // The imgui shell's rule, now expressed in egui's model:
                // primary drag orbits, middle drag pans, wheel dollies.
                if resp.dragged_by(egui::PointerButton::Primary) {
                    let d = resp.drag_delta();
                    self.cam.orbit(d.x, d.y);
                }
                if resp.dragged_by(egui::PointerButton::Middle) {
                    let d = resp.drag_delta();
                    self.cam.pan(d.x, d.y);
                }
                if resp.hovered() {
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if scroll != 0.0 {
                        self.cam.zoom(scroll);
                    }
                }
            }
        }
    }
}

/// A stock box around a smaller solid part, both centered on z=20 (the
/// vantage the gpu_render smoke tests frame): the two pipelines the docked
/// View3D must show, without dragging the ECS in before S2's command wiring.
fn sample_parts() -> Vec<GpuPart> {
    // Proportions mirror the gpu_render smoke tests (60x60x40 stock around
    // a 40mm model) so the changed-pixel band the headless proof asserts
    // against is a known-good frame footprint on both adapters.
    let model = box_soup([0.0, 0.0, 20.0], [20.0, 20.0, 20.0]);
    let stock = box_soup([0.0, 0.0, 20.0], [30.0, 30.0, 20.0]);
    vec![
        GpuPart { verts: stock, selected: false, stock: true, fill: false },
        GpuPart { verts: model, selected: false, stock: false, fill: false },
    ]
}

/// Axis-aligned box as a triangle soup centered on `c` with half-extents `h`.
fn box_soup(c: [f32; 3], h: [f32; 3]) -> Vec<[f32; 3]> {
    let [cx, cy, cz] = c;
    let [x, y, z] = h;
    let shift = |p: [f32; 3]| [p[0] + cx, p[1] + cy, p[2] + cz];
    let faces: [[ [f32; 3]; 4]; 6] = [
        [[-x, -y, z], [x, -y, z], [x, y, z], [-x, y, z]],
        [[-x, y, -z], [x, y, -z], [x, -y, -z], [-x, -y, -z]],
        [[x, -y, -z], [x, y, -z], [x, y, z], [x, -y, z]],
        [[-x, y, -z], [-x, -y, -z], [-x, -y, z], [-x, y, z]],
        [[-x, y, z], [x, y, z], [x, y, -z], [-x, y, -z]],
        [[-x, -y, -z], [x, -y, -z], [x, -y, z], [-x, -y, z]],
    ];
    let mut v = Vec::with_capacity(36);
    for f in faces {
        for tri in [[f[0], f[1], f[2]], [f[0], f[2], f[3]]] {
            for p in tri {
                v.push(shift(p));
            }
        }
    }
    v
}

/// Headless proof (the S1 gate): no window, no surface - the fallback
/// adapter (the same deterministic choice as tests/gpu_render.rs), View3D
/// on the spike's Rgba8Unorm offscreen format, a cube rendered, and the
/// readback showing a solid footprint of pixels changed from the corner
/// (background) color. The UI-logic half of egui needs no GPU and is
/// pinned by kittest in S3.
mod headless {
    use crate::{Camera, View3D, wgpu};

    pub fn run() -> Result<(), String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            force_fallback_adapter: true,
            compatible_surface: None,
            ..Default::default()
        }))
        .map_err(|err| err.to_string())?;
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor::default(),
        ))
        .map_err(|err| format!("device: {err}"))?;

        let mut view = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            View3D::new(&device, crate::VIEW3D_FORMAT, 320, 240)
        }))
        .map_err(|_| "View3D pipeline panicked on validation".to_string())?;
        let parts = crate::sample_parts();
        view.set_document(&device, &queue, &parts);

        // Same vantage the gpu_render smoke tests use on equivalent parts.
        let cam = Camera {
            target: [0.0, 0.0, 20.0],
            yaw: 0.9,
            pitch: 0.5,
            dist: 160.0,
        };
        let (w, h) = view.extent();
        let mvp = cam.mvp(f64::from(w.max(1)) / f64::from(h.max(1)));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("egui-spike-headless"),
        });
        view.render(&queue, &mut encoder, &mvp, cam.eye_f32());
        queue.submit([encoder.finish()]);

        let (rgba, w, h) = view
            .snapshot_rgba(&device, &queue)
            .ok_or_else(|| "no readback path".to_string())?;
        // The corner pixel is untouched clear color; anything differing from
        // it by the gpu_render epsilon is rasterized geometry (the format-
        // agnostic assertion - no assumptions about shading brightness).
        let bg: [u8; 4] = rgba
            .first_chunk::<4>()
            .copied()
            .ok_or_else(|| "empty readback".to_string())?;
        let changed = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| {
                p.iter().zip(bg).any(|(a, b)| {
                    i32::from(*a) - i32::from(b) > 6 || i32::from(b) - i32::from(*a) > 6
                })
            })
            .count();
        let total = (w as usize * h as usize).max(1);
        if changed * 20 < total {
            // less than 5% of the frame: the cube did not rasterize
            return Err(format!("readback looks empty ({changed}/{total} changed px)"));
        }
        println!(
            "headless snapshot: {w}x{h}, {}% pixels changed from background",
            changed * 100 / total
        );
        Ok(())
    }
}

#[cfg(test)]
mod resolution {
    /// Compile-time proof of the S0 gate, kept through S1: exactly ONE wgpu
    /// in the tree. If egui-wgpu resolved a different wgpu major than the
    /// imgui renderer's re-export, the reference below would not typecheck
    /// and the spike's registered-texture plan would be dead on arrival.
    #[test]
    fn wgpu_types_are_unified() {
        // Elided lifetimes bind output to the single input lifetime, so the
        // coercion is only accepted when both paths name the SAME type.
        fn coerce(d: &dear_imgui_wgpu::wgpu::Device) -> &egui_wgpu::wgpu::Device {
            d
        }
        // The coercion itself is the assertion; the pointer bind is the use.
        let _f: fn(&dear_imgui_wgpu::wgpu::Device) -> &egui_wgpu::wgpu::Device = coerce;
    }
}
