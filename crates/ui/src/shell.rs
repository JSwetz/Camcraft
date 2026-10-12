//! The shell's chrome: menu bar, dock host, status line, global shortcuts.
//! Provenance: our own layout, specced from public CAM shell conventions.
//!
//! Everything the menus touch goes through `State::pending` - dialogs must
//! never open in the middle of an ImGui frame.

use dear_imgui_rs::{
    Condition, DockLayout, DockLayoutApply, DockNodeFlags, DockSplit, Key, MouseButton, Ui,
    WindowFlags, WindowKey,
};

use crate::state::{CommandKind, State};
use crate::tree;

/// Stable identity for the 3D panel; shared by the layout and the window.
pub(crate) fn viewport_key() -> Option<WindowKey> {
    WindowKey::new("viewport", "Viewport").ok()
}

/// Declarative startup layout: tree on the left, 3D filling the rest.
/// Applied only when the dock tree is missing (first run or lost layout);
/// after that the user's arrangement persists in the ImGui ini.
fn dock_layout() -> Option<DockLayout> {
    let tree = WindowKey::new("models", "Models").ok()?;
    let view = viewport_key()?;
    Some(DockLayout::split(
        DockSplit::Left,
        0.24,
        DockLayout::tabs([tree]),
        DockLayout::tabs([view]),
    ))
}

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
            let mut space = ui
                .dockspace()
                .current_window(size)
                .flags(DockNodeFlags::AUTO_HIDE_TAB_BAR);
            let layout = dock_layout();
            if let Some(layout) = layout.as_ref() {
                space = space.layout(layout, DockLayoutApply::IfMissing);
            }
            // Stable root id for this dock tree.
            let host = space.root_id(ui.get_id("hsm-dockspace"));
            match host.build() {
                Ok(_) => panels(ui, state),
                Err(err) => {
                    // Show chrome rather than a blank frame if docking fails.
                    ui.text(format!("dockspace unavailable: {err}"));
                    tree::models_panel(ui, state);
                    viewport_panel(ui, state);
                }
            }
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

        // Camera controls: MMB orbits, RMB pans, wheel zooms (cursor-
        // centered). The hover test uses the raw window rect, not
        // is_item_hovered: a single large Image can report no hover while
        // the cursor is inside the panel.
        let hovered = ui.is_window_hovered();
        if hovered && !ui.io().want_capture_mouse() {
            let delta = ui.io().mouse_delta();
            if ui.is_mouse_down(MouseButton::Middle) {
                state.cam.orbit(delta[0], delta[1]);
            }
            if ui.is_mouse_down(MouseButton::Right) {
                state.cam.pan(delta[0], delta[1]);
            }
        }
        let wheel = ui.io().mouse_wheel();
        if wheel != 0.0 && hovered {
            // Cursor-centered zoom: map the pointer into the image rect's
            // normalized device coords (x right, y up).
            let ([rx0, ry0], [rx1, ry1]) = ui.item_rect();
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
        viewport_help(ui, hovered);
    });
}

/// Corner hint: empty by default, live mode string while dragging.
fn viewport_help(ui: &Ui, hovered: bool) {
    if !hovered {
        return;
    }
    let hint = if ui.is_mouse_down(MouseButton::Middle) {
        "orbit"
    } else if ui.is_mouse_down(MouseButton::Right) {
        "pan"
    } else {
        "MMB orbit · RMB pan · wheel zoom · F frame all"
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
