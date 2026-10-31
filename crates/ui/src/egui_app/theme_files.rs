//! Themes as files (M6.1 B): the `camcraft-theme/1` JSON format, its
//! hostile-input caps, the disk discovery walk, and the parse cache.
//! A theme file names a base (`dark` or, since M6.1 C, `light` - the
//! two compiled-in columns) and overrides individual tokens BY NAME, so
//! a custom theme is a handful of lines and `Tokens::DARK`/`LIGHT` stay
//! the single truth for the defaults: a binary with no assets directory
//! themes exactly as it always did (the roadmap's include_str! intent,
//! carried by the Rust constants - two theme files for the same
//! numbers would be two truths; the note in RECOMMENDED.md owns the
//! deviation). Unknown tokens refuse by name, never half-apply;
//! unknown schema keys are reported, never silently dropped. Since
//! M6.1 D the reader also derives and gates: an accent-state the file
//! leaves unspelled follows the file's accent (`contrast::follow_
//! states`), and a table whose text cannot be read refuses BY NAME
//! (`contrast::readable`) - an unreadable theme is a visible refusal,
//! not a style preference. No file paths live inside a theme - icons
//! stay in `assets/ui/` by convention (M6.1's rule, kept).
//! Provenance: our own format, specced from the ROADMAP's M6.1 notes
//! and the sibling repo's file-shape at the user's direction; every
//! name, key and number here is ours.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use camcraft_app::schema::ThemeRef;
use egui::Color32;
use serde_json::Value;

use super::contrast;
use super::theme::{Tokens, clamp};

/// The one format string this build reads; anything else refuses whole.
pub const FORMAT: &str = "camcraft-theme/1";
/// Hostile-input caps, from day one (the Prime Directive, chrome
/// edition: a corrupt theme is a visible refusal, never a crash and
/// never a half-applied repaint).
pub const MAX_BYTES: u64 = 256 * 1024;
pub const MAX_LIST: usize = 64;
pub const MAX_MAP_ENTRIES: usize = 64;
pub const MAX_KEY_LEN: usize = 48;
pub const MAX_NAME_LEN: usize = 64;
pub const MAX_CREATOR_LEN: usize = 64;
pub const MAX_DESCRIPTION_LEN: usize = 200;

/// The token names the format speaks, spelled exactly once; the
/// parser's membership check and the overlay's field wiring both read
/// these constants, so the file's vocabulary and the code's fields
/// cannot drift (the all-names-equal-DARK test closes the other way).
mod names {
    pub const INK: &str = "ink";
    pub const INK_ALT: &str = "ink_alt";
    pub const WELL: &str = "well";
    pub const TEXT: &str = "text";
    pub const TEXT_DIM: &str = "text_dim";
    pub const TEXT_FAINT: &str = "text_faint";
    pub const BORDER: &str = "border";
    pub const ACCENT: &str = "accent";
    pub const ACCENT_HOVER: &str = "accent_hover";
    pub const FIELD_BG: &str = "field_bg";
    pub const FIELD_HOVER: &str = "field_hover";
    pub const FIELD_ACTIVE: &str = "field_active";
    pub const SELECTION_BG: &str = "selection_bg";
    pub const SELECTION_STROKE: &str = "selection_stroke";
    pub const RADIUS_WIDGET: &str = "radius_widget";
    pub const RADIUS_WINDOW: &str = "radius_window";
    pub const RADIUS_MENU: &str = "radius_menu";
    pub const ITEM_SPACING_X: &str = "item_spacing_x";
    pub const ITEM_SPACING_Y: &str = "item_spacing_y";
    pub const BUTTON_PADDING_X: &str = "button_padding_x";
    pub const BUTTON_PADDING_Y: &str = "button_padding_y";
    pub const CONTROL_HEIGHT: &str = "control_height";
    pub const ROW_HEIGHT: &str = "row_height";
    pub const CELL_H: &str = "cell_h";
    pub const CELL_W: &str = "cell_w";
    pub const GLYPH_PX: &str = "glyph_px";
    pub const FONT_LABEL: &str = "font_label";
    pub const FONT_CAPTION: &str = "font_caption";
    pub const FONT_ROW: &str = "font_row";
    pub const FONT_ROW_SUFFIX: &str = "font_row_suffix";
}

