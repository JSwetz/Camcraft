//! M1.4.1 spike binary (branch ui-egui-spike; see ROADMAP.md).
//!
//! S2: the S1 shell wearing the real UI - the demo document is a seeded
//! `State` (the same one the imgui app runs), the dock holds three leaves
//! (browser 17% left, properties 22% right, mirroring shell.rs's ratios),
//! the ribbon and status strips are painted one-panel widgets (panels.rs),
//! and the property pages bind to the SAME commands as imgui - every edit
//! queues a `CommandKind` and lands in the undo history.
//!
//! The GPU loop is S1's, unchanged: winit 0.30, our own wgpu instance/
//! surface (mirroring app.rs, no unwrap), egui-wgpu's low-level Renderer,
//! View3D's offscreen texture registered once as a native egui texture and
//! rebound on resize. What is new per frame: `drain_actions` first (the
//! commands the panels queued last frame), then a digest-gated re-upload of
//! document + overlay - the imgui shell's exact upload law, mirrored.
//! Provenance: our own spike harness, specced from the egui/egui-wgpu/
//! egui_dock docs and vendored sources; no HSMWorks material involved.

use std::sync::Arc;

use egui_dock::{
    DockArea, DockState, Node, NodePath, Split, Style as DockStyle, SurfaceIndex, TabViewer,
};
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

// The unified-wgpu guarantee from the S0 resolution test: every wgpu type in
// this file names egui_wgpu's re-export, so Renderer and View3D cannot drift
// apart onto different wgpu majors.
use egui_wgpu::wgpu;
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};

use hsmworks_ui::state::{CommandKind, State};
use hsmworks_ui::viewport::{Camera, View3D};

// The Cargo bin declares an explicit `path` (src/bin/egui_spike.rs), so
// Rust looks for children in src/bin/ rather than src/bin/egui_spike/ -
// pin the paths.
#[path = "egui_spike/painted.rs"]
mod painted;
#[path = "egui_spike/panels.rs"]
mod panels;
#[path = "egui_spike/shots.rs"]
mod shots;
#[path = "egui_spike/theme.rs"]
mod theme;
#[path = "egui_spike/world.rs"]
mod world;
// M1.4.1 spike S3: the frame tests run THIS bin's shell_layout, so they live
// here (a bin's modules are invisible to tests/) behind the test cfg.
#[cfg(test)]
#[path = "egui_spike/tests.rs"]
mod tests;

const VIEW3D_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Ribbon strip height (cells at top+3, group captions at top+31 - the
/// imgui ribbon's geometry, which painted::cell reproduces).
const RIBBON_H: f32 = 49.0;
/// Status strip height (imgui's single-line bottom bar).
const STATUS_H: f32 = 22.0;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--headless") {
        std::process::exit(match headless::run() {
            Ok(()) => {
                println!("egui spike S2 headless: seeded document, panels' world, View3D green");
                0
            }
            Err(err) => {
                eprintln!("egui spike S2 headless FAILED: {err}");
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
/// borrows the window's raw handle, so it must drop before `window`, and
/// `state`'s world holds Rust-owned components whose delete hooks touch no
/// GPU resource - dropping it last is safe and keeps the ECS alive for
/// every other field's drop.
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
    /// Last laid-out viewport size in logical points; drives the View3D
    /// extent for the NEXT frame (imgui's shell uses the same deferral).
    viewport_points: [f32; 2],
    /// Content digest of the scene last uploaded to View3D; re-uploads are
    /// gated on it (App::sync_gpu's law, mirrored).
    scene_digest: u64,
    exit_code: i32,
    failed: bool,
    shots_done: bool,
    exit_requested: bool,
    state: State,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Tab {
    Viewport,
    Browser,
    Properties,
}

impl Tab {
    fn title(&self) -> &'static str {
        match self {
            Tab::Viewport => "viewport",
            Tab::Browser => "browser",
            Tab::Properties => "properties",
        }
    }
}

/// The spike's fixed three-pane layout, as egui_dock splits: the browser
/// takes 17% at the left and the properties pane 22% at the right of what
/// remains - shell.rs's DockLayout ratios, restated as split fractions.
/// A fraction governs the TOP/LEFT child (SplitNode docs), and a `Left`
/// split puts the NEW node left, a `Right` split the OLD one - so the
/// first fraction is the browser's share, the second the viewport's.
fn build_dock() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Viewport]);
    let [vp, _browser] =
        dock.split(NodePath::MAIN_ROOT, Split::Left, 0.17, Node::leaf(Tab::Browser));
    dock.split(
        NodePath::new(SurfaceIndex::main(), vp),
        Split::Right,
        0.735,
        Node::leaf(Tab::Properties),
    );
    dock
}

