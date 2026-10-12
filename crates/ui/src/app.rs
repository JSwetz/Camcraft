//! The winit application: window, event flow, GPU frame graph.
//! Provenance: our own shell; winit/wgpu/ImGui wiring follows the public
//! examples of those crates.
//!
//! Every GPU-side failure path logs and degrades - nothing here may panic
//! on a user-triggered path (AGENTS.md Prime Directive).

use std::sync::Arc;

use dear_imgui_rs::{ConfigFlags, Context};
use dear_imgui_wgpu::{wgpu, ExternalTextureId, FramebufferExtent, WgpuInitInfo, WgpuRenderer};
use dear_imgui_winit::{HiDpiMode, WinitPlatform};
use hsmworks_app::geometry::transform_point;
use hsmworks_app::{model, pick};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::shell;
use crate::state::{CommandKind, DragMode, State};
use crate::viewport::View3D;

#[must_use]
pub fn run() -> i32 {
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(err) => {
            eprintln!("cannot create event loop: {err}");
            return 1;
        }
    };
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new();
    let code = match event_loop.run_app(&mut app) {
        Ok(()) => app.exit_code,
        Err(err) => {
            eprintln!("event loop failed: {err}");
            1
        }
    };
    app.shutdown();
    code
}

struct App {
    state: State,
    ctx: Option<Context>,
    platform: Option<WinitPlatform>,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    adapter: Option<wgpu::Adapter>,
    device: Option<wgpu::Device>,
    queue: Option<wgpu::Queue>,
    renderer: Option<WgpuRenderer>,
    view: Option<View3D>,
    external: Option<ExternalTextureId>,
    config: Option<wgpu::SurfaceConfiguration>,
    doc_synced: u64,
    /// Selection changed (or the document was re-sent) since last frame;
    /// app.rs re-sends meshes because the renderer caches external texture
    /// views by id and the vertex stream bakes selection state.
    sel_synced: u64,
    /// Last cursor position in logical px; delta base for viewport drags.
    cursor_pos: [f32; 2],
    /// Left-press origin, so release can tell a click (pick) from a drag.
    left_press: Option<[f32; 2]>,
    exit_code: i32,
}

impl App {
    fn new() -> Self {
        App {
            state: State::new(),
            ctx: None,
            platform: None,
            window: None,
            surface: None,
            adapter: None,
            device: None,
            queue: None,
            renderer: None,
            view: None,
            external: None,
            config: None,
            doc_synced: u64::MAX,
            sel_synced: u64::MAX,
            cursor_pos: [0.0, 0.0],
            left_press: None,
            exit_code: 0,
        }
    }

    /// Free renderer/ImGui resources in the right order (no leaks).
    fn shutdown(&mut self) {
        self.external = None;
        self.view = None;
        if let (Some(renderer), Some(ctx)) = (self.renderer.as_mut(), self.ctx.as_mut())
            && let Err(err) = renderer.shutdown(ctx)
        {
            eprintln!("renderer shutdown: {err}");
        }
        self.renderer = None;
        if let (Some(platform), Some(ctx)) = (self.platform.as_mut(), self.ctx.as_mut())
            && let Err(err) = platform.shutdown(ctx)
        {
            eprintln!("platform shutdown: {err}");
        }
        self.platform = None;
        self.config = None;
        self.surface = None;
        self.queue = None;
        self.device = None;
        self.adapter = None;
        self.window = None;
        self.ctx = None;
    }

    fn fail(&mut self, el: &ActiveEventLoop, what: &str, err: impl std::fmt::Display) {
        eprintln!("{what}: {err}");
        self.exit_code = 1;
        el.exit();
    }

