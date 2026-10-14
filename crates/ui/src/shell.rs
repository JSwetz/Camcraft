//! The shell's chrome: menu bar, dock host, status line, global shortcuts.
//! Provenance: our own layout, specced from public CAM shell conventions.
//!
//! Everything the menus touch goes through `State::pending` - dialogs must
//! never open in the middle of an ImGui frame.

use dear_imgui_rs::{
    Condition, DockLayout, DockLayoutApply, DockNodeFlags, DockSplit, Id, Key, StyleVar, Ui,
    WindowFlags, WindowKey,
};

use crate::actions;
use crate::browser;
use crate::operations;
use crate::ribbon;
use crate::state::{CommandKind, State, UiTarget};

/// Stable identity for the 3D panel; shared by the layout and the window.
pub(crate) fn viewport_key() -> Option<WindowKey> {
    WindowKey::new("viewport", "Viewport").ok()
}

/// Stable identity for the dock tree: FNV-1a over a constant label, so the
/// root id depends on nothing but this string (a nonzero id is required
/// when the dockspace is hosted in a window rather than the main viewport).
fn dock_root_id() -> Id {
    let mut h: u32 = 0x811c_9dc5;
    for b in b"hsmworks-dock-v1" {
        h ^= u32::from(*b);
        h = h.wrapping_mul(0x0100_0193);
    }
    Id::from(h.max(1))
}

/// Declarative startup layout: the browser tree on the left, the viewport
/// DOMINANT in the middle (~two-thirds of the width - it is the thing you
/// look at), the contextual Operations pane on the right. Applied only
/// when the dock tree is missing (first run or lost layout); the layout is
/// rebuilt every launch because the ini file is not persisted, so panels
/// can be moved without migration.
///
/// `DockLayout::split` puts the FIRST child in the strip at `direction`
/// sized `ratio`, and the second in the remainder (the `DockSplit` doc).
/// Getting that backwards is what crammed the viewport against the right
/// edge (hand-test twice over): the split named `Right` must list the
/// Operations pane first, so the viewport keeps the middle remainder.
fn dock_layout() -> Option<DockLayout> {
    let browser = WindowKey::new("browser", "Browser").ok()?;
    let view = viewport_key()?;
    let ops = WindowKey::new("operations", "Operations").ok()?;
    Some(DockLayout::split(
        DockSplit::Left,
        0.17,
        DockLayout::tabs([browser]),
        DockLayout::split(
            DockSplit::Right,
            0.22,
            DockLayout::tabs([ops]),
            DockLayout::tabs([view]),
        ),
    ))
}

/// Build one frame of UI; queued actions run via `State::drain_actions`.
pub fn build(ui: &Ui, state: &mut State) {
    shortcuts(ui, state);

    // The main menu bar must be issued outside any window.
    menu_bar(ui, state);

    let ribbon_y = ui.frame_height();
    dock_area(ui, state, ribbon_y);
    // Submitted last so it always overlays docked panels dragged up.
    ribbon::bar(ui, state, ribbon_y);

    status_bar(ui, state);
}

/// The dock tree, hosted in a fixed invisible window between the menu bar
/// and the status strip - geometry computed from font metrics, so the
/// panels can never slide under the menu (the bug a hand-rolled full-screen
/// host had). The dockspace must be submitted before the windows it hosts,
/// hence `panels` runs after, not inside, the host window.
fn dock_area(ui: &Ui, state: &mut State, ribbon_y: f32) {
    let display = ui.io().display_size();
    let line = ui.text_line_height();
    let top = ribbon_y + ribbon::height(ui); // menu strip + ribbon
    let status_h = line + 8.0;
    // `current_window` demands a positive finite size; clamp for tiny windows.
    let host_h = (display[1] - top - status_h).max(16.0);
    let layout = dock_layout();
    let mut dock_err: Option<String> = None;
    ui.window("##dockhost")
        .flags(
            WindowFlags::NO_TITLE_BAR
                | WindowFlags::NO_COLLAPSE
                | WindowFlags::NO_MOVE
                | WindowFlags::NO_RESIZE
                | WindowFlags::NO_BACKGROUND
                | WindowFlags::NO_BRING_TO_FRONT_ON_FOCUS
                | WindowFlags::NO_DOCKING
                | WindowFlags::NO_SCROLLBAR
                | WindowFlags::NO_SCROLL_WITH_MOUSE
                | WindowFlags::NO_NAV_INPUTS,
        )
        .position([0.0, top], Condition::Always)
        .size([display[0], host_h], Condition::Always)
        .build(|| {
            let _pad = ui.push_style_var(StyleVar::WindowPadding([0.0, 0.0]));
            let size = ui.content_region_avail();
            let mut space = ui
                .dockspace()
                .root_id(dock_root_id())
                .current_window(size)
                .flags(DockNodeFlags::AUTO_HIDE_TAB_BAR);
            if let Some(layout) = layout.as_ref() {
                space = space.layout(layout, DockLayoutApply::IfMissing);
            }
            if let Err(err) = space.build() {
                dock_err = Some(err.to_string());
            }
        });
    if let Some(err) = dock_err {
        // Show chrome rather than a blank frame if docking fails; the
        // panels still build, as floating windows.
        ui.window("##dockerror")
            .flags(
                WindowFlags::NO_TITLE_BAR
                    | WindowFlags::NO_MOVE
                    | WindowFlags::NO_DOCKING
                    | WindowFlags::NO_BRING_TO_FRONT_ON_FOCUS,
            )
            .position([8.0, line * 2.0], Condition::Always)
            .size([display[0] - 16.0, line * 4.0], Condition::Always)
            .build(|| {
                ui.text_wrapped(format!(
                    "dockspace unavailable: {err}\nThe panels are floating instead."
                ));
            });
    }
    panels(ui, state);
}