/// The colour vocabulary, in table order (refusals quote it).
pub const COLOR_NAMES: &[&str] = &[
    names::INK, names::INK_ALT, names::WELL, names::TEXT, names::TEXT_DIM,
    names::TEXT_FAINT, names::BORDER, names::ACCENT, names::ACCENT_HOVER,
    names::FIELD_BG, names::FIELD_HOVER, names::FIELD_ACTIVE,
    names::SELECTION_BG, names::SELECTION_STROKE,
];
/// The geometry vocabulary (every override is clamped through the
/// same windows the built-in const guard checks).
pub const GEOM_NAMES: &[&str] = &[
    names::RADIUS_WIDGET, names::RADIUS_WINDOW, names::RADIUS_MENU,
    names::ITEM_SPACING_X, names::ITEM_SPACING_Y, names::BUTTON_PADDING_X,
    names::BUTTON_PADDING_Y, names::CONTROL_HEIGHT, names::ROW_HEIGHT,
    names::CELL_H, names::CELL_W, names::GLYPH_PX, names::FONT_LABEL,
    names::FONT_CAPTION, names::FONT_ROW, names::FONT_ROW_SUFFIX,
];

/// One theme, read and applied: what it is called, the token table it
/// describes, and the honest notes gathered on the way (reported
/// keys, never silently dropped).
#[derive(Debug)]
pub struct Theme {
    pub name: String,
    pub creator: String,
    pub description: String,
    pub tokens: Tokens,
    pub notes: Vec<String>,
}

/// Read and parse one theme file. Every refusal names the path and the
/// rule; the byte budget is spent before the file is paid for.
pub fn load(path: &Path) -> Result<Theme, String> {
    let origin = path.display().to_string();
    let mut buf = Vec::new();
    let file = std::fs::File::open(path).map_err(|e| format!("{origin}: {e}"))?;
    let mut limited = std::io::Read::take(file, MAX_BYTES + 1);
    std::io::Read::read_to_end(&mut limited, &mut buf)
        .map_err(|e| format!("{origin}: read failed: {e}"))?;
    if buf.len() as u64 > MAX_BYTES {
        return Err(format!("{origin}: over the {MAX_BYTES}-byte theme budget - refused whole"));
    }
    let text = std::str::from_utf8(&buf).map_err(|_| format!("{origin}: not UTF-8 text"))?;
    parse_text(text, &origin)
}

/// List the `*.json` theme files of a directory: sorted for a stable
/// switcher, capped at `MAX_LIST` with a note when the cap bites. A
/// missing directory is a normal state (a no-assets install), not a
/// diagnostic; an unreadable entry names itself.
pub fn discover(dir: &Path) -> (Vec<PathBuf>, Vec<String>) {
    let mut notes = Vec::new();
    let read = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return (Vec::new(), notes),
    };
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in read {
        match entry {
            Ok(e) => {
                let p = e.path();
                if p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("json")) {
                    paths.push(p);
                }
            }
            Err(err) => notes.push(format!("{dir:?}: directory entry unreadable: {err}")),
        }
    }
    paths.sort();
    if paths.len() > MAX_LIST {
        notes.push(format!(
            "{dir:?}: {} theme files, more than the {MAX_LIST} this build lists - the rest are not shown",
            paths.len()
        ));
        paths.truncate(MAX_LIST);
    }
    (paths, notes)
}

/// Parsed themes by path, held across the session: the realtime swap
/// costs one `mtime` stat per hit, never a re-read or re-parse per
/// frame. Files whose timestamp vanished (unmounted, deleted) simply
/// re-enter the load path and refuse by name.
#[derive(Default)]
pub struct Cache {
    entries: HashMap<PathBuf, (SystemTime, Arc<Result<Theme, String>>)>,
}

impl Cache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The theme at `path`, parsed at most once per change.
    pub fn get(&mut self, path: &Path) -> Arc<Result<Theme, String>> {
        let stamp = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if let (Some(now), Some((then, held))) = (stamp, self.entries.get(path))
            && *then == now
        {
            return held.clone();
        }
        let loaded = Arc::new(load(path));
        if let Some(now) = stamp {
            self.entries.insert(path.to_path_buf(), (now, loaded.clone()));
        }
        loaded
    }

    /// The parsed theme's name if this path is already cached AND
    /// loaded - a label read for the switcher's rows and selected
    /// text, which must never trigger a parse or a re-read (the pick
    /// itself is where a file is first paid for).
    #[must_use]
    pub fn peek_label(&self, path: &Path) -> Option<String> {
        self.entries.get(path).and_then(|(_, held)| match &**held {
            Ok(theme) => Some(theme.name.clone()),
            Err(_) => None,
        })
    }
}

