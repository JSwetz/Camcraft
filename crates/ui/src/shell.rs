//! The shell's chrome: menu bar, dock host, status line, global shortcuts.
//! Provenance: our own layout, specced from public CAM shell conventions.
//!
//! Everything the menus touch goes through `State::pending` - dialogs must
//! never open in the middle of an ImGui frame.

use dear_imgui_rs::{Condition, Key, MouseButton, Ui, WindowFlags};

use crate::state::{CommandKind, State};
use crate::tree;

/// Build one frame of UI; queued actions run via `State::drain_actions`.
pub fn build(ui: &Ui, state: &mut State) {
    shortcuts(ui, state);

    // The main menu bar must be issued outside any window.
    menu_bar(ui, state);

    let display = ui.io().display_size();
    let status_h = 4.0 + ui.text_line_height();

    if ui
        .window("##host")
        .flags(
            WindowFlags::NO_TITLE_BAR
                | WindowFlags::NO_COLLAPSE
                | WindowFlags::NO_MOVE
                | WindowFlags::NO_RESIZE
                | WindowFlags::NO_BACKGROUND
                | WindowFlags::NO_BRING_TO_FRONT_ON_FOCUS
                | WindowFlags::NO_DOCKING
                | WindowFlags::NO_NAV_INPUTS,
        )
        .size([display[0], (display[1] - status_h).max(1.0)], Condition::Always)
        .position([0.0, 0.0], Condition::Always)
        .build(|| {
            // A dockspace filling the host; the panels below land inside it.
            let size = ui.content_region_avail();
            if ui.dockspace().current_window(size).build().is_err() {
                ui.text("dockspace unavailable");
            }
            panels(ui, state);
        })
        .is_none()
    {
        return;
    }

    ui.window("##status")
        .flags(
            WindowFlags::NO_TITLE_BAR
                | WindowFlags::NO_COLLAPSE
                | WindowFlags::NO_MOVE
                | WindowFlags::NO_RESIZE
                | WindowFlags::NO_BACKGROUND
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
    viewport_panel(ui, state);
}

fn viewport_panel(ui: &Ui, state: &mut State) {
    ui.window("Viewport").build(|| {
        let Some(tex) = state.viewport_tex else {
            ui.text("3D viewport not initialized");
            return;
        };
        let size = ui.content_region_avail();
        if size[0] > 1.0 && size[1] > 1.0 {
            state.viewport_size = size;
        }
        ui.image(tex, size);

        if !ui.io().want_capture_mouse() && ui.is_item_hovered() {
            let delta = ui.io().mouse_delta();
            if ui.is_mouse_down(MouseButton::Middle) {
                state.cam.orbit(delta[0], delta[1]);
            }
            if ui.is_mouse_down(MouseButton::Right) {
                state.cam.pan(delta[0], delta[1]);
            }
            let wheel = ui.io().mouse_wheel();
            if wheel != 0.0 {
                state.cam.zoom(wheel);
            }
        }
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
