//! The `camcraft-theme/1` format, end to end (M6.1 B): the shipped
//! worked example, the overlay law (every override lands clamped), the
//! hostile-input refusals (each must name its rule), the byte budget,
//! the listing walk, and the parse cache's hit identity. The switcher
//! that consumes all this arrives with stage C; this file pins the
//! doors it will open.
//! Provenance: our own tests, specced from the ROADMAP's M6.1 notes.
//!
//! Gate map (M4.4): covers ui::egui_app::theme_files, ui::egui_app::theme.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use camcraft_ui::egui_app::theme_files as tf;
use egui::Color32;

fn tmp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft_ui_theme_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the fixture dir");
    d
}

fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, text).expect("write the fixture file");
    p
}

/// The repo's own example theme: parsed, applied, and every untouched
/// field still DARK's number.
#[test]
fn the_shipped_example_parses() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/themes/teal-night.json");
    let theme = tf::load(&path).unwrap_or_else(|e| panic!("the shipped example must load: {e}"));
    assert_eq!(theme.name, "Teal Night");
    assert_eq!(theme.creator, "camcraft");
    assert!(theme.notes.is_empty(), "a clean file carries no notes: {:?}", theme.notes);
    assert_eq!(theme.tokens.accent, Color32::from_rgb(0x2A, 0x7F, 0x79));
    assert_eq!(theme.tokens.accent_hover, Color32::from_rgb(0x3C, 0xA0, 0x99));
    assert_eq!(theme.tokens.radius_widget, 3);
    assert_eq!(theme.tokens.glyph_px, 22.0);
    // Untouched fields keep the dark column's numbers.
    assert_eq!(theme.tokens.ink, Color32::from_rgb(0x23, 0x26, 0x2B));
    assert_eq!(theme.tokens.cell_h, 35.0);
    assert_eq!(theme.tokens.font_row_suffix, 10.5);
}

/// The ugly-but-legal law: a file can make the shell odd, never
/// impossible - every hostile geometry value lands inside its clamp
/// window instead of refusing (the refusals are for SHAPE, the clamps
/// for size).
#[test]
fn hostile_geometry_lands_clamped() {
    let t = tf::parse_text(
        r#"{"format":"camcraft-theme/1","descriptor":{"name":"Pushy"},"base":"dark",
            "geometry":{"radius_widget":400,"radius_window":1000,"cell_h":0,
            "font_row":999,"item_spacing_x":-50,"control_height":7,"glyph_px":5}}"#,
        "pushy",
    )
    .expect("a big number is ugly, not corrupt");
    assert_eq!(t.tokens.radius_widget, 12); // clamp::RADIUS
    assert_eq!(t.tokens.radius_window, 24); // the window/menu cap
    assert_eq!(t.tokens.cell_h, 24.0); // CELL floor
    assert_eq!(t.tokens.font_row, 32.0); // FONT ceiling
    assert_eq!(t.tokens.item_spacing_x, 0.0); // SPACING floor
    assert_eq!(t.tokens.control_height, 12.0); // CONTROL_H floor
    assert_eq!(t.tokens.glyph_px, 8.0); // GLYPH floor
}

/// The whole vocabulary lands: colours by name, alpha included.
#[test]
fn colours_override_by_name() {
    let t = tf::parse_text(
        r##"{"format":"camcraft-theme/1","descriptor":{"name":"Tinted"},"base":"dark",
            "colors":{"ink":"#101112","well":"#0A0B0CFF","accent_hover":"#AABBCC"}}"##,
        "tinted",
    )
    .expect("three overrides must parse");
    assert_eq!(t.tokens.ink, Color32::from_rgb(0x10, 0x11, 0x12));
    assert_eq!(t.tokens.well, Color32::from_rgba_unmultiplied(0x0A, 0x0B, 0x0C, 0xFF));
    assert_eq!(t.tokens.accent_hover, Color32::from_rgb(0xAA, 0xBB, 0xCC));
    assert_eq!(t.tokens.accent, Color32::from_rgb(0x35, 0x6D, 0xA8), "untouched stays dark");
}