/// Bottom status strip. Padding and height are computed from the font so
/// the line never clips outside the window.
fn status_bar(ui: &Ui, state: &State) {
    let display = ui.io().display_size();
    let line = ui.text_line_height();
    let status_h = line + 8.0;
    let _pad = ui.push_style_var(StyleVar::WindowPadding([8.0, 4.0]));
    ui.window("##status")
        .flags(
            WindowFlags::NO_TITLE_BAR
                | WindowFlags::NO_COLLAPSE
                | WindowFlags::NO_MOVE
                | WindowFlags::NO_RESIZE
                | WindowFlags::NO_BRING_TO_FRONT_ON_FOCUS
                | WindowFlags::NO_DOCKING
                | WindowFlags::NO_NAV_INPUTS,
        )
        .size([display[0], status_h], Condition::Always)
        .position([0.0, display[1] - status_h], Condition::Always)
        .build(|| {
            ui.text(&state.status);
        });
}

/// Menu wiring: every item calls an `actions` entry point - the same ones
/// the ribbon will call - and nothing here pushes a `CommandKind` itself.
fn menu_bar(ui: &Ui, state: &mut State) {
    let _bar = ui.begin_main_menu_bar();
    if let Some(file) = ui.begin_menu("File") {
        if ui.menu_item_with_shortcut("Open Model...", "Ctrl+O") {
            actions::open_model_dialog(state);
        }
        if ui.menu_item_with_shortcut("New", "Ctrl+N") {
            actions::new_document(state);
        }
        ui.separator();
        if ui.menu_item_with_shortcut("Open Project...", "Ctrl+Shift+O") {
            actions::open_project(state);
        }
        if ui.menu_item_with_shortcut("Save Project...", "Ctrl+S") {
            actions::save_project(state);
        }
        ui.separator();
        if ui.menu_item("Exit") {
            actions::quit(state);
        }
        drop(file);
    }
    if let Some(edit) = ui.begin_menu("Edit") {
        if ui.menu_item_enabled_selected_with_shortcut("Undo", "Ctrl+Z", false, state.history.can_undo()) {
            actions::undo(state);
        }
        if ui.menu_item_enabled_selected_with_shortcut("Redo", "Ctrl+Y", false, state.history.can_redo()) {
            actions::redo(state);
        }
        ui.separator();
        let has_sel = state.has_selection();
        if ui.menu_item_enabled_selected_with_shortcut("Delete Selected", "Del", false, has_sel) {
            actions::delete_selected(state);
        }
        drop(edit);
    }
    if let Some(view) = ui.begin_menu("View") {
        if ui.menu_item_with_shortcut("Frame All", "F") {
            actions::frame_all(state);
        }
        drop(view);
    }
    if let Some(cam) = ui.begin_menu("CAM") {
        // The Operations pane is a persistent dock tab whose page follows
        // the pane target, so these route rather than open: New Operation
        // aims it at the creation form (and clears the selection it would
        // fall back to), Tool Library points it at the tool page.
        if ui.menu_item("New Operation") {
            actions::new_operation(state);
        }
        if ui.menu_item("Tool Library...") {
            actions::target(state, UiTarget::Tool(String::new()));
        }
        ui.separator();
        let sel = state.selected_operation();
        if ui.menu_item_enabled_selected("Generate##cam", None::<&str>, false, sel.is_some()) {
            actions::generate_selected(state);
        }
        if ui.menu_item_enabled_selected("Delete Operation", None::<&str>, false, sel.is_some()) {
            actions::delete_operation(state);
        }
        drop(cam);
    }
}

fn panels(ui: &Ui, state: &mut State) {
    browser::browser_panel(ui, state);
    viewport_panel(ui, state);
    operations::operations_panel(ui, state);
}

