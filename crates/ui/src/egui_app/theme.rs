//! The dark Visuals, graduated from the M1.4.1 spike (theme.rs, unchanged
//! law): the numbers are the imgui shell's dark theme (app.rs's style block)
//! restated as egui tokens.
//!
//! Every color is a named token here and nowhere else: the painted ribbon
//! and browser (M-b will graduate painted.rs) read the same struct the egui
//! widgets get via `Visuals`, so a retheme is one edit and the two render
//! paths (widgets vs draw-list port) cannot drift. The numbers are the
//! imgui shell's dark theme (app.rs's style block): the same slate grounds,
//! the same steel-blue accent, hover/active stepping one notch each, flat
//! 1-px strokes.
//! Provenance: our own module (graduated from the spike), specced from the
//! imgui theme we already ship and the egui Visuals docs; no third-party
//! theme copied.

use egui::{Color32, Stroke, Visuals};

/// One frame of the shell's color truth.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    /// Window/panel ground.
    pub ink: Color32,
    /// Ribbon / browser / header ground (one step above ink).
    pub ink_alt: Color32,
    /// Input fields and empty recesses (one step below ink).
    pub well: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub border: Color32,
    /// The selection/hover accent (steel blue).
    pub accent: Color32,
    pub accent_hover: Color32,
    /// Widget-field ground, by interaction state.
    pub field_bg: Color32,
    pub field_hover: Color32,
    pub field_active: Color32,
    /// Text selection and combo-open highlight.
    pub selection_bg: Color32,
    pub selection_stroke: Color32,
}

impl Palette {
    /// The imgui shell's numbers, as tokens.
    pub const DARK: Palette = Palette {
        ink: Color32::from_rgb(0x23, 0x26, 0x2B),
        ink_alt: Color32::from_rgb(0x2C, 0x30, 0x37),
        well: Color32::from_rgb(0x1B, 0x1E, 0x22),
        text: Color32::from_rgb(0xD6, 0xDA, 0xE0),
        text_dim: Color32::from_rgb(0x9A, 0xA1, 0xAB),
        text_faint: Color32::from_rgb(0x6E, 0x75, 0x7E),
        border: Color32::from_rgb(0x3D, 0x43, 0x4C),
        accent: Color32::from_rgb(0x35, 0x6D, 0xA8),
        accent_hover: Color32::from_rgb(0x4A, 0x86, 0xC2),
        field_bg: Color32::from_rgb(0x1E, 0x21, 0x26),
        field_hover: Color32::from_rgb(0x2A, 0x2E, 0x35),
        field_active: Color32::from_rgb(0x1A, 0x2B, 0x3A),
        selection_bg: Color32::from_rgb(0x26, 0x4A, 0x6E),
        selection_stroke: Color32::from_rgb(0x4A, 0x86, 0xC2),
    };

    /// Build the egui Visuals from the tokens: every default-egui color the
    /// shell shows is named here, so the widget path and the painted path
    /// cannot disagree about what the theme is.
    #[must_use]
    pub fn visuals(self) -> Visuals {
        let mut v = Visuals::dark();
        v.panel_fill = self.ink;
        v.window_fill = self.ink_alt;
        v.faint_bg_color = self.well;
        v.extreme_bg_color = self.well;
        v.code_bg_color = self.well;
        v.override_text_color = Some(self.text);
        v.weak_text_color = Some(self.text_dim);
        v.hyperlink_color = self.accent_hover;
        v.selection = egui::style::Selection {
            bg_fill: self.selection_bg,
            stroke: Stroke::new(v.selection.stroke.width, self.selection_stroke),
        };
        let stroke_1px = Stroke::new(1.0, self.border);
        v.widgets.noninteractive.bg_fill = self.ink_alt;
        v.widgets.noninteractive.weak_bg_fill = Color32::TRANSPARENT;
        v.widgets.noninteractive.bg_stroke = stroke_1px;
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.text);
        v.widgets.inactive.bg_fill = self.field_bg;
        v.widgets.inactive.weak_bg_fill = self.ink_alt;
        v.widgets.inactive.bg_stroke = stroke_1px;
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, self.text);
        v.widgets.hovered.bg_fill = self.field_hover;
        v.widgets.hovered.weak_bg_fill = self.accent;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, self.accent_hover);
        v.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
        v.widgets.active.bg_fill = self.field_active;
        v.widgets.active.weak_bg_fill = self.accent_hover;
        v.widgets.active.bg_stroke = Stroke::new(1.0, self.accent_hover);
        v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
        v.widgets.open.bg_fill = self.field_bg;
        v.widgets.open.bg_stroke = stroke_1px;
        for exp in [
            &mut v.widgets.noninteractive.expansion,
            &mut v.widgets.inactive.expansion,
            &mut v.widgets.hovered.expansion,
            &mut v.widgets.active.expansion,
            &mut v.widgets.open.expansion,
        ] {
            *exp = 0.0; // flat chrome: hovered widgets must not bleed past their bounds
        }
        v
    }

    /// The panel ground behind the ribbon/browser strips (painted.rs, M-b).
    #[must_use]
    pub fn strip_fill(self) -> Color32 {
        self.ink_alt
    }
}

/// Install the palette into a context. Called once per Context (windowed).
/// Visuals are per-context.
pub fn apply(ctx: &egui::Context) {
    ctx.set_visuals(Palette::DARK.visuals());
}