    fn init_gpu(&mut self, el: &ActiveEventLoop) {
        let Ok(mut ctx) = Context::try_create() else {
            self.fail(el, "imgui context", "another context is already active");
            return;
        };
        if let Err(err) = ctx.set_ini_filename(None::<String>) {
            self.fail(el, "imgui ini", err);
            return;
        }
        ctx.io_mut()
            .set_config_flags(ConfigFlags::DOCKING_ENABLE);
        let mut platform = match WinitPlatform::new(&mut ctx) {
            Ok(p) => p,
            Err(err) => {
                self.fail(el, "imgui winit platform", err);
                return;
            }
        };
        let attrs = winit::window::Window::default_attributes()
            .with_title("HSMWorks (clean-room shell)")
            .with_inner_size(LogicalSize::new(1280.0, 800.0));
        let window = match el.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(err) => {
                self.fail(el, "window creation", err);
                return;
            }
        };
        // The platform needs shared ownership of the main window; this also
        // sets the initial display size and framebuffer scale.
        if let Err(err) = platform.attach_window(Arc::clone(&window), HiDpiMode::Default, &mut ctx)
        {
            self.fail(el, "imgui window attach", err);
            return;
        }

        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        // SAFETY: `window` lives in self.window and the surface is dropped
        // before it in shutdown(); the raw handle stays valid meanwhile.
        // wgpu-core requires a display handle from somewhere: the instance
        // has none (new_without_display_handle), so carry it from the window.
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
        let format = config.format;
        surface.configure(&device, &config);