/// Unknown schema keys are reported, unknown TOKENS are refusals -
/// the roadmap's two sentences, kept distinct.
#[test]
fn unknown_keys_report_unknown_tokens_refuse() {
    let t = tf::parse_text(
        r#"{"format":"camcraft-theme/1","descriptor":{"name":"Odd","colourist":"nobody"},"base":"dark","extra":1}"#,
        "odd",
    )
    .expect("unknown keys only report");
    assert!(t.notes.iter().any(|n| n.contains("\"extra\"")), "{:?}", t.notes);
    assert!(t.notes.iter().any(|n| n.contains("\"colourist\"")), "{:?}", t.notes);
    let err = tf::parse_text(
        r##"{"format":"camcraft-theme/1","descriptor":{"name":"Bad"},"base":"dark","colors":{"hover":"#FFF"}}"##,
        "bad",
    )
    .expect_err("\"hover\" is not a token - a theme that sets nothing must not pretend");
    assert!(err.contains("unknown token \"hover\""), "{err}");
    assert!(err.contains("selection_stroke"), "{err}"); // names what IS legal
}

/// The misplaced-token hint: the format has two maps, and saying so
/// turns a paste mistake into a sentence.
#[test]
fn misplaced_tokens_say_where_they_belong() {
    let err = tf::parse_text(
        r##"{"format":"camcraft-theme/1","descriptor":{"name":"Swap"},"base":"dark","colors":{"cell_h":"#FFFFFF"}}"##,
        "swap",
    )
    .expect_err("cell_h is geometry");
    assert!(err.contains("belongs under \"geometry\""), "{err}");
    let err = tf::parse_text(
        r#"{"format":"camcraft-theme/1","descriptor":{"name":"Swap"},"base":"dark","geometry":{"ink":3}}"#,
        "swap",
    )
    .expect_err("ink is a colour");
    assert!(err.contains("belongs under \"colors\""), "{err}");
}

