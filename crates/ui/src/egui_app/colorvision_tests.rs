//! M13.4, the ColorVision law: under each remap every must-distinguish
//! pair keeps a CIEDE2000 distance of at least `MIN_DE00` as that eye
//! sees it (Machado 2009 simulation), both built-in token tables still
//! pass the WCAG readability gate through that eye, Off is the house
//! palette exactly, and the pick rides the command law (undo puts the
//! previous mode back; a refused mode never enters the history).
//! Provenance: our own tests over our own palettes.

use camcraft_app::commands::color_vision_of;
use camcraft_app::schema::ColorVision;

use super::colorvision::{self, MIN_DE00, Semantic};
use super::{contrast, cvd, theme::Tokens};
use crate::state::{CommandKind, State};

const REMAPS: [ColorVision; 3] = [ColorVision::Protan, ColorVision::Deutan, ColorVision::Tritan];

fn wide(c: [f32; 3]) -> [f64; 3] {
    [f64::from(c[0]), f64::from(c[1]), f64::from(c[2])]
}

#[test]
fn every_remap_keeps_each_must_distinguish_pair_apart() {
    for mode in REMAPS {
        for (name, a, b) in colorvision::palette(mode).must_distinguish() {
            let d = cvd::seen_apart(wide(a), wide(b), mode);
            assert!(d >= MIN_DE00, "{mode:?} {name}: CIEDE2000 {d:.1} < {MIN_DE00}");
        }
        let (_, weakest) = colorvision::weakest_pair(mode);
        assert!(weakest >= MIN_DE00, "{mode:?}: weakest pair {weakest:.1}");
    }
}

/// The remaps are not decoration: the house palette, seen by each
/// deficient eye, has a pair under the floor that the remap lifts
/// (deutan's feed/plunge is the textbook green/red collapse).
#[test]
fn the_house_palette_needs_the_remap() {
    let off = Semantic::OFF;
    let d = cvd::seen_apart(wide(off.feed), wide(off.plunge), ColorVision::Deutan);
    assert!(d < MIN_DE00, "deutan feed/plunge on the house palette reads {d:.1}");
    let rg = colorvision::palette(ColorVision::Deutan);
    assert!(cvd::seen_apart(wide(rg.feed), wide(rg.plunge), ColorVision::Deutan) >= MIN_DE00);
}

#[test]
fn both_built_in_tables_read_through_every_eye() {
    for mode in ColorVision::ALL {
        for (col, t) in [("dark", Tokens::DARK), ("light", Tokens::LIGHT)] {
            let seen = colorvision::seen_tokens(&t, mode);
            assert_eq!(contrast::readable(&seen), None, "{mode:?} {col}");
            assert_eq!(colorvision::gate(mode, &t), None, "{mode:?} {col}: the gate admits it");
            // The selection line still stands off its strip for that eye.
            let d = cvd::seen_apart(
                [f64::from(t.selection_stroke.r()) / 255.0, f64::from(t.selection_stroke.g()) / 255.0, f64::from(t.selection_stroke.b()) / 255.0],
                [f64::from(t.ink_alt.r()) / 255.0, f64::from(t.ink_alt.g()) / 255.0, f64::from(t.ink_alt.b()) / 255.0],
                mode,
            );
            assert!(d >= MIN_DE00, "{mode:?} {col}: selection_stroke/ink_alt {d:.1}");
        }
    }
}

/// Off changes nothing: the palette is the literals the shaders and
/// the rings carried before M13.4, the token table is its own image,
/// and a fresh world is Off.
#[test]
fn off_is_the_house_look_exactly() {
    let off = colorvision::palette(ColorVision::Off);
    assert_eq!(*off, Semantic::OFF);
    assert_eq!(off.clean, [0.42, 0.50, 0.62]);
    assert_eq!(off.rest, [0.98, 0.72, 0.20]);
    assert_eq!(off.gouge, [0.90, 0.18, 0.14]);
    assert_eq!(off.unjudged, [0.45, 0.46, 0.48]);
    assert_eq!(off.select_hot, [0.98, 0.72, 0.20]);
    assert_eq!(off.select_warm, [0.55, 0.35, 0.05]);
    assert_eq!(off.feed, crate::interact::FEED_RGB);
    assert_eq!(off.rapid, crate::interact::RAPID_RGB);
    assert_eq!(off.plunge, crate::interact::PLUNGE_RGB);
    assert_eq!(colorvision::ink(off.rest), egui::Color32::from_rgb(250, 184, 51));
    assert_eq!(colorvision::ink(off.gouge), egui::Color32::from_rgb(230, 46, 36));
    for t in [Tokens::DARK, Tokens::LIGHT] {
        assert_eq!(colorvision::seen_tokens(&t, ColorVision::Off), t);
    }
    assert_eq!(color_vision_of(&State::new().world), ColorVision::Off);
}

/// The pick is a command: queued, gated, executed onto the history;
/// undo and redo walk it; re-picking the live mode is refused and
/// pushes nothing.
#[test]
fn switching_is_a_command_with_undo() {
    let mut state = State::new();
    let run = |state: &mut State, k: CommandKind| {
        state.pending.push_back(k);
        state.drain_actions();
    };
    run(&mut state, CommandKind::SelectColorVision(ColorVision::Protan));
    assert_eq!(color_vision_of(&state.world), ColorVision::Protan);
    assert!(state.status.contains("Protanopia"), "{}", state.status);
    run(&mut state, CommandKind::SelectColorVision(ColorVision::Tritan));
    assert_eq!(color_vision_of(&state.world), ColorVision::Tritan);
    run(&mut state, CommandKind::Undo);
    assert_eq!(color_vision_of(&state.world), ColorVision::Protan);
    run(&mut state, CommandKind::Undo);
    assert_eq!(color_vision_of(&state.world), ColorVision::Off);
    run(&mut state, CommandKind::Redo);
    assert_eq!(color_vision_of(&state.world), ColorVision::Protan);
    run(&mut state, CommandKind::SelectColorVision(ColorVision::Protan));
    assert_eq!(color_vision_of(&state.world), ColorVision::Protan);
    run(&mut state, CommandKind::Undo);
    assert_eq!(color_vision_of(&state.world), ColorVision::Off, "the refused re-pick pushed nothing");
}