        let mut renderer = match WgpuRenderer::new(
            WgpuInitInfo::new(device.clone(), queue.clone(), format),
            &mut ctx,
        ) {
            Ok(r) => r,
            Err(err) => {
                self.fail(el, "imgui wgpu renderer", err);
                return;
            }
        };
        let scale = window.scale_factor().clamp(0.1, 8.0);
        // wgpu raises validation errors (e.g. a WGSL typo) by panicking;
        // catch it so the process degrades with a diagnostic instead.
        let view = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            View3D::new(
                &device,
                format,
                (f64::from(self.state.viewport_size[0]) * scale).max(1.0) as u32,
                (f64::from(self.state.viewport_size[1]) * scale).max(1.0) as u32,
            )
        })) {
            Ok(v) => v,
            Err(_) => {
                self.fail(el, "viewport3d pipeline", "GPU validation failed (see log)");
                return;
            }
        };
        let external = match renderer.register_external_texture(view.color_view()) {
            Ok(id) => id,
            Err(err) => {
                self.fail(el, "viewport texture registration", err);
                return;
            }
        };
        self.state.viewport_tex = Some(external.texture_id());

        self.ctx = Some(ctx);
        self.platform = Some(platform);
        self.window = Some(window);
        self.surface = Some(surface);
        self.adapter = Some(adapter);
        self.device = Some(device);
        self.queue = Some(queue);
        self.renderer = Some(renderer);
        self.view = Some(view);
        self.external = Some(external);
        self.config = Some(config);
    }

    fn on_resize(&mut self, size: PhysicalSize<u32>) {
        let (w, h) = (size.width.max(1), size.height.max(1));
        let (Some(surface), Some(device)) = (self.surface.as_ref(), self.device.as_ref()) else {
            return;
        };
        let Some(config) = self.config.as_mut() else {
            return;
        };
        if config.width == w && config.height == h {
            return;
        }
        config.width = w;
        config.height = h;
        surface.configure(device, config);
    }

    /// Viewport drags/picks are driven from raw winit events, not ImGui
    /// hover state: a single large Image inside a docked window proved
    /// unreliable for button tracking. Clicks only start inside the image
    /// rect; the drag then continues even past its edges.
    fn inside_viewport(&self, p: [f32; 2]) -> bool {
        let ([x0, y0], [x1, y1]) = self.state.viewport_rect;
        x1 > x0 && y1 > y0 && p[0] >= x0 && p[0] <= x1 && p[1] >= y0 && p[1] <= y1
    }

    fn on_press(&mut self, button: &MouseButton) {
        if !self.inside_viewport(self.cursor_pos) {
            return;
        }
        match button {
            MouseButton::Middle => self.state.drag = Some(DragMode::Orbit),
            MouseButton::Right => self.state.drag = Some(DragMode::Pan),
            MouseButton::Left => self.left_press = Some(self.cursor_pos),
            _ => {} // Back/Forward/Other: no viewport binding
        }
    }

    fn on_release(&mut self, button: &MouseButton) {
        match button {
            MouseButton::Middle | MouseButton::Right => self.state.drag = None,
            MouseButton::Left => {
                // A press-and-release under 4 logical px is a click: pick.
                // Past that it was an orbit drag - end it here instead.
                let Some(press) = self.left_press.take() else {
                    return;
                };
                if self.state.drag == Some(DragMode::Orbit) {
                    self.state.drag = None;
                }
                let p = self.cursor_pos;
                let dx = p[0] - press[0];
                let dy = p[1] - press[1];
                if (dx * dx + dy * dy).sqrt() <= 4.0 && self.inside_viewport(p) {
                    self.pick_at(p);
                }
            }
            _ => {}
        }
    }

    /// Raycast at a logical-px point inside the viewport; queues Select
    /// (None on a miss, which clears selection - standard CAM behavior).
    fn pick_at(&mut self, p: [f32; 2]) {
        let ([x0, y0], [x1, y1]) = self.state.viewport_rect;
        let (w, h) = (x1 - x0, y1 - y0);
        if w <= 1.0 || h <= 1.0 {
            return;
        }
        let ndc = [
            (p[0] - x0) / w * 2.0 - 1.0,
            1.0 - (p[1] - y0) / h * 2.0,
        ];
        let Some((origin, dir)) = self.state.cam.ray_through(ndc, f64::from(w / h)) else {
            return;
        };
        let hit = pick::first_hit(&self.state.world, origin, dir);
        // Status is the feedback channel: "nothing happened" should be
        // impossible to believe when a pick always says what it did.
        self.state.status = match &hit {
            Some(name) => format!("selected {name}"),
            None => String::from("no model under the cursor"),
        };
        self.state.pending.push_back(CommandKind::Select(hit));
    }

    fn draw(&mut self, el: &ActiveEventLoop) {
        state_drain_and_quit(self, el);
        if self.state.request_quit || self.ctx.is_none() {
            return;
        }

        // Split disjoint borrows once; bail quietly if any piece is missing.
        let Self {
            state,
            ctx,
            platform,
            window,
            surface,
            device,
            queue,
            renderer,
            view,
            external,
            config,
            doc_synced,
            sel_synced,
            exit_code,
            ..
        } = self;
        let (
            Some(ctx),
            Some(platform),
            Some(window),
            Some(surface),
            Some(device),
            Some(queue),
            Some(renderer),
            Some(view),
            Some(ext_id),
            Some(conf),
        ) = (
            ctx.as_mut(),
            platform.as_mut(),
            window.as_ref(),
            surface.as_ref(),
            device.as_mut(),
            queue.as_mut(),
            renderer.as_mut(),
            view.as_mut(),
            external.as_mut(),
            config.as_ref(),
        )
        else {
            return;
        };

        if let Err(err) = platform.prepare_frame(ctx, window) {
            eprintln!("prepare_frame: {err}");
            return;
        }
        let scale = window.scale_factor().clamp(0.1, 8.0);
        let logical = window.inner_size().to_logical::<f64>(scale);
        ctx.io_mut().set_display_size([logical.width as f32, logical.height as f32]);

        // UI first: the panels report their size, the GPU pass follows.
        let frame = ctx.begin_frame();
        let sel_dirty = state.selection_version != *sel_synced;
        shell::build(frame.ui(), state);
        *sel_synced = state.selection_version;

        let want = (
            (f64::from(state.viewport_size[0]) * scale).max(1.0) as u32,
            (f64::from(state.viewport_size[1]) * scale).max(1.0) as u32,
        );
        let resized = view.extent() != want;
        view.resize(device, conf.format, want.0, want.1);
        if resized
            && let Err(err) = renderer.update_external_texture(*ext_id, view.color_view())
        {
            eprintln!("external texture update: {err}");
        }

        if state.doc_version != *doc_synced || sel_dirty {
            let parts = collect_parts(state);
            let refs: Vec<(&[[f32; 3]], bool)> =
                parts.iter().map(|(v, s)| (v.as_slice(), *s)).collect();
            view.set_document(device, queue, &refs);
            *doc_synced = state.doc_version;
        }

        let (ew, eh) = view.extent();
        let mvp = state.cam.mvp(f64::from(ew.max(1)) / f64::from(eh.max(1)));

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
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("hsmworks-frame"),
        });
        let eye = state.cam.eye_f32();
        view.render(queue, &mut encoder, &mvp, eye);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("imgui-pass"),
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
                occlusion_query_set: None,
                multiview_mask: None,
                timestamp_writes: None,
            });
            let consumer = match renderer.renderer_consumer() {
                Ok(c) => c,
                Err(err) => {
                    eprintln!("renderer consumer: {err}");
                    return;
                }
            };
            match frame.try_render(consumer) {
                Ok(pending) => {
                    if let Err(err) = renderer.render(
                        pending,
                        &mut pass,
                        FramebufferExtent::from_texture(&st.texture),
                    ) {
                        eprintln!("imgui draw: {err}");
                    }
                }
                Err(err) => eprintln!("imgui render snapshot: {err}"),
            }
        }
        queue.submit([encoder.finish()]);
        queue.present(st);
        let _ = exit_code;
    }
}

