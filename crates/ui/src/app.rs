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
use hsmworks_app::cl::MoveKind;
use hsmworks_app::geometry::feature;
use hsmworks_app::geometry::{Aabb, Point3, transform_point};
use hsmworks_app::geo2d::Region;
use hsmworks_app::operation::{FeatureSel, GeoSel};
use hsmworks_app::schema::{STOCK_NAME, WCS_SLOTS};
use hsmworks_app::{model, operation, pick};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::keyboard::ModifiersState;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::shell;
use crate::state::{CommandKind, DragMode, GeoPick, HoverHit, State};
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
    /// not carry (anchor-mode drafts, world-axes visibility, the
    /// show-all-toolpaths toggle, the browser eye lists). It changes with
    /// pane widgets that don't bump `doc_version`, so it gates a rebuild of
    /// its own (pick-mode markers would otherwise lag a click by a
    /// document edit).
    draft_synced: [u64; 10],
    /// Last cursor position in logical px; delta base for viewport drags.
    cursor_pos: [f32; 2],
    /// Left-press origin, so release can tell a click (pick) from a drag.
    left_press: Option<[f32; 2]>,
    /// Live keyboard modifiers, fed by `WindowEvent::ModifiersChanged`
    /// (winit 0.30's MouseInput carries none). A feature pick reads Ctrl
    /// (multi-add) and Alt (single edge segment) from here.
    mods: ModifiersState,
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
            draft_synced: [0; 10],
            cursor_pos: [0.0, 0.0],
            left_press: None,
            mods: ModifiersState::default(),
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
        // Ribbon glyphs ride the same external-texture path as the
        // viewport image; a decode failure is one grey-free text cell,
        // never a lost frame (icons.rs logs per glyph).
        self.state.icons = Some(crate::icons::Icons::new(&device, &queue, &mut renderer));

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
        // Bypass ImGui hover bookkeeping entirely except for this one flag:
        // presses that belong to a menu, popup or the ribbon (any window
        // on top of the image) must never start an orbit or a pick.
        if !self.state.viewport_hot || !self.inside_viewport(self.cursor_pos) {
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

    /// A click inside the viewport, routed by the pick mode: models,
    /// feature faces, or edge chains.
    fn pick_at(&mut self, p: [f32; 2]) {
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
        match self.state.geo_pick {
            GeoPick::Model => self.pick_model(p),
            GeoPick::Face => self.pick_face(p),
            GeoPick::Edge => self.pick_edge(p),
        }
    }

    /// Raycast at a logical-px point inside the viewport; queues Select
    /// (None on a miss, which clears selection - standard CAM behavior).
    fn pick_model(&mut self, p: [f32; 2]) {
        let Some((origin, dir)) = self.ray_at(p) else {
            return;
        };
        // A browser-eye hidden model is display-off, not gone: it still
        // feeds stock and paths, but it is transparent to the ray here -
        // a click selects what the user can SEE, and geometry behind a
        // hidden model is honestly pickable (filtered `first_hit_f`).
        let hit = pick::first_hit_f(&self.state.world, origin, dir, |name| {
            self.state.hidden_models.iter().any(|h| h == name)
        });
        // The click also tells us the exact facet under the pointer;
        // keeping it alive so the highlight matches the selection.
        self.state.hover = hit
            .as_ref()
            .map(|(name, facet, _)| HoverHit {
                entity: name.clone(),
                facet: *facet,
                feature: None,
            });
        let hit = hit.map(|(name, _, _)| name);
        // Status is the feedback channel: "nothing happened" should be
        // impossible to believe when a pick always says what it did.
        self.state.status = match &hit {
            Some(name) => format!("selected {name}"),
            None => String::from("no model under the cursor"),
        };
        self.state.pending.push_back(CommandKind::Select(hit));
    }

    /// Face pick: the raycast facet's feature face becomes a geometry pick
    /// for the open operation. Queues ONLY SetOperationGeometry - a feature
    /// click never touches entity selection, so Ctrl+Z walks the pick list,
    /// not the tree highlight. A miss is a status line, nothing else.
    fn pick_face(&mut self, p: [f32; 2]) {
        let Some((entity, sel)) = self.hover_feature_at(p) else {
            self.state.status = String::from("no face under the cursor");
            return;
        };
        let append = self.mods.control_key() || self.mods.shift_key();
        queue_feature_pick(&mut self.state, GeoSel { entity, feature: sel }, append);
    }

    /// Edge pick: the nearest visible feature edge in screen space names a
    /// chain (its tangent run), or just that segment under Alt. With no
    /// edge near the cursor the click falls through to the face rule -
    /// faces own the pixels between edges.
    fn pick_edge(&mut self, p: [f32; 2]) {
        let Some((entity, edge)) = edge_nearest(&self.state, p, PICK_EDGE_PX) else {
            self.pick_face(p);
            return;
        };
        let Some(found) = model::find_by_name(&self.state.world, &entity) else {
            return;
        };
        let Some(features) = model::features_of(&self.state.world, &found) else {
            return;
        };
        let sel = if self.mods.alt_key() {
            feature::singleton_chain_id(&features, edge).map(FeatureSel::Chain)
        } else {
            feature::chains(&features)
                .iter()
                .find(|c| c.edges.contains(&edge))
                .map(|c| FeatureSel::Chain(c.id))
        };
        let Some(sel) = sel else {
            self.pick_face(p);
            return;
        };
        // Mirror the pick in the hover so highlight and click agree even
        // when the cursor sits on the silhouette, where the ray misses.
        let facet = self
            .ray_at(p)
            .and_then(|(origin, dir)| {
                pick::first_hit_f(&self.state.world, origin, dir, |name| {
                    self.state.hidden_models.iter().any(|h| h == name)
                })
            })
            .map_or(usize::MAX, |(_, facet, _)| facet);
        self.state.hover = Some(HoverHit {
            entity: entity.clone(),
            facet,
            feature: Some(sel),
        });
        let append = self.mods.control_key() || self.mods.shift_key();
        queue_feature_pick(&mut self.state, GeoSel { entity, feature: sel }, append);
    }

    /// Shared hover/raycast for the Face pick: sets state.hover with the
    /// face feature resolved and returns (entity, feature). None (with the
    /// hover reflecting the raw hit) on a miss or a facetless mesh.
    fn hover_feature_at(&mut self, p: [f32; 2]) -> Option<(String, FeatureSel)> {
        let (origin, dir) = self.ray_at(p)?;
        let (entity, facet, _) = pick::first_hit_f(&self.state.world, origin, dir, |name| {
            self.state.hidden_models.iter().any(|h| h == name)
        })?;
        let sel = feature_of_facet(&self.state, &entity, facet);
        self.state.hover = Some(HoverHit {
            entity: entity.clone(),
            facet,
            feature: sel,
        });
        Some((entity, sel?))
    }

    /// Camera ray through a logical-px viewport point; None outside a
    /// laid-out image or behind the camera. Shared by picking and hover.
    fn ray_at(&self, p: [f32; 2]) -> Option<([f64; 3], [f64; 3])> {
        let ([x0, y0], [x1, y1]) = self.state.viewport_rect;
        let (w, h) = (x1 - x0, y1 - y0);
        if w <= 1.0 || h <= 1.0 {
            return None;
        }
        let ndc = [
            (p[0] - x0) / w * 2.0 - 1.0,
            1.0 - (p[1] - y0) / h * 2.0,
        ];
        self.state.cam.ray_through(ndc, f64::from(w / h))
    }

    /// Refresh the hover highlight from the cursor: a filtered raycast (plus
    /// the screen-space edge pass in Edge mode), only while the pointer is
    /// over the live viewport image with no button held and no menu or popup
    /// on top of it. Anywhere else the highlight clears - it must never
    /// linger where the cursor isn't. In Face/Edge mode the hover carries
    /// the feature it resolves to, so the highlight draws the same loops or
    /// chain the click would pick; the rule and the projection are shared,
    /// so they cannot disagree. Ephemeral view state; never a command
    /// (AGENTS.md).
    fn update_hover(&mut self) {
        let idle_over_viewport = self.state.drag.is_none()
            && self.left_press.is_none()
            && self.state.viewport_hot
            && self.inside_viewport(self.cursor_pos);
        if !idle_over_viewport {
            self.state.hover = None;
            return;
        }
        // Edge mode leads with the screen-space pass: the silhouette of a
        // chain is exactly where a ray misses but an edge pick belongs.
        if self.state.geo_pick == GeoPick::Edge
            && let Some((entity, edge)) = edge_nearest(&self.state, self.cursor_pos, PICK_EDGE_PX)
            && let Some(found) = model::find_by_name(&self.state.world, &entity)
            && let Some(features) = model::features_of(&self.state.world, &found)
            && let Some(c) = feature::chains(&features)
                .into_iter()
                .find(|c| c.edges.contains(&edge))
        {
            self.state.hover = Some(HoverHit {
                entity,
                facet: usize::MAX, // no facet drives this highlight
                feature: Some(FeatureSel::Chain(c.id)),
            });
            return;
        }
        let Some((origin, dir)) = self.ray_at(self.cursor_pos) else {
            self.state.hover = None;
            return;
        };
        let hit = pick::first_hit_f(&self.state.world, origin, dir, |name| {
            self.state.hidden_models.iter().any(|h| h == name)
        });
        self.state.hover = hit.map(|(entity, facet, _)| {
            let feature = match self.state.geo_pick {
                // The triangle highlight is exactly what the feature modes
                // replace - Model mode keeps hover free of features. In
                // Edge mode a facet's face is the fallback, matching the
                // click rule (faces own the pixels between edges).
                GeoPick::Model => None,
                GeoPick::Face | GeoPick::Edge => feature_of_facet(&self.state, &entity, facet),
            };
            HoverHit { entity, facet, feature }
        });
    }

    fn draw(&mut self, el: &ActiveEventLoop) {
        state_drain_and_quit(self, el);
        if self.state.request_quit || self.ctx.is_none() {
            return;
        }
        // Per-frame clear of the Geometry-list row hover: the pane sets it
        // again below when the pointer is over a row, and anywhere else -
        // another page, pointer elsewhere - the highlight must not
        // linger. Same lifetime rule as `hover` in `update_hover`.
        self.state.geo_hover = None;

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

        let draft_key = draft_key(state);
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

/// Order-sensitive FNV-1a fold over a name list, seeded by its length: the
/// GPU rebuild gate needs a cheap "did this set change" key for the browser
/// visibility lists (they are user-sized; nothing hashes per frame
/// otherwise). The length goes into the seed so a same-fold alias across a
/// count change is impossible, and the 64-bit body makes an accidental
/// collision on lists of equal size vanishingly unlikely.
fn fold_names(list: &[String]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64 ^ ((list.len() as u64).wrapping_mul(0x517c_c1b7_2722_0a95));
    for name in list {
        for b in name.bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        h = (h ^ 0xff).wrapping_mul(0x0000_0100_0000_01b3); // separator
    }
    h
}

/// Flatten the world into GPU parts: models by name first, then the stock
/// solid (it renders translucent, so it must be submitted last). Browser-eye
/// hidden models are simply not uploaded: hiding is display, not deletion —
/// their facets still feed stock, paths and the ray below.
fn collect_parts(state: &State) -> Vec<GpuPart> {
    let mut parts: Vec<GpuPart> = model::model_entities(&state.world)
        .into_iter()
        .filter(|(name, _)| !state.hidden_models.iter().any(|h| h == name))
        .map(|(_, entity)| GpuPart {
            verts: world_verts(state, &entity),
            selected: model::is_selected(&state.world, &entity),
            stock: false,
            fill: false,
        })
        .collect();
    if let Some(entity) = model::find_by_name(&state.world, STOCK_NAME)
        && model::is_stock(&state.world, &entity)
    {
        parts.push(GpuPart {
            verts: world_verts(state, &entity),
            selected: model::is_selected(&state.world, &entity),
            stock: true,
            fill: false,
        });
    }
    // Cut-region fills: a translucent film over the material each shown
    // region covers, at the same level `region_lines` outlines it - the
    // same gate, so a fill never floats where no outline stands. A region
    // `triangles()` refuses (degenerate, too dense) stays wireframe-only:
    // never a partial or wrong sheet.
    for (z, region) in shown_regions(state) {
        let Some(region) = region else { continue };
        let Ok(tris) = region.triangles() else { continue };
        if tris.is_empty() {
            continue;
        }
        let mut verts = Vec::with_capacity(tris.len() * 3);
        for t in &tris {
            for p in t {
                verts.push([ftoz(p.x), ftoz(p.y), ftoz(z)]);
            }
        }
        parts.push(GpuPart {
            verts,
            selected: false,
            stock: false,
            fill: true,
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
/// Fold of every height a draft parameter stores, kind included (switching
/// kinds at coincident numbers still moves the key). FNV-style mix, our
/// own one-liner; the values are not hashed for equality, only churned.
fn fold_heights(params: &operation::OpParams) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    let mut eat = |v: u64| {
        h ^= v;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    eat(match params {
        operation::OpParams::Face { .. } => 0,
        operation::OpParams::Contour { .. } => 1,
        operation::OpParams::Pocket { .. } => 2,
        operation::OpParams::Slot { .. } => 3,
        operation::OpParams::Trace { .. } => 4,
    });
    // The reference itself, never the resolved z: a moved source changes
    // the planes through the document version (the other key half), and
    // two references that happen to resolve equal must still tell
    // drafts apart.
    let mut eat_h = |height: &operation::Height| {
        eat(height.reference.kind_index() as u64);
        eat(height.reference.wcs_slot() as u64);
        eat(height.offset.to_bits());
    };
    eat_h(&params.clearance());
    eat_h(&params.region_height());
    match params {
        operation::OpParams::Face { top_z, .. } => {
            if let Some(t) = top_z {
                eat_h(t);
            }
        }
        operation::OpParams::Contour { retract_z, bottom_z, .. } => {
            eat_h(retract_z);
            eat_h(bottom_z);
        }
        operation::OpParams::Pocket { bottom_z, .. }
        | operation::OpParams::Slot { bottom_z, .. }
        | operation::OpParams::Trace { bottom_z, .. } => eat_h(bottom_z),
    }
    h
}

/// The draft-state half of the overlay rebuild gate: everything
/// `collect_overlay` reads that no document version covers. The height
/// planes and their labels read the open operation's draft heights and
/// tool, so those join the key - a spinbox edit walks its plane live,
/// without touching the document.
fn draft_key(state: &State) -> [u64; 10] {
    [
        u64::from(state.wcs_ui.pick_anchor),
        state.wcs_ui.anchor_source as u64,
        state.wcs_ui.anchor_z as u64,
        state.wcs_ui.anchor_xy as u64,
        u64::from(state.show_world_axes),
        u64::from(state.show_all_paths),
        fold_names(&state.hidden_models),
        fold_names(&state.hidden_paths),
        fold_heights(&state.op_ui.params),
        fold_names(&[state.op_ui.name.clone(), state.op_ui.tool.clone()]),
    ]
}

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
    out.extend(toolpath_lines(state));
    out.extend(region_lines(state));
    out.extend(height_plane_lines(state));
    out.extend(selection_feature_lines(state));
    capped(out)
}

/// The selected operation's cutting region, in warning orange (our own
/// display choice): "what gets cut", outlined even before the operation
/// has ever been generated. The loops come straight from
/// [`operation::region_of`] and the open runs a Contour pick list adds
/// from [`operation::chains_of`] - both the generators' own construction,
/// one source each - so the outline cannot disagree with the path, and an
/// operation whose source vanished shows nothing rather than a stale
/// outline.
pub(crate) fn region_lines(state: &State) -> Vec<[f32; 6]> {
    let mut out = Vec::new();
    for (_, entity) in model::named_entities(&state.world) {
        if !model::is_operation(&state.world, &entity)
            || !model::is_selected(&state.world, &entity)
        {
            continue;
        }
        let Some(op) = model::operation_of(&state.world, &entity) else {
            continue;
        };
        let Some(z) = operation::region_z(&state.world, &op.params) else {
            continue; // the region plane's source is gone - draw nothing
                      // rather than an outline at a guessed level
        };
        if let Some(region) = operation::region_of(&state.world, &op) {
            for l in region.loops() {
                let pts = l.points();
                if pts.len() < 3 {
                    continue; // a closed ring needs three corners; never mod 0
                }
                for i in 0..pts.len() {
                    let a = Point3::new(pts[i].x, pts[i].y, z);
                    let q = pts[(i + 1) % pts.len()];
                    push_line(&mut out, a, Point3::new(q.x, q.y, z), REGION_RGB);
                }
            }
        }
        // Centerline runs: open polylines, no closing chord - the run's
        // ends are where the path stops, and the outline says so.
        for run in operation::chains_of(&state.world, &op) {
            for w in run.windows(2) {
                let a = Point3::new(w[0].x, w[0].y, z);
                let b = Point3::new(w[1].x, w[1].y, z);
                push_line(&mut out, a, b, REGION_RGB);
            }
        }
    }
    out
}

/// The region outline's orange, shared by `region_lines` and the height
/// planes so the "what gets cut" level reads as one thing.
const REGION_RGB: [f32; 3] = [0.95, 0.58, 0.15];

/// What `region_lines` actually paints, per SELECTED operation whose
/// source resolves geometry: the operation's committed `region_z` and its
/// region - `None` for an open-runs-only pick (a Contour chain has outline
/// but no area to fill). `height_planes` defers a region rectangle to
/// these levels and `collect_parts` fills these areas: the same gate the
/// outline runs under, no parallel truth, so a rectangle neither doubles a
/// standing outline nor vanishes because the panel disagrees with the
/// selection about who is shown - and a translucent fill never covers
/// where no outline stands.
pub(crate) fn shown_regions(state: &State) -> Vec<(f64, Option<Region>)> {
    let mut out = Vec::new();
    for (_, entity) in model::named_entities(&state.world) {
        if !model::is_operation(&state.world, &entity)
            || !model::is_selected(&state.world, &entity)
        {
            continue;
        }
        let Some(op) = model::operation_of(&state.world, &entity) else {
            continue;
        };
        let region = operation::region_of(&state.world, &op);
        if let Some(z) = operation::region_z(&state.world, &op.params)
            && (region.is_some() || !operation::chains_of(&state.world, &op).is_empty())
        {
            out.push((z, region));
        }
    }
    out
}

/// The levels of [`shown_regions`] - one truth, read at the height-plane
/// gate.
pub(crate) fn shown_region_levels(state: &State) -> Vec<f64> {
    shown_regions(state).into_iter().map(|(z, _)| z).collect()
}
/// Safety level (our own palette): the clearance plane everything starts
/// above, and - Contour only - the shorter retract between passes.
const CLEARANCE_RGB: [f32; 3] = [0.55, 0.60, 0.68];
const RETRACT_RGB: [f32; 3] = [0.40, 0.70, 0.95];
/// The depth the stock is cut to: red, unmistakably the low water mark.
const CUT_RGB: [f32; 3] = [0.90, 0.35, 0.30];

/// One horizontal level of the open operation's DRAFT parameters: a plane
/// to mark in the viewport, with the corner label it carries.
pub(crate) struct HeightPlane {
    pub z: f64,
    pub rgb: [f32; 3],
    pub name: &'static str,
    /// Which rectangle corner carries the label - 0 NE, 1 NW, 2 SE, 3 SW,
    /// distinct per kind so simultaneous labels never stack.
    pub corner: usize,
    /// The region plane's rectangle is dropped where `region_lines`
    /// already outlines the region at the same level: the outline IS the
    /// plane, and a coincident double would only thicken one line.
    pub skip_rect: bool,
}

/// The open operation's height planes, read from the draft parameters -
/// what the spinboxes point at right now, not what was last applied, so
/// spinning Clearance visibly walks the plane before Apply commits it.
/// No operation open (empty or unresolved name): no planes, ever.
pub(crate) fn height_planes(state: &State) -> Vec<HeightPlane> {
    if state.op_ui.name.is_empty() {
        return Vec::new();
    }
    let Some(entity) = model::find_by_name(&state.world, &state.op_ui.name) else {
        return Vec::new();
    };
    if model::operation_of(&state.world, &entity).is_none() {
        return Vec::new(); // a typed name is not an operation
    }
    let draft = &state.op_ui.params;
    // Where the orange outline actually stands, level by level: the
    // shared `shown_region_levels` gate, so `region_lines` and the
    // plane rectangles answer to one predicate, never two.
    let levels = shown_region_levels(state);
    let stands = |z: f64| levels.contains(&z);
    // Heights resolve against the world before they reach the viewport:
    // an unavailable source drops its plane (never a rectangle at a
    // guessed z) - the panel says why, and generation refuses by name.
    let world = &state.world;
    let mut out = Vec::new();
    if let Some(z) = draft.clearance().z(world) {
        out.push(HeightPlane {
            z,
            rgb: CLEARANCE_RGB,
            name: "Clearance",
            corner: 0,
            skip_rect: false,
        });
    }
    // Retract is the between-pass hop plane, every kind that stores one.
    // It draws at the effective hop - `min(retract, clearance)` of the
    // RESOLVED planes - because that is the Z the rapids actually travel
    // at, even when a legacy file parked retract above clearance. Kinds
    // whose retract is unset hop at clearance and get no extra plane: the
    // Clearance plane above already says where those rapids run.
    let hop = match draft {
        operation::OpParams::Contour { retract_z, .. } => Some(*retract_z),
        operation::OpParams::Pocket { retract_z, .. }
        | operation::OpParams::Slot { retract_z, .. }
        | operation::OpParams::Trace { retract_z, .. } => *retract_z,
        operation::OpParams::Face { .. } => None,
    };
    if let Some(hop) = hop
        && let (Some(r), Some(c)) = (hop.z(world), draft.clearance().z(world))
    {
        out.push(HeightPlane {
            z: r.min(c),
            rgb: RETRACT_RGB,
            name: "Retract",
            corner: 1,
            skip_rect: false,
        });
    }
    match draft {
        // For the depth kinds, Top is the section the paths run in and
        // Bottom the last step down. Face skims one plane unless it
        // stages: then its Top is the first level, drawn like the depth
        // kinds' Top, with the finished Cut below it.
        operation::OpParams::Face { top_z, cut_z, .. } => {
            let cut = cut_z.z(world);
            if let (Some(t), Some(c)) = ((*top_z).to_owned(), cut)
                && let Some(tz) = t.z(world)
                && tz != c
            {
                out.push(HeightPlane {
                    z: tz,
                    rgb: REGION_RGB,
                    name: "Top",
                    corner: 1,
                    skip_rect: stands(tz),
                });
            }
            if let Some(z) = cut {
                out.push(HeightPlane {
                    z,
                    rgb: CUT_RGB,
                    name: "Cut",
                    corner: 2,
                    // Stands only where no outline is painted AT the cut
                    // level: a staged Face's outline sits up at its Top,
                    // so the finished Cut keeps its own rectangle - the
                    // old blanket flag used to erase it.
                    skip_rect: stands(z),
                });
            }
        }
        operation::OpParams::Contour { top_z, bottom_z, .. }
        | operation::OpParams::Pocket { top_z, bottom_z, .. }
        | operation::OpParams::Slot { top_z, bottom_z, .. }
        | operation::OpParams::Trace { top_z, bottom_z, .. } => {
            if let Some(z) = top_z.z(world) {
                out.push(HeightPlane {
                    z,
                    rgb: REGION_RGB,
                    name: "Top",
                    corner: 2,
                    skip_rect: stands(z),
                });
            }
            if let Some(z) = bottom_z.z(world) {
                out.push(HeightPlane {
                    z,
                    rgb: CUT_RGB,
                    name: "Bottom",
                    corner: 3,
                    skip_rect: false,
                });
            }
        }
    }
    out.retain(|p| p.z.is_finite());
    out
}

/// The planes' XY reach: the stock box (the model bounds when there is no
/// stock), padded outward past the tool so a rectangle always sits clear
/// of the material. Nothing resolvable, or a non-finite pad, leaves the
/// planes unplaced - never a rectangle of nonsense extent.
fn plane_rect(state: &State) -> Option<[f64; 4]> {
    let b = model::stock_world_aabb(&state.world).or_else(|| model::solid_world_aabb(&state.world))?;
    let pad = model::find_by_name(&state.world, &state.op_ui.tool)
        .and_then(|e| model::tool_of(&state.world, &e))
        .map(|t| t.diameter)
        .filter(|d| d.is_finite() && *d > 0.0)
        .map_or(0.0, |d| d * 0.5 + 2.0);
    let rect = [b.min.x - pad, b.min.y - pad, b.max.x + pad, b.max.y + pad];
    rect.iter()
        .all(|v| v.is_finite())
        .then_some(rect)
}

fn plane_corner(rect: [f64; 4], corner: usize) -> (f64, f64) {
    let [x0, y0, x1, y1] = rect;
    match corner {
        0 => (x1, y1),
        1 => (x0, y1),
        2 => (x1, y0),
        _ => (x0, y0),
    }
}

/// The height planes as overlay geometry: one rectangle each, at the
/// plane's level. Under the same document/draft gates as the rest of
/// `collect_overlay` - `draft_key` folds the draft heights, so a spinbox
/// edit walks its plane without touching the document.
pub(crate) fn height_plane_lines(state: &State) -> Vec<[f32; 6]> {
    let planes = height_planes(state);
    if planes.iter().all(|p| p.skip_rect) {
        return Vec::new();
    }
    let Some(rect) = plane_rect(state) else {
        return Vec::new();
    };
    let [x0, y0, x1, y1] = rect;
    let ring = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    let mut out = Vec::new();
    for p in planes.iter().filter(|p| !p.skip_rect) {
        for i in 0..4 {
            let a = ring[i];
            let b = ring[(i + 1) % 4];
            push_line(
                &mut out,
                Point3::new(a[0], a[1], p.z),
                Point3::new(b[0], b[1], p.z),
                p.rgb,
            );
        }
    }
    out
}

/// The open operation's resolved pick list in 3D: each picked face's
/// boundary loops and each picked chain's polyline, drawn where the
/// feature actually is - violet, deliberately not region orange. Orange
/// answers "what the path cuts, at the cut plane" (see `region_lines`);
/// violet answers "what you picked, where it is" - the two disagree
/// honestly (a face picked low on a wall cuts at its own height, a chain
/// runs in the solid's skin, not at some level's offset). The hover
/// highlight is screen-space and evaporates under camera moves; this is
/// overlay-buffer geometry, rebuilt under the same document and
/// selection gates as the rest of `collect_overlay`, so picks hold their
/// place while orbiting. A vanished pick - hidden model, deleted entity,
/// stale id - draws nothing rather than a ghost, the hover rule again.
pub(crate) fn selection_feature_lines(state: &State) -> Vec<[f32; 6]> {
    const PICK_RGB: [f32; 3] = [0.72, 0.44, 0.86];
    if state.op_ui.name.is_empty() {
        return Vec::new(); // nothing open: nothing picked to pin down
    }
    let Some(op) = model::find_by_name(&state.world, &state.op_ui.name)
        .and_then(|e| model::operation_of(&state.world, &e))
    else {
        return Vec::new();
    };
    let operation::SrcRef::Selection(items) = &op.src else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in items {
        if state.hidden_models.iter().any(|m| m == &item.entity) {
            continue; // display-off models are transparent to picking and drawing
        }
        let Some(entity) = model::find_by_name(&state.world, &item.entity) else {
            continue;
        };
        let Some(features) = model::features_of(&state.world, &entity) else {
            continue;
        };
        let loc = model::transform_of(&state.world, &entity).unwrap_or_default();
        let draw = |out: &mut Vec<[f32; 6]>, pts: &[[f64; 3]], closed: bool| {
            let n = pts.len();
            if n < 2 || (closed && n < 3) {
                return; // never mod 0; a ring needs three corners
            }
            let last = if closed { n } else { n - 1 };
            for i in 0..last {
                let a = transform_point(Point3::new(pts[i][0], pts[i][1], pts[i][2]), loc.pos, loc.rot);
                let q = &pts[(i + 1) % n];
                let b = transform_point(Point3::new(q[0], q[1], q[2]), loc.pos, loc.rot);
                push_line(out, a, b, PICK_RGB);
            }
        };
        match item.feature {
            FeatureSel::Face(fid) => {
                let Some(face) = features.faces.iter().find(|f| f.id == fid) else {
                    continue; // stale id: the model re-meshed since the pick
                };
                for l in &face.loops {
                    let pts: Vec<[f64; 3]> = l
                        .verts
                        .iter()
                        .filter_map(|&vi| features.verts.get(vi as usize).copied())
                        .collect();
                    draw(&mut out, &pts, !matches!(l.kind, feature::LoopKind::Open));
                }
            }
            FeatureSel::Chain(cid) => {
                let Some(chain) =
                    feature::chains(&features).into_iter().find(|c| c.id == cid)
                else {
                    continue;
                };
                let pts = feature::chain_points(&features, &chain);
                draw(&mut out, &pts, chain.closed);
            }
            FeatureSel::Patch(pid) => {
                // The weld's outline, not the seams inside it: the seam
                // edges are exactly what the patch dissolved, and
                // drawing them would advertise strips the pick is not.
                let Some(rings) = feature::patch_rings(&features, pid) else {
                    continue; // stale id: the model re-meshed since the pick
                };
                for r in &rings {
                    let pts: Vec<[f64; 3]> = r
                        .verts
                        .iter()
                        .filter_map(|&vi| features.verts.get(vi as usize).copied())
                        .collect();
                    draw(&mut out, &pts, r.closed);
                }
            }
        }
    }
    out
}

/// Screen-space segments of the FEATURE under the cursor - in the viewport,
/// or on a Geometry-list row (`state.geo_hover`) - as viewport-uv
/// pairs (0..1 within the image, like `overlay_labels` - the shell maps
/// them through the image rect and draws with the window draw list). A
/// draw-list producer on purpose: hovering must never rebuild the 3D
/// overlay buffer, and the projection tracks camera moves without a new
/// raycast. A face lights its boundary loops, a chain its polyline; the
/// per-segment projection drops any part behind the camera rather than
/// the whole feature, so a ring half off-screen still draws honestly.
/// Gone entities, hidden models, deleted features and stale ids draw
/// nothing - a highlight must never promise a pick the click cannot make.
pub(crate) fn hover_features(state: &State, aspect: f64) -> Vec<([f32; 2], [f32; 2])> {
    let mut out = Vec::new();
    let mut viewport: Option<(String, FeatureSel)> = None;
    if let Some(hit) = &state.hover
        && let Some(sel) = hit.feature
    {
        feature_segments(state, &hit.entity, sel, aspect, &mut out);
        viewport = Some((hit.entity.clone(), sel));
    }
    // The Geometry list's row hover lights the same feature the same way:
    // the list answering "which one is that?" without a click. Viewport
    // hover keeps precedence, and one feature never draws twice.
    if let Some(g) = &state.geo_hover
        && !viewport.as_ref().is_some_and(|(e, f)| *e == g.entity && *f == g.feature)
    {
        feature_segments(state, &g.entity, g.feature, aspect, &mut out);
    }
    out
}

/// One (entity, feature) resolved to viewport-uv segments: gone entities,
/// hidden models, deleted features and stale ids draw nothing.
fn feature_segments(
    state: &State,
    entity: &str,
    sel: FeatureSel,
    aspect: f64,
    out: &mut Vec<([f32; 2], [f32; 2])>,
) {
    if state.hidden_models.iter().any(|m| m == entity) {
        return;
    }
    let Some(found) = model::find_by_name(&state.world, entity) else {
        return;
    };
    let Some(features) = model::features_of(&state.world, &found) else {
        return;
    };
    let loc = model::transform_of(&state.world, &found).unwrap_or_default();
    let mvp = state.cam.mvp(aspect);
    match sel {
        FeatureSel::Face(fid) => {
            let Some(face) = features.faces.iter().find(|f| f.id == fid) else {
                return; // stale id: the model re-meshed under the hover
            };
            for l in &face.loops {
                let pts: Vec<[f64; 3]> = l
                    .verts
                    .iter()
                    .filter_map(|&vi| features.verts.get(vi as usize).copied())
                    .collect();
                push_poly(&mvp, &loc, &pts, !matches!(l.kind, feature::LoopKind::Open), out);
            }
        }
        FeatureSel::Chain(cid) => {
            let Some(chain) = feature::chains(&features).into_iter().find(|c| c.id == cid) else {
                return;
            };
            let pts = feature::chain_points(&features, &chain);
            push_poly(&mvp, &loc, &pts, chain.closed, out);
        }
        FeatureSel::Patch(pid) => {
            let Some(rings) = feature::patch_rings(&features, pid) else {
                return; // stale id: the model re-meshed under the hover
            };
            for r in &rings {
                let pts: Vec<[f64; 3]> = r
                    .verts
                    .iter()
                    .filter_map(|&vi| features.verts.get(vi as usize).copied())
                    .collect();
                push_poly(&mvp, &loc, &pts, r.closed, out);
            }
        }
    }
}

/// Project a local-space polyline (feature vertices, entity transform
/// applied) to viewport-uv segments, closing the ring when it is a loop.
/// A vertex behind the camera drops only the segments that touch it.
fn push_poly(
    mvp: &[[f32; 4]; 4],
    loc: &hsmworks_app::schema::Location,
    pts: &[[f64; 3]],
    closed: bool,
    out: &mut Vec<([f32; 2], [f32; 2])>,
) {
    let uv = |p: &[f64; 3]| {
        let w = transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
        project_uv(mvp, [w.x, w.y, w.z])
    };
    let n = pts.len();
    if n < 2 {
        return;
    }
    let last = if closed { n } else { n - 1 };
    for i in 0..last {
        let (a, b) = match (uv(&pts[i]), uv(&pts[(i + 1) % n])) {
            (Some(a), Some(b)) => (a, b),
            _ => continue,
        };
        if a != b {
            out.push((a, b));
        }
    }
}

/// The face feature a raycast facet belongs to, through the model's
/// facet-to-face map. A strip of a welded wall promotes to its patch:
/// the wall tessellates into strips only because it curves, and the
/// user who picks "the wall" must get the wall, not the one strip under
/// the cursor (M1.5.2 issue: "selects a facet - we need welded faces").
/// Flat faces are their own logical unit and stay a plain Face - the
/// promotion cannot re-key what was never split. None when the entity
/// or its features are gone.
fn feature_of_facet(state: &State, entity: &str, facet: usize) -> Option<FeatureSel> {
    let found = model::find_by_name(&state.world, entity)?;
    let features = model::features_of(&state.world, &found)?;
    let fi = *features.facet_face.get(facet)?;
    let fid = features.faces.get(fi as usize)?.id;
    Some(
        feature::patches(&features)
            .into_iter()
            .find(|p| p.faces.contains(&fid))
            .map_or(FeatureSel::Face(fid), |p| FeatureSel::Patch(p.id)),
    )
}

/// Fold one feature pick into the open operation's geometry list and
/// queue it: plain click replaces, Ctrl/Shift adds, a repeat of what is
/// already selected is refused as a no-op (the command would dedupe it
/// anyway - saying so is the honest feedback). The entity name rides along
/// in the pick, so selecting the stock's top face needs no tree selection.
/// A pure State write: it queues, the command validates, undo is the
/// history's job. The Geometry section's chip buttons queue the same
/// commands from the other end - viewport and chips are two entrances to
/// one list, pinned together in `tests/operations.rs`.
pub fn queue_feature_pick(state: &mut State, pick: GeoSel, append: bool) {
    let name = state.op_ui.name.clone();
    if name.is_empty() {
        state.status = String::from("open an operation first - geometry picks fill the open one");
        return;
    }
    let base: Vec<GeoSel> = if append {
        model::find_by_name(&state.world, &name)
            .and_then(|e| model::operation_of(&state.world, &e))
            .map_or(Vec::new(), |op| match op.src {
                operation::SrcRef::Selection(items) => items,
                _ => Vec::new(),
            })
    } else {
        Vec::new()
    };
    if base.iter().any(|g| g.entity == pick.entity && g.feature == pick.feature) {
        state.status = format!("{} of '{}' is already selected", pick.feature, pick.entity);
        return;
    }
    let mut items = base;
    items.push(pick.clone());
    state.status = format!("{} of '{}' added to {name}", pick.feature, pick.entity);
    state.pending.push_back(CommandKind::SetOperationGeometry { name, items });
}

/// How close a feature edge must draw to the cursor to be pickable, in
/// logical px - the same tolerance the anchor markers use.
const PICK_EDGE_PX: f32 = 12.0;

/// The feature edge nearest a logical-px point, within `radius` px:
/// (entity, edge index). Screen-space by design - the raycast Face pick
/// cannot see the silhouette, the edge the user actually aims at. Every
/// visible model's feature edges are projected through the camera and
/// measured against the point; hidden models drop out, stock is included
/// (its top edges are pickable geometry like any model's). Iteration runs
/// in name order (`model_entities` sorts), so the nearest wins
/// deterministically and a tie goes to the earlier name.
pub(crate) fn edge_nearest(state: &State, p: [f32; 2], radius: f32) -> Option<(String, u32)> {
    let ([x0, y0], [x1, y1]) = state.viewport_rect;
    let (w, h) = (x1 - x0, y1 - y0);
    if w <= 1.0 || h <= 1.0 {
        return None; // no laid-out image: nothing to project into
    }
    let mvp = state.cam.mvp(f64::from(w / h));
    let mut entities: Vec<(String, flecs_ecs::prelude::Entity)> = model::model_entities(&state.world)
        .into_iter()
        .filter(|(name, _)| !state.hidden_models.iter().any(|hid| hid == name))
        .collect();
    if let Some(stock) = model::find_by_name(&state.world, STOCK_NAME) {
        entities.push((STOCK_NAME.to_string(), stock));
    }
    let to_px = |uv: [f32; 2]| [x0 + uv[0] * w, y0 + uv[1] * h];
    let mut best: Option<(f32, String, u32)> = None;
    for (name, entity) in entities {
        let Some(features) = model::features_of(&state.world, &entity) else {
            continue;
        };
        let loc = model::transform_of(&state.world, &entity).unwrap_or_default();
        for (i, e) in features.edges.iter().enumerate() {
            let (Some(a), Some(b)) = (features.verts.get(e.a as usize), features.verts.get(e.b as usize)) else {
                continue;
            };
            let proj = |v: &[f64; 3]| {
                let w = transform_point(Point3::new(v[0], v[1], v[2]), loc.pos, loc.rot);
                project_uv(&mvp, [w.x, w.y, w.z]).map(to_px)
            };
            let (Some(a), Some(b)) = (proj(a), proj(b)) else {
                continue; // behind the camera: not what is on screen
            };
            let d = point_seg_dist_px(p, a, b);
            if d <= radius && best.as_ref().is_none_or(|&(bd, _, _)| d < bd) {
                best = Some((d, name.clone(), i as u32));
            }
        }
    }
    best.map(|(_, name, edge)| (name, edge))
}

/// Distance from a point to a segment, px. Clamped-parameter form: no
/// division, no NaN through a zero-length segment (a weld-degenerate edge
/// then reads as the distance to its one endpoint).
fn point_seg_dist_px(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (abx, aby) = (b[0] - a[0], b[1] - a[1]);
    let len2 = abx * abx + aby * aby;
    let t = if len2 > 0.0 {
        ((( p[0] - a[0]) * abx + (p[1] - a[1]) * aby) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (qx, qy) = (a[0] + t * abx - p[0], a[1] + t * aby - p[1]);
    (qx * qx + qy * qy).sqrt()
}

/// Overlay palette for cached paths: feed green, rapid blue, plunge red.
/// Provenance: our own display choice; the three-way move vocabulary
/// follows the RS-274 motion words, not any vendor's screen.
const FEED_RGB: [f32; 3] = [0.20, 0.85, 0.35];
const RAPID_RGB: [f32; 3] = [0.30, 0.50, 0.95];
const PLUNGE_RGB: [f32; 3] = [0.92, 0.30, 0.25];
/// Unselected operations under "Show all toolpaths" draw behind the
/// selected one.
const OTHERS_DIM: f32 = 0.45;
/// One direction chevron per this much feed travel.
const CHEVRON_SPACING: f64 = 10.0;
/// Vertex budget for the overlay buffer (24 bytes each, ~25 MB). Past it
/// the line list is truncated with a logged diagnostic - never silently,
/// and never by refusing to draw at all.
const MAX_OVERLAY_VERTICES: usize = 1 << 20;
/// Sagitta bound for drawing arc moves: arcs flatten through the same
/// law the verifier and the golden dump use, at a display tolerance
/// fine enough that zoomed-in curvature still reads as a curve.
const DISPLAY_CHORD_TOL: f64 = 0.02;

/// One overlay line: two `[x, y, z, r, g, b]` vertices.
fn push_line(out: &mut Vec<[f32; 6]>, a: Point3, b: Point3, c: [f32; 3]) {
    out.push([ftoz(a.x), ftoz(a.y), ftoz(a.z), c[0], c[1], c[2]]);
    out.push([ftoz(b.x), ftoz(b.y), ftoz(b.z), c[0], c[1], c[2]]);
}

/// The cached CL paths as overlay segments: straight moves as one line in
/// their kind color with direction chevrons along feed travel, arc moves
/// as their chord runs. The selected
/// operation always draws bright; other operations show dimmed only under
/// "Show all toolpaths", and never at all when hidden by its browser eye.
/// Staleness is not a display state here - what is drawn is the cache
/// itself, and the tree's STALE badge is where a stale path is called
/// out.
pub(crate) fn toolpath_lines(state: &State) -> Vec<[f32; 6]> {
    let mut out = Vec::new();
    let arm = chevron_arm(state);
    for (name, entity) in model::named_entities(&state.world) {
        // Browser-eye hidden: the cache still exists and still posts, it
        // just does not draw (display state, never a geometry change).
        if state.hidden_paths.contains(&name) {
            continue;
        }
        let Some(cache) = model::toolpath_of(&state.world, &entity) else {
            continue;
        };
        let selected = model::is_selected(&state.world, &entity);
        if !selected && !state.show_all_paths {
            continue;
        }
        let dim = if selected { 1.0 } else { OTHERS_DIM };
        for m in &cache.path.moves {
            let base = match m.kind {
                MoveKind::Rapid => RAPID_RGB,
                MoveKind::Feed => FEED_RGB,
                MoveKind::Plunge => PLUNGE_RGB,
            };
            let c = [base[0] * dim, base[1] * dim, base[2] * dim];
            if m.arc.is_some() {
                // Arcs draw as chords (one law, one tolerance story) and
                // take no chevrons: the curve itself reads as direction.
                let cs = m.chords(DISPLAY_CHORD_TOL);
                if out.len() + 2 * cs.len() + 2 > MAX_OVERLAY_VERTICES {
                    eprintln!(
                        "overlay: the cached paths outgrew the {MAX_OVERLAY_VERTICES}-vertex \
                         display budget; the rest is truncated (the full path still posts)"
                    );
                    out.truncate(out.len() & !1);
                    return out;
                }
                for (a, b) in cs {
                    let za = m.from.z;
                    push_line(
                        &mut out,
                        Point3::new(a.x, a.y, za),
                        Point3::new(b.x, b.y, za),
                        c,
                    );
                }
                continue;
            }
            if out.len() + 10 > MAX_OVERLAY_VERTICES {
                // A pathological cache (millions of file-loaded moves)
                // must not allocate millions of segments either.
                eprintln!(
                    "overlay: the cached paths outgrew the {MAX_OVERLAY_VERTICES}-vertex \
                     display budget; the rest is truncated (the full path still posts)"
                );
                out.truncate(out.len() & !1);
                return out;
            }
            push_line(&mut out, m.from, m.to, c);
            if m.kind == MoveKind::Feed {
                chevrons(&mut out, m.from, m.to, c, arm);
            }
        }
    }
    out
}

/// Chevron size follows the document: 1% of the solid extent, floored so
/// a small part still reads direction.
fn chevron_arm(state: &State) -> f64 {
    let extent = model::solid_world_aabb(&state.world)
        .map(|b| {
            (b.max.x - b.min.x)
                .max(b.max.y - b.min.y)
                .max(b.max.z - b.min.z)
        })
        .unwrap_or(0.0);
    (extent * 0.01).max(0.6)
}

/// Two trailing wings at each mark: the apex sits on the path and the
/// wings sweep backwards, so the pair reads as motion, not a diamond.
/// A feed too short to hold legible wings gets no chevrons at all.
fn chevrons(out: &mut Vec<[f32; 6]>, from: Point3, to: Point3, c: [f32; 3], arm: f64) {
    let d = to - from;
    let len2 = d.dot(d);
    let min2 = (2.0 * arm) * (2.0 * arm);
    if !len2.is_finite() || len2 <= min2 {
        return;
    }
    let len = len2.sqrt();
    let dir = Point3::new(d.x / len, d.y / len, d.z / len);
    // Any perpendicular works for the wing spread; a steep feed can't use
    // Z as the helper, so it takes X.
    let refv = if dir.z.abs() > 0.99 {
        Point3::new(1.0, 0.0, 0.0)
    } else {
        Point3::new(0.0, 0.0, 1.0)
    };
    let n0 = refv.cross(dir);
    let n2 = n0.dot(n0);
    if !n2.is_finite() || n2 <= f64::EPSILON {
        return;
    }
    let nl = n2.sqrt();
    let n = Point3::new(n0.x / nl, n0.y / nl, n0.z / nl);
    let count = ((len / CHEVRON_SPACING).floor().max(1.0) as usize).min(10_000);
    let (back, spread) = (arm * 0.8, arm * 0.45);
    for i in 0..count {
        let t = len * (i as f64 + 0.5) / count as f64;
        let q = Point3::new(
            from.x + dir.x * t,
            from.y + dir.y * t,
            from.z + dir.z * t,
        );
        for s in [-1.0, 1.0] {
            let tip = Point3::new(
                q.x - dir.x * back + n.x * spread * s,
                q.y - dir.y * back + n.y * spread * s,
                q.z - dir.z * back + n.z * spread * s,
            );
            push_line(out, q, tip, c);
        }
    }
}

/// The whole overlay's hard ceiling, enforced at the one place every line
/// producer passes through: truncate on even pairs (never half a line)
/// and say so in the log.
fn capped(mut lines: Vec<[f32; 6]>) -> Vec<[f32; 6]> {
    if lines.len() > MAX_OVERLAY_VERTICES {
        eprintln!(
            "overlay: {} vertices exceed the display budget; the rest of this frame's \
             lines are dropped",
            lines.len()
        );
        lines.truncate(MAX_OVERLAY_VERTICES & !1);
    }
    lines
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
pub(crate) fn overlay_labels(state: &State, aspect: f64) -> Vec<([f32; 2], [f32; 3], String)> {
    let len = gizmo_len(state);
    let mvp = state.cam.mvp(aspect);
    let mut out = Vec::new();
    // Corner tags for the height planes - "Operation1 · Clearance 25.00"
    // - one per plane, including the region plane when the orange outline
    // carries it. The operation's name rides along: with two ops open in
    // one session the plane set must say whose heights these are. Screen
    // space like the axis letters, so they track the camera for free and
    // never rebuild the 3D buffer. A corner that projects outside the
    // frame (a tight zoom leaves the padded rectangle's far corners)
    // clamps to a margin instead of dragging its label off-screen: the
    // plane's level and value stay readable, and `project_uv` itself is
    // untouched - picking shares it and must keep true positions.
    let planes = height_planes(state);
    if !planes.is_empty()
        && let Some(rect) = plane_rect(state)
    {
        for p in &planes {
            let (cx, cy) = plane_corner(rect, p.corner);
            if let Some(uv) = project_uv(&mvp, [cx, cy, p.z]) {
                const M: f32 = 0.02; // uv margin: pixels, whichever way the viewport measures
                let uv = [uv[0].clamp(M, 1.0 - M), uv[1].clamp(M, 1.0 - M)];
                out.push((
                    uv,
                    p.rgb,
                    format!("{} · {} {:.2}", state.op_ui.name, p.name, p.z),
                ));
            }
        }
    }
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
            out.push((uv, c, AXIS_NAMES[axis].to_string()));
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
                        } else {
                            // Free pointer motion: refresh the hover facet
                            // (the screen-space edge highlight; the 3D
                            // overlay deliberately does not rebuild here).
                            self.update_hover();
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
            // winit 0.30 delivers modifier state as its own events (the
            // MouseInput variant carries none), so this is the single feed
            // for Ctrl/Alt at pick time.
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
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

#[cfg(test)]
mod path_tests {
    //! The display half of M1.3: a generated path shows as colored lines
    //! with direction chevrons, and the toggle says how much of that is on
    //! screen. The tool/operation commands run straight off the history -
    //! their ribbon buttons (M1.4) are elsewhere; the producer feeding
    //! the overlay buffer is what these pin down.

    use super::*;
    use hsmworks_app::commands::{AddTool, CreateOperation, GenerateOperation, ImportModel};
    use hsmworks_app::geometry::cube;
    use hsmworks_app::operation::{Height, OpParams, SrcRef};
    use hsmworks_app::stock::StockShape;
    use hsmworks_app::tools::catalog;

    fn state_with_generated_path() -> State {
        let mut state = State::new();
        state.pending.push_back(CommandKind::SetStock {
            shape: StockShape::Box {
                dx: 60.0,
                dy: 60.0,
                dz: 20.0,
            },
            pos: [0.0, 0.0, 0.0],
            yaw_deg: 0.0,
        });
        state.drain_actions();
        let em = catalog()
            .into_iter()
            .find(|(n, _)| *n == "EM-0400-2FL")
            .map(|(_, t)| t)
            .expect("catalog entry");
        state
            .history
            .execute(&state.world, AddTool::new(&state.world, "EM-0400-2FL", em).expect("tool"));
        state.history.execute(
            &state.world,
            CreateOperation::new(
                &state.world,
                "Face1",
                "EM-0400-2FL",
                SrcRef::Stock,
                OpParams::Face {
                    stepover: 3.0,
                    clearance_z: Height::fixed(25.0),
                    cut_z: Height::fixed(19.5),
                    top_z: None,
                    stepdown: None,
                    feed: 1200.0,
                    plunge_feed: 300.0,
                    climb: true,
                },
            )
            .expect("operation"),
        );
        state.history.execute(
            &state.world,
            GenerateOperation::new(&state.world, "Face1").expect("generate"),
        );
        state
    }

    fn select(state: &mut State, name: Option<&str>) {
        state.pending.push_back(CommandKind::Select(
            name.map(|n| n.to_string()),
        ));
        state.drain_actions();
    }

    #[test]
    fn nothing_shows_until_the_op_is_selected() {
        let state = state_with_generated_path();
        assert!(toolpath_lines(&state).is_empty(), "an unselected path stays hidden by default");
    }

    #[test]
    fn selecting_the_operation_brings_its_path_up() {
        let mut state = state_with_generated_path();
        select(&mut state, Some("Face1"));
        let lines = toolpath_lines(&state);
        assert!(!lines.is_empty());
        let cache = model::find_by_name(&state.world, "Face1")
            .and_then(|e| model::toolpath_of(&state.world, &e))
            .expect("cache");
        assert!(
            lines.len() > cache.path.moves.len() * 2,
            "chevrons ride on top of the move lines"
        );
        // One vertex pair per move minimum: every move's own segment made it.
        assert!(lines.iter().all(|v| v[3] <= 1.0 && v[4] <= 1.0 && v[5] <= 1.0));
        let green = lines.iter().any(|v| v[3..6] == FEED_RGB);
        let blue = lines.iter().any(|v| v[3..6] == RAPID_RGB);
        let red = lines.iter().any(|v| v[3..6] == PLUNGE_RGB);
        assert!(green && blue && red, "feed, rapid and plunge each show: {green} {blue} {red}");
    }

    #[test]
    fn deselecting_hides_the_path_again() {
        let mut state = state_with_generated_path();
        select(&mut state, Some("Face1"));
        assert!(!toolpath_lines(&state).is_empty());
        select(&mut state, None);
        assert!(toolpath_lines(&state).is_empty(), "no lingering path after deselect");
    }

    #[test]
    fn show_all_dims_instead_of_hiding() {
        let mut state = state_with_generated_path();
        state.show_all_paths = true;
        let lines = toolpath_lines(&state);
        assert!(!lines.is_empty(), "the toggle draws unselected paths");
        assert!(
            lines.iter().all(|v| v[3] < 0.5 && v[4] < 0.5 && v[5] < 0.5),
            "everything behind the selection is dimmed"
        );
        select(&mut state, Some("Face1"));
        let bright = toolpath_lines(&state)
            .iter()
            .any(|v| v[3..6] == FEED_RGB || v[3..6] == RAPID_RGB || v[3..6] == PLUNGE_RGB);
        assert!(bright, "the selected one draws at full strength");
    }

    #[test]
    fn collect_overlay_adds_the_path_after_the_furniture() {
        let mut state = state_with_generated_path();
        select(&mut state, Some("Face1"));
        let all = collect_overlay(&state);
        let paths = toolpath_lines(&state);
        // Step 5's region preview joins the tail; the invariant (furniture,
        // then path) is asserted positionally, not by totals.
        assert_eq!(all.len(), 6 + paths.len() + region_lines(&state).len());
        assert_eq!(&all[6..6 + paths.len()], &paths[..], "axes first, then the path");
    }

    #[test]
    fn arc_moves_draw_curved() {
        // Cylinder stock, contour at a tolerance the tessellated wall
        // satisfies: the cache holds arc moves, and the overlay must
        // draw each as a run of chords, not one straight chord.
        let mut state = State::new();
        state.pending.push_back(CommandKind::SetStock {
            shape: StockShape::Cylinder {
                diameter: 30.0,
                height: 10.0,
            },
            pos: [0.0, 0.0, 0.0],
            yaw_deg: 0.0,
        });
        state.drain_actions();
        let em = catalog()
            .into_iter()
            .find(|(n, _)| *n == "EM-0400-2FL")
            .map(|(_, t)| t)
            .expect("catalog entry");
        state
            .history
            .execute(&state.world, AddTool::new(&state.world, "EM-0400-2FL", em).expect("tool"));
        state.history.execute(
            &state.world,
            CreateOperation::new(
                &state.world,
                "Skin1",
                "EM-0400-2FL",
                SrcRef::Stock,
                OpParams::Contour {
                    stock_to_leave: 0.0,
                    clearance_z: Height::fixed(20.0),
                    retract_z: Height::fixed(15.0),
                    top_z: Height::fixed(10.0),
                    bottom_z: Height::fixed(8.0),
                    stepdown: 2.0,
                    feed: 800.0,
                    plunge_feed: 200.0,
                    climb: true,
                    lead: hsmworks_app::machining2d::Lead::None,
                    tolerance: 0.1,
                    tab_count: 0,
                    tab_width: 0.0,
                    tab_height: 0.0,
                },
            )
            .expect("operation"),
        );
        state.history.execute(
            &state.world,
            GenerateOperation::new(&state.world, "Skin1").expect("generate"),
        );
        select(&mut state, Some("Skin1"));
        let cache = model::find_by_name(&state.world, "Skin1")
            .and_then(|e| model::toolpath_of(&state.world, &e))
            .expect("cache");
        assert!(cache.path.has_arcs(), "the tessellated wall fits to arcs at tol 0.1");

        let lines = toolpath_lines(&state);
        // At the display chord tolerance a half-circle arc fans into a
        // couple dozen chords; a straight-chord draw would emit exactly
        // one line (two vertices) per arc move, and arcs carry no
        // chevrons to pad the count with.
        let arcs = cache.path.arc_count().max(1);
        let feed = lines.iter().filter(|v| v[3..6] == FEED_RGB).count();
        assert!(
            feed > 8 * 2 * arcs,
            "{feed} feed vertices for {arcs} arc moves: arcs must draw as chord runs"
        );
    }

    #[test]
    fn the_cap_truncates_on_whole_lines() {
        let over = vec![[0.0f32; 6]; MAX_OVERLAY_VERTICES + 3];
        let out = capped(over);
        assert_eq!(out.len(), MAX_OVERLAY_VERTICES & !1);
        assert_eq!(out.len() % 2, 0, "never half a line");
    }

    #[test]
    fn degenerate_feeds_get_no_chevrons() {
        // A zero-length feed is the pathological cache entry; it must draw
        // its (zero-length) segment and nothing else - no NaN marker.
        let mut out = Vec::new();
        let p = Point3::new(1.0, 2.0, 3.0);
        chevrons(&mut out, p, p, FEED_RGB, 2.0);
        assert!(out.is_empty());
        let mut out = Vec::new();
        chevrons(&mut out, p, Point3::new(1.0, 2.0, 3.5), FEED_RGB, 2.0);
        assert!(out.is_empty(), "a feed shorter than two chevron arms stays plain");
    }

    #[test]
    fn the_eye_hides_a_path_without_touching_the_cache() {
        let mut state = state_with_generated_path();
        select(&mut state, Some("Face1"));
        assert!(!toolpath_lines(&state).is_empty());
        State::toggle_visibility(&mut state.hidden_paths, "Face1");
        assert!(toolpath_lines(&state).is_empty(), "hidden: nothing draws");
        let cache = model::find_by_name(&state.world, "Face1")
            .and_then(|e| model::toolpath_of(&state.world, &e))
            .expect("hiding is display: the path still exists");
        assert!(!cache.path.moves.is_empty());
        State::toggle_visibility(&mut state.hidden_paths, "Face1");
        assert!(!toolpath_lines(&state).is_empty(), "the second click shows it again");
    }

    #[test]
    fn hiding_a_model_undraws_only_its_triangles() {
        let mut state = state_with_generated_path();
        state.history.execute(
            &state.world,
            ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 10.0))),
        );
        let with = collect_parts(&state);
        State::toggle_visibility(&mut state.hidden_models, "part");
        let hidden = collect_parts(&state);
        assert_eq!(with.len(), hidden.len() + 1, "just the model leaves the buffer");
        assert_eq!(model::model_entities(&state.world).len(), 1, "the entity is not gone");
    }

    #[test]
    fn the_visibility_fold_follows_the_lists() {
        // The rebuild gate is the only consumer: a stale key would leave a
        // toggle visually dead for the session, so the fold must separate
        // every set difference, including reordering and regrouping.
        let l = |names: &[&str]| -> Vec<String> { names.iter().map(|s| s.to_string()).collect() };
        assert_eq!(fold_names(&l(&[])), fold_names(&l(&[])));
        assert_ne!(fold_names(&l(&[])), fold_names(&l(&["x"])));
        assert_ne!(fold_names(&l(&["a"])), fold_names(&l(&["ab"])));
        assert_ne!(fold_names(&l(&["a"])), fold_names(&l(&["a", ""])));
        assert_ne!(fold_names(&l(&["a", "b"])), fold_names(&l(&["ab"])));
        assert_ne!(fold_names(&l(&["a", "b"])), fold_names(&l(&["b", "a"])));
    }
}

#[cfg(test)]
mod preview_tests {
    //! Step 5's "what gets cut": the selected operation's region loops in
    //! the 3D overlay, and the hovered facet's edges as screen-space
    //! segments. Both are pure world reads; pinned down headless.

    use super::*;
    use hsmworks_app::commands::ImportModel;
    use hsmworks_app::geometry::cube;
    use hsmworks_app::operation::{Height, OpParams, SrcRef};
    use hsmworks_app::stock::StockShape;
    use hsmworks_app::tools::catalog;

    fn face_state() -> State {
        let mut state = State::new();
        let em = catalog()
            .into_iter()
            .find(|(n, _)| *n == "EM-0400-2FL")
            .map(|(_, t)| t)
            .expect("catalog entry");
        state.pending.extend([
            CommandKind::SetStock {
                shape: StockShape::Box {
                    dx: 60.0,
                    dy: 60.0,
                    dz: 20.0,
                },
                pos: [0.0, 0.0, 0.0],
                yaw_deg: 0.0,
            },
            CommandKind::AddTool {
                name: "EM-0400-2FL".into(),
                tool: em,
            },
            CommandKind::CreateOperation {
                name: "Face1".into(),
                tool: "EM-0400-2FL".into(),
                src: SrcRef::Stock,
                params: OpParams::Face {
                    stepover: 3.0,
                    clearance_z: Height::fixed(25.0),
                    cut_z: Height::fixed(19.5),
                    top_z: None,
                    stepdown: None,
                    feed: 1200.0,
                    plunge_feed: 300.0,
                    climb: true,
                },
            },
        ]);
        state.drain_actions();
        state
    }

    #[test]
    fn selecting_an_operation_outlines_its_cut_region() {
        let mut state = face_state();
        // Created but never generated: the preview answers "what WOULD get
        // cut", which is the whole point of showing it before Generate.
        assert!(region_lines(&state).is_empty(), "nothing selected: nothing previewed");
        state.pending.push_back(CommandKind::Select(Some("Face1".into())));
        state.drain_actions();
        let lines = region_lines(&state);
        assert!(!lines.is_empty(), "the footprint of the 60 mm box shows");
        assert_eq!(lines.len() % 2, 0, "whole line segments only");
        for v in &lines {
            assert!(
                (v[2] - ftoz(19.5)).abs() <= f32::EPSILON,
                "a face region sits at cut_z, got {v:?}"
            );
            assert!(v[3] > 0.8 && v[4] > 0.4 && v[4] < 0.8 && v[5] < 0.3, "warning orange");
        }
        // A parameter edit moves the outline at once: it is a live world
        // read, never a leftover from the last generation.
        state.pending.push_back(CommandKind::EditOperationParams {
            name: "Face1".into(),
            params: OpParams::Face {
                stepover: 3.0,
                clearance_z: Height::fixed(25.0),
                cut_z: Height::fixed(10.0),
                top_z: None,
                stepdown: None,
                feed: 1200.0,
                plunge_feed: 300.0,
                climb: true,
            },
        });
        state.drain_actions();
        assert!(region_lines(&state).iter().all(|v| (v[2] - 10.0).abs() < 1e-4));
    }

    #[test]
    fn picks_are_pinned_in_the_world_not_the_window() {
        // The overlay twin of hover: the open operation's resolved pick
        // list draws where the features ARE, in violet, so the picks
        // hold their place under orbiting (the screen-space highlight
        // cannot). Orange `region_lines` is the complement - "what the
        // path cuts, at the cut plane" - and the two coexist on purpose.
        let mut state = face_state();
        state.history.execute(
            &state.world,
            ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
        );
        state.op_ui.name = "Face1".into();
        assert!(
            selection_feature_lines(&state).is_empty(),
            "the source is still Stock: no picks, nothing pinned"
        );
        let (top, rim) = {
            let e = model::find_by_name(&state.world, "part").expect("entity");
            let f = model::features_of(&state.world, &e).expect("features");
            let face = f
                .faces
                .iter()
                .filter(|fa| {
                    matches!(fa.kind, feature::FaceKind::Planar) && fa.normal[2].abs() > 0.9
                })
                .max_by(|a, b| a.offset.total_cmp(&b.offset))
                .map(|fa| fa.id)
                .expect("the box top");
            let chain = feature::chains(&f)
                .into_iter()
                .find(|c| {
                    c.edges.len() == 1
                        && feature::chain_points(&f, c).iter().all(|p| (p[2] - 5.0).abs() < 1e-9)
                })
                .expect("one top-rim edge")
                .id;
            (FeatureSel::Face(face), FeatureSel::Chain(chain))
        };
        state.pending.push_back(CommandKind::SetOperationGeometry {
            name: "Face1".into(),
            items: vec![
                GeoSel { entity: "part".into(), feature: top },
                GeoSel { entity: "part".into(), feature: rim },
            ],
        });
        state.drain_actions();
        let lines = selection_feature_lines(&state);
        assert_eq!(lines.len(), 10, "four-sided top ring plus one rim edge");
        for v in &lines {
            assert!(
                (v[3] - 0.72).abs() < 0.01
                    && (v[4] - 0.44).abs() < 0.01
                    && (v[5] - 0.86).abs() < 0.01,
                "violet, never region orange: {v:?}"
            );
            assert!(
                (v[2] - ftoz(5.0)).abs() <= f32::EPSILON,
                "the picks draw at their own z, not the op's cut plane: {v:?}"
            );
        }
        assert!(
            collect_overlay(&state).iter().any(|v| (v[5] - 0.86).abs() < 0.01),
            "the picks reach the real overlay buffer, not just the helper"
        );

        state.hidden_models.push("part".into());
        assert!(
            selection_feature_lines(&state).is_empty(),
            "a hidden model's picks draw nothing - the hover rule"
        );
        state.hidden_models.clear();

        let name = state.op_ui.name.clone();
        state.op_ui.name.clear();
        assert!(
            selection_feature_lines(&state).is_empty(),
            "no operation open: the pick list shown is no one's"
        );
        state.op_ui.name = name;

        // Stale ids: only a file can queue them (the command validates),
        // so swap the mesh through the app's write chokepoint - the top
        // face moves to z 8 and every picked id goes stale.
        let part = model::find_by_name(&state.world, "part").expect("part");
        model::set_facets(
            &state.world,
            &part,
            cube(Point3::ZERO, Point3::new(10.0, 10.0, 8.0)).facets,
        );
        assert!(
            selection_feature_lines(&state).is_empty(),
            "a pick that outlived its geometry leaves no ghost on screen"
        );
    }

    #[test]
    fn hover_features_lights_the_face_boundary() {
        let mut state = State::new();
        state.history.execute(
            &state.world,
            ImportModel::from_mesh("c", cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0))),
        );
        // The triangle outline is gone with the feature modes: only a
        // resolved feature highlights anything.
        assert!(hover_features(&state, 4.0 / 3.0).is_empty(), "no hover: nothing draws");
        state.hover = Some(HoverHit { entity: "c".into(), facet: 0, feature: None });
        assert!(
            hover_features(&state, 4.0 / 3.0).is_empty(),
            "a bare facet hover (Model mode) draws no feature outline"
        );

        // Face hover: the planar face's boundary loop - the square its
        // corner edges weld into, not the two triangles that tessellate it.
        let top = {
            let e = model::find_by_name(&state.world, "c").expect("entity");
            let f = model::features_of(&state.world, &e).expect("features");
            f.faces
                .iter()
                .find(|fa| matches!(fa.kind, feature::FaceKind::Planar) && fa.normal[2] > 0.9)
                .map(|fa| fa.id)
                .expect("a planar top face")
        };
        state.hover =
            Some(HoverHit { entity: "c".into(), facet: 0, feature: Some(FeatureSel::Face(top)) });
        let segs = hover_features(&state, 4.0 / 3.0);
        assert_eq!(segs.len(), 4, "the square's four sides");
        for (a, b) in &segs {
            for uv in [a, b] {
                assert!(
                    uv.iter().all(|v| (0.0..=1.0).contains(v)),
                    "uv lands inside the image: {uv:?}"
                );
            }
        }

        // Chain hover: a box edge is its own open run - one segment.
        let chain = {
            let e = model::find_by_name(&state.world, "c").expect("entity");
            let f = model::features_of(&state.world, &e).expect("features");
            feature::chains(&f).into_iter().next().expect("a box edge")
        };
        state.hover = Some(HoverHit {
            entity: "c".into(),
            facet: usize::MAX,
            feature: Some(FeatureSel::Chain(chain.id)),
        });
        assert_eq!(hover_features(&state, 4.0 / 3.0).len(), 1, "one edge, one segment");

        // Gone or stale: never light up a stranger's feature.
        state.hover =
            Some(HoverHit { entity: "c".into(), facet: 0, feature: Some(FeatureSel::Face(7)) });
        assert!(
            hover_features(&state, 4.0 / 3.0).is_empty(),
            "a face id the re-mesh dropped draws nothing"
        );
        state.hover =
            Some(HoverHit { entity: "nope".into(), facet: 2, feature: Some(FeatureSel::Face(top)) });
        assert!(hover_features(&state, 4.0 / 3.0).is_empty());
        state.hover =
            Some(HoverHit { entity: "c".into(), facet: 2, feature: Some(FeatureSel::Face(top)) });
        state.hidden_models.push("c".into());
        assert!(
            hover_features(&state, 4.0 / 3.0).is_empty(),
            "a hidden model's feature draws nothing - it is display-off"
        );
    }

    #[test]
    fn row_hover_highlights_in_viewport() {
        // The Geometry list's row hover rides the same amber channel as
        // viewport hover: same segments, same nothing-for-stale rules, and
        // one feature pointed at twice draws once.
        let mut state = State::new();
        state.history.execute(
            &state.world,
            ImportModel::from_mesh("c", cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0))),
        );
        let (top, chain) = {
            let e = model::find_by_name(&state.world, "c").expect("entity");
            let f = model::features_of(&state.world, &e).expect("features");
            let top = f
                .faces
                .iter()
                .find(|fa| matches!(fa.kind, feature::FaceKind::Planar) && fa.normal[2] > 0.9)
                .map(|fa| fa.id)
                .expect("a planar top face");
            let chain = feature::chains(&f).into_iter().next().expect("a box edge").id;
            (top, chain)
        };
        let face = GeoSel { entity: "c".into(), feature: FeatureSel::Face(top) };
        let run = GeoSel { entity: "c".into(), feature: FeatureSel::Chain(chain) };

        state.geo_hover = Some(face.clone());
        let segs = hover_features(&state, 4.0 / 3.0);
        assert_eq!(segs.len(), 4, "the row's face lights its four sides");
        for (a, b) in &segs {
            for uv in [a, b] {
                assert!(
                    uv.iter().all(|v| (0.0..=1.0).contains(v)),
                    "uv lands inside the image: {uv:?}"
                );
            }
        }

        // Both pointers on the same feature: once, not twice.
        state.hover =
            Some(HoverHit { entity: "c".into(), facet: 0, feature: Some(face.feature) });
        assert_eq!(hover_features(&state, 4.0 / 3.0).len(), 4, "no double-draw");

        // Two different features: the viewport keeps its, the row adds its.
        state.hover = Some(HoverHit { entity: "c".into(), facet: 0, feature: Some(run.feature) });
        assert_eq!(hover_features(&state, 4.0 / 3.0).len(), 5, "one edge plus the square");

        // Stale ids and hidden models draw nothing through the row either -
        // the highlight never promises a pick the click cannot make.
        state.hover = None;
        state.geo_hover = Some(GeoSel { entity: "c".into(), feature: FeatureSel::Face(7) });
        assert!(hover_features(&state, 4.0 / 3.0).is_empty(), "a stale face id draws no ghost");
        state.geo_hover = Some(GeoSel { entity: "nope".into(), feature: face.feature });
        assert!(hover_features(&state, 4.0 / 3.0).is_empty(), "a gone entity draws nothing");
        state.geo_hover = Some(face);
        state.hidden_models.push("c".into());
        assert!(hover_features(&state, 4.0 / 3.0).is_empty(), "display-off stays off");
    }
}

#[cfg(test)]
mod feature_pick_tests {
    //! The Face/Edge click path as it lands on State: a plain click
    //! replaces the open operation's geometry list, Ctrl/Shift adds to it,
    //! a repeat is refused as a no-op, and with no operation open nothing
    //! queues. The screen-space edge search - the half `pick_edge` reads
    //! but headless tests cannot drive through a window - is pinned here
    //! too, at the same projection the gizmo and anchor tests use.

    use super::*;
    use hsmworks_app::commands::{AddTool, CreateOperation, ImportModel};
    use hsmworks_app::geometry::{cube, cylinder};
    use hsmworks_app::operation::{GeoSel, Height, OpParams, SrcRef};
    use hsmworks_app::stock::StockShape;
    use hsmworks_app::tools::catalog;

    fn state_with_op() -> State {
        let mut state = State::new();
        state.pending.push_back(CommandKind::SetStock {
            shape: StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 },
            pos: [0.0, 0.0, 0.0],
            yaw_deg: 0.0,
        });
        state.drain_actions();
        let em = catalog()
            .into_iter()
            .find(|(n, _)| *n == "EM-0400-2FL")
            .map(|(_, t)| t)
            .expect("catalog entry");
        state
            .history
            .execute(&state.world, AddTool::new(&state.world, "EM-0400-2FL", em).expect("tool"));
        state.history.execute(
            &state.world,
            ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
        );
        state.history.execute(
            &state.world,
            CreateOperation::new(
                &state.world,
                "G1",
                "EM-0400-2FL",
                SrcRef::Stock,
                OpParams::Face {
                    stepover: 3.0,
                    clearance_z: Height::fixed(25.0),
                    cut_z: Height::fixed(4.5),
                    top_z: None,
                    stepdown: None,
                    feed: 1200.0,
                    plunge_feed: 300.0,
                    climb: true,
                },
            )
            .expect("operation"),
        );
        state.op_ui.name = "G1".into();
        state
    }

    fn src(state: &State) -> SrcRef {
        let e = model::find_by_name(&state.world, "G1").expect("op");
        model::operation_of(&state.world, &e).expect("operation").src
    }

    /// A horizontal planar face of the named entity, the higher or lower
    /// one. Face normals are sign-canonical (winding-independent), so the
    /// two horizontal faces of a box share a normal and are told apart by
    /// plane offset - exactly what the generation-side tilt test does.
    fn face_sel(state: &State, name: &str, top: bool) -> FeatureSel {
        let e = model::find_by_name(&state.world, name).expect("entity");
        let f = model::features_of(&state.world, &e).expect("features");
        let mut hs: Vec<&_> = f
            .faces
            .iter()
            .filter(|fa| matches!(fa.kind, feature::FaceKind::Planar) && fa.normal[2].abs() > 0.9)
            .collect();
        hs.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        let fa = if top { hs.last() } else { hs.first() }.expect("a horizontal planar face");
        FeatureSel::Face(fa.id)
    }

    /// The screen midpoint of the first edge of `name` whose ends both sit
    /// above `zmin` in local space - where a rim edge actually draws.
    fn rim_mid_px(
        state: &State,
        mvp: &[[f32; 4]; 4],
        name: &str,
        zmin: f64,
        w: f32,
        h: f32,
    ) -> Option<(u32, [f32; 2])> {
        let e = model::find_by_name(&state.world, name)?;
        let f = model::features_of(&state.world, &e)?;
        let loc = model::transform_of(&state.world, &e).unwrap_or_default();
        let at = |vi: u32| {
            let v = *f.verts.get(vi as usize)?;
            let x = transform_point(Point3::new(v[0], v[1], v[2]), loc.pos, loc.rot);
            project_uv(mvp, [x.x, x.y, x.z]).map(|uv| [uv[0] * w, uv[1] * h])
        };
        f.edges.iter().enumerate().find_map(|(i, ed)| {
            let va = *f.verts.get(ed.a as usize)?;
            let vb = *f.verts.get(ed.b as usize)?;
            if va[2] < zmin || vb[2] < zmin {
                return None;
            }
            let (a, b) = (at(ed.a)?, at(ed.b)?);
            Some((i as u32, [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]))
        })
    }

    #[test]
    fn a_plain_click_replaces_and_ctrl_adds() {
        let mut state = state_with_op();
        assert_eq!(src(&state), SrcRef::Stock);
        let top = face_sel(&state, "part", true);
        let bottom = face_sel(&state, "part", false);
        let part = |feature| GeoSel { entity: "part".into(), feature };

        queue_feature_pick(&mut state, part(top), false);
        assert!(state.status.contains("added to G1"), "the click says what it did: {}", state.status);
        state.drain_actions();
        assert_eq!(src(&state), SrcRef::Selection(vec![part(top)]));
        assert!(
            state.status.contains("updated for G1"),
            "and the executed command says so too: {}",
            state.status
        );

        // Ctrl adds: the base is the applied list, so order is stable.
        queue_feature_pick(&mut state, part(bottom), true);
        state.drain_actions();
        assert_eq!(src(&state), SrcRef::Selection(vec![part(top), part(bottom)]));

        // A repeat is refused where it stands: no command, no history entry.
        let before = state.pending.len();
        queue_feature_pick(&mut state, part(top), true);
        assert_eq!(state.pending.len(), before, "the duplicate queues nothing");
        assert!(state.status.contains("already selected"), "{}", state.status);

        // A plain click replaces: the list is exactly what was clicked.
        queue_feature_pick(&mut state, part(bottom), false);
        state.drain_actions();
        assert_eq!(src(&state), SrcRef::Selection(vec![part(bottom)]));
    }

    #[test]
    fn a_chain_pick_rides_the_same_list() {
        // Edge mode folds a Chain id into exactly the same queue: the
        // geometry list is kind-blind and order-keeping.
        let mut state = state_with_op();
        let chain = {
            let e = model::find_by_name(&state.world, "part").expect("entity");
            let f = model::features_of(&state.world, &e).expect("features");
            feature::chains(&f).into_iter().next().expect("a box edge").id
        };
        let top = face_sel(&state, "part", true);
        let (face_pick, chain_pick) = (
            GeoSel { entity: "part".into(), feature: top },
            GeoSel { entity: "part".into(), feature: FeatureSel::Chain(chain) },
        );
        queue_feature_pick(&mut state, face_pick.clone(), false);
        state.drain_actions();
        queue_feature_pick(&mut state, chain_pick.clone(), true);
        state.drain_actions();
        assert_eq!(src(&state), SrcRef::Selection(vec![face_pick, chain_pick]));
    }

    #[test]
    fn a_wall_face_pick_lands_on_the_patch() {
        // The hand-test complaint on the pick side: clicking a faceted
        // bore answered with the one tessellation strip under the
        // cursor. A strip of a welded wall now resolves to the whole
        // weld; flat faces answer with themselves, so an ordinary box
        // re-keys nothing.
        let mut state = state_with_op();
        state.history.execute(
            &state.world,
            ImportModel::from_mesh("boss", cylinder(20.0, 8.0, 16)),
        );
        // `cylinder`'s facet layout: per segment, two wall triangles,
        // then the top and bottom cap fans.
        let wall = feature_of_facet(&state, "boss", 0).expect("wall facet classified");
        let cap = feature_of_facet(&state, "boss", 2).expect("cap facet classified");
        assert!(
            matches!(wall, FeatureSel::Patch(_)),
            "a welded strip picks as the wall, not a strip: {wall}"
        );
        assert!(matches!(cap, FeatureSel::Face(_)), "a planar cap stays a face: {cap}");
        let pid = match wall {
            FeatureSel::Patch(id) => id,
            _ => panic!("wall pick is a patch"),
        };
        let e = model::find_by_name(&state.world, "boss").expect("entity");
        let f = model::features_of(&state.world, &e).expect("features");
        let p = feature::patches(&f)
            .into_iter()
            .find(|x| x.id == pid)
            .expect("the weld the pick named");
        assert_eq!(p.faces.len(), 16, "all sixteen wall strips, no cap");
        // Same facet slot on the box: a single flat strip, no weld.
        let boxy = feature_of_facet(&state, "part", 4).expect("box facet classified");
        assert!(matches!(boxy, FeatureSel::Face(_)), "a flat wall picks as itself: {boxy}");
    }

    #[test]
    fn no_open_operation_queues_nothing() {
        let mut state = state_with_op();
        state.op_ui.name.clear();
        let top = face_sel(&state, "part", true);
        queue_feature_pick(&mut state, GeoSel { entity: "part".into(), feature: top }, false);
        assert!(state.pending.is_empty(), "a click with nothing open queues nothing");
        assert!(state.status.contains("open an operation"), "{}", state.status);
    }

    #[test]
    fn edge_picking_finds_the_drawn_edge_and_respects_the_eye() {
        let mut state = state_with_op();
        state.viewport_rect = ([0.0, 0.0], [800.0, 600.0]);
        let mvp = state.cam.mvp(800.0 / 600.0);
        let (edge, mid) = rim_mid_px(&state, &mvp, "part", 4.9, 800.0, 600.0)
            .expect("a part rim edge visible from the default camera");
        let (name, hit) =
            edge_nearest(&state, mid, PICK_EDGE_PX).expect("an edge under its own midpoint");
        assert_eq!(name, "part", "the part draws over the stock here");
        assert_eq!(hit, edge, "and it is the edge that drew there");

        // The browser eye closes the geometry off the screen too.
        state.hidden_models.push("part".into());
        assert!(
            edge_nearest(&state, mid, 4.0 * PICK_EDGE_PX).is_none_or(|(n, _)| n != "part"),
            "a hidden model's edges are not on screen, even nearby"
        );
    }

    #[test]
    fn stock_edges_are_pickable_and_an_unlaid_rect_finds_nothing() {
        let mut state = State::new();
        state.pending.push_back(CommandKind::SetStock {
            shape: StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 },
            pos: [0.0, 0.0, 0.0],
            yaw_deg: 0.0,
        });
        state.drain_actions();
        assert!(
            edge_nearest(&state, [10.0, 10.0], 50.0).is_none(),
            "pre-layout there is no image to project into - no phantom picks"
        );
        state.viewport_rect = ([0.0, 0.0], [800.0, 600.0]);
        let mvp = state.cam.mvp(800.0 / 600.0);
        let (_, mid) = rim_mid_px(&state, &mvp, STOCK_NAME, 19.9, 800.0, 600.0)
            .expect("a stock rim edge visible from the default camera");
        let (name, _) = edge_nearest(&state, mid, PICK_EDGE_PX).expect("the stock's own edge");
        assert_eq!(name, STOCK_NAME, "the stock's top is pickable geometry, no tree needed");
    }
}

