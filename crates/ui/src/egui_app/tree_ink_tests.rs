//! #506: the browser tree's row ink reads on the ground it is painted on,
//! in both built-in themes and whichever appearance the OS reports.
//! Structural (no pixels, no GPU): the inks `painted::row_inks` hands the
//! tree are measured against the grounds the tree paints on, with the
//! same WCAG floor `contrast::readable` holds the theme table to; and the
//! egui context a theme is applied to carries that table in BOTH of its
//! theme slots, so the panel the tree sits on is the table's own fill.
//! Provenance: our own tests over our own palettes.

use super::contrast;
use super::painted::row_inks;
use super::theme::{self, Tokens};

const THEMES: [(&str, Tokens); 2] = [("dark", Tokens::DARK), ("light", Tokens::LIGHT)];

/// Every ground a tree row can sit on: the panel, the strip, and the
/// row's own hover and selection fills.
fn grounds(t: &Tokens) -> [(&'static str, egui::Color32); 4] {
    [
        ("ink", t.ink),
        ("strip", t.strip_fill()),
        ("field_hover", t.field_hover),
        ("selection_bg", t.selection_bg),
    ]
}

#[test]
fn tree_row_ink_clears_body_contrast_on_every_row_ground() {
    for (name, t) in THEMES {
        for hidden in [false, true] {
            for (ground, bg) in grounds(&t) {
                // Only a selected row sits on the selection fill.
                let selected = ground == "selection_bg";
                let (label, suffix) = row_inks(&t, hidden, selected);
                for (part, ink) in [("label", label), ("suffix", suffix)] {
                    let r = contrast::ratio(ink, bg);
                    assert!(
                        r >= contrast::BODY,
                        "{name}: {part} (hidden={hidden}) {ink:?} on {ground} {bg:?} reads {r:.2}:1"
                    );
                }
            }
        }
        assert_eq!(row_inks(&t, false, false).0, t.text, "{name}: a row label is body text");
    }
}

/// The root cause: egui keeps a dark AND a light style and follows the
/// OS appearance. A light-mode system must not swap the table out from
/// under the painted tree - both slots carry the applied table.
#[test]
fn applied_theme_fills_both_egui_theme_slots() {
    for (name, t) in THEMES {
        let ctx = egui::Context::default();
        theme::apply_to(&ctx, &t);
        for slot in [egui::Theme::Dark, egui::Theme::Light] {
            let v = &ctx.style_of(slot).visuals;
            assert_eq!(v.panel_fill, t.ink, "{name}: {slot:?} slot's panel is the table's ink");
            assert_eq!(v.window_fill, t.ink_alt, "{name}: {slot:?} slot's window is the table's");
            assert_eq!(v.override_text_color, Some(t.text), "{name}: {slot:?} slot's text");
            let (label, _) = row_inks(&t, false, false);
            let r = contrast::ratio(label, v.panel_fill);
            assert!(r >= contrast::BODY, "{name}: {slot:?} slot tree label reads {r:.2}:1");
        }
        // The appearance the OS reports flips the active slot; the panel
        // the tree is drawn on stays the table's.
        for os in [egui::Theme::Light, egui::Theme::Dark] {
            ctx.set_theme(os);
            assert_eq!(ctx.global_style().visuals.panel_fill, t.ink, "{name}: OS {os:?} kept the table");
        }
    }
}