impl Shell {
    fn new() -> Self {
        // The demo document through the real command machinery (world.rs);
        // FrameAll rides the same queue a ribbon click would use.
        let mut state = State::new();
        world::seed(&mut state);
        state.pending.push_back(CommandKind::FrameAll);
        state.drain_actions();
        let dock_state = build_dock();
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
            viewport_points: [600.0, 400.0],
            scene_digest: 0,
            exit_code: 0,
            failed: false,
            shots_done: false,
            exit_requested: false,
            state,
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
            .with_title("HSMWorks (egui spike S2)")
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
        let scale = window.scale_factor().clamp(0.1, 8.0);
        let mut view = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // View3D renders into its OWN texture (Rgba8Unorm, the format
            // egui's native-texture registration demands), decoupled from
            // the surface format - that is the registered-texture plan.
            View3D::new(
                &device,
                VIEW3D_FORMAT,
                (f64::from(self.viewport_points[0]) * scale).max(1.0) as u32,
                (f64::from(self.viewport_points[1]) * scale).max(1.0) as u32,
            )
        })) {
            Ok(v) => v,
            Err(_) => {
                self.fail(el, "viewport3d pipeline", "GPU validation failed (see log)");
                return;
            }
        };
        let scene = world::collect(&self.state);
        self.scene_digest = scene.digest();
        view.set_document(&device, &queue, &scene.parts);
        view.set_overlay(&device, &queue, &scene.overlay);
        let tex = renderer.register_native_texture(
            &device,
            view.color_view(),
            wgpu::FilterMode::Linear,
        );
        let winit_state = egui_winit::State::new(
            self.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &*window,
            Some(scale as f32),
            window.theme(),
            Some(device.limits().max_texture_dimension_2d as usize),
        );
        theme::apply(&self.egui_ctx);

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
        // Commands the panels queued last frame land before the scene is
        // collected, so this frame draws what the user's click did.
        self.state.drain_actions();
        let scene = world::collect(&self.state);

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
            viewport_points,
            scene_digest,
            shots_done,
            exit_requested,
            state,
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

        // Re-upload only a scene whose content actually changed (the imgui
        // shell's digest gate; camera motion costs zero traffic).
        let d = scene.digest();
        if d != *scene_digest {
            view.set_document(device, queue, &scene.parts);
            view.set_overlay(device, queue, &scene.overlay);
            *scene_digest = d;
        }

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
        let mut full_output = egui_ctx.run_ui(raw_input, |ui| {
            shell_layout(ui, state, dock_state, tex_id, viewport_points);
        });
        drain_textures(renderer, device, queue, &mut full_output);
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
        // camera the panels' FrameAll and the orbit gestures just moved).
        let (ew, eh) = view.extent();
        let mvp = state.cam.mvp(f64::from(ew.max(1)) / f64::from(eh.max(1)));
        let eye = state.cam.eye_f32();
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

        if state.request_quit {
            *exit_requested = true;
        }
        // First frame only: the text-crispness dumps S2 owes the user's
        // eyes (shots.rs). The viewport image rides on this frame's own
        // offscreen readback - the dump shows what the shell shows.
        if !*shots_done {
            *shots_done = true;
            match view.snapshot_rgba(device, queue) {
                Some((rgba, w, h)) => {
                    if let Err(err) = shots::dump(device, queue, rgba, w, h) {
                        eprintln!("egui spike: shots: {err}");
                    }
                }
                None => eprintln!("egui spike: shots: no readback path"),
            }
        }
    }
}