fn viewport_panel(ui: &Ui, state: &mut State) {
    let Some(key) = viewport_key() else {
        return;
    };
    ui.window(key.label("Viewport")).build(|| {
        let Some(tex) = state.viewport_tex else {
            ui.text("3D viewport not initialized");
            return;
        };
        let size = ui.content_region_avail();
        if size[0] > 1.0 && size[1] > 1.0 {
            state.viewport_size = size;
        }
        ui.image(tex, size);

        // Publish the image rect (logical screen coords) so app.rs can
        // hit-test raw winit pointer events against it: ImGui's hover/capture
        // bookkeeping over docked windows proved unreliable for button drags,
        // so orbit/pan/pick bypass it entirely.
        let viewport_rect = ui.item_rect();
        state.viewport_rect = viewport_rect;

        // Axis letters over the gizmo tips (feedback 7). Projected through
        // the same camera math every frame, so they track orbit/zoom live.
        let ([rx0, ry0], [rx1, ry1]) = viewport_rect;
        let (rw, rh) = (rx1 - rx0, ry1 - ry0);
        if rw > 1.0 && rh > 1.0 {
            let labels = crate::app::overlay_labels(state, (rw / rh) as f64);
            let edges = crate::app::hover_edges(state, (rw / rh) as f64);
            if !labels.is_empty() || !edges.is_empty() {
                let dl = ui.get_window_draw_list();
                for (uv, rgb, text) in labels {
                    let pos = [rx0 + uv[0] * rw, ry0 + uv[1] * rh];
                    dl.add_text(pos, [rgb[0], rgb[1], rgb[2], 1.0], text);
                }
                // The facet under the cursor, outlined in screen space -
                // the hover channel that never touches the 3D overlay.
                for (a, b) in edges {
                    dl.add_line(
                        [rx0 + a[0] * rw, ry0 + a[1] * rh],
                        [rx0 + b[0] * rw, ry0 + b[1] * rh],
                        [1.0, 0.85, 0.40, 1.0],
                    )
                    .build();
                }
            }
        }
        let hovered = ui.is_window_hovered();
        // Raw pointer events (app.rs) may only start viewport drags while
        // the viewport is the topmost hovered window - menus and popups
        // over the image otherwise orbit the camera behind the user's click.
        state.viewport_hot = hovered;
        let wheel = ui.io().mouse_wheel();
        if wheel != 0.0 && hovered {
            // Cursor-centered zoom: map the pointer into the image rect's
            // normalized device coords (x right, y up).
            let ([rx0, ry0], [rx1, ry1]) = viewport_rect;
            let (mw, mh) = (rx1 - rx0, ry1 - ry0);
            let [mx, my] = ui.io().mouse_pos();
            if mw > 1.0 && mh > 1.0 && mx >= rx0 && mx <= rx1 && my >= ry0 && my <= ry1 {
                let ndc = [
                    (mx - rx0) / mw * 2.0 - 1.0,
                    1.0 - (my - ry0) / mh * 2.0,
                ];
                state.cam.dolly_cursor(ndc, f64::from(mw / mh), wheel);
            }
        }
        viewport_help(ui, hovered, state);
    });
}

/// Corner hint: control legend, or the live mode string while dragging.
fn viewport_help(ui: &Ui, hovered: bool, state: &State) {
    if !hovered && state.drag.is_none() {
        return;
    }
    let hint = match state.drag {
        Some(crate::state::DragMode::Orbit) => "orbit",
        Some(crate::state::DragMode::Pan) => "pan",
        None => "LMB pick / drag-orbit · MMB orbit · RMB pan · wheel · F frame",
    };
    let pad = 6.0;
    let [sw, sh] = ui.calc_text_size(hint);
    // item_rect is screen space, matching io.MousePos and window positions.
    let (_, img_max) = ui.item_rect();
    let pos = [img_max[0] - sw - pad * 2.0, img_max[1] - sh - pad * 2.0];
    ui.window("##vcue")
        .flags(
            WindowFlags::NO_TITLE_BAR
                | WindowFlags::NO_BACKGROUND
                | WindowFlags::NO_MOVE
                | WindowFlags::NO_DOCKING
                | WindowFlags::NO_NAV
                | WindowFlags::NO_NAV_INPUTS
                | WindowFlags::NO_FOCUS_ON_APPEARING
                | WindowFlags::NO_BRING_TO_FRONT_ON_FOCUS,
        )
        .position(pos, Condition::Always)
        .size([sw + pad * 2.0, sh + pad * 2.0], Condition::Always)
        .build(|| {
            ui.text(hint);
        });
}

fn shortcuts(ui: &Ui, state: &mut State) {
    if ui.io().want_capture_keyboard() {
        return;
    }
    let ctrl = ui.is_key_down(Key::ModCtrl);
    if ctrl && ui.is_key_pressed(Key::Z) {
        state.pending.push_back(CommandKind::Undo);
    }
    if ctrl && ui.is_key_pressed(Key::Y) {
        state.pending.push_back(CommandKind::Redo);
    }
    if ctrl && ui.is_key_pressed(Key::O) {
        state.pending.push_back(CommandKind::OpenModelDialog);
    }
    if ctrl && ui.is_key_pressed(Key::S) {
        state.pending.push_back(CommandKind::SaveProjectDialog);
    }
    if !ctrl && ui.is_key_pressed(Key::Delete) {
        state.pending.push_back(CommandKind::DeleteSelected);
    }
    if !ctrl && ui.is_key_pressed(Key::F) {
        state.pending.push_back(CommandKind::FrameAll);
    }
}
