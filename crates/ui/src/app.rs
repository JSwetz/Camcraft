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
use hsmworks_app::geometry::{Aabb, Point3, transform_point};
use hsmworks_app::schema::{STOCK_NAME, WCS_SLOTS};
use hsmworks_app::{model, pick};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::shell;
use crate::state::{CommandKind, DragMode, State};
use crate::viewport::{GpuPart, View3D};

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
    /// View-affecting state the overlay is baked from but the document does
    /// not carry (anchor-mode drafts, world-axes visibility). It changes
    /// with pane widgets that don't bump `doc_version`, so it gates a
    /// rebuild of its own (pick-mode markers would otherwise lag a click by
    /// a document edit).
    draft_synced: [u64; 5],
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
            draft_synced: [0; 5],
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
        // Anchor picking wins over entity picking: in pick mode a click ON a
        // marker places the origin there (same command the pane grid
        // queues); a click beside one still picks normally, so the viewport
        // never stops behaving like the viewport.
        if self.state.wcs_ui.pick_anchor
            && let Some((k, pos)) = anchor_hit(&self.state, p, 12.0)
        {
            let slot = *WCS_SLOTS
                .get(self.state.wcs_ui.slot.min(WCS_SLOTS.len() - 1))
                .unwrap_or(&WCS_SLOTS[0]);
            self.state.wcs_ui.anchor_xy = k;
            self.state.pending.push_back(CommandKind::SetWcs {
                slot: slot.to_string(),
                pos,
                roll_deg: self.state.wcs_ui.roll_deg,
                pitch_deg: self.state.wcs_ui.pitch_deg,
                yaw_deg: self.state.wcs_ui.yaw_deg,
            });
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
            draft_synced,
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

        let draft_key = [
            u64::from(state.wcs_ui.pick_anchor),
            state.wcs_ui.anchor_source as u64,
            state.wcs_ui.anchor_z as u64,
            state.wcs_ui.anchor_xy as u64,
            u64::from(state.show_world_axes),
        ];
        if state.doc_version != *doc_synced || sel_dirty || draft_key != *draft_synced {
            let parts = collect_parts(state);
            view.set_document(device, queue, &parts);
            let overlay = collect_overlay(state);
            view.set_overlay(device, queue, &overlay);
            *doc_synced = state.doc_version;
            *draft_synced = draft_key;
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

/// Flatten the world into GPU parts: models by name first, then the stock
/// solid (it renders translucent, so it must be submitted last).
fn collect_parts(state: &State) -> Vec<GpuPart> {
    let mut parts: Vec<GpuPart> = model::model_entities(&state.world)
        .into_iter()
        .map(|(_, entity)| GpuPart {
            verts: world_verts(state, &entity),
            selected: model::is_selected(&state.world, &entity),
            stock: false,
        })
        .collect();
    if let Some(entity) = model::find_by_name(&state.world, STOCK_NAME)
        && model::is_stock(&state.world, &entity)
    {
        parts.push(GpuPart {
            verts: world_verts(state, &entity),
            selected: model::is_selected(&state.world, &entity),
            stock: true,
        });
    }
    parts
}

/// World-space vertices for the GPU: three `[f32;3]` rows per triangle,
/// rigid transform applied here so the shader stays transform-free.
fn world_verts(state: &State, entity: &flecs_ecs::prelude::Entity) -> Vec<[f32; 3]> {
    let tris = model::facets_of(&state.world, entity).unwrap_or_default();
    let loc = model::transform_of(&state.world, entity).unwrap_or_default();
    let mut verts = Vec::with_capacity(tris.len() * 3);
    for t in tris {
        for p in [t.a, t.b, t.c] {
            let p = transform_point(p, loc.pos, loc.rot);
            verts.push([ftoz(p.x), ftoz(p.y), ftoz(p.z)]);
        }
    }
    verts
}

/// One origin gizmo to draw: a triad at `pos`/`rot`; the dimmed variant is
/// the neutral world origin shown while no work offset claims that corner.
pub(crate) struct Gizmo {
    pub pos: [f64; 3],
    pub rot: [f64; 4],
    /// Triad axes sit on X/Y/Z at this exact position with no rotation.
    pub world_axes: bool,
}

/// Axis colors and tip offsets shared by the line overlay and the labels.
const AXES: [([f32; 3], [f64; 3]); 3] = [
    ([0.85, 0.22, 0.18], [1.0, 0.0, 0.0]),
    ([0.20, 0.78, 0.30], [0.0, 1.0, 0.0]),
    ([0.28, 0.48, 0.95], [0.0, 0.0, 1.0]),
];

const AXIS_NAMES: [&str; 3] = ["X", "Y", "Z"];

/// Reference-geometry overlay: world origin axes (dimmed) plus one colored
/// triad per defined work offset. Length scales with the model extent, with
/// a floor so an empty or tiny document still shows orientation.
fn collect_overlay(state: &State) -> Vec<[f32; 6]> {
    let len = gizmo_len(state);
    let mut out: Vec<[f32; 6]> = Vec::new();
    for gizmo in gizmos(state) {
        let (px, py, pz) = (ftoz(gizmo.pos[0]), ftoz(gizmo.pos[1]), ftoz(gizmo.pos[2]));
        for (rgb, dir) in &AXES {
            let p = transform_point(
                Point3::new(dir[0] * len, dir[1] * len, dir[2] * len),
                Point3::new(gizmo.pos[0], gizmo.pos[1], gizmo.pos[2]),
                gizmo.rot,
            );
            let c = if gizmo.world_axes {
                [rgb[0] * 0.55, rgb[1] * 0.55, rgb[2] * 0.55]
            } else {
                *rgb
            };
            out.push([px, py, pz, c[0], c[1], c[2]]);
            out.push([ftoz(p.x), ftoz(p.y), ftoz(p.z), c[0], c[1], c[2]]);
        }
    }
    if state.wcs_ui.pick_anchor {
        // Virtual vertices (feedback): a small 3-axis cross per anchor of
        // the chosen level, so the box points can be picked where they
        // actually are; the pane-highlighted one is drawn brighter.
        let arm = (gizmo_len(state) * 0.09).max(0.5);
        for (k, (pt, _)) in anchor_points(state).iter().enumerate() {
            let c = if k == state.wcs_ui.anchor_xy.min(8) {
                [0.98, 0.98, 0.60]
            } else {
                [0.82, 0.68, 0.22]
            };
            for axis in 0..3 {
                let (mut a, mut b) = (*pt, *pt);
                a[axis] -= arm;
                b[axis] += arm;
                out.push([ftoz(a[0]), ftoz(a[1]), ftoz(a[2]), c[0], c[1], c[2]]);
                out.push([ftoz(b[0]), ftoz(b[1]), ftoz(b[2]), c[0], c[1], c[2]]);
            }
        }
    }
    out
}

/// Triad extent for the current document (never smaller than 10 mm).
fn gizmo_len(state: &State) -> f64 {
    let extent = model::solid_world_aabb(&state.world)
        .map(|b| {
            (b.max.x - b.min.x)
                .max(b.max.y - b.min.y)
                .max(b.max.z - b.min.z)
        })
        .unwrap_or(0.0);
    (extent * 0.25).max(10.0)
}

/// Which origin gizmos to draw (feedback 6): a triad per defined work
/// offset, plus the dimmed world axes - unless some triad already sits
/// exactly on the world origin, in which case its own lines are the world
/// axes and drawing both reads as a duplicated gizmo, or the Models panel
/// has the axes hidden: two gizmos around a moved G5x read as clutter, and
/// the world axes are context furniture, not setup. The triads stay: they
/// are the work offsets themselves.
fn gizmos(state: &State) -> Vec<Gizmo> {
    let mut out: Vec<Gizmo> = Vec::new();
    for (_, entity) in model::named_entities(&state.world) {
        if !model::is_wcs(&state.world, &entity) {
            continue;
        }
        let Some(loc) = model::transform_of(&state.world, &entity) else {
            continue;
        };
        out.push(Gizmo {
            pos: [loc.pos.x, loc.pos.y, loc.pos.z],
            rot: loc.rot,
            world_axes: false,
        });
    }
    let claims_origin = out.iter().any(|g| {
        let on_spot = g.pos[0].abs() < 1e-3 && g.pos[1].abs() < 1e-3 && g.pos[2].abs() < 1e-3;
        // Unit quaternion up to sign: [0,0,0,±1] is identity rotation.
        let unrotated = g.rot[0].abs() < 1e-6
            && g.rot[1].abs() < 1e-6
            && g.rot[2].abs() < 1e-6
            && (1.0 - g.rot[3].abs()).abs() < 1e-6;
        on_spot && unrotated
    });
    if !claims_origin && state.show_world_axes {
        out.push(Gizmo {
            pos: [0.0, 0.0, 0.0],
            rot: [0.0, 0.0, 0.0, 1.0],
            world_axes: true,
        });
    }
    out
}

/// Compass names for the 3x3 XY anchor grid, row-major north-to-south,
/// west-to-east (viewed from +Z: X east, Y north). Point `k` has XY
/// indices `[k % 3, 2 - k / 3]` in the box's lo/center/hi axes.
const COMPASS: [&str; 9] = ["NW", "N", "NE", "W", "C", "E", "SW", "S", "SE"];

/// Which box the anchor points live on, per the pane's explicit choice
/// (feedback: "a dropdown under WCS that states if the origin is on stock
/// or on model"), falling back honestly to the other when the chosen one
/// doesn't exist. Returns the box and which one it is, for the pane note.
pub(crate) fn anchor_reference(state: &State) -> Option<(Aabb, &'static str)> {
    let stock = || model::stock_world_aabb(&state.world).map(|b| (b, "stock box"));
    let solid = || model::solid_world_aabb(&state.world).map(|b| (b, "model bounds"));
    if state.wcs_ui.anchor_source == 1 {
        solid().or_else(stock)
    } else {
        stock().or_else(solid)
    }
}

/// The nine anchor points of the current Z level, in grid order, with their
/// compass names. The pane buttons and the viewport markers both come from
/// here, so what you click and what you see are the same points by
/// construction.
pub(crate) fn anchor_points(state: &State) -> Vec<([f64; 3], &'static str)> {
    let Some((b, _)) = anchor_reference(state) else {
        return Vec::new();
    };
    let z = state.wcs_ui.anchor_z.min(2);
    (0..9)
        .map(|k| (model::anchor_point(&b, [k % 3, 2 - k / 3, z]), COMPASS[k]))
        .collect()
}

/// Nearest on-screen anchor marker to a logical-px click, within `radius`
/// px. Markers are overlay lines with no pickable geometry and no depth
/// pass, so selection is screen-space nearest-within-threshold; points
/// behind the camera never win.
pub(crate) fn anchor_hit(state: &State, p: [f32; 2], radius: f32) -> Option<(usize, [f64; 3])> {
    let ([x0, y0], [x1, y1]) = state.viewport_rect;
    let (w, h) = (x1 - x0, y1 - y0);
    if w <= 1.0 || h <= 1.0 {
        return None;
    }
    let mvp = state.cam.mvp(f64::from(w) / f64::from(h));
    let pts = anchor_points(state);
    let mut best: Option<(f32, usize)> = None;
    for (k, (pt, _)) in pts.iter().enumerate() {
        let Some(uv) = project_uv(&mvp, *pt) else {
            continue;
        };
        let dx = x0 + uv[0] * w - p[0];
        let dy = y0 + uv[1] * h - p[1];
        let d2 = dx * dx + dy * dy;
        if d2 <= radius * radius && best.is_none_or(|(bd, _)| d2 < bd) {
            best = Some((d2, k));
        }
    }
    best.map(|(_, k)| (k, pts[k].0))
}

/// Clip-projection of a world point through the column-major `mvp` to
/// viewport uv (x right, y down from the top edge); None behind the camera
/// or at degenerate `w`. Shared by the gizmo labels and anchor picking.
pub(crate) fn project_uv(mvp: &[[f32; 4]; 4], p: [f64; 3]) -> Option<[f32; 2]> {
    // Column-major mvp: clip_i = sum_c mvp[c][i] * v_c.
    let v = [p[0] as f32, p[1] as f32, p[2] as f32, 1.0f32];
    let mut clip = [0.0f32; 4];
    for (i, out_i) in clip.iter_mut().enumerate() {
        *out_i = mvp[0][i] * v[0] + mvp[1][i] * v[1] + mvp[2][i] * v[2] + mvp[3][i] * v[3];
    }
    let w = clip[3];
    if !w.is_finite() || w <= 1e-6 {
        return None; // behind the camera or degenerate
    }
    let (ndc_x, ndc_y) = (clip[0] / w, clip[1] / w);
    if !ndc_x.is_finite() || !ndc_y.is_finite() {
        return None;
    }
    Some([(ndc_x + 1.0) * 0.5, (1.0 - ndc_y) * 0.5])
}

/// Screen-space labels for the gizmos just drawn (feedback 7): each axis
/// tip gets its letter, projected through the same camera math. Entries are
/// (uv within the viewport image, linear rgb, text); tips behind the camera
/// or at degenerate `w` are skipped.
pub(crate) fn overlay_labels(state: &State, aspect: f64) -> Vec<([f32; 2], [f32; 3], &'static str)> {
    let len = gizmo_len(state);
    let mvp = state.cam.mvp(aspect);
    let mut out = Vec::new();
    for gizmo in gizmos(state) {
        let base = Point3::new(gizmo.pos[0], gizmo.pos[1], gizmo.pos[2]);
        for (axis, (rgb, dir)) in AXES.iter().enumerate() {
            let p = transform_point(
                Point3::new(
                    base.x + dir[0] * len * 1.08,
                    base.y + dir[1] * len * 1.08,
                    base.z + dir[2] * len * 1.08,
                ),
                base,
                gizmo.rot,
            );
            let Some(uv) = project_uv(&mvp, [p.x, p.y, p.z]) else {
                continue;
            };
            let c = if gizmo.world_axes {
                [rgb[0] * 0.65, rgb[1] * 0.65, rgb[2] * 0.65]
            } else {
                *rgb
            };
            out.push((uv, c, AXIS_NAMES[axis]));
        }
    }
    out
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

#[cfg(test)]
mod gizmo_tests {
    //! The overlay's gizmo rules (feedbacks 6 and 7) are pure world reads;
    //! these pin them down without a GPU.

    use super::*;
    use crate::state::CommandKind;

    fn set_origin(state: &mut State, slot: &str, pos: [f64; 3], yaw_deg: f64) {
        state.pending.push_back(CommandKind::SetWcs {
            slot: slot.to_string(),
            pos,
            roll_deg: 0.0,
            pitch_deg: 0.0,
            yaw_deg,
        });
        state.drain_actions();
    }

    #[test]
    fn a_fresh_world_shows_only_the_world_axes() {
        let state = State::new();
        let lines = collect_overlay(&state);
        assert_eq!(lines.len(), 6, "three dimmed axes");
        assert!(lines[0][3] < 0.5, "world axes are dimmed");
    }

    #[test]
    fn an_origin_placed_at_machine_zero_does_not_duplicate_the_gizmo() {
        let mut state = State::new();
        set_origin(&mut state, "G54", [0.0, 0.0, 0.0], 0.0);
        let lines = collect_overlay(&state);
        assert_eq!(lines.len(), 6, "the triad replaces the world axes");
        assert!(lines[0][3] > 0.5, "the single gizmo is the bright triad");
        let labels = overlay_labels(&state, 1.0);
        assert_eq!(labels.len(), 3, "one letter per triad axis, no ghost set");
        assert!(
            labels.iter().all(|(_, rgb, _)| rgb[0].max(rgb[1]).max(rgb[2]) > 0.5),
            "all labels are bright"
        );
    }

    #[test]
    fn a_moved_origin_shows_axes_and_triad_together() {
        let mut state = State::new();
        set_origin(&mut state, "G54", [50.0, 0.0, 0.0], 0.0);
        assert_eq!(collect_overlay(&state).len(), 12, "both gizmos");
        let labels = overlay_labels(&state, 1.0);
        assert_eq!(labels.len(), 6, "three tips per gizmo, both gizmos");
        let xs: Vec<f32> = labels
            .iter()
            .filter(|(_, _, text)| *text == "X")
            .map(|(_, rgb, _)| rgb[0])
            .collect();
        assert!(
            xs.iter().any(|r| *r > 0.8) && xs.iter().any(|r| *r < 0.6),
            "one bright and one dimmed X: {xs:?}"
        );
    }

    #[test]
    fn a_rotated_offset_at_the_origin_still_shows_the_world_axes() {
        // Yaw 90 moves the triad axes off X/Y/Z - the coincidence was about
        // lines, not just the origin point.
        let mut state = State::new();
        set_origin(&mut state, "G54", [0.0, 0.0, 0.0], 90.0);
        assert_eq!(collect_overlay(&state).len(), 12, "the triad no longer claims the axes");
    }

    #[test]
    fn hiding_the_world_axes_leaves_the_offset_triads_alone() {
        let mut state = State::new();
        state.show_world_axes = false;
        assert!(collect_overlay(&state).is_empty(), "nothing but hidden furniture");
        assert!(overlay_labels(&state, 1.0).is_empty(), "and no labels for it");
        set_origin(&mut state, "G54", [50.0, 0.0, 0.0], 0.0);
        assert_eq!(collect_overlay(&state).len(), 6, "the work offset still shows");
        assert_eq!(overlay_labels(&state, 1.0).len(), 3);
        assert!(
            overlay_labels(&state, 1.0)
                .iter()
                .all(|(_, rgb, _)| rgb[0].max(rgb[1]).max(rgb[2]) > 0.5),
            "only the bright triad labels remain, no dimmed world letters"
        );
    }
}

#[cfg(test)]
mod anchor_tests {
    //! The pane buttons, the viewport markers and the click hit-test all
    //! read the same anchor helpers; these pin that shared math down.

    use super::*;
    use crate::state::{CommandKind, quat_yaw_deg};
    use hsmworks_app::stock::StockShape;

    fn stock_box(state: &mut State, dx: f64, dy: f64, dz: f64, pos: [f64; 3]) {
        state.pending.push_back(CommandKind::SetStock {
            shape: StockShape::Box { dx, dy, dz },
            pos,
            yaw_deg: 0.0,
        });
        state.drain_actions();
    }

    /// Box 60x40x20, XY-centered at (10, -5), bottom at z=0:
    /// corners span (-20,-25,0)..(40,15,20).
    fn state_with_stock() -> State {
        let mut state = State::new();
        stock_box(&mut state, 60.0, 40.0, 20.0, [10.0, -5.0, 0.0]);
        state
    }

    #[test]
    fn anchors_need_a_reference_and_prefer_the_chosen_source() {
        let mut state = State::new();
        assert!(anchor_reference(&state).is_none(), "empty world anchors nothing");
        stock_box(&mut state, 60.0, 40.0, 20.0, [0.0, 0.0, 0.0]);
        state.wcs_ui.anchor_source = 0;
        assert_eq!(anchor_reference(&state).map(|(_, w)| w), Some("stock box"));
        state.wcs_ui.anchor_source = 1;
        assert_eq!(
            anchor_reference(&state).map(|(_, w)| w),
            Some("stock box"),
            "no model bounds yet - fall back to stock, and the pane says so"
        );
    }

    #[test]
    fn the_nine_markers_are_the_box_points_of_the_chosen_level() {
        let mut state = state_with_stock();
        state.wcs_ui.anchor_z = 2;
        let pts = anchor_points(&state);
        assert_eq!(pts.len(), 9);
        assert_eq!(pts[0], ([-20.0, 15.0, 20.0], "NW"));
        assert_eq!(pts[2], ([40.0, 15.0, 20.0], "NE"));
        assert_eq!(pts[4], ([10.0, -5.0, 20.0], "C"));
        assert_eq!(pts[8], ([40.0, -25.0, 20.0], "SE"));
        state.wcs_ui.anchor_z = 0;
        assert_eq!(anchor_points(&state)[6], ([-20.0, -25.0, 0.0], "SW"));
        state.wcs_ui.anchor_z = 1;
        assert_eq!(anchor_points(&state)[1], ([10.0, 15.0, 10.0], "N"));
    }

    #[test]
    fn markers_appear_in_the_overlay_only_in_pick_mode() {
        let mut state = state_with_stock();
        assert_eq!(collect_overlay(&state).len(), 6, "world axes only");
        state.wcs_ui.pick_anchor = true;
        let lines = collect_overlay(&state);
        assert_eq!(lines.len(), 6 + 9 * 6, "nine crosses plus the axes");
        assert!(
            lines[6..].iter().any(|v| v[3] > 0.9 && v[4] > 0.9),
            "the pane-highlighted anchor (center, draft default) is brightest"
        );
        assert!(
            lines[6..].iter().any(|v| (v[3] - 0.82).abs() < 0.01),
            "the others are yellow"
        );
    }

    #[test]
    fn clicking_a_marker_selects_exactly_the_anchor_it_draws() {
        let mut state = state_with_stock();
        state.wcs_ui.pick_anchor = true;
        state.wcs_ui.anchor_z = 2;
        state.viewport_rect = ([0.0, 0.0], [800.0, 600.0]);
        let mvp = state.cam.mvp(800.0 / 600.0);
        let pts = anchor_points(&state);
        for (k, (pt, _)) in pts.iter().enumerate() {
            let Some(uv) = project_uv(&mvp, *pt) else {
                panic!("marker {k} ({}) off-screen from the default camera", pts[k].1);
            };
            let (hit_k, hit_pos) =
                anchor_hit(&state, [uv[0] * 800.0, uv[1] * 600.0], 8.0).expect("own position hits self");
            assert_eq!((hit_k, hit_pos), (k, *pt), "nearest marker to a click is the one drawn there");
        }
        // A degenerate rect (pre-layout) can never hit, so no phantom origin.
        state.viewport_rect = ([0.0, 0.0], [0.0, 0.0]);
        assert!(anchor_hit(&state, [4.0, 4.0], 12.0).is_none());
    }

    #[test]
    fn a_marker_click_queues_the_same_placement_as_the_grid() {
        // The end of the viewport path: a picked anchor point plus the pane
        // drafts lands exactly like the button grid's command would.
        let mut state = state_with_stock();
        state.wcs_ui.anchor_z = 2;
        state.viewport_rect = ([0.0, 0.0], [800.0, 600.0]);
        let mvp = state.cam.mvp(800.0 / 600.0);
        let pts = anchor_points(&state);
        let uv = project_uv(&mvp, pts[8].0).expect("SE visible");
        let (_, pos) = anchor_hit(&state, [uv[0] * 800.0, uv[1] * 600.0], 8.0).expect("hit SE");
        assert_eq!(pos, [40.0, -25.0, 20.0]);
        state.pending.push_back(CommandKind::SetWcs {
            slot: "G54".to_string(),
            pos,
            roll_deg: 0.0,
            pitch_deg: 0.0,
            yaw_deg: 30.0,
        });
        state.drain_actions();
        let entity = model::find_by_name(&state.world, "G54").expect("origin entity");
        let loc = model::transform_of(&state.world, &entity).expect("location");
        assert_eq!([loc.pos.x, loc.pos.y, loc.pos.z], [40.0, -25.0, 20.0]);
        assert!(
            (quat_yaw_deg(loc.rot) - 30.0).abs() < 1e-9,
            "the draft orientation rides along with the clicked position"
        );
    }
}
