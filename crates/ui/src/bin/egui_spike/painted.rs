//! S2: the draw-list port - ribbon cells and browser rows as Shape/Painter
//! custom paint, not stock widgets.
//!
//! In the imgui shell these strips are one `get_window_draw_list()` call
//! per mark (ribbon.rs's `cell`, browser.rs's row loop): background rects,
//! glyphs and captions added as ink over invisible buttons. egui's answer
//! is the same division of labor - `Painter` + `Shape` for the ink,
//! `ui.interact` for the input - and this module is that port: what is
//! painted could not be done with a widget table, while every click is
//! still a real egui response with real hover/active state.
//!
//! The shapes are original marks (rectangle outlines, triangles, a
//! two-stroke eye), drawn from the geometry of what they name - not
//! adapted from any icon set, per AGENTS.md. Colors are the shared
//! `Palette` tokens, so the painted strips and the widget panes cannot
//! drift apart on a retheme.

use egui::{Color32, FontId, Pos2, Rect, Response, Shape, Sense, Stroke, Ui, Vec2, Align2};

use super::theme::Palette;

/// One painted ribbon cell: fixed-size, text caption centered, tinted
/// background by state, dimmed ink when disabled (the imgui rule: a
/// disabled cell stays hoverable and greys, it never vanishes).
pub fn cell(
    ui: &mut Ui,
    id: egui::Id,
    rect: Rect,
    label: &str,
    pal: &Palette,
    enabled: bool,
    toggled: bool,
) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let bg = if !enabled {
        None
    } else if resp.clicked() || resp.is_pointer_button_down_on() {
        Some(pal.accent)
    } else if toggled {
        Some(pal.selection_bg)
    } else if resp.hovered() {
        Some(pal.field_hover)
    } else {
        None
    };
    let painter = ui.painter();
    if let Some(bg) = bg {
        painter.add(Shape::rect_filled(rect, 2.0, bg));
    }
    let ink = if enabled { pal.text } else { pal.text_faint };
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.5),
        ink,
    );
    resp
}

/// A group divider: one vertical hairline, ink only.
pub fn divider(painter: &egui::Painter, x: f32, rect: Rect, pal: &Palette) {
    painter.line_segment(
        [Pos2::new(x, rect.top() + 4.0), Pos2::new(x, rect.bottom() - 4.0)],
        Stroke::new(1.0, pal.border),
    );
}

/// A group caption: small dim text centered under its cells' row.
pub fn caption(painter: &egui::Painter, center_x: f32, y: f32, title: &str, pal: &Palette) {
    painter.text(Pos2::new(center_x, y), Align2::CENTER_TOP, title, FontId::proportional(10.0), pal.text_dim);
}

/// One browser row: what it says, what it is, how it must read.
pub struct Row {
    pub label: String,
    /// Faint tail text after the label - the browser's suffix column
    /// (tool sizes, op kind/status), mirrored from browser.rs.
    pub suffix: String,
    pub kind: Kind,
    /// Selection band - the WORLD's selection mirrored, never a local flag.
    pub selected: bool,
    pub hidden: bool,
    /// Operation extras: cache present (green dot), STALE badge.
    pub has_path: bool,
    pub stale: bool,
    /// Indent level (sections 0, children 1).
    pub depth: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Section,
    Model,
    Stock,
    Wcs,
    Tool,
    Op,
}

pub const ROW_H: f32 = 20.0;

/// Hit result for one frame of the tree.
pub struct TreeHit {
    pub row: Option<usize>,
    pub eye: Option<usize>,
}

/// Paint the rows and collect this frame's clicks. Input first (real
/// `ui.interact` responses), then the ink - so hover state and painted
/// background cannot disagree within a frame.
pub fn tree(ui: &mut Ui, rect: Rect, rows: &[Row], pal: &Palette) -> TreeHit {
    let mut hit = TreeHit { row: None, eye: None };
    for (ix, row) in rows.iter().enumerate() {
        let top = rect.top() + ix as f32 * ROW_H;
        if top >= rect.bottom() {
            break; // scrolled past the pane: no row, no input, no ink
        }
        let rr = Rect::from_min_size(rect.left_top() + Vec2::new(0.0, ix as f32 * ROW_H), Vec2::new(rect.width(), ROW_H));
        let row_resp = ui.interact(rr, ui.id().with(("tree-row", ix)), Sense::click());
        let eye_rect = if matches!(row.kind, Kind::Model | Kind::Op) {
            let er = Rect::from_center_size(
                Pos2::new(rr.right() - 12.0, rr.center().y),
                Vec2::new(18.0, 16.0),
            );
            if ui.interact(er, ui.id().with(("tree-eye", ix)), Sense::click()).clicked() {
                hit.eye = Some(ix);
            }
            Some(er)
        } else {
            None
        };
        let painter = ui.painter();
        if row.selected {
            painter.add(Shape::rect_filled(rr.shrink(1.0), 2.0, pal.selection_bg));
        } else if row_resp.hovered() {
            painter.add(Shape::rect_filled(rr.shrink(1.0), 2.0, pal.field_hover));
        }
        let gx = rr.left() + 10.0 + row.depth as f32 * 14.0;
        let mut shapes = Vec::new();
        glyph(&mut shapes, Pos2::new(gx, rr.center().y), row.kind, pal);
        if row.kind == Kind::Op && row.has_path {
            shapes.push(Shape::circle_filled(
                Pos2::new(rr.right() - 40.0, rr.center().y),
                3.0,
                Color32::from_rgb(0x3C, 0xC8, 0x64),
            ));
        }
        if let Some(er) = eye_rect {
            eye_glyph(&mut shapes, er, row.hidden, pal);
        }
        painter.add(Shape::Vec(shapes));
        let ink = if row.hidden { pal.text_faint } else { pal.text };
        let lrect = painter.text(
            Pos2::new(gx + 12.0, rr.center().y),
            Align2::LEFT_CENTER,
            &row.label,
            FontId::proportional(12.5),
            ink,
        );
        if !row.suffix.is_empty() {
            painter.text(
                Pos2::new(lrect.right() + 8.0, rr.center().y),
                Align2::LEFT_CENTER,
                &row.suffix,
                FontId::proportional(10.5),
                if row.hidden { pal.text_faint } else { pal.text_dim },
            );
        }
        if row.stale {
            painter.text(
                Pos2::new(rr.right() - 34.0, rr.center().y),
                Align2::RIGHT_CENTER,
                "STALE",
                FontId::proportional(9.5),
                Color32::from_rgb(0xC8, 0x96, 0x32),
            );
        }
        if row_resp.clicked() && hit.eye != Some(ix) {
            hit.row = Some(ix);
        }
    }
    hit
}