/// Run queued actions; honor a Quit request.
fn state_drain_and_quit(app: &mut App, el: &ActiveEventLoop) {
    app.state.drain_actions();
    if app.state.request_quit {
        el.exit();
    }
}

/// Flatten the world into triangle soup (world space + selection flag).
/// World-space vertices for the GPU: three `[f32;3]` rows per triangle.
fn collect_parts(state: &State) -> Vec<(Vec<[f32; 3]>, bool)> {
    model::named_entities(&state.world)
        .into_iter()
        .map(|(_, entity)| {
            let tris = model::facets_of(&state.world, &entity).unwrap_or_default();
            let selected = model::is_selected(&state.world, &entity);
            let loc = model::transform_of(&state.world, &entity).unwrap_or_default();
            let mut verts = Vec::with_capacity(tris.len() * 3);
            for t in tris {
                for p in [t.a, t.b, t.c] {
                    let p = transform_point(p, loc.pos, loc.rot);
                    verts.push([ftoz(p.x), ftoz(p.y), ftoz(p.z)]);
                }
            }
            (verts, selected)
        })
        .collect()
}

fn ftoz(v: f64) -> f32 {
    if v.is_finite() { v as f32 } else { 0.0 }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.ctx.is_none() && self.exit_code == 0 {
            self.init_gpu(el);
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let (Some(ctx), Some(platform), Some(window)) =
            (self.ctx.as_mut(), self.platform.as_mut(), self.window.as_ref())
            && let Err(err) = platform.handle_window_event(ctx, window, &event)
        {
            eprintln!("input event: {err}");
        }
        match &event {
            WindowEvent::CloseRequested => {
                self.state.request_quit = true;
                el.exit();
            }
            WindowEvent::Resized(size) => self.on_resize(*size),
            WindowEvent::RedrawRequested => self.draw(el),
            // Import validation lives in the command (app layer); the shell
            // only queues, so a dropped corrupt/unsupported file surfaces
            // as the same honest status as File > Open.
            WindowEvent::DroppedFile(path) => {
                self.state.pending.push_back(CommandKind::ImportFile(path.to_path_buf()));
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.window.as_ref().map_or(1.0, |w| w.scale_factor());
                let pos = position.to_logical::<f32>(scale);
                let delta = [pos.x - self.cursor_pos[0], pos.y - self.cursor_pos[1]];
                self.cursor_pos = [pos.x, pos.y];
                match self.state.drag {
                    Some(DragMode::Orbit) => self.state.cam.orbit(delta[0], delta[1]),
                    Some(DragMode::Pan) => self.state.cam.pan(delta[0], delta[1]),
                    None => {
                        // A left-drag past a small threshold becomes an
                        // orbit, so the primary button does something
                        // everywhere; a short press stays a pick.
                        if let Some(press) = self.left_press {
                            let dx = self.cursor_pos[0] - press[0];
                            let dy = self.cursor_pos[1] - press[1];
                            if (dx * dx + dy * dy).sqrt() > 4.0 {
                                self.state.drag = Some(DragMode::Orbit);
                                self.state.cam.orbit(delta[0], delta[1]);
                            }
                        }
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => self.on_press(button),
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button,
                ..
            } => self.on_release(button),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}