/// Resolve a document's theme reference into the table to wear, and
/// the notes gathered on the way (M6.1 C - the switcher's read).
/// Built-ins land their compiled columns - `Dark` and `Named("dark")`
/// are the same table, per the omission law - and a file lands its
/// overlay, parsed at most once per change. Anything that cannot land
/// - a stranger name, a vanished file, a corrupt parse - refuses BY
///   NAME with no silent fallback to dark (the `script_source` law
///   worn in theme clothes); the CALLER keeps the live tokens on
///   refusal, which is what makes the refusal safe to state once.
pub fn resolve(cache: &mut Cache, reference: &ThemeRef) -> Result<(Tokens, Vec<String>), String> {
    match reference {
        ThemeRef::Dark => Ok((Tokens::DARK, Vec::new())),
        ThemeRef::Named(name) => match name.as_str() {
            "dark" => Ok((Tokens::DARK, Vec::new())),
            "light" => Ok((Tokens::LIGHT, Vec::new())),
            other => Err(format!("built-in theme {other} is not one of dark, light")),
        },
        ThemeRef::File(path) => match &*cache.get(Path::new(path)) {
            Ok(theme) => Ok((theme.tokens, theme.notes.clone())),
            Err(why) => Err(why.clone()),
        },
    }
}

/// Parse one theme's text. Split from `load` so a test (and the C
/// stage's paste-a-theme door) can walk the refusals without a disk.
pub fn parse_text(text: &str, origin: &str) -> Result<Theme, String> {
    let root: Value =
        serde_json::from_str(text).map_err(|e| format!("{origin}: not valid JSON: {e}"))?;
    let Value::Object(top) = root else {
        return Err(format!("{origin}: a theme file is one JSON object at the top level"));
    };
    match top.get("format") {
        Some(Value::String(f)) if f == FORMAT => {}
        Some(Value::String(f)) => {
            return Err(format!(
                "{origin}: format \"{f}\" is not \"{FORMAT}\" - this build cannot read it, refused whole"
            ));
        }
        _ => return Err(format!("{origin}: missing \"format\" (expected \"{FORMAT}\")")),
    }
    let mut notes: Vec<String> = Vec::new();
    for key in top.keys() {
        if !matches!(key.as_str(), "format" | "descriptor" | "base" | "colors" | "geometry") {
            notes.push(format!(
                "{origin}: unknown top-level key \"{key}\" ignored (reported, not silently dropped)"
            ));
        }
    }
    let Some(Value::Object(desc)) = top.get("descriptor") else {
        return Err(format!("{origin}: missing \"descriptor\" - a theme that cannot name itself is not listed"));
    };
    for key in desc.keys() {
        if !matches!(key.as_str(), "name" | "creator" | "description") {
            notes.push(format!("{origin}: unknown descriptor key \"{key}\" ignored"));
        }
    }
    let name = req_str(desc, "name", MAX_NAME_LEN, origin)?;
    let creator = opt_str(desc, "creator", MAX_CREATOR_LEN, origin)?;
    let description = opt_str(desc, "description", MAX_DESCRIPTION_LEN, origin)?;
    let base = match top.get("base") {
        Some(Value::String(b)) if b == "dark" => Tokens::DARK,
        // The arm that refused light "when M6.1 C lands" is C landing:
        // Tokens::LIGHT is a real table now, and it is the law that
        // both built-ins are base-able.
        Some(Value::String(b)) if b == "light" => Tokens::LIGHT,
        Some(Value::String(b)) => return Err(format!("{origin}: unknown theme base \"{b}\"")),
        _ => return Err(format!("{origin}: missing \"base\" (\"dark\" or \"light\")")),
    };
    let colors = read_map(&top, "colors", COLOR_NAMES, origin, &mut |k, v| match v {
        Value::String(s) => parse_hex(s)
            .ok_or_else(|| format!("{origin}: colors.{k} is not \"#rrggbb\"/\"#rrggbbaa\" (got \"{s}\")")),
        Value::Array(_) => Err(format!(
            "{origin}: colors.{k} carries a ramp array - not implemented in {FORMAT}"
        )),
        _ => Err(format!("{origin}: colors.{k} must be a colour string")),
    })?;
    let geom = read_map(&top, "geometry", GEOM_NAMES, origin, &mut |k, v| match v {
        Value::Number(n) => match n.as_f64() {
            Some(x) if x.is_finite() => Ok(x),
            _ => Err(format!("{origin}: geometry.{k} is not a finite number")),
        },
        _ => Err(format!("{origin}: geometry.{k} must be a number")),
    })?;
    let mut tokens = apply_overlay(base, &colors, &geom);
    // M6.1 D, after the overlay and on the whole table: every accent
    // state the file left unspelled follows its accent (generated, not
    // trusted from a hand - each derivation says so), and a table
    // whose text cannot be read refuses here, naming its pair and its
    // ratio. Both built-in columns clear the gate by construction, so
    // a file that spells no colours lands exactly its base.
    notes.extend(contrast::follow_states(&mut tokens, &base, &|k| colors.contains_key(k)));
    if let Some(why) = contrast::readable(&tokens) {
        return Err(format!("{origin}: {why}"));
    }
    Ok(Theme { name, creator, description, tokens, notes })
}

