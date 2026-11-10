//! M-a: the graduated egui shell — the loop, GPU init, dock, and the
//! View3D-texture law, moved from the M1.4.1 spike binary into the lib
//! (docs/spike-egui-go-no-go.md, migration step M-a). The law is the
//! spike's, unchanged; what changed is where it lives and that the panes
//! are stubs that name their migration step.
//!
//! The GPU loop mirrors app.rs (our own wgpu instance/surface, no unwrap —
//! every failure is an eprintln plus a nonzero exit), plus egui-wgpu's
//! low-level Renderer and View3D's offscreen texture registered once as a
//! native egui texture and rebound on resize. Per frame: `drain_actions`
//! first (commands the panels queued land before the scene is collected),
//! then the digest-gated re-upload of document + overlay — the imgui
//! shell's exact upload law, mirrored.
//! Provenance: our own code, graduated from our own spike, specced from the
//! egui/egui-wgpu/egui_dock docs and vendored sources; no HSMWorks material
//! involved.

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

use crate::interact;
use crate::state::{DragMode, State};
use crate::viewport::View3D;

use super::icons;
use super::layout;
use super::machine_editor;
use super::machines_pane;
use super::unsaved_prompt;
use super::frame_watch::FrameWatch;
use super::gpu;
use super::gpu_init;
use super::pages;
use super::panes;
use super::post_refusal;
use super::theme;
use super::viewcube;
use super::world;

const VIEW3D_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Ribbon strip height: the cell box plus the caption row and the pad
/// (`theme::ribbon_h` = `cell_h + 26`), imgui's `height()` formula restated
/// as one derivation off the token - 61 at the dark defaults. The panel
/// sizes it live from `theme::current()`; this const is the test fixture's
/// number (a theme that grows the cells grows the ribbon with it).
#[cfg(test)]
pub(crate) const RIBBON_H: f32 = theme::ribbon_h(&theme::Tokens::DARK);
/// Status strip height (imgui's single-line bottom bar).
const STATUS_H: f32 = 22.0;

/// The windowed shell (the `--ui egui` entry without `--headless`); returns
/// the process exit code.
pub(crate) fn run_windowed(pick: gpu::GpuPick, want: Option<String>, mcp_port: Option<u16>) -> i32 {
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(err) => {
            eprintln!("cannot create event loop: {err}");
            return 1;
        }
    };
    event_loop.set_control_flow(ControlFlow::Wait);
    // #144: macOS's terminate path (Cmd+Q, the app menu's Quit) asks
    // through the guarded Quit instead of ending the process.
    let _ = super::os_quit::install();
    let mut shell = Shell::new(pick, want);
    // #183: the opt-in live MCP server. Its waker posts an empty user
    // event, which wakes the idle loop for a frame that answers the call.
    if let Some(port) = mcp_port {
        let proxy = event_loop.create_proxy();
        let wake: camcraft_app::mcp::Wake = Box::new(move || {
            let _ = proxy.send_event(());
        });
        match camcraft_app::mcp::Inbox::listen(port, wake) {
            Ok(inbox) => {
                if let Some(addr) = inbox.addr() {
                    eprintln!("mcp: listening on {addr} (connect with: camcraft-cli mcp --connect {addr})");
                }
                shell.state.mcp = Some(inbox);
            }
            Err(err) => {
                eprintln!("mcp: cannot listen on 127.0.0.1:{port}: {err}");
                shell.state.status = format!("MCP server not started: 127.0.0.1:{port}: {err}");
            }
        }
    }
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
    window_title: String,
    device: Option<wgpu::Device>,
    queue: Option<wgpu::Queue>,
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    renderer: Option<Renderer>,
    winit_state: Option<egui_winit::State>,
    egui_ctx: egui::Context,
    dock_state: DockState<Tab>,
    view: Option<View3D>,
    /// M7.2 B3b: the traced-stock eye; `None` where the device cannot
    /// composite it (the stock sheet then stands, said once at init).
    stock_eye: Option<crate::trace::eye::StockEye>,
    view_tex: Option<egui::TextureId>,
    /// The bundled glyphs, built once on the first GPU-backed frame (the
    /// retired imgui loader's law: lazy, and `None` means painted fallback,
    /// never blank strips).
    icons: Option<icons::IconSet>,
    /// Last laid-out viewport size in logical points; drives the View3D
    /// extent for the NEXT frame (imgui's shell uses the same deferral).
    viewport_points: [f32; 2],
    /// Where the last primary press landed (viewport panel coords), until its
    /// release decides pick-vs-orbit - the imgui App's `left_press` plumbed
    /// into the frame builder the same way `viewport_points` is (M-e).
    left_press: Option<[f32; 2]>,
    /// Content digest of the scene last uploaded to View3D; re-uploads are
    /// gated on it (App::sync_gpu's law, mirrored).
    scene_digest: u64,
    exit_code: i32,
    failed: bool,
    exit_requested: bool,
    /// M4.2: the validated GPU backend pick; drives init_gpu's attempts
    /// chain (preferred mask first, then auto, then all).
    gpu_pick: gpu::GpuPick,
    /// #74: the `--gpu` / `CAMCRAFT_GPU` adapter-name substring, tried first.
    gpu_want: Option<String>,
    /// #74: per-frame phase timing; a frame over budget is logged by phase.
    frame_watch: FrameWatch,
    /// Next redraw requested by egui or an active background task.
    next_repaint: Option<std::time::Instant>,
    /// One extra frame lets input-driven viewport changes reach the scene.
    followup_redraw: bool,
    /// A hidden surface has no useful frame timer.
    occluded: bool,
    /// Resize events can arrive faster than a GPU frame. Configure only for
    /// the newest size when the next resize frame is due.
    pending_resize: Option<PhysicalSize<u32>>,
    last_resize_frame: Option<std::time::Instant>,
    state: State,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Tab {
    Viewport,
    Browser,
    Properties,
    Machines,
}

impl Tab {
    fn title(&self) -> &'static str {
        match self {
            Tab::Viewport => "viewport",
            Tab::Browser => "browser",
            Tab::Properties => "properties",
            Tab::Machines => "machines",
        }
    }
}

/// The default dock lives with the layout law (layout.rs); re-exported
/// here because the shell and its frame tests build it by this name.
pub(crate) use super::layout::build_dock;

impl Shell {
    fn new(pick: gpu::GpuPick, want: Option<String>) -> Self {
        // M-d: startup is a plain State::new() - byte-identical in shape to
        // imgui's App::new. The demo seed left the window path; world::seed
        // is what the harnesses and the headless proof call directly now.
        // File work is the real thing: New/Model/Project/Save/Save As on the ribbon
        // queue the shared command kinds, rfd picks the path, State::run
        // does the work - the machinery was never egui's to rebuild.
        let mut state = State::new();
        // #662: the user's recent projects and models, read once.
        state.load_recent();
        let dock_state = build_dock();
        Self {
            window: None,
            window_title: state.window_title(),
            device: None,
            queue: None,
            surface: None,
            config: None,
            renderer: None,
            winit_state: None,
            egui_ctx: egui::Context::default(),
            dock_state,
            view: None,
            stock_eye: None,
            view_tex: None,
            icons: None,
            viewport_points: [600.0, 400.0],
            left_press: None,
            scene_digest: 0,
            exit_code: 0,
            failed: false,
            exit_requested: false,
            gpu_pick: pick,
            gpu_want: want,
            frame_watch: FrameWatch::new(std::time::Instant::now()),
            next_repaint: None,
            followup_redraw: false,
            occluded: false,
            pending_resize: None,
            last_resize_frame: None,
            state,
        }
    }

