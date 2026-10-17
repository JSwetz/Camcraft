//! M-a: the egui shell's pane layer. The keyboard half is graduated from
//! the spike verbatim; the visual half is stubs that say which migration
//! step replaces them — the whole-pane law (docs/spike-egui-go-no-go.md)
//! forbids porting a pane by halves.
//!
//! Layering holds (AGENTS.md): world access is `hsmworks_app`'s public API
//! only, and every change goes through `state.pending`, never an ad-hoc
//! field poke.

use egui::{Color32, Key, RichText, Ui};

use crate::state::{CommandKind, State};

use super::theme::Palette;

/// The placeholder gray (theme's text-faint token).
const PLACEHOLDER: Color32 = Palette::DARK.text_faint;

// ---------------------------------------------------------------- shortcuts

/// The shell's keyboard layer: history keys always, destructive keys only
/// while no text field owns focus (imgui's io.WantTextInput guard).
pub fn shortcuts(ui: &mut Ui, state: &mut State) {
    let typing = ui.memory(|m| m.focused().is_some());
    let (modifiers, z, y, del, f) = ui.input(|i| {
        (
            i.modifiers,
            i.key_pressed(Key::Z),
            i.key_pressed(Key::Y),
            i.key_pressed(Key::Delete),
            i.key_pressed(Key::F),
        )
    });
    let cmd = modifiers.command;
    if cmd && z && !modifiers.shift && !modifiers.alt {
        state.pending.push_back(CommandKind::Undo);
    }
    if cmd && y {
        // Ctrl+Y and Ctrl+Shift+Y both redo (imgui law).
        state.pending.push_back(CommandKind::Redo);
    }
    if del && !typing {
        state.pending.push_back(CommandKind::DeleteSelected);
    }
    if f && !typing && !cmd && !modifiers.alt {
        state.pending.push_back(CommandKind::FrameAll);
    }
}

// -------------------------------------------------------------------- ribbon

/// Stub: the painted ribbon (cells, groups, glyphs) arrives whole in M-b,
/// graduated from the spike's panels.rs + painted.rs. The strip's ground is
/// already the real one, so the reserved height is honest at hand-test.
pub fn ribbon(ui: &mut Ui) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, Palette::DARK.strip_fill());
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(RichText::new("ribbon - painted cells arrive in M-b").color(PLACEHOLDER).small());
    });
}

/// The status line is real from frame one: a seed or command refusal must
/// be visible before any pane is graduated. The M-d step adds the
/// error-surface display half.
pub fn status(ui: &mut Ui, state: &State) {
    ui.horizontal(|ui| {
        let text = if state.status.is_empty() { "ready" } else { state.status.as_str() };
        ui.label(RichText::new(text).color(Palette::DARK.text_dim).small());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new("egui shell (M-a) - imgui stays default until M-g")
                    .color(PLACEHOLDER)
                    .small(),
            );
        });
    });
}

/// Stub: the browser tree (columns, eyes, ops rows, warning chips) arrives
/// whole in M-b.
pub fn browser(ui: &mut Ui) {
    ui.centered_and_justified(|ui| {
        ui.label(RichText::new("browser tree - arrives in M-b").color(PLACEHOLDER).small());
    });
}

/// Stub: the property pages (all five op types, setup pages) arrive whole
/// in M-c; the spike proved the Contour page end to end.
pub fn properties(ui: &mut Ui) {
    ui.centered_and_justified(|ui| {
        ui.label(RichText::new("property pages - arrive in M-c").color(PLACEHOLDER).small());
    });
}