/// Walk one override map through the shared refusals (size cap, key
/// length, vocabulary, wrong-map hint); `check` owns the per-map
/// value shape.
fn read_map<T>(
    top: &serde_json::Map<String, Value>,
    field: &str,
    vocab: &[&str],
    origin: &str,
    check: &mut dyn FnMut(&str, &Value) -> Result<T, String>,
) -> Result<HashMap<String, T>, String> {
    let Some(value) = top.get(field) else { return Ok(HashMap::new()) };
    let Value::Object(map) = value else {
        return Err(format!("{origin}: \"{field}\" must be an object of name -> value"));
    };
    if map.len() > MAX_MAP_ENTRIES {
        return Err(format!(
            "{origin}: {field} carries {} entries, past the {MAX_MAP_ENTRIES} cap",
            map.len()
        ));
    }
    let other = if field == "colors" { "geometry" } else { "colors" };
    let mut out = HashMap::with_capacity(map.len());
    for (k, v) in map {
        if k.len() > MAX_KEY_LEN {
            return Err(format!("{origin}: {field} key of {} bytes, past the {MAX_KEY_LEN} cap", k.len()));
        }
        if !vocab.contains(&k.as_str()) {
            let mut msg = format!(
                "{origin}: unknown token \"{k}\" under \"{field}\" - this format's {field} names are: {}",
                vocab.join(", ")
            );
            if other_vocab(field).contains(&k.as_str()) {
                msg += &format!(" (\"{k}\" belongs under \"{other}\")");
            }
            return Err(msg);
        }
        out.insert(k.clone(), check(k, v)?);
    }
    Ok(out)
}

fn other_vocab(field: &str) -> &'static [&'static str] {
    if field == "colors" { GEOM_NAMES } else { COLOR_NAMES }
}