#[cfg(test)]
mod height_plane_tests {
    //! M1.5.1 step 3: the viewport answers "where are the heights". The
    //! planes read the OPEN operation's DRAFT levels - the spinbox value,
    //! not the applied one - and the region plane defers to `region_lines`
    //! wherever the orange outline already stands at the same level, so a
    //! plane never draws twice and never silently vanishes. No op open, no
    //! planes; a source that bounds nothing still gets its levels marked.

    use super::*;
    use hsmworks_app::commands::{AddTool, CreateOperation, ImportModel};
    use hsmworks_app::geometry::cube;
    use hsmworks_app::machining2d::{Lead, PocketStrategy};
    use hsmworks_app::operation::{Height, OpParams, SrcRef};
    use hsmworks_app::stock::StockShape;
    use hsmworks_app::tools::catalog;

    fn contour(clearance_z: Height) -> OpParams {
        OpParams::Contour {
            stock_to_leave: 0.0,
            clearance_z,
            retract_z: Height::fixed(15.0),
            top_z: Height::fixed(10.0),
            bottom_z: Height::fixed(2.0),
            stepdown: 2.0,
            feed: 800.0,
            plunge_feed: 200.0,
            climb: true,
            lead: Lead::None,
            tolerance: 0.01,
            tab_count: 0,
            tab_width: 0.0,
            tab_height: 0.0,
        }
    }