/// The whole shell for one egui pass: keyboard shortcuts, the painted
/// ribbon and status strips, and the three-leaf dock. Shared by the live
/// frame (draw) and the crispness dumps (shots.rs) so the two can never
/// disagree about layout; the borrow order is deliberate - each pane
/// reborrows `state` in sequence, the dock tabs only after the strips end.
fn shell_layout(
    ui: &mut egui::Ui,
    state: &mut State,
    dock: &mut DockState<Tab>,
    tex: egui::TextureId,
    viewport_points: &mut [f32; 2],
) {
    panels::shortcuts(ui, state);
    egui::Panel::top("ribbon")
        .frame(egui::Frame::NONE)
        .exact_size(RIBBON_H)
        .show(ui, |ui| {
            panels::ribbon(ui, &mut *state, &theme::Palette::DARK);
        });
    egui::Panel::bottom("status")
        .frame(egui::Frame::NONE)
        .exact_size(STATUS_H)
        .show(ui, |ui| {
            panels::status(ui, &*state, &theme::Palette::DARK);
        });
    egui::CentralPanel::default().show(ui, |ui| {
        DockArea::new(&mut *dock)
            .style(DockStyle::from_egui(ui.style().as_ref()))
            .show_inside(ui, &mut ShellTabs {
                state: &mut *state,
                tex,
                viewport_points,
            });
    });
}

/// Feed this pass's CPU-texture deltas (the font atlas first of all) to the
/// renderer. 0.36's TexturesDelta asserts on drop unless drained - consume,
/// don't peek (the S1 panic this replaced was exactly that).
fn drain_textures(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    out: &mut egui::FullOutput,
) {
    for (id, deltas) in out.textures_delta.set.drain() {
        for delta in deltas {
            renderer.update_texture(device, queue, id, &delta);
        }
    }
    for id in out.textures_delta.free.drain() {
        renderer.free_texture(&id);
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
            WindowEvent::RedrawRequested => {
                self.draw();
                if self.exit_requested {
                    el.exit();
                }
            }
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
        // Surface/renderer first, window last (see the field-order note);
        // `state` (and its world) drops after all GPU objects.
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
    state: &'a mut State,
    tex: egui::TextureId,
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
                    self.state.cam.orbit(d.x, d.y);
                }
                if resp.dragged_by(egui::PointerButton::Middle) {
                    let d = resp.drag_delta();
                    self.state.cam.pan(d.x, d.y);
                }
                if resp.hovered() {
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if scroll != 0.0 {
                        self.state.cam.zoom(scroll);
                    }
                }
            }
            Tab::Browser => {
                panels::browser(ui, self.state, &theme::Palette::DARK);
            }
            Tab::Properties => {
                panels::properties(ui, self.state, &theme::Palette::DARK);
            }
        }
    }
}

/// Headless proof (the S2 gate): no window, no surface - the fallback
/// adapter (the same deterministic choice as tests/gpu_render.rs), View3D
/// on the spike's Rgba8Unorm offscreen format, the SEEDED document rendered
/// (block + stock + region + cached path through set_overlay), and the
/// readback showing a solid footprint of changed pixels - the S1 gate, now
/// driven by the real command machinery instead of a hand-built cube.
/// The second half of the proof is the world itself: Contour1 carries a
/// non-empty cached toolpath, i.e. seed -> commands -> generate worked.
mod headless {
    use hsmworks_app::model;
    use hsmworks_ui::state::{CommandKind, State};

    use crate::{Camera, View3D, wgpu, world};

    pub fn run() -> Result<(), String> {
        let mut state = State::new();
        world::seed(&mut state);
        state.pending.push_back(CommandKind::FrameAll);
        state.drain_actions();
        let op_ok = model::find_by_name(&state.world, "Contour1")
            .and_then(|e| model::toolpath_of(&state.world, &e))
            .map_or(0, |tp| tp.path.moves.len());
        if op_ok == 0 {
            return Err(format!(
                "seed produced no cached toolpath for Contour1 (status: {})",
                state.status
            ));
        }

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
        let scene = world::collect(&state);
        view.set_document(&device, &queue, &scene.parts);
        view.set_overlay(&device, &queue, &scene.overlay);

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
            // less than 5% of the frame: the seeded stock did not rasterize
            return Err(format!("readback looks empty ({changed}/{total} changed px)"));
        }
        println!(
            "headless snapshot: {w}x{h}, {}% pixels changed; Contour1 cached {op_ok} moves",
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