/// Overlay the parsed maps on the base table. The destructuring is the
/// M6.1 A exhaustiveness law, restated for files: a token added to
/// `Tokens` must be named here (pattern AND lookup) or the build
/// fails - the format can never be able to set less than the whole
/// table. Geometry lands clamped, so every reader (draw sites
/// included) sees shell-legal numbers.
fn apply_overlay(
    base: Tokens,
    colors: &HashMap<String, Color32>,
    geom: &HashMap<String, f64>,
) -> Tokens {
    let Tokens {
        ink, ink_alt, well, text, text_dim, text_faint, border, accent,
        accent_hover, field_bg, field_hover, field_active, selection_bg,
        selection_stroke, radius_widget, radius_window, radius_menu,
        item_spacing_x, item_spacing_y, button_padding_x, button_padding_y,
        control_height, row_height, cell_h, cell_w, glyph_px, font_label,
        font_caption, font_row, font_row_suffix,
    } = base;
    let c = |name: &str, keep: Color32| colors.get(name).copied().unwrap_or(keep);
    let f = |name: &str, keep: f32, win: (f32, f32)| {
        geom.get(name).map(|v| clamp::range(*v as f32, win)).unwrap_or(keep)
    };
    let rad = |name: &str, keep: u8, cap: f32| {
        geom.get(name)
            .map(|v| clamp::range(*v as f32, (0.0, cap)).round() as u8)
            .unwrap_or(keep)
    };
    Tokens {
        ink: c(names::INK, ink),
        ink_alt: c(names::INK_ALT, ink_alt),
        well: c(names::WELL, well),
        text: c(names::TEXT, text),
        text_dim: c(names::TEXT_DIM, text_dim),
        text_faint: c(names::TEXT_FAINT, text_faint),
        border: c(names::BORDER, border),
        accent: c(names::ACCENT, accent),
        accent_hover: c(names::ACCENT_HOVER, accent_hover),
        field_bg: c(names::FIELD_BG, field_bg),
        field_hover: c(names::FIELD_HOVER, field_hover),
        field_active: c(names::FIELD_ACTIVE, field_active),
        selection_bg: c(names::SELECTION_BG, selection_bg),
        selection_stroke: c(names::SELECTION_STROKE, selection_stroke),
        radius_widget: rad(names::RADIUS_WIDGET, radius_widget, clamp::RADIUS as f32),
        radius_window: rad(names::RADIUS_WINDOW, radius_window, 24.0),
        radius_menu: rad(names::RADIUS_MENU, radius_menu, 24.0),
        item_spacing_x: f(names::ITEM_SPACING_X, item_spacing_x, clamp::SPACING),
        item_spacing_y: f(names::ITEM_SPACING_Y, item_spacing_y, clamp::SPACING),
        button_padding_x: f(names::BUTTON_PADDING_X, button_padding_x, clamp::SPACING),
        button_padding_y: f(names::BUTTON_PADDING_Y, button_padding_y, clamp::SPACING),
        control_height: f(names::CONTROL_HEIGHT, control_height, clamp::CONTROL_H),
        row_height: f(names::ROW_HEIGHT, row_height, clamp::CONTROL_H),
        cell_h: f(names::CELL_H, cell_h, clamp::CELL),
        cell_w: f(names::CELL_W, cell_w, clamp::CELL),
        glyph_px: f(names::GLYPH_PX, glyph_px, clamp::GLYPH),
        font_label: f(names::FONT_LABEL, font_label, clamp::FONT),
        font_caption: f(names::FONT_CAPTION, font_caption, clamp::FONT),
        font_row: f(names::FONT_ROW, font_row, clamp::FONT),
        font_row_suffix: f(names::FONT_ROW_SUFFIX, font_row_suffix, clamp::FONT),
    }
}

fn req_str(
    obj: &serde_json::Map<String, Value>,
    key: &str,
    max: usize,
    origin: &str,
) -> Result<String, String> {
    match obj.get(key) {
        Some(Value::String(s)) if s.len() <= max => Ok(s.clone()),
        Some(Value::String(s)) => Err(format!(
            "{origin}: descriptor.{key} is {} bytes, past the {max}-byte cap",
            s.len()
        )),
        Some(_) => Err(format!("{origin}: descriptor.{key} must be a string")),
        None => Err(format!("{origin}: descriptor.{key} is required")),
    }
}

fn opt_str(
    obj: &serde_json::Map<String, Value>,
    key: &str,
    max: usize,
    origin: &str,
) -> Result<String, String> {
    match obj.get(key) {
        None => Ok(String::new()),
        Some(_) => req_str(obj, key, max, origin),
    }
}

/// `#rrggbb` / `#rrggbbaa`, lower or upper case - and nothing else:
/// egui's own `from_hex` also takes 3/4-digit shorthand and no `#`,
/// and a theme file spells its colours one way (refusals name the
/// token, so a typo is a sentence, not a wrong paint).
fn parse_hex(s: &str) -> Option<Color32> {
    let digits = s.strip_prefix('#')?;
    if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let pair = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
    match digits.len() {
        6 => Some(Color32::from_rgb(pair(0)?, pair(2)?, pair(4)?)),
        8 => Some(Color32::from_rgba_unmultiplied(pair(0)?, pair(2)?, pair(4)?, pair(6)?)),
        _ => None,
    }
}