    fn pocket() -> OpParams {
        OpParams::Pocket {
            stepover: 3.0,
            stock_to_leave: 0.0,
            clearance_z: Height::fixed(25.0),
            retract_z: None,
            top_z: Height::fixed(10.0),
            bottom_z: Height::fixed(2.0),
            stepdown: 2.0,
            feed: 800.0,
            plunge_feed: 200.0,
            climb: true,
            strategy: PocketStrategy::Zigzag,
        }
    }

    /// Stock, a tool, a part (so a fallback extent exists even stockless)
    /// and one operation - with the pane draft mirroring it exactly, the
    /// way sync_ops_ui leaves it when the op is opened.
    fn state_with(name: &str, params: OpParams) -> State {
        let mut state = State::new();
        state.pending.push_back(CommandKind::SetStock {
            shape: StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 },
            pos: [0.0, 0.0, 0.0],
            yaw_deg: 0.0,
        });
        state.drain_actions();
        let em = catalog()
            .into_iter()
            .find(|(n, _)| *n == "EM-0400-2FL")
            .map(|(_, t)| t)
            .expect("catalog entry");
        state
            .history
            .execute(&state.world, AddTool::new(&state.world, "EM-0400-2FL", em).expect("tool"));
        state.history.execute(
            &state.world,
            ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
        );
        state.history.execute(
            &state.world,
            CreateOperation::new(&state.world, name, "EM-0400-2FL", SrcRef::Stock, params.clone())
                .expect("operation"),
        );
        state.op_ui.name = name.into();
        state.op_ui.tool = "EM-0400-2FL".into();
        state.op_ui.params = params;
        // Open means selected in the pane, and the plane gates read the
        // selection, not the pane: mirror what a click on the tree does.
        state.pending.push_back(CommandKind::Select(Some(name.into())));
        state.drain_actions();
        state
    }

    /// Vertices at world height `z` painted exactly `rgb` - rectangle
    /// corners are the only geometry that carries plane colors.
    fn count_at(lines: &[[f32; 6]], z: f64, rgb: [f32; 3]) -> usize {
        lines
            .iter()
            .filter(|v| {
                (f64::from(v[2]) - z).abs() < 1e-3
                    && (0..3).all(|k| (v[3 + k] - rgb[k]).abs() < 1e-3)
            })
            .count()
    }

    #[test]
    fn height_planes_draw_for_open_contour_only_where_defined() {
        let state = state_with("C1", contour(Height::fixed(25.0)));
        let lines = height_plane_lines(&state);
        assert_eq!(count_at(&lines, 25.0, CLEARANCE_RGB), 8, "clearance rectangle");
        assert_eq!(count_at(&lines, 15.0, RETRACT_RGB), 8, "retract is a Contour level");
        assert_eq!(count_at(&lines, 2.0, CUT_RGB), 8, "bottom depth");
        assert_eq!(
            count_at(&lines, 10.0, REGION_RGB),
            0,
            "the region outline already stands at top_z - never a coincident double"
        );

        // Pocket stores no retract: nothing honest to draw at 15.
        let pocket = state_with("P1", pocket());
        let plines = height_plane_lines(&pocket);
        assert_eq!(count_at(&plines, 25.0, CLEARANCE_RGB), 8);
        assert_eq!(count_at(&plines, 15.0, RETRACT_RGB), 0, "no retract level exists");
        assert_eq!(count_at(&plines, 2.0, CUT_RGB), 8);

        // Every drawn plane carries its corner label - the outline-borne
        // Top included, which is exactly the one with no rectangle.
        let texts: Vec<String> =
            overlay_labels(&state, 800.0 / 600.0).into_iter().map(|(_, _, t)| t).collect();
        for want in ["C1 · Clearance 25.00", "C1 · Retract 15.00", "C1 · Top 10.00", "C1 · Bottom 2.00"] {
            assert!(texts.iter().any(|t| t == want), "{want} among {texts:?}");
        }
    }

    /// The Face that stages: outline at its Top, the finished Cut below.
    /// The old blanket gate skipped the Cut rectangle off a single
    /// "region shown" bool - invisible level, no outline standing for it.
    /// The level-by-level gate leaves the Cut rectangle exactly when no
    /// outline paints its level, and drops it exactly when one does.
    #[test]
    fn height_planes_stand_in_only_when_the_level_is_undrawn() {
        let staged = state_with(
            "F1",
            OpParams::Face {
                stepover: 3.0,
                clearance_z: Height::fixed(25.0),
                cut_z: Height::fixed(18.0),
                top_z: Some(Height::fixed(20.0)),
                stepdown: Some(1.0),
                feed: 1200.0,
                plunge_feed: 400.0,
                climb: false,
            },
        );
        let lines = height_plane_lines(&staged);
        assert_eq!(
            count_at(&lines, 20.0, REGION_RGB),
            0,
            "the outline already stands at the staged Top"
        );
        assert_eq!(
            count_at(&lines, 18.0, CUT_RGB),
            8,
            "the finished Cut level carries no outline - the rectangle marks it"
        );
        // Unstaged: region_z IS the cut, one outline at one level.
        let flat = state_with(
            "F2",
            OpParams::Face {
                stepover: 3.0,
                clearance_z: Height::fixed(25.0),
                cut_z: Height::fixed(18.0),
                top_z: None,
                stepdown: None,
                feed: 1200.0,
                plunge_feed: 400.0,
                climb: false,
            },
        );
        let lines = height_plane_lines(&flat);
        assert_eq!(
            count_at(&lines, 18.0, CUT_RGB),
            0,
            "the outline stands at the cut - no coincident double"
        );
    }

    #[test]
    fn height_planes_stand_in_when_the_source_bounds_nothing() {
        let mut state = state_with("C1", contour(Height::fixed(25.0)));
        // The hand-test C shape from the other side: an operation whose
        // source vanished shows no region outline, so the Top rectangle
        // appears in its orange to mark the level all the same.
        state.pending.push_back(CommandKind::DeleteByName(STOCK_NAME.into()));
        state.drain_actions();
        let lines = height_plane_lines(&state);
        assert_eq!(count_at(&lines, 10.0, REGION_RGB), 8, "the standing-in rectangle");
        assert_eq!(count_at(&lines, 25.0, CLEARANCE_RGB), 8, "the rest is unchanged");
    }

    #[test]
    fn height_planes_track_draft_edits() {
        let mut state = state_with("C1", contour(Height::fixed(25.0)));
        let before = draft_key(&state);
        if let OpParams::Contour { clearance_z, .. } = &mut state.op_ui.params {
            *clearance_z = Height::fixed(30.0);
        }
        assert_ne!(draft_key(&state), before, "a spin edit must re-gate the overlay rebuild");
        let lines = height_plane_lines(&state);
        assert_eq!(count_at(&lines, 30.0, CLEARANCE_RGB), 8, "the plane moved with the draft");
        assert_eq!(count_at(&lines, 25.0, CLEARANCE_RGB), 0, "and not half-old");
    }

    #[test]
    fn plane_labels_stay_inside_the_viewport() {
        let mut state = state_with("C1", contour(Height::fixed(25.0)));
        // Zoom so tight the padded rectangle's corners project far off
        // frame: their labels must ride the margin, not vanish with the
        // corner. (project_uv itself is untouched - picking needs true
        // positions; only the label placement clamps.)
        state.cam.dist = 6.0;
        let labels = overlay_labels(&state, 800.0 / 600.0);
        let planes: Vec<&([f32; 2], [f32; 3], String)> =
            labels.iter().filter(|(_, _, t)| t.contains(" · ")).collect();
        assert!(!planes.is_empty(), "the open operation's planes still label");
        for (uv, _, t) in &planes {
            assert!(
                (0.0..=1.0).contains(&uv[0]) && (0.0..=1.0).contains(&uv[1]),
                "label {t} at {uv:?} left the viewport"
            );
        }
        // And the clamp earned its keep: raw, some corner did project out.
        let rect = plane_rect(&state).expect("stock gives an extent");
        let mvp = state.cam.mvp(800.0 / 600.0);
        let pushed_out = height_planes(&state).iter().any(|p| {
            let (cx, cy) = plane_corner(rect, p.corner);
            project_uv(&mvp, [cx, cy, p.z])
                .is_some_and(|uv| !(0.0..=1.0).contains(&uv[0]) || !(0.0..=1.0).contains(&uv[1]))
        });
        assert!(pushed_out, "a camera that fits everything proves nothing");
    }

    #[test]
    fn plane_labels_name_their_operation() {
        let state = state_with("Operation1", contour(Height::fixed(25.0)));
        let texts: Vec<String> = overlay_labels(&state, 800.0 / 600.0)
            .into_iter()
            .map(|(_, _, t)| t)
            .collect();
        assert!(
            texts.iter().any(|t| t.starts_with("Operation1 · Clearance")),
            "the label says whose plane it is: {texts:?}"
        );
        assert!(
            texts
                .iter()
                .all(|t| !t.contains(" · ") || t.starts_with("Operation1 ·")),
            "no plane labels anonymously: {texts:?}"
        );
    }

    #[test]
    fn height_planes_gone_when_no_op_open() {
        let state = State::new();
        assert!(height_planes(&state).is_empty() && height_plane_lines(&state).is_empty());
        // A name typed but not an operation: no planes, no phantom labels.
        let mut typed = state_with("C1", contour(Height::fixed(25.0)));
        typed.op_ui.name = "Ghost".into();
        assert!(height_planes(&typed).is_empty());
        assert!(height_plane_lines(&typed).is_empty());
        let texts: Vec<String> =
            overlay_labels(&typed, 800.0 / 600.0).into_iter().map(|(_, _, t)| t).collect();
        assert!(
            !texts.iter().any(|t| t.contains("Clearance")),
            "nothing open labels nothing: {texts:?}"
        );
    }

    #[test]
    fn face_top_z_walks_its_plane() {
        // An unstaged face draws only its Cut plane - no phantom Top for a
        // level it will never cut. Staged, Top appears at the first level,
        // and editing the draft walks it live, the same honesty rule the
        // depth kinds follow.
        let face_at = |top_z: Option<Height>| OpParams::Face {
            stepover: 8.0,
            clearance_z: Height::fixed(15.0),
            cut_z: Height::fixed(2.0),
            top_z,
            stepdown: if top_z.is_some() { Some(2.0) } else { None },
            feed: 800.0,
            plunge_feed: 200.0,
            climb: true,
        };
        let plain = state_with("F1", face_at(None));
        let names: Vec<&str> = height_planes(&plain).iter().map(|p| p.name).collect();
        assert_eq!(names, ["Clearance", "Cut"], "unstaged: no Top plane: {names:?}");

        let mut staged = state_with("F2", face_at(Some(Height::fixed(6.0))));
        let at = |s: &State| -> Vec<(&'static str, f64)> {
            height_planes(s).iter().map(|p| (p.name, p.z)).collect()
        };
        assert_eq!(
            at(&staged),
            [("Clearance", 15.0), ("Top", 6.0), ("Cut", 2.0)],
            "staged facing draws its first level above the finish"
        );
        // Walk the draft's Top Z: the plane follows before Apply, the
        // committed entity does not move at all.
        if let OpParams::Face { top_z, .. } = &mut staged.op_ui.params {
            *top_z = Some(Height::fixed(9.0));
        }
        assert_eq!(
            at(&staged),
            [("Clearance", 15.0), ("Top", 9.0), ("Cut", 2.0)],
            "the draft owns the plane"
        );
        let zs: Vec<f32> = height_plane_lines(&staged).iter().map(|l| l[2]).collect();
        assert!(zs.contains(&9.0), "the ring geometry walks too: {zs:?}");
        let back = state_with("F2", face_at(Some(Height::fixed(6.0))));
        assert_eq!(
            operation::region_z(&back.world, &back.op_ui.params),
            Some(6.0),
            "region_z stages with top_z - the draft's 9.0 never reached the document"
        );
    }
}