    fn fail(&mut self, el: &ActiveEventLoop, what: &str, err: impl std::fmt::Display) {
        eprintln!("egui shell: {what}: {err}");
        self.failed = true;
        self.exit_code = 1;
        el.exit();
    }

    fn init_gpu(&mut self, el: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title(self.window_title.clone())
            .with_inner_size(LogicalSize::new(1280.0, 800.0));
        let window = match el.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(err) => {
                self.fail(el, "window creation", err);
                return;
            }
        };
        // M4.2 + #74: the honest fallback chain, adapter by adapter inside
        // each backend mask (gpu_init); a refusal names every adapter tried.
        // The View3D pipelines are built per candidate: an adapter that
        // cannot build them fails like one that cannot configure, and the
        // next adapter is tried (the "Intel never renders" path).
        let scale = window.scale_factor().clamp(0.1, 8.0);
        let (vw, vh) = (
            (f64::from(self.viewport_points[0]) * scale).max(1.0) as u32,
            (f64::from(self.viewport_points[1]) * scale).max(1.0) as u32,
        );
        // View3D renders into its OWN texture (Rgba8Unorm, the format
        // egui's native-texture registration demands), decoupled from the
        // surface format - that is the registered-texture plan.
        let build = |device: &wgpu::Device, _: &wgpu::Queue, _: &wgpu::SurfaceConfiguration| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| View3D::new(device, VIEW3D_FORMAT, vw, vh)))
                .map_err(|_| "viewport3d pipeline creation aborted".to_string())
        };
        let gpu_init::Bring { surface, adapter, device, queue, config, built: mut view } =
            match gpu_init::bring_up(&window, &self.gpu_pick, self.gpu_want.as_deref(), build) {
                Ok(b) => b,
                Err(err) => {
                    refuse_dialog(&err);
                    self.fail(el, "GPU", err);
                    return;
                }
            };
        let gpu_line = gpu::describe(&adapter).log_line();
        println!("{gpu_line}");
        self.state.gpu_line = Some(gpu_line);

        let mut renderer = Renderer::new(&device, config.format, RendererOptions::default());
        let scene = world::collect(&self.state);
        self.scene_digest = scene.digest();
        view.set_document(&device, &queue, &scene.parts);
        view.set_overlay(&device, &queue, &scene.overlay);
        let stock_eye = match crate::trace::eye::StockEye::new(&device, VIEW3D_FORMAT) {
            Ok(e) => Some(e),
            Err(err) => {
                eprintln!("stock eye: {err} - the deviate sheet stands in");
                None
            }
        };
        let tex =
            renderer.register_native_texture(&device, view.color_view(), wgpu::FilterMode::Linear);
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
        self.stock_eye = stock_eye;
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
        let started = std::time::Instant::now();
        surface.configure(device, config);
        let elapsed = started.elapsed();
        if elapsed > super::frame_watch::BUDGET {
            eprintln!("gpu: slow resize configure {} ms", elapsed.as_millis());
        }
    }

    fn draw(&mut self) {
        if self.failed || self.occluded {
            return;
        }
        let now = std::time::Instant::now;
        let followup_redraw = std::mem::take(&mut self.followup_redraw);
        self.frame_watch.begin(now());
        // Commands the panels queued last frame land before the scene is
        // collected, so this frame draws what the user's click did.
        self.state.drain_actions();
        let title = self.state.window_title();
        if title != self.window_title {
            if let Some(window) = &self.window {
                window.set_title(&title);
            }
            self.window_title = title;
        }
        self.frame_watch.mark("commands", now());
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
            stock_eye,
            view_tex,
            icons,
            viewport_points,
            left_press,
            scene_digest,
            exit_requested,
            frame_watch,
            next_repaint,
            state,
            ..
        } = self;
        frame_watch.mark("scene", now());
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

        // Icons decode once, on the first frame that reaches this point -
        // the GPU-backed one. The deltas they queue ride the drain below
        // like the font atlas does.
        if icons.is_none() {
            *icons = Some(icons::IconSet::load(egui_ctx));
        }

        // UI logic first: the dock reports the viewport tab's size, the GPU
        // passes follow (the imgui shell's ordering law - panels drive
        // extent).
        let mut full_output = egui_ctx.run_ui(raw_input, |ui| {
            shell_layout(
                ui,
                state,
                dock_state,
                tex_id,
                icons.as_ref(),
                viewport_points,
                left_press,
            );
        });
        let egui_delay = full_output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(std::time::Duration::MAX, |v| v.repaint_delay);
        let delay = redraw_delay(
            egui_delay,
            followup_redraw || !state.pending.is_empty() || state.layout_request.is_some(),
            state.import_in_progress(),
            state.sim.is_some_and(|sim| sim.playing),
            state.field_play.as_ref().is_some_and(|play| play.needs_poll()),
        );
        *next_repaint = delay.and_then(|d| now().checked_add(d));
        drain_textures(renderer, device, queue, &mut full_output);
        winit_state.handle_platform_output(window, full_output.platform_output);

        let clipped: Vec<_> = egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point)
            .into_iter()
            .filter(renderable_paint_job)
            .collect();
        frame_watch.mark("ui", now());

        let st = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(st)
            | wgpu::CurrentSurfaceTexture::Suboptimal(st) => st,
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Validation => {
                *next_repaint = now().checked_add(std::time::Duration::from_millis(16));
                return;
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                // Some platforms report this through wgpu without sending
                // WindowEvent::Occluded. Wait for the next window event.
                *next_repaint = None;
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                surface.configure(device, conf);
                *next_repaint = now().checked_add(std::time::Duration::from_millis(16));
                return;
            }
        };
        frame_watch.mark("acquire", now());
        let target = st.texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Pass 1: the 3D offscreen target (its own bind group, mvp from the
        // camera the panels' FrameAll and the orbit gestures just moved).
        let (ew, eh) = view.extent();
        let mvp = state.cam.mvp(f64::from(ew.max(1)) / f64::from(eh.max(1)));
        let eye = state.cam.eye_f32();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("egui-shell-frame"),
        });
        // The traced stock (M7.2 B3b) takes the sheet's slot when the eye
        // has a frame; the field view is walked once per Deviate arming.
        let mut traced = false;
        if let Some(se) = stock_eye.as_mut().filter(|_| !state.active_stock_hidden()) {
            if state.display != crate::state::DisplayMode::Wireframe {
                crate::actions::ensure_field_view(state);
            }
            let fv = match &state.field_view {
                Some(Ok(fv)) if state.display != crate::state::DisplayMode::Wireframe => Some(fv),
                _ => None,
            };
            match se.prepare(device, queue, &mut encoder, fv, &state.cam, &mvp, (ew, eh)) {
                Ok(t) => traced = t,
                Err(err) => {
                    state.status += &format!(" | traced stock: {err} - the sheet stands");
                }
            }
        }
        // M13.4: both verdict painters wear the viewer's palette.
        let pal = super::colorvision::palette_of(&state.world);
        view.set_palette(pal);
        if let Some(se) = stock_eye.as_mut() {
            se.set_palette(pal);
        }
        match stock_eye.as_ref().filter(|_| traced) {
            Some(se) => {
                let draw = |pass: &mut wgpu::RenderPass<'_>| se.draw(pass);
                view.render_with(queue, &mut encoder, &mvp, eye, state.display, Some(&draw));
            }
            None => view.render(queue, &mut encoder, &mvp, eye, state.display),
        }

        // Pass 2: egui over the surface.
        let sd = ScreenDescriptor {
            size_in_pixels: [conf.width, conf.height],
            pixels_per_point: scale as f32,
        };
        let mut buffer_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("egui-shell-buffers"),
        });
        let extra = renderer.update_buffers(device, queue, &mut buffer_encoder, &clipped, &sd);
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
        frame_watch.mark("encode", now());
        let mut submits = extra;
        submits.push(buffer_encoder.finish());
        submits.push(encoder.finish());
        queue.submit(submits);
        queue.present(st);
        frame_watch.mark("submit+present", now());
        if let Some(line) = frame_watch.end(now()) {
            eprintln!("{line}");
        }

        if state.request_quit {
            *exit_requested = true;
        }
    }
}

