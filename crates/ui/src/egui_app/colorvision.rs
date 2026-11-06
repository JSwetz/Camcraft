//! The ColorVision layer (M13.4, DONE M6.1's named follow-on): protan,
//! deutan and tritan remaps of the colours that CARRY MEANING - the
//! Deviate verdict classes (clean / rest / gouge / unjudged), the
//! toolpath move kinds (feed / rapid / plunge), the viewport's
//! selection lift and the pages' warning / error / missing text inks
//! (per panel ground, gated against the body text and the panel
//! itself), applied on top of whatever theme is live. The
//! chrome tokens are not remapped: they are a blue-on-neutral
//! luminance design every dichromacy already reads, and the gate below
//! proves that per mode instead of trusting it.
//!
//! The law: under each remap, every pair that must be told apart stays
//! at least [`MIN_DE00`] apart (CIEDE2000 of the two colours as the
//! deficient eye sees them, simulated with Machado, Oliveira &
//! Fernandes 2009 - see `cvd`), and the live token table, seen through
//! the same eye, still passes `contrast::readable` (WCAG 2.1). A mode
//! that fails is refused at the pick, never rendered. `Off` is the
//! house palette byte for byte - the default changes no pixel.
//! Provenance: our own module and numbers; the simulation and the
//! metric are the published models cited in `cvd`; the remap's shape
//! (meaning on the blue-yellow and lightness axes for the red-green
//! deficiencies, on red-cyan and lightness for tritan) is general
//! colour-science practice. No vendor material consulted.

use camcraft_app::commands::color_vision_of;
use camcraft_app::schema::ColorVision;
use egui::{Color32, Ui};

use super::{contrast, cvd, theme::Tokens};
use crate::state::{CommandKind, State};

/// The separation floor every must-distinguish pair keeps under its
/// mode, in CIEDE2000 units. 20 is well past "obviously different"
/// (a just-noticeable difference is ~1-2, and categorical palettes are
/// commonly held to 10-20); the shipped remaps clear it by >= 4.
pub const MIN_DE00: f64 = 20.0;

/// The meaning-carrying colours, display sRGB 0..1 as the shaders and
/// the overlay lines consume them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Semantic {
    /// Stock with no deviation (the steel tint; also plain stock).
    pub clean: [f32; 3],
    /// Material left behind.
    pub rest: [f32; 3],
    /// Material cut too deep.
    pub gouge: [f32; 3],
    /// No verdict rendered (never a clean bill).
    pub unjudged: [f32; 3],
    pub feed: [f32; 3],
    pub rapid: [f32; 3],
    pub plunge: [f32; 3],
    /// Selected stock fill and selected edges in wireframe.
    pub select_hot: [f32; 3],
    /// Selected edges over shaded faces.
    pub select_warm: [f32; 3],
    /// Boundary roles (M11.5 UX): picked include / exclude / open
    /// boundary edges in the viewport and their page labels.
    pub bound_include: [f32; 3],
    pub bound_exclude: [f32; 3],
    pub bound_open: [f32; 3],
    /// The page's meaning-carrying TEXT inks (M13.4 owed item), one set
    /// per ground: a remap picks by the panel's luminance, since no one
    /// ink reads at 4.5:1 on both a dark and a light panel.
    pub text_dark: TextInks,
    pub text_light: TextInks,
}

/// The warning / error / missing-reference text inks of the op page,
/// the status refusals and the boundary notes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextInks {
    /// Warnings (egui's `warn_fg_color` slot: preview refusals, the
    /// not-judged header line).
    pub warn: Color32,
    /// Refusals (the status-line red of a refused command).
    pub error: Color32,
    /// A geometry chip whose feature no longer resolves.
    pub missing: Color32,
}

impl TextInks {
    /// The house inks, exactly what the pages drew before: egui's own
    /// dark-visuals warning (the theme builds on `Visuals::dark()` and
    /// never respells it), the page red and the chip grey.
    pub const HOUSE: TextInks = TextInks {
        warn: Color32::from_rgb(255, 143, 0),
        error: Color32::from_rgb(0xFF, 0x59, 0x59),
        missing: Color32::from_rgb(0xB8, 0xB8, 0xC8),
    };
}