/// The ~9-px marks, per kind: original geometry, one stroke family.
fn glyph(shapes: &mut Vec<Shape>, c: Pos2, kind: Kind, pal: &Palette) {
    let s = Stroke::new(1.0, pal.text_dim);
    match kind {
        Kind::Section => {
            // Open folder: a body with a back-tab.
            let body = Rect::from_min_size(c + Vec2::new(-5.0, -3.0), Vec2::new(10.0, 7.0));
            shapes.push(Shape::rect_stroke(body, 1.0, s, egui::StrokeKind::Middle));
            shapes.push(Shape::rect_filled(
                Rect::from_min_size(body.left_top() + Vec2::new(1.0, -2.5), Vec2::new(4.0, 2.5)),
                0.0,
                pal.text_dim,
            ));
        }
        Kind::Model => {
            let r = Rect::from_center_size(c, Vec2::new(9.0, 7.0));
            shapes.push(Shape::rect_stroke(r, 0.0, s, egui::StrokeKind::Middle));
        }
        Kind::Stock => {
            let r = Rect::from_center_size(c, Vec2::new(9.0, 8.0));
            shapes.push(Shape::rect_stroke(r, 0.0, s, egui::StrokeKind::Middle));
            shapes.push(Shape::line_segment([r.left_top(), r.right_bottom()], s));
        }
        Kind::Wcs => {
            shapes.push(Shape::line_segment([c - Vec2::X * 4.5, c + Vec2::X * 4.5], s));
            shapes.push(Shape::line_segment([c - Vec2::Y * 4.5, c + Vec2::Y * 4.5], s));
        }
        Kind::Tool => {
            shapes.push(Shape::circle_stroke(c, 4.0, s));
        }
        Kind::Op => {
            // A play-triangle in a square frame: "this op owns a path",
            // drawn from what it names, original.
            let r = Rect::from_center_size(c, Vec2::new(9.0, 9.0));
            shapes.push(Shape::rect_stroke(r, 1.0, s, egui::StrokeKind::Middle));
            shapes.push(Shape::convex_polygon(
                vec![
                    Pos2::new(c.x - 1.8, c.y - 3.0),
                    Pos2::new(c.x + 3.0, c.y),
                    Pos2::new(c.x - 1.8, c.y + 3.0),
                ],
                Color32::TRANSPARENT,
                Stroke::new(1.0, pal.text_dim),
            ));
        }
    }
}

/// The browser eye: an outlined lid-rect with a pupil, or crossed when
/// hidden (display state - the row click still selects, the eye never
/// destroys).
fn eye_glyph(shapes: &mut Vec<Shape>, r: Rect, hidden: bool, pal: &Palette) {
    let s = Stroke::new(1.0, if hidden { pal.text_faint } else { pal.text_dim });
    shapes.push(Shape::rect_stroke(r, 4.0, s, egui::StrokeKind::Middle));
    if hidden {
        shapes.push(Shape::line_segment([r.left_top(), r.right_bottom()], s));
        shapes.push(Shape::line_segment([r.left_bottom(), r.right_top()], s));
    } else {
        shapes.push(Shape::circle_filled(r.center(), 2.2, pal.text_dim));
    }
}

/// Ground and hairline under a painted strip (ribbon rows, the status
/// line): the caller sizes the panel, this makes it read as chrome.
pub fn strip_bg(painter: &egui::Painter, rect: Rect, pal: &Palette) {
    painter.rect_filled(rect, 0.0, pal.strip_fill());
    let y = rect.bottom() - 0.5;
    painter.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], Stroke::new(1.0, pal.border));
}