/// The GPU refusal, also as a native dialog: a console-less launch must
/// not end in a window that silently vanishes (Prime Directive).
fn refuse_dialog(err: &str) {
    let _ = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("CamCraft cannot start the viewport")
        .set_description(format!(
            "{err}

Try --gpu <adapter name> (or CAMCRAFT_GPU) to choose an adapter, or --gpu-backend dx12|vulkan|gl."
        ))
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

/// The whole shell for one egui pass: keyboard shortcuts, the ribbon and
/// status strips, and the three-leaf dock. Shared by the live frame (draw)
/// and the frame tests (tests.rs) so the two can never disagree about
/// layout; the borrow order is deliberate - each pane reborrows `state` in
/// sequence, the dock tabs only after the strips end.
pub(crate) fn shell_layout(
    ui: &mut egui::Ui,
    state: &mut State,
    dock: &mut DockState<Tab>,
    tex: egui::TextureId,
    icons: Option<&icons::IconSet>,
    viewport_points: &mut [f32; 2],
    left_press: &mut Option<[f32; 2]>,
) {
    // The document's theme lands here, on the frame edge (M6.1 C): the
    // world singleton is the truth (command law - undo, the picker, and
    // a project load all move it between frames), and resolution lives
    // on this side of the seam (layering). The per-frame cost is a
    // clone and an equality test; the parse and the style swap happen
    // only on the change edge, before `current()` is read below.
    state.sync_theme(ui.ctx());
    // Ephemeral hover clears every frame before anything recomputes it (the
    // imgui App::draw law); the viewport's update_hover and the property-page
    // chip hovers then own it until the pointer moves on.
    state.geo_hover = None;
    panes::shortcuts(ui, state);
    // The live theme, read once per pass: the strips and every pane below
    // share one token table, so the widget path and the painted ink cannot
    // disagree on a retheme (M6.1 A). `ribbon_h` tracks the cell box.
    let pal = theme::current();
    // The View door's checkmarks read the dock as it stands; a toggle the
    // door files lands right after the ribbon, before the dock lays out.
    layout::settle(state, dock);
    egui::Panel::top("ribbon")
        .frame(egui::Frame::NONE)
        .exact_size(theme::ribbon_h(&pal))
        .show(ui, |ui| {
            panes::ribbon(ui, &mut *state, &pal, icons);
        });
    layout::settle(state, dock);
    egui::Panel::bottom("status")
        .frame(egui::Frame::NONE)
        .exact_size(STATUS_H)
        .show(ui, |ui| {
            panes::status(ui, &mut *state, &pal, icons);
        });
    // #667: the Setups page pick sweep runs here, once a frame, whatever
    // the dock shows. Viewport clicks route inside the dock (egui_dock
    // draws leaves breadth-first, so in the default layout the viewport
    // draws before the properties leaf), so the sweep brackets it:
    // a properties pane that will not draw this frame - closed, or behind
    // another tab in its leaf - disarms every pick BEFORE the dock, and a
    // drawn pane's rows mark their setups during it, the rest disarmed
    // AFTER it. A pick left armed at a frame's end therefore had its rows
    // on screen that frame, and the next frame's click sees the result:
    // no frame where a hidden draft takes a click.
    if !layout::is_shown(dock, &Tab::Properties) {
        crate::axis_pick::disarm_all(state);
        // #673: the locate draft's form is on that pane too - dropped
        // here, before the dock routes a viewport release to it.
        crate::locate_pick::disarm(state);
    }
    state.locate_form_drawn = false;
    egui::CentralPanel::default().show(ui, |ui| {
        DockArea::new(&mut *dock)
            .style(DockStyle::from_egui(ui.style().as_ref()))
            // No per-area close-all: it is the X that took whole panes
            // away by accident. Tabs keep their own X where closable.
            .show_leaf_close_all_buttons(false)
            .show_inside(ui, &mut ShellTabs {
                state: &mut *state,
                tex,
                icons,
                viewport_points,
                left_press,
            });
    });
    // A locate draft whose form the dock did not draw is dropped (#566).
    crate::locate_pick::sweep(state);
    crate::axis_pick::disarm_hidden(state); // #664/#667: undrawn rows' picks stop
    // Last, so it floats over the dock: the M1.4 graphical tool editor.
    // An in-app egui::Window - this is the shell's only kind of extra
    // surface; the single-OS-window law stands. Closed by default
    // (`state.tool_editor` is None on a fresh State), so no panel below
    // it ever moves in the snapshot until something opens it.
    pages::tool_editor_window(ui.ctx(), state);
    // M1.7d: the G-code viewer - same window law as the tool editor (an
    // in-app egui::Window over the dock; the single-OS-window law
    // stands). None on a fresh State, closed by ✖, reopened by the next
    // successful post, so the snapshot never moves until a post happens.
    pages::gcode_window(ui.ctx(), state);
    // #265: a refused post's reason, kept on screen past the status
    // line's next write. None on a fresh State, so no snapshot moves.
    post_refusal::window(ui.ctx(), state);
    // M3.3 stage C: the machine editor - same window law as both above
    // (in-app egui::Window over the dock, None on a fresh State, so
    // snapshots never move until the Setup pane opens it).
    machine_editor::window(ui.ctx(), state);
    // #157: the Debug info window - same window law (closed on a fresh
    // State, opened from the status strip's bug button).
    super::debug_info::window(ui.ctx(), state);
    // #339 slice 6: the fixture library dialogs (None on a fresh State).
    super::fixture_library_dialog::window(ui.ctx(), state);
    // #107: the unsaved-changes prompt, last so it sits over everything.
    unsaved_prompt::window(ui.ctx(), state);
}

/// Feed this pass's CPU-texture deltas (the font atlas first of all) to the
/// renderer. 0.36's TexturesDelta asserts on drop unless drained - consume,
/// don't peek (the S1 panic this replaced was exactly that).
/// egui-wgpu 0.36 binds one buffer slice per mesh, including meshes with
/// no vertices or indices. wgpu 30 rejects those zero-length slices.
pub(crate) fn renderable_paint_job(job: &egui::ClippedPrimitive) -> bool {
    match &job.primitive {
        egui::epaint::Primitive::Mesh(mesh) =>
            !mesh.vertices.is_empty() && !mesh.indices.is_empty(),
        egui::epaint::Primitive::Callback(_) => true,
    }
}

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
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    /// #183: the MCP listener's wake-up - draw a frame, which answers it.
    fn user_event(&mut self, _el: &ActiveEventLoop, _event: ()) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let WindowEvent::Occluded(occluded) = &event {
            self.occluded = *occluded;
        }
        // Feed egui first (pointer/keyboard/text/scale), like the imgui
        // shell's platform.prepare_frame; the EventResponse (child-viewport
        // control flow) is inert for our single root viewport.
        let repaint = match (self.winit_state.as_mut(), self.window.as_ref()) {
            (Some(winit_state), Some(window)) => winit_state.on_window_event(window, &event).repaint,
            _ => false,
        };
        if repaint && !self.occluded
            && !matches!(event, WindowEvent::RedrawRequested | WindowEvent::Resized(_))
            && let Some(window) = &self.window
        {
            self.followup_redraw = true;
            window.request_redraw();
        }
        match &event {
            // #107: the window X is a Quit like Ctrl+Q, so unsaved changes
            // ask first. A shell whose GPU failed draws (and drains)
            // nothing, so it closes at once.
            WindowEvent::CloseRequested if self.failed => el.exit(),
            WindowEvent::CloseRequested => {
                crate::actions::quit(&mut self.state);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::Resized(size) => {
                self.pending_resize = Some(*size);
            }
            WindowEvent::Occluded(false) | WindowEvent::Focused(true) => {
                if let Some(window) = &self.window && !self.occluded {
                    self.followup_redraw = true;
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                let resized = if let Some(size) = self.pending_resize {
                    let deadline = resize_frame_deadline(self.last_resize_frame);
                    if std::time::Instant::now() < deadline {
                        return;
                    }
                    self.pending_resize = None;
                    self.on_resize(size);
                    self.followup_redraw = true;
                    true
                } else {
                    false
                };
                self.draw();
                if resized {
                    let done = std::time::Instant::now();
                    self.last_resize_frame = Some(done);
                    // The layout's View3D size lands on a follow-up frame.
                    // Keep that frame from immediately blocking the next
                    // batch of pointer and resize events.
                    if let (Some(next), Some(earliest)) = (
                        self.next_repaint.as_mut(),
                        done.checked_add(std::time::Duration::from_millis(16)),
                    ) {
                        *next = (*next).max(earliest);
                    }
                }
                if self.exit_requested || self.state.request_quit {
                    el.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // #144: an OS quit (macOS Cmd+Q / app-menu Quit) is a Quit like
        // the window X; the window comes forward so the prompt is seen.
        if super::os_quit::route_pending(&mut self.state) {
            if self.failed {
                el.exit();
                return;
            }
            if let Some(window) = &self.window {
                window.set_minimized(false);
                window.focus_window();
                window.request_redraw();
            }
        }
        if self.occluded {
            el.set_control_flow(ControlFlow::Wait);
            return;
        }
        let resize_due = self.pending_resize.map(|_| resize_frame_deadline(self.last_resize_frame));
        // A pending resize carries the repaint too. Its frame deadline
        // supersedes older egui timers, including an immediate follow-up.
        let next = resize_due.or(self.next_repaint);
        match (next, self.window.as_ref()) {
            (Some(deadline), Some(window)) if deadline <= std::time::Instant::now() => {
                self.next_repaint = None;
                window.request_redraw();
                el.set_control_flow(ControlFlow::Wait);
            }
            (Some(deadline), Some(_)) => el.set_control_flow(ControlFlow::WaitUntil(deadline)),
            _ => el.set_control_flow(ControlFlow::Wait),
        }
    }
}

/// A resize frame is enough to update the window promptly without making
/// GPU reconfiguration and rendering block every pointer event in the drag.
fn resize_frame_deadline(last: Option<std::time::Instant>) -> std::time::Instant {
    last.and_then(|t| t.checked_add(std::time::Duration::from_millis(16)))
        .unwrap_or_else(std::time::Instant::now)
}

/// Idle documents need no redraw. Active egui animation, queued commands,
/// imports and simulation each name the next useful frame.
fn redraw_delay(
    egui_delay: std::time::Duration,
    pending: bool,
    importing: bool,
    sim_playing: bool,
    field_play_pending: bool,
) -> Option<std::time::Duration> {
    use std::time::Duration;
    let mut delay = egui_delay;
    if pending {
        delay = Duration::ZERO;
    }
    if importing || field_play_pending {
        delay = delay.min(Duration::from_millis(50));
    }
    if sim_playing {
        delay = delay.min(Duration::from_millis(16));
    }
    (delay != Duration::MAX).then_some(delay)
}

#[cfg(test)]
mod redraw_tests {
    use super::redraw_delay;
    use std::time::Duration;

    #[test]
    fn idle_document_needs_no_frame_timer() {
        assert_eq!(
            redraw_delay(Duration::MAX, false, false, false, false),
            None
        );
    }

    #[test]
    fn queued_and_live_work_keep_the_window_moving() {
        assert_eq!(
            redraw_delay(Duration::MAX, true, false, false, false),
            Some(Duration::ZERO)
        );
        assert_eq!(
            redraw_delay(Duration::MAX, false, true, false, false),
            Some(Duration::from_millis(50))
        );
        assert_eq!(
            redraw_delay(Duration::MAX, false, false, true, false),
            Some(Duration::from_millis(16))
        );
        assert_eq!(
            redraw_delay(Duration::from_millis(10), false, true, false, false),
            Some(Duration::from_millis(10))
        );
    }
}

impl Shell {
    fn shutdown(&mut self) {
        // Surface/renderer first, window last (see the field-order note);
        // `state` (and its world) drops after all GPU objects.
        // #183: the MCP listener stops (and its thread is joined) first,
        // so no wake-up reaches a closing loop.
        self.state.mcp = None;
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
    icons: Option<&'a icons::IconSet>,
    viewport_points: &'a mut [f32; 2],
    left_press: &'a mut Option<[f32; 2]>,
}

impl TabViewer for ShellTabs<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        // Title-derived: stable across frames, collision-free while the
        // dockspace holds one tab per title (true for the whole migration).
        egui::Id::new(tab.title())
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.title().into()
    }

    /// The viewport is permanent: no X, no middle-click close, no
    /// context-menu close, and a float holding it cannot close either.
    /// The browser and the properties pane may close - the ribbon's
    /// View door reopens them (layout.rs).
    fn is_closeable(&self, tab: &Self::Tab) -> bool {
        layout::closeable(tab)
    }

    /// The viewport never floats: it stays in the main dock, so the
    /// main surface always has a root to reopen panels against.
    fn allowed_in_windows(&self, tab: &mut Self::Tab) -> bool {
        layout::closeable(tab)
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            Tab::Viewport => {
                viewport_panel(
                    ui,
                    self.state,
                    self.tex,
                    self.viewport_points,
                    self.left_press,
                );
            }
            Tab::Browser => {
                // Same seam, same table: every reader of the live theme
                // calls `current()` (M6.1 C makes it document state).
                let pal = theme::current();
                panes::browser(ui, self.state, &pal, self.icons);
            }
            Tab::Properties => {
                let pal = theme::current();
                pages::properties(ui, self.state, &pal);
            }
            Tab::Machines => {
                let pal = theme::current();
                machines_pane::draw(ui, self.state, &pal);
            }
        }
    }
}

/// The viewport tab: the 3D image, the pointer law, and the screen-space
/// overlays. M-e moved the law to `crate::interact`, where the retired
/// imgui shell had it first: a short LMB press picks (Ctrl adds, the
/// gesture table), an LMB drag past the click-travel threshold orbits, MMB
/// orbits, RMB pans, wheel dollies toward the cursor. egui's press/drag
/// bookkeeping differs from winit's raw stream, so the gestures are read
/// off `ui.input` directly, the same CLICK_TRAVEL_PX law applied by hand;
/// the spike's simplified mapping is retired.
fn viewport_panel(
    ui: &mut egui::Ui,
    state: &mut State,
    tex: egui::TextureId,
    viewport_points: &mut [f32; 2],
    left_press: &mut Option<[f32; 2]>,
) {
    // M1.7c move simulation: the timeline is rebuilt from the world on
    // every document change (`cached_script` below, keyed on
    // `doc_version`), which is why a document switch needs no reset -
    // the playhead simply indexes the new program. The transport
    // bar takes its strip BEFORE the image so `avail` measures what the 3D
    // view actually gets. Like the view cube and the hover hint, all of it
    // draws only while the ribbon cell is armed: a snapshot with no
    // simulation running sees zero churn. The playhead is immediate-mode
    // view state (AGENTS.md's explicit exemption), never a command.
    let mut sim_marker: Option<([f64; 3], [f64; 3])> = None;
    if state.sim.is_some() {
        let sc = cached_script(state);
        let total = sc.total.max(0.0);
        let unit = camcraft_app::units::of(&state.world);
        if let Some(sim) = state.sim.as_mut() {
            // Clamp a stalled frame's dt: a hiccup never teleports the
            // tool through material.
            sim.advance(f64::from(ui.input(|i| i.stable_dt).clamp(0.0, 0.1)), total);
        }
        let mut stop = false;
        ui.horizontal(|ui| {
            let Some(sim) = state.sim.as_mut() else {
                return;
            };
            let at_end = total > 0.0 && !sim.playing && sim.t >= total;
            let label = if sim.playing { "Pause" } else { "Play" };
            if ui
                .button(label)
                .on_hover_text("Start or stop the tool at the playhead")
                .clicked()
            {
                if at_end {
                    // Pressing Play on a finished run rewinds; the
                    // transport has no separate back-to-start button.
                    sim.t = 0.0;
                    sim.playing = true;
                } else {
                    sim.playing = !sim.playing;
                }
            }
            if ui.button("Stop").on_hover_text("Leave simulation").clicked() {
                stop = true;
            }
            if total > 0.0 {
                ui.add(egui::Slider::new(&mut sim.t, 0.0..=total).show_value(false));
                egui::ComboBox::from_id_salt("sim_speed")
                    .selected_text(format!("{:.0}x", sim.speed))
                    .show_ui(ui, |ui| {
                        for sp in [1.0, 2.0, 4.0, 8.0] {
                            ui.selectable_value(&mut sim.speed, sp, format!("{sp:.0}x"));
                        }
                    });
                // Readout AFTER the widgets: the scrub sets t this frame.
                let t = sim.t;
                ui.label(sim_readout(&sc, t, unit));
            } else {
                ui.label(if sc.truncated {
                    "timeline truncated - too many moves to time"
                } else {
                    "nothing to run"
                });
            }
        });
        if stop {
            state.sim = None;
        } else if !sc.is_empty()
            && let Some(sim) = &state.sim
        {
            let f = sc.at(sim.t);
            sim_marker = Some(([f.pos.x, f.pos.y, f.pos.z], f.axis));
        }
        ui.separator();
    }
    // M2.3 stage B: the Deviate verdict bar, parallel to the transport -
    // it takes its strip before the image, and draws only while the
    // sheet is armed (a snapshot with no verdict sees zero churn). The
    // sentence is re-derived from the frozen verdict every frame through
    // the one pure string (actions::deviate_readout - the status line
    // says the same thing at arming time, from the same function). The
    // buttons fly the camera to the worst rest and gouge cells: a
    // located verdict you cannot look at is only half located. Ephemeral
    // like every other camera move - no command.
    if let Some(ps) = &state.program_sim {
        let mut fly: Option<[f64; 3]> = None;
        let mut export = false;
        ui.horizontal(|ui| {
            ui.label(crate::actions::deviate_readout(&ps.dev, state.units()));
            if ps.dev.rest_cells > 0
                && let Some(q) = ps.dev.max_rest_at
                && ui.button("Frame rest").on_hover_text("Look at the worst uncut cell").clicked()
            {
                fly = Some([q.x, q.y, q.z]);
            }
            if ps.dev.gouge_cells > 0
                && let Some(q) = ps.dev.max_gouge_at
                && ui.button("Frame gouge").on_hover_text("Look at the deepest overcut cell").clicked()
            {
                fly = Some([q.x, q.y, q.z]);
            }
            // M7.2 D: what the program leaves, out as a mesh.
            if ui
                .button("Export STL")
                .on_hover_text("Write the machined stock as an STL mesh (extracted from the traced field)")
                .clicked()
            {
                export = true;
            }
        });
        if let Some(p) = fly {
            crate::actions::frame_at(state, p);
        }
        if export {
            state.pending.push_back(crate::state::CommandKind::ExportStockDialog);
        }
        ui.separator();
    }
    let avail = ui.available_size();
    if avail.x > 1.0 && avail.y > 1.0 {
        // The 3D extent deferral (last frame's layout sizes next frame's
        // target); `interact` hit-tests pointers against viewport_rect,
        // set below - no separate size channel since M-g.
        *viewport_points = [avail.x.max(1.0), avail.y.max(1.0)];
    }
    let resp = ui.add(
        egui::Image::from_texture(egui::load::SizedTexture::new(tex, avail))
            .max_size(avail)
            .sense(egui::Sense::click_and_drag()),
    );
    // The pane's rect is the projection screen, so hit-tests take the same
    // global pointer coords imgui's viewport_rect fed the raw-event path.
    let ([rx0, ry0], [rx1, ry1]) = (parr(resp.rect.min), parr(resp.rect.max));
    state.viewport_rect = ([rx0, ry0], [rx1, ry1]);
    let hot = resp.hovered();
    state.viewport_hot = hot;
    let (rw, rh) = (rx1 - rx0, ry1 - ry0);
    // The M1.5 corner furniture, laid out from the pane rect alone: the
    // pointer law consults it before it arms any gesture, the overlay
    // pass draws it last, and both calls see the same rects.
    let hud = viewcube::Hud::new(resp.rect);

    // One wheel notch must dolly by one line in both shells: egui bakes
    // `line_scroll_speed` pixels into smooth_scroll_delta, so undo it here
    // (the imgui shell sees raw mouse_wheel = notches).
    let line_px = ui.ctx().options(|o| o.input_options.line_scroll_speed).max(1.0);
    let (pos, mods, mid_press, right_press, left_pressed, left_released, other_released, motion, wheel) =
        ui.input(|a| {
            let p = &a.pointer;
            (
                p.interact_pos().or_else(|| p.hover_pos()),
                interact::Mods {
                    ctrl: a.modifiers.ctrl,
                    shift: a.modifiers.shift,
                    alt: a.modifiers.alt,
                },
                p.button_pressed(egui::PointerButton::Middle),
                p.button_pressed(egui::PointerButton::Secondary),
                p.button_pressed(egui::PointerButton::Primary),
                p.button_released(egui::PointerButton::Primary),
                p.button_released(egui::PointerButton::Middle)
                    || p.button_released(egui::PointerButton::Secondary),
                // Frame-to-frame pointer delta. egui's `motion()` only
                // fills from raw OS motion events (never from PointerMoved
                // in a kittest harness), while `delta()` tracks every
                // position change in both the window and the harness - the
                // per-frame motion both shells want.
                p.delta(),
                if a.smooth_scroll_delta.y != 0.0 {
                    Some(a.smooth_scroll_delta.y / line_px)
                } else {
                    None
                },
            )
        });

    // Presses - only over the pane, like the imgui hot-window gate. A press
    // is remembered, not acted on: the release decides pick vs orbit. The
    // corner furniture answers first (M1.5): a press inside it acts at
    // press time and is never remembered, so the release-pick below sees
    // no press to release and the motion pass sees nothing to promote -
    // the whole gesture is absorbed, exactly like a ribbon button's.
    if hot && let Some(p) = pos {
        let over_hud = hud.as_ref().is_some_and(|h| h.contains(p));
        if let Some(h) = &hud
            && over_hud
            && left_pressed
        {
            viewcube::click(state, h, p);
        }
        if !over_hud && interact::inside_viewport(state, parr(p)) {
            if mid_press {
                state.drag = Some(DragMode::Orbit);
            }
            if right_press {
                state.drag = Some(DragMode::Pan);
            }
            if left_pressed {
                *left_press = Some(parr(p));
            }
        }
        if let (Some(steps), Some(p)) = (wheel, pos)
            && rw > 1.0
            && rh > 1.0
        {
            let [x, y] = parr(p);
            let ndc = [(x - rx0) / rw * 2.0 - 1.0, 1.0 - (y - ry0) / rh * 2.0];
            state.cam.dolly_cursor(ndc, (rw / rh) as f64, steps);
        }
    }

    // Releases. A short press released inside the pane is a pick; a same-
    // frame press+release (fast click) is handled here, not at the press.
    if left_released {
        let press = left_press.take();
        if state.drag == Some(DragMode::Orbit) {
            state.drag = None;
        }
        if let (Some(q), Some(p)) = (press, pos)
            && (p - egui::pos2(q[0], q[1])).length() <= interact::CLICK_TRAVEL_PX
            && interact::inside_viewport(state, parr(p))
        {
            interact::pick_at(state, parr(p), mods);
        }
    }
    if other_released && state.drag.is_some() {
        state.drag = None;
    }

    // Motion: drags move the camera; a held-but-not-yet-dragging left
    // button promotes to orbit past the travel threshold. Hover recomputes
    // every frame (imgui checks inside draw, so a parked cursor keeps its
    // feature outline); interact gates it on drag/hot/inside itself.
    if motion != egui::Vec2::ZERO {
        match state.drag {
            Some(DragMode::Orbit) => state.cam.orbit(motion.x, motion.y),
            Some(DragMode::Pan) => state.cam.pan(motion.x, motion.y),
            None => {
                if let (Some(q), Some(p)) = (*left_press, pos)
                    && p.distance(egui::pos2(q[0], q[1])) > interact::CLICK_TRAVEL_PX
                {
                    state.drag = Some(DragMode::Orbit);
                    state.cam.orbit(motion.x, motion.y);
                }
            }
        }
    }
    if let Some(p) = pos {
        interact::update_hover(state, parr(p), left_press.is_some());
    }

    // Screen-space overlays on the image: axis/anchor/gizmo labels and the
    // hovered feature's outline, projected through the same camera math.
    if rw > 1.0 && rh > 1.0 {
        let aspect = (rw / rh) as f64;
        // 0.36 has no Style::text_style lookup - text_styles is the map.
        let font = ui
            .style()
            .text_styles
            .get(&egui::TextStyle::Small)
            .cloned()
            .unwrap_or_else(|| egui::FontId::proportional(11.0));
        let to_px = |uv: [f32; 2]| egui::pos2(rx0 + uv[0] * rw, ry0 + uv[1] * rh);
        let rgb8 = |rgb: [f32; 3]| {
            egui::Color32::from_rgb(
                (rgb[0] * 255.0).clamp(0.0, 255.0) as u8,
                (rgb[1] * 255.0).clamp(0.0, 255.0) as u8,
                (rgb[2] * 255.0).clamp(0.0, 255.0) as u8,
            )
        };
        // #195: the origin triads, chunky - a dark outline, then the axis
        // colour - over the 1 px GPU lines (a wgpu line has no width).
        let shade = egui::Color32::from_black_alpha(170);
        let segs = interact::gizmo_segments(state, aspect);
        for (a, b, _) in &segs {
            ui.painter().line_segment([to_px(*a), to_px(*b)], egui::Stroke::new(interact::GIZMO_OUTLINE_PX, shade));
        }
        for (a, b, rgb) in &segs {
            ui.painter().line_segment([to_px(*a), to_px(*b)], egui::Stroke::new(interact::GIZMO_STROKE_PX, rgb8(*rgb)));
        }
        let axis_font = egui::FontId::proportional((font.size * 1.4).max(14.0));
        for (uv, rgb, text) in interact::overlay_labels(state, aspect) {
            let at = to_px(uv);
            if interact::is_axis_label(&text) {
                // Bigger, with a shadow, so a letter reads on any ground.
                ui.painter().text(at + egui::vec2(1.0, 1.0), egui::Align2::LEFT_TOP, &text, axis_font.clone(), shade);
                ui.painter().text(at, egui::Align2::LEFT_TOP, text, axis_font.clone(), rgb8(rgb));
            } else {
                ui.painter().text(at, egui::Align2::LEFT_TOP, text, font.clone(), rgb8(rgb));
            }
        }
        for (a, b) in interact::hover_features(state, aspect) {
            ui.painter().line_segment(
                [
                    egui::pos2(rx0 + a[0] * rw, ry0 + a[1] * rh),
                    egui::pos2(rx0 + b[0] * rw, ry0 + b[1] * rh),
                ],
                egui::Stroke::new(1.0, egui::Color32::from_rgb(255, 217, 102)),
            );
        }
        // The machined mesh's hovered facet (#25): the hover ink's yellow,
        // a veil over the class colour and its outline.
        if let Some(tri) = crate::machined::hover_facet_uv(state, aspect) {
            let ink = egui::Color32::from_rgb(255, 217, 102);
            ui.painter().add(egui::Shape::convex_polygon(
                tri.iter().map(|uv| to_px(*uv)).collect(),
                egui::Color32::from_rgba_unmultiplied(255, 217, 102, 90),
                egui::Stroke::new(1.5, ink),
            ));
        }
        // The Boundary group's marks (M11.5 UX): role-coloured picked
        // edges, the edge a click would take, chain ends, machined side.
        for m in crate::boundary_viz::boundary_marks(state, aspect) {
            ui.painter().line_segment(
                [
                    egui::pos2(rx0 + m.a[0] * rw, ry0 + m.a[1] * rh),
                    egui::pos2(rx0 + m.b[0] * rw, ry0 + m.b[1] * rh),
                ],
                egui::Stroke::new(m.width, super::colorvision::ink(m.rgb)),
            );
        }
        // The tool marker, last of the scene-space furniture but under the
        // chrome: a white ring at the tool point with three stubs along the
        // axes it actually moves on, projected through the same camera math
        // as the labels. Stubs keep screen-space length (a zoomed-out view
        // must not lose the marker) and drop when they project away, so a
        // behind-the-camera point never draws garbage.
        if let Some((p, axis)) = sim_marker {
            let mvp = state.cam.mvp(aspect);
            if let Some(u) = interact::project_uv(&mvp, p) {
                let c = egui::pos2(rx0 + u[0] * rw, ry0 + u[1] * rh);
                for (d, rgb) in [
                    ([4.0, 0.0, 0.0], (235u8, 90, 90)),
                    ([0.0, 4.0, 0.0], (90, 210, 110)),
                    ([0.0, 0.0, 4.0], (100, 140, 255)),
                ] {
                    let q = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
                    if let Some(v) = interact::project_uv(&mvp, q) {
                        let dir = egui::pos2(rx0 + v[0] * rw, ry0 + v[1] * rh) - c;
                        let len = dir.length();
                        if len > 0.5 {
                            let k = len.min(12.0) / len;
                            ui.painter().line_segment(
                                [c, c + dir * k],
                                egui::Stroke::new(
                                    1.5,
                                    egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2),
                                ),
                            );
                        }
                    }
                }
                // The tool axis (M5.1 stage G): a white stub up the
                // spindle, so a leaning head LEANS in the viewport and
                // the mid-turn law has a visible referent. It points
                // tip-to-spindle, standing in where the tool's real
                // body would be; on a prismatic path it is the Z stub
                // a hair longer and reads as before.
                let tip = [
                    p[0] + axis[0] * 10.0,
                    p[1] + axis[1] * 10.0,
                    p[2] + axis[2] * 10.0,
                ];
                if let Some(v) = interact::project_uv(&mvp, tip) {
                    let dir = egui::pos2(rx0 + v[0] * rw, ry0 + v[1] * rh) - c;
                    let len = dir.length();
                    if len > 0.5 {
                        let k = len.min(26.0) / len;
                        ui.painter().line_segment(
                            [c, c + dir * k],
                            egui::Stroke::new(2.0, egui::Color32::from_gray(225)),
                        );
                    }
                }
                ui.painter().circle_stroke(
                    c,
                    5.0,
                    egui::Stroke::new(1.5, egui::Color32::from_gray(240)),
                );
                ui.painter()
                    .circle_filled(c, 2.0, egui::Color32::WHITE);
            }
        }
        // The G-code viewer's sync cursor: the move that line posts,
        // drawn amber over the paths - the segment arriving at the
        // scroll line's position, plus a ring at its end. Same display
        // law as the tool marker: reads view state, never the world,
        // and drops points that project behind the camera.
        if let Some((a, b)) = gcode_segment(state) {
            let mvp = state.cam.mvp(aspect);
            let pa = interact::project_uv(&mvp, a)
                .map(|u| egui::pos2(rx0 + u[0] * rw, ry0 + u[1] * rh));
            let pb = interact::project_uv(&mvp, b)
                .map(|u| egui::pos2(rx0 + u[0] * rw, ry0 + u[1] * rh));
            let amber = egui::Color32::from_rgb(255, 170, 60);
            if let (Some(pa), Some(pb)) = (pa, pb) {
                ui.painter()
                    .line_segment([pa, pb], egui::Stroke::new(2.5, amber));
            }
            if let Some(pb) = pb {
                ui.painter().circle_stroke(pb, 4.0, egui::Stroke::new(1.5, amber));
            }
        }
        // The Deviate verdict, located: a ring at the worst rest cell
        // (the sheet's amber) and the worst gouge (its red) - the two
        // numbers that carry the sentence in the verdict bar, drawn
        // where they happened. Same display law as every other overlay:
        // reads the frozen verdict, never the world, and drops points
        // that project behind the camera.
        if let Some(ps) = &state.program_sim {
            let mvp = state.cam.mvp(aspect);
            // M13.4: the rings wear the colour-vision palette (Off's
            // rest/gouge round to the 250,184,51 / 230,46,36 they were).
            let pal = super::colorvision::palette_of(&state.world);
            for (q, col) in [
                (ps.dev.max_rest_at, super::colorvision::ink(pal.rest)),
                (ps.dev.max_gouge_at, super::colorvision::ink(pal.gouge)),
            ] {
                let Some(q) = q else { continue };
                if let Some(u) = interact::project_uv(&mvp, [q.x, q.y, q.z]) {
                    let c = egui::pos2(rx0 + u[0] * rw, ry0 + u[1] * rh);
                    ui.painter().circle_stroke(c, 6.0, egui::Stroke::new(2.0, col));
                    ui.painter().circle_filled(c, 1.5, col);
                }
            }
        }
        // Corner hint: the control legend, or the live mode while dragging.
        let hint = match state.drag {
            Some(DragMode::Orbit) => "orbit",
            Some(DragMode::Pan) => "pan",
            None => "LMB pick / drag-orbit · MMB orbit · RMB pan · wheel · F frame",
        };
        if hot || state.drag.is_some() {
            ui.painter().text(
                resp.rect.max - egui::vec2(12.0, 12.0),
                egui::Align2::RIGHT_BOTTOM,
                hint,
                font.clone(),
                egui::Color32::from_white_alpha(200),
            );
        }
        // The corner furniture paints last of all: opaque over the scene,
        // and it lights the feature a click would command.
        if let Some(h) = &hud {
            let hp = if state.drag.is_none() {
                pos.filter(|p| hot && h.contains(*p))
            } else {
                None
            };
            viewcube::draw(ui, h, &state.cam, state, hp, font);
        }
    }
    // The furniture asks for the pointer's hand, not the crosshair.
    if hot && hud.as_ref().is_some_and(|h| pos.is_some_and(|p| h.contains(p))) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

/// The transport bar's right-hand text: `{op} · {verb}[ · A/B/C] ·
/// {t}/{total} s`, the verb the current segment reports (RAPID, the live
/// feed through the units lens, DWELL n s, END at the finish, STANDBY
/// before the first move), and - since M5.1 stage G - the commanded
/// letters whenever the head is off home. Pure and `pub(crate)` so the
/// tests pin the string law without a window.
pub(crate) fn sim_readout(
    sc: &camcraft_app::sim::Script,
    t: f64,
    unit: camcraft_app::units::Unit,
) -> String {
    let f = sc.at(t);
    let verb = match f.readout {
        camcraft_app::sim::Readout::Rapid => "RAPID".to_owned(),
        camcraft_app::sim::Readout::Feed(v) => {
            format!("F {} {}", camcraft_app::units::feed(v, unit), unit.feed_suffix())
        }
        camcraft_app::sim::Readout::Dwell(x) => format!("DWELL {x:.1} s"),
        camcraft_app::sim::Readout::Idle if f.done => "END".to_owned(),
        camcraft_app::sim::Readout::Idle => "STANDBY".to_owned(),
    };
    let op = f
        .op
        .and_then(|i| sc.ops.get(i))
        .map_or("(none)", |s| s.as_str());
    // The head's letters (M5.1 stage G), stated only while it is off
    // home: a prismatic program's readout is the exact string it
    // always was (the transport-law tests pin it), and the moment
    // the timeline carries a tilt, the numbers the post wrote and
    // the mid-turn law judged become readable mid-swing.
    let pose = if camcraft_app::cl::is_vertical(&f.axis) {
        String::new()
    } else {
        let [a, b, c] = camcraft_app::cl::axis_to_angles(f.axis);
        format!(" · A{a:.0} B{b:.0} C{c:.0}")
    };
    format!("{op} · {verb}{pose} · {t:.1}/{:.1} s", sc.total.max(0.0))
}

/// The timeline for this document version: `sim_script` once per
/// `doc_version` bump, shared by the transport and the traced
/// playback, so no frame re-walks (or re-clones) every op's path.
pub(crate) fn cached_script(state: &mut State) -> std::sync::Arc<camcraft_app::sim::Script> {
    if let Some((v, sc)) = &state.sim_script_cache
        && *v == state.doc_version
    {
        return std::sync::Arc::clone(sc);
    }
    let sc = std::sync::Arc::new(sim_script(state));
    state.sim_script_cache = Some((state.doc_version, std::sync::Arc::clone(&sc)));
    sc
}

/// Rebuild the timeline from the live world (`cached_script` is the
/// per-frame door): the same
/// tree-order walk the post takes (hidden paths included - hidden is
/// display, not suppression; the law stands on `State::has_toolpaths`),
/// so the transport clock and the posted program count the same moves.
pub(crate) fn sim_script(state: &State) -> camcraft_app::sim::Script {
    use camcraft_app::{model, posting, setup, sim};
    let tr = sim::Traverse::from_machine(&posting::machine_value(&state.world));
    let paths: Vec<(String, camcraft_app::cl::Toolpath)> = setup::operation_order(&state.world)
        .into_iter()
        .filter_map(|(n, e)| model::toolpath_of(&state.world, &e).map(|tp| (n, tp.path)))
        .collect();
    sim::script(paths.iter().map(|(n, t)| (n.as_str(), t)), &tr)
}

/// The move the G-code viewer's cursor points at, as the segment that
/// arrives at it: the last recorded position at or before the scroll
/// line, drawn from the one before that (so the highlight is the path's
/// own polyline, not a floating point - the first move's segment
/// collapses to its endpoint ring). None with no viewer up, the window
/// closed, or a map that holds no position yet. Pure, like the readout
/// - the viewport pass draws what this resolves, nothing more.
pub(crate) fn gcode_segment(state: &State) -> Option<([f64; 3], [f64; 3])> {
    let view = state.post_view.as_ref()?;
    if !view.open {
        return None;
    }
    let cursor = view.line.min(view.line_pos.len().saturating_sub(1));
    let mut to = None;
    let mut from = None;
    for (i, p) in view.line_pos.iter().enumerate() {
        if i > cursor {
            break;
        }
        if let Some(p) = p {
            from = to;
            to = Some(*p);
        }
    }
    let to = to?;
    Some((from.unwrap_or(to), to))
}

/// egui Pos2 -> the `[f32; 2]` convention interact.rs speaks (0.36 has no
/// `to_array` on Pos2).
fn parr(p: egui::Pos2) -> [f32; 2] {
    [p.x, p.y]
}

/// Headless proof (the M-a gate, graduated from the spike's S2 gate): no
/// window, no surface - `headless_gpu::device()` (software adapter first,
/// else hardware; the same choice as tests/gpu_render.rs), View3D on the shell's Rgba8Unorm offscreen
/// format, the SEEDED document rendered (block + stock + region + cached
/// path through set_overlay), and the readback showing a solid footprint of
/// changed pixels. The second half of the proof is the world itself:
/// Contour1 carries a non-empty cached toolpath, i.e. seed -> commands ->
/// generate worked.
pub(crate) mod headless {
    use camcraft_app::model;

    use crate::state::{CommandKind, State};
    use crate::viewport::{Camera, View3D};

    use super::VIEW3D_FORMAT;
    use super::wgpu;
    use super::world;

    pub(crate) fn run() -> Result<(), String> {
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

        // Software adapter first, else the hardware one (#615: macOS's
        // Metal has no software fallback).
        let gpu = crate::headless_gpu::device()?;
        println!("headless: {} adapter {}", gpu.kind.label(), gpu.name);
        let (device, queue) = (gpu.device, gpu.queue);

        let mut view = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            View3D::new(&device, VIEW3D_FORMAT, 320, 240)
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
            ..Camera::default()
        };
        let (w, h) = view.extent();
        let mvp = cam.mvp(f64::from(w.max(1)) / f64::from(h.max(1)));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("egui-shell-headless"),
        });
        view.render(&queue, &mut encoder, &mvp, cam.eye_f32(), state.display);
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