/// The unselected edge inks the selection must stand off from: the
/// shaded-mode edge and the wireframe line (viewport `fs_edge`).
const EDGE_DARK: [f32; 3] = [0.06, 0.07, 0.09];
const EDGE_WIRE: [f32; 3] = [0.62, 0.66, 0.72];

impl Semantic {
    /// The house palette, exactly what the shell drew before M13.4
    /// (the overlay's own consts and the shaders' literals).
    pub const OFF: Semantic = Semantic {
        clean: [0.42, 0.50, 0.62],
        rest: [0.98, 0.72, 0.20],
        gouge: [0.90, 0.18, 0.14],
        unjudged: [0.45, 0.46, 0.48],
        feed: crate::interact::FEED_RGB,
        rapid: crate::interact::RAPID_RGB,
        plunge: crate::interact::PLUNGE_RGB,
        select_hot: [0.98, 0.72, 0.20],
        select_warm: [0.55, 0.35, 0.05],
        bound_include: [0.20, 0.75, 0.95],
        bound_exclude: [0.92, 0.25, 0.60],
        bound_open: [0.98, 0.62, 0.12],
        text_dark: TextInks::HOUSE,
        text_light: TextInks::HOUSE,
    };

    /// Protan and deutan share the red-green confusion line, so one
    /// remap serves both (each is still gated under its own eye):
    /// rest goes lemon, gouge a dark oxblood, feed yellow against the
    /// unchanged blue rapid, plunge a dark burnt orange - lightness
    /// does the work the lost red-green axis did - and the unjudged
    /// grey lifts clear of the steel.
    pub const RED_GREEN: Semantic = Semantic {
        clean: [0.42, 0.50, 0.62],
        rest: [0.98, 0.85, 0.20],
        gouge: [0.75, 0.10, 0.10],
        unjudged: [0.80, 0.80, 0.80],
        feed: [0.95, 0.85, 0.25],
        rapid: [0.30, 0.50, 0.95],
        plunge: [0.75, 0.30, 0.00],
        select_hot: [0.98, 0.85, 0.20],
        select_warm: [0.55, 0.48, 0.10],
        bound_include: [0.25, 0.45, 1.00],
        bound_exclude: [0.95, 0.90, 0.30],
        bound_open: [0.95, 0.95, 0.95],
        // Warning on the yellow end, refusal on the violet end of the
        // surviving blue-yellow axis; the missing chip reads as a warning.
        text_dark: TextInks {
            warn: Color32::from_rgb(255, 230, 51),
            error: Color32::from_rgb(204, 140, 255),
            missing: Color32::from_rgb(255, 230, 51),
        },
        text_light: TextInks {
            warn: Color32::from_rgb(140, 77, 0),
            error: Color32::from_rgb(140, 0, 217),
            missing: Color32::from_rgb(140, 77, 0),
        },
    };

    /// Tritan loses blue-yellow: rest moves to pink (red-cyan survives),
    /// gouge to a deeper red, rapid to a dark slate so lightness parts
    /// it from the green feed.
    pub const TRITAN: Semantic = Semantic {
        clean: [0.42, 0.50, 0.62],
        rest: [1.00, 0.45, 0.70],
        gouge: [0.80, 0.05, 0.05],
        unjudged: [0.80, 0.80, 0.80],
        feed: crate::interact::FEED_RGB,
        rapid: [0.20, 0.40, 0.60],
        plunge: crate::interact::PLUNGE_RGB,
        select_hot: [1.00, 0.45, 0.70],
        select_warm: [0.56, 0.25, 0.40],
        bound_include: [0.10, 0.70, 0.75],
        bound_exclude: [0.90, 0.15, 0.20],
        bound_open: [0.95, 0.95, 0.95],
        // Red-cyan survives: warning goes cyan, refusal stays red.
        text_dark: TextInks {
            warn: Color32::from_rgb(77, 217, 217),
            error: Color32::from_rgb(255, 128, 128),
            missing: Color32::from_rgb(77, 217, 217),
        },
        text_light: TextInks {
            warn: Color32::from_rgb(0, 107, 107),
            error: Color32::from_rgb(179, 0, 26),
            missing: Color32::from_rgb(0, 107, 107),
        },
    };

