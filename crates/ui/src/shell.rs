//! The shell's chrome: menu bar, dock host, status line, global shortcuts.
//! Provenance: our own layout, specced from public CAM shell conventions.
//!
//! Everything the menus touch goes through `State::pending` - dialogs must
//! never open in the middle of an ImGui frame.

use dear_imgui_rs::{
    Condition, DockLayout, DockLayoutApply, DockNodeFlags, DockSplit, Key, StyleVar, Ui,
    WindowFlags, WindowKey,
};

use crate::setup;
use crate::state::{CommandKind, State};
use crate::tree;

/// Stable identity for the 3D panel; shared by the layout and the window.
pub(crate) fn viewport_key() -> Option<WindowKey> {
    WindowKey::new("viewport", "Viewport").ok()
}

/// Declarative startup layout: tree + setup as left tabs, 3D filling the
/// rest. Applied only when the dock tree is missing (first run or lost
/// layout); the layout is rebuilt every launch because the ini file is not
/// persisted, so panels can be added without migration.
fn dock_layout() -> Option<DockLayout> {
    let tree = WindowKey::new("models", "Models").ok()?;
    let setup_key = WindowKey::new("setup", "Setup").ok()?;
    let view = viewport_key()?;
    Some(DockLayout::split(
        DockSplit::Left,
        0.24,
        DockLayout::tabs([tree, setup_key]),
        DockLayout::tabs([view]),
    ))
}

/// Build one frame of UI; queued actions run via `State::drain_actions`.
pub fn build(ui: &Ui, state: &mut State) {
    shortcuts(ui, state);

    // The main menu bar must be issued outside any window.
    menu_bar(ui, state);

    // Dockspace over the main viewport. ImGui derives its host window and
    // reserves the menu-bar strip automatically, so panels can never sit
    // under the menu (a hand-rolled full-screen host window did).
    let layout = dock_layout();
    let mut space = ui.dockspace().flags(DockNodeFlags::AUTO_HIDE_TAB_BAR);
    if let Some(layout) = layout.as_ref() {
        space = space.layout(layout, DockLayoutApply::IfMissing);
    }
    match space.build() {
        Ok(_) => panels(ui, state),
        Err(err) => {
            // Show chrome rather than a blank frame if docking fails.
            let display = ui.io().display_size();
            let line_h = ui.text_line_height();
            ui.window("##dockerror")
                .flags(WindowFlags::NO_TITLE_BAR | WindowFlags::NO_MOVE)
                .position([8.0, line_h * 2.0], Condition::Always)
                .size([display[0] - 16.0, line_h * 4.0], Condition::Always)
                .build(|| {
                    ui.text_wrapped(format!(
                        "dockspace unavailable: {err}\nThe panels are floating instead."
                    ));
                });
            panels(ui, state);
        }
    }

    status_bar(ui, state);
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

fn menu_bar(ui: &Ui, state: &mut State) {
    let _bar = ui.begin_main_menu_bar();
    if let Some(file) = ui.begin_menu("File") {
        if ui.menu_item_with_shortcut("Open Model...", "Ctrl+O") {
            state.pending.push_back(CommandKind::OpenModelDialog);
        }
        if ui.menu_item_with_shortcut("New", "Ctrl+N") {
            state.pending.push_back(CommandKind::NewDocument);
        }
        ui.separator();
        if ui.menu_item_with_shortcut("Open Project...", "Ctrl+Shift+O") {
            state.pending.push_back(CommandKind::OpenProjectDialog);
        }
        if ui.menu_item_with_shortcut("Save Project...", "Ctrl+S") {
            state.pending.push_back(CommandKind::SaveProjectDialog);
        }
        ui.separator();
        if ui.menu_item("Exit") {
            state.pending.push_back(CommandKind::Quit);
        }
        drop(file);
    }
    if let Some(edit) = ui.begin_menu("Edit") {
        if ui.menu_item_enabled_selected_with_shortcut("Undo", "Ctrl+Z", false, state.history.can_undo()) {
            state.pending.push_back(CommandKind::Undo);
        }
        if ui.menu_item_enabled_selected_with_shortcut("Redo", "Ctrl+Y", false, state.history.can_redo()) {
            state.pending.push_back(CommandKind::Redo);
        }
        ui.separator();
        let has_sel = state.has_selection();
        if ui.menu_item_enabled_selected_with_shortcut("Delete Selected", "Del", false, has_sel) {
            state.pending.push_back(CommandKind::DeleteSelected);
        }
        drop(edit);
    }
    if let Some(view) = ui.begin_menu("View") {
        if ui.menu_item_with_shortcut("Frame All", "F") {
            state.pending.push_back(CommandKind::FrameAll);
        }
        drop(view);
    }
}

fn panels(ui: &Ui, state: &mut State) {
    tree::models_panel(ui, state);
    setup::setup_panel(ui, state);
    viewport_panel(ui, state);
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
        let hovered = ui.is_window_hovered();
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