/// Every refusal shape, each named. Nothing here may panic; the
/// refusals ARE the behaviour.
#[test]
fn the_refusal_battery() {
    let long_name = format!(
        r#"{{"format":"camcraft-theme/1","descriptor":{{"name":"{}"}},"base":"dark"}}"#,
        "n".repeat(70)
    );
    let fat_keys = (0..65)
        .map(|i| format!("\"k{i}\":\"#112233\""))
        .collect::<Vec<_>>()
        .join(",");
    let fat_map = format!(
        r#"{{"format":"camcraft-theme/1","descriptor":{{"name":"n"}},"base":"dark","colors":{{{fat_keys}}}}}"#
    );
    let bad: Vec<(&str, &str)> = vec![
        ("not json at all", "not valid JSON"),
        ("[]", "one JSON object"),
        (r#"{"format":"camcraft-theme/2"}"#, "refused whole"),
        (r#"{"descriptor":{"name":"x"},"base":"dark"}"#, "missing \"format\""),
        (r#"{"format":"camcraft-theme/1","base":"dark"}"#, "missing \"descriptor\""),
        (r#"{"format":"camcraft-theme/1","descriptor":{},"base":"dark"}"#, "name is required"),
        (
            r#"{"format":"camcraft-theme/1","descriptor":{"name":"n"},"base":"solarpunk"}"#,
            "unknown theme base",
        ),
        (
            r#"{"format":"camcraft-theme/1","descriptor":{"name":"n"}}"#,
            "missing \"base\"",
        ),
        (
            r#"{"format":"camcraft-theme/1","descriptor":{"name":"n"},"base":"dark","colors":["x"]}"#,
            "must be an object",
        ),
        (
            r##"{"format":"camcraft-theme/1","descriptor":{"name":"n"},"base":"dark","colors":{"accent":["#112233","#445566"]}}"##,
            "ramp array",
        ),
        (
            r##"{"format":"camcraft-theme/1","descriptor":{"name":"n"},"base":"dark","colors":{"accent":"red"}}"##,
            "not \"#rrggbb\"",
        ),
        (
            r##"{"format":"camcraft-theme/1","descriptor":{"name":"n"},"base":"dark","colors":{"accent":"#11223"}}"##,
            "not \"#rrggbb\"",
        ),
        (
            r##"{"format":"camcraft-theme/1","descriptor":{"name":"n"},"base":"dark","colors":{"accent":"#1122334Z"}}"##,
            "not \"#rrggbb\"",
        ),
        (
            r#"{"format":"camcraft-theme/1","descriptor":{"name":"n"},"base":"dark","geometry":{"cell_h":"35"}}"#,
            "must be a number",
        ),
        (
            // serde_json refuses a magnitude past f64 at the parse step,
            // before the module's own finite check can see it; the
            // refusal is still by name, so the battery pins the text.
            r#"{"format":"camcraft-theme/1","descriptor":{"name":"n"},"base":"dark","geometry":{"cell_h":1e999}}"#,
            "number out of range",
        ),
        (
            r#"{"format":"camcraft-theme/1","descriptor":{"name":"n"},"base":"dark","geometry":3}"#,
            "must be an object",
        ),
        (long_name.as_str(), "past the 64-byte cap"),
        (fat_map.as_str(), "past the 64 cap"),
    ];
    for (text, want) in bad {
        let err = tf::parse_text(text, "battery").expect_err(&format!("must refuse: {want}"));
        assert!(err.contains(want), "wanted {want:?}, got {err}");
    }
}

/// The battery case that refused `base:"light"` "when M6.1 C lands"
/// flipped to a support check: the light column is a real table, so a
/// file may build on it, and an overlay on top of light lands on
/// light's numbers - not dark's.
#[test]
fn a_light_base_is_a_real_starting_point() {
    use camcraft_ui::egui_app::theme::Tokens;
    let t = tf::parse_text(
        r#"{"format":"camcraft-theme/1","descriptor":{"name":"L"},"base":"light"}"#,
        "light",
    )
    .expect("M6.1 C landed: light is a base now");
    assert_eq!(t.tokens.text, Tokens::LIGHT.text);
    let u = tf::parse_text(
        r##"{"format":"camcraft-theme/1","descriptor":{"name":"L2"},"base":"light",
            "colors":{"accent":"#FF0000"}}"##,
        "light",
    )
    .expect("an overlay rides the light base");
    assert_eq!(u.tokens.accent, Color32::from_rgb(0xFF, 0x00, 0x00));
    assert_eq!(u.tokens.text, Tokens::LIGHT.text); // untouched: light's
    assert_eq!(u.tokens.cell_h, Tokens::LIGHT.cell_h); // geometry: same in both
}

#[test]
fn the_byte_budget_refuses_before_reading_everything() {
    let dir = tmp_dir("budget");
    let path = write(&dir, "fat.json", &format!(r#"{{"pad":"{}"}}"#, " ".repeat(300 * 1024)));
    let err = tf::load(&path).expect_err("a fat file is not a theme");
    assert!(err.contains("budget"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn discovery_lists_sorted_and_says_what_it_skipped() {
    let dir = tmp_dir("discover");
    write(&dir, "zeta.json", "{}"); // malformed content is still listed
    write(&dir, "alpha.json", "{}");
    write(&dir, "notes.txt", "not a theme");
    write(&dir, "sub.json", "{}");
    let (paths, notes) = tf::discover(&dir);
    let names: Vec<String> = paths
        .iter()
        .map(|p| p.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string())
        .collect();
    assert_eq!(names, vec!["alpha.json", "sub.json", "zeta.json"]);
    assert!(notes.is_empty(), "{notes:?}");
    // A listed file may still refuse at load time - naming, never panicking.
    assert!(tf::load(&paths[0]).is_err());
    let (paths, notes) = tf::discover(&dir.join("no-such-dir"));
    assert!(paths.is_empty() && notes.is_empty(), "a missing dir is a normal state");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_cache_holds_one_parse_per_unchanged_file() {
    let dir = tmp_dir("cache");
    let path = write(
        &dir,
        "one.json",
        r#"{"format":"camcraft-theme/1","descriptor":{"name":"One"},"base":"dark"}"#,
    );
    let mut cache = tf::Cache::new();
    let first = cache.get(&path);
    let again = cache.get(&path);
    assert!(Arc::ptr_eq(&first, &again), "an unchanged file must not re-parse");
    let gone = dir.join("vanished.json");
    let miss = cache.get(&gone);
    assert!(miss.is_err(), "a vanished file refuses by name");
    let _ = std::fs::remove_dir_all(&dir);
}