    /// The pairs whose difference IS the message (the boundary roles since
/// M11.5's pick UX), named for the gate's
    /// refusal sentence.
    #[must_use]
    pub fn must_distinguish(&self) -> [(&'static str, [f32; 3], [f32; 3]); 16] {
        [
            ("clean/rest", self.clean, self.rest),
            ("clean/gouge", self.clean, self.gouge),
            ("rest/gouge", self.rest, self.gouge),
            ("unjudged/clean", self.unjudged, self.clean),
            ("unjudged/rest", self.unjudged, self.rest),
            ("unjudged/gouge", self.unjudged, self.gouge),
            ("feed/rapid", self.feed, self.rapid),
            ("feed/plunge", self.feed, self.plunge),
            ("rapid/plunge", self.rapid, self.plunge),
            ("selected/plain stock", self.select_hot, self.clean),
            ("selected/plain edge", self.select_warm, EDGE_DARK),
            ("selected/plain wire", self.select_hot, EDGE_WIRE),
            ("selected/unjudged", self.select_hot, self.unjudged),
            ("include/exclude boundary", self.bound_include, self.bound_exclude),
            ("include/open boundary", self.bound_include, self.bound_open),
            ("exclude/open boundary", self.bound_exclude, self.bound_open),
        ]
    }

    /// The text inks for a panel ground: the light set on a light
    /// panel, the dark set otherwise (WCAG luminance past mid-grey).
    #[must_use]
    pub fn text_on(&self, ground: Color32) -> TextInks {
        if contrast::luma(ground) > LIGHT_GROUND { self.text_light } else { self.text_dark }
    }

    /// The text pairs that must stay apart on this theme's panel:
    /// error, warning and the body text, each against the others.
    #[must_use]
    pub fn text_pairs(&self, t: &Tokens) -> [(&'static str, Color32, Color32); 4] {
        let i = self.text_on(t.ink);
        [
            ("error/warning text", i.error, i.warn),
            ("error/body text", i.error, t.text),
            ("warning/body text", i.warn, t.text),
            ("missing/body text", i.missing, t.text),
        ]
    }
}

/// The luminance past which a panel counts as light for [`Semantic::text_on`]
/// (dark's ink is 0.02, light's 0.85; mid-grey sits at 0.18).
const LIGHT_GROUND: f64 = 0.18;

/// The palette a mode wears.
#[must_use]
pub const fn palette(mode: ColorVision) -> &'static Semantic {
    match mode {
        ColorVision::Off => &Semantic::OFF,
        ColorVision::Protan | ColorVision::Deutan => &Semantic::RED_GREEN,
        ColorVision::Tritan => &Semantic::TRITAN,
    }
}

/// The palette the world's view preference selects.
#[must_use]
pub fn palette_of(world: &flecs_ecs::prelude::World) -> &'static Semantic {
    palette(color_vision_of(world))
}

/// A palette colour as egui ink (the located-verdict rings). Half-up
/// with a 1e-4 nudge, so a channel rounds as the decimal it was typed
/// as: f32's 0.90 is 229.49999.. x 255, and the ring was always 230.
#[must_use]
pub fn ink(c: [f32; 3]) -> Color32 {
    let u = |v: f32| (f64::from(v.clamp(0.0, 1.0)) * 255.0 + 0.5 + 1e-4).floor().min(255.0) as u8;
    Color32::from_rgb(u(c[0]), u(c[1]), u(c[2]))
}

/// The page text inks the world's colour-vision mode gives for this
/// panel (the live ground is the visuals' panel fill, the theme's ink).
#[must_use]
pub fn text_inks(ui: &Ui, world: &flecs_ecs::prelude::World) -> TextInks {
    palette_of(world).text_on(ui.visuals().panel_fill)
}

fn rgb(c: Color32) -> [f64; 3] {
    [f64::from(c.r()) / 255.0, f64::from(c.g()) / 255.0, f64::from(c.b()) / 255.0]
}

/// One ink as the given eye sees it, back on the 8-bit grid.
#[must_use]
pub fn seen_ink(c: Color32, mode: ColorVision) -> Color32 {
    let v = cvd::simulate(rgb(c), mode);
    let u = |x: f64| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(u(v[0]), u(v[1]), u(v[2]), c.a())
}

fn wide(c: [f32; 3]) -> [f64; 3] {
    [f64::from(c[0]), f64::from(c[1]), f64::from(c[2])]
}

