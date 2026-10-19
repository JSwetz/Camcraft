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

use crate::interact;
use crate::state::{DragMode, State};
use crate::viewport::View3D;

use super::icons;
use super::pages;
use super::panes;
use super::theme;
use super::viewcube;
use super::world;

const VIEW3D_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Ribbon strip height: imgui's `height()` formula - `2*PAD_V + cell_h +
/// 5 + line` = 8 + 35 + 5 + 13. Since M-g the cells are that formula's
/// real size (icon 20 over the label at +22, captions at top+44), where
/// the pre-icon spike squeezed them into 49.
pub(crate) const RIBBON_H: f32 = 61.0;
/// Status strip height (imgui's single-line bottom bar).
const STATUS_H: f32 = 22.0;

/// The windowed shell (the `--ui egui` entry without `--headless`); returns
/// the process exit code.
pub(crate) fn run_windowed() -> i32 {
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
    state: State,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Tab {
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

/// The shell's fixed three-pane layout, as egui_dock splits: the browser
/// takes 17% at the left and the properties pane 22% at the right of what
/// remains - shell.rs's DockLayout ratios, restated as split fractions.
/// A fraction governs the TOP/LEFT child (SplitNode docs), and a `Left`
/// split puts the NEW node left, a `Right` split the OLD one - so the first
/// fraction is the browser's share, the second the viewport's.
pub(crate) fn build_dock() -> DockState<Tab> {
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
        // M-d: startup is a plain State::new() - byte-identical in shape to
        // imgui's App::new. The demo seed left the window path; world::seed
        // is what the harnesses and the headless proof call directly now.
        // File work is the real thing: New/Model/Project/Save on the ribbon
        // queue the shared command kinds, rfd picks the path, State::run
        // does the work - the machinery was never egui's to rebuild.
        let state = State::new();
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
            icons: None,
            viewport_points: [600.0, 400.0],
            left_press: None,
            scene_digest: 0,
            exit_code: 0,
            failed: false,
            exit_requested: false,
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
            .with_title("HSMWorks (egui shell)")
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
            icons,
            viewport_points,
            left_press,
            scene_digest,
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
            label: Some("egui-shell-frame"),
        });
        view.render(queue, &mut encoder, &mvp, eye, state.display);

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
        let mut submits = extra;
        submits.push(buffer_encoder.finish());
        submits.push(encoder.finish());
        queue.submit(submits);
        queue.present(st);

        if state.request_quit {
            *exit_requested = true;
        }
    }
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
    // Ephemeral hover clears every frame before anything recomputes it (the
    // imgui App::draw law); the viewport's update_hover and the property-page
    // chip hovers then own it until the pointer moves on.
    state.geo_hover = None;
    panes::shortcuts(ui, state);
    egui::Panel::top("ribbon")
        .frame(egui::Frame::NONE)
        .exact_size(RIBBON_H)
        .show(ui, |ui| {
            panes::ribbon(ui, &mut *state, &theme::Palette::DARK, icons);
        });
    egui::Panel::bottom("status")
        .frame(egui::Frame::NONE)
        .exact_size(STATUS_H)
        .show(ui, |ui| {
            panes::status(ui, &*state, &theme::Palette::DARK);
        });
    egui::CentralPanel::default().show(ui, |ui| {
        DockArea::new(&mut *dock)
            .style(DockStyle::from_egui(ui.style().as_ref()))
            .show_inside(ui, &mut ShellTabs {
                state: &mut *state,
                tex,
                icons,
                viewport_points,
                left_press,
            });
    });
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
                panes::browser(ui, self.state, &theme::Palette::DARK, self.icons);
            }
            Tab::Properties => {
                pages::properties(ui, self.state, &theme::Palette::DARK);
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
    // M1.7c move simulation: the timeline is rebuilt from the world every
    // frame (`sim_script` below), which is why a document switch needs no
    // reset - the playhead simply indexes the new program. The transport
    // bar takes its strip BEFORE the image so `avail` measures what the 3D
    // view actually gets. Like the view cube and the hover hint, all of it
    // draws only while the ribbon cell is armed: a snapshot with no
    // simulation running sees zero churn. The playhead is immediate-mode
    // view state (AGENTS.md's explicit exemption), never a command.
    let mut sim_marker: Option<[f64; 3]> = None;
    if state.sim.is_some() {
        let sc = sim_script(state);
        let total = sc.total.max(0.0);
        let unit = hsmworks_app::units::of(&state.world);
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
            let p = sc.at(sim.t).pos;
            sim_marker = Some([p.x, p.y, p.z]);
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
        for (uv, rgb, text) in interact::overlay_labels(state, aspect) {
            ui.painter().text(
                egui::pos2(rx0 + uv[0] * rw, ry0 + uv[1] * rh),
                egui::Align2::LEFT_TOP,
                text,
                font.clone(),
                egui::Color32::from_rgb(
                    (rgb[0] * 255.0).clamp(0.0, 255.0) as u8,
                    (rgb[1] * 255.0).clamp(0.0, 255.0) as u8,
                    (rgb[2] * 255.0).clamp(0.0, 255.0) as u8,
                ),
            );
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
        // The tool marker, last of the scene-space furniture but under the
        // chrome: a white ring at the tool point with three stubs along the
        // axes it actually moves on, projected through the same camera math
        // as the labels. Stubs keep screen-space length (a zoomed-out view
        // must not lose the marker) and drop when they project away, so a
        // behind-the-camera point never draws garbage.
        if let Some(p) = sim_marker {
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

/// The transport bar's right-hand text: `{op} · {verb} · {t}/{total} s`,
/// the verb the current segment reports (RAPID, the live feed through the
/// units lens, DWELL n s, END at the finish, STANDBY before the first
/// move). Pure and `pub(crate)` so the tests pin the string law without a
/// window.
pub(crate) fn sim_readout(
    sc: &hsmworks_app::sim::Script,
    t: f64,
    unit: hsmworks_app::units::Unit,
) -> String {
    let f = sc.at(t);
    let verb = match f.readout {
        hsmworks_app::sim::Readout::Rapid => "RAPID".to_owned(),
        hsmworks_app::sim::Readout::Feed(v) => {
            format!("F {} {}", hsmworks_app::units::feed(v, unit), unit.feed_suffix())
        }
        hsmworks_app::sim::Readout::Dwell(x) => format!("DWELL {x:.1} s"),
        hsmworks_app::sim::Readout::Idle if f.done => "END".to_owned(),
        hsmworks_app::sim::Readout::Idle => "STANDBY".to_owned(),
    };
    let op = f
        .op
        .and_then(|i| sc.ops.get(i))
        .map_or("(none)", |s| s.as_str());
    format!("{op} · {verb} · {t:.1}/{:.1} s", sc.total.max(0.0))
}

/// Rebuild the timeline from the live world, every frame: the same
/// tree-order walk the post takes (hidden paths included - hidden is
/// display, not suppression; the law stands on `State::has_toolpaths`),
/// so the transport clock and the posted program count the same moves.
pub(crate) fn sim_script(state: &State) -> hsmworks_app::sim::Script {
    use hsmworks_app::{model, posting, sim};
    let tr = sim::Traverse::from_machine(&posting::default_machine());
    let paths: Vec<(String, hsmworks_app::cl::Toolpath)> = model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| model::is_operation(&state.world, e))
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
/// window, no surface - the fallback adapter (the same deterministic choice
/// as tests/gpu_render.rs), View3D on the shell's Rgba8Unorm offscreen
/// format, the SEEDED document rendered (block + stock + region + cached
/// path through set_overlay), and the readback showing a solid footprint of
/// changed pixels. The second half of the proof is the world itself:
/// Contour1 carries a non-empty cached toolpath, i.e. seed -> commands ->
/// generate worked.
pub(crate) mod headless {
    use hsmworks_app::model;

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