/// The token table as the given eye sees it (every colour field run
/// through the simulation; geometry untouched) - what the readability
/// gate reads under a remap.
#[must_use]
pub fn seen_tokens(t: &Tokens, mode: ColorVision) -> Tokens {
    let s = |c: Color32| seen_ink(c, mode);
    Tokens {
        ink: s(t.ink),
        ink_alt: s(t.ink_alt),
        well: s(t.well),
        text: s(t.text),
        text_dim: s(t.text_dim),
        text_faint: s(t.text_faint),
        border: s(t.border),
        accent: s(t.accent),
        accent_hover: s(t.accent_hover),
        field_bg: s(t.field_bg),
        field_hover: s(t.field_hover),
        field_active: s(t.field_active),
        selection_bg: s(t.selection_bg),
        selection_stroke: s(t.selection_stroke),
        ..*t
    }
}

/// The weakest must-distinguish pair under the mode: its name and its
/// simulated CIEDE2000 distance.
#[must_use]
pub fn weakest_pair(mode: ColorVision) -> (&'static str, f64) {
    palette(mode)
        .must_distinguish()
        .iter()
        .map(|(name, a, b)| (*name, cvd::seen_apart(wide(*a), wide(*b), mode)))
        .fold(("", f64::INFINITY), |w, p| if p.1 < w.1 { p } else { w })
}

/// The gate a pick must pass: `None` when the mode's palette keeps
/// every pair apart and the live table still reads through that eye;
/// `Some(sentence)` naming the first failure. `Off` is the house look,
/// not a remap - it owes the theme's readability, not the separation
/// floor (the remaps are what promise it).
#[must_use]
pub fn gate(mode: ColorVision, tokens: &Tokens) -> Option<String> {
    if mode != ColorVision::Off {
        let (name, d) = weakest_pair(mode);
        if d < MIN_DE00 {
            return Some(format!(
                "{name} sits {d:.1} apart (CIEDE2000) for {} - under the {MIN_DE00} floor",
                mode.label()
            ));
        }
        if let Some(why) = text_gate(mode, tokens) {
            return Some(why);
        }
    }
    contrast::readable(&seen_tokens(tokens, mode)).map(|why| format!("{}: {why}", mode.label()))
}

/// The text half of a remap's gate: on this theme's panel, every
/// warning/error/missing ink reads at body contrast against both
/// grounds the pages sit on, and the text pairs keep the floor - all as
/// the mode's eye sees them. `None` = admitted.
#[must_use]
pub fn text_gate(mode: ColorVision, tokens: &Tokens) -> Option<String> {
    let p = palette(mode);
    let inks = p.text_on(tokens.ink);
    for (role, c) in [("error", inks.error), ("warning", inks.warn), ("missing", inks.missing)] {
        for (ground, bg) in [("ink", tokens.ink), ("ink_alt", tokens.ink_alt)] {
            let r = contrast::ratio(seen_ink(c, mode), seen_ink(bg, mode));
            if r < contrast::BODY {
                return Some(format!(
                    "{role} text on {ground} reads {r:.1}:1 for {}, under the {}:1 floor",
                    mode.label(),
                    contrast::BODY
                ));
            }
        }
    }
    for (name, a, b) in p.text_pairs(tokens) {
        let d = cvd::seen_apart(rgb(a), rgb(b), mode);
        if d < MIN_DE00 {
            return Some(format!(
                "{name} sits {d:.1} apart (CIEDE2000) for {} - under the {MIN_DE00} floor",
                mode.label()
            ));
        }
    }
    None
}

/// The picker beside the theme switcher: a combo of the four modes,
/// read live off the world like the theme row above it; a pick queues
/// the command, never writes the singleton.
pub(crate) fn picker(ui: &mut Ui, state: &mut State) {
    let current = color_vision_of(&state.world);
    ui.horizontal(|ui| {
        ui.label("Colour vision");
        let mut pick: Option<ColorVision> = None;
        egui::ComboBox::from_id_salt("color_vision_switcher")
            .selected_text(current.label())
            .show_ui(ui, |ui| {
                for mode in ColorVision::ALL {
                    if ui.selectable_label(mode == current, mode.label()).clicked() && mode != current {
                        pick = Some(mode);
                    }
                }
            });
        if let Some(mode) = pick {
            state.pending.push_back(CommandKind::SelectColorVision(mode));
        }
    });
}
