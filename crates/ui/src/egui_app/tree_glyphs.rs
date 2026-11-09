//! The browser tree's painted marks (split from `painted`, #343): the
//! ~9-px per-kind glyphs - the no-GPU twins of the bundled PNGs - and the
//! eye. Original geometry.
//! Provenance: our own drawings; no vendor material consulted.

use egui::{Color32, Pos2, Rect, Shape, Stroke, Vec2};

use super::painted::Kind;
use super::theme::Tokens;

/// The ~9-px marks, per kind: original geometry, one stroke family.
pub(super) fn glyph(shapes: &mut Vec<Shape>, c: Pos2, kind: Kind, pal: &Tokens) {
    let s = Stroke::new(1.0, pal.text_dim);
    match kind {
        Kind::Parent(expanded) => {
            let points = if expanded {
                vec![c + Vec2::new(-4.0, -2.0), c + Vec2::new(4.0, -2.0), c + Vec2::new(0.0, 3.0)]
            } else {
                vec![c + Vec2::new(-2.0, -4.0), c + Vec2::new(3.0, 0.0), c + Vec2::new(-2.0, 4.0)]
            };
            shapes.push(Shape::convex_polygon(points, pal.text_dim, Stroke::NONE));
        }
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
        Kind::Fixture => {
            // The vise: two parallel jaws, the screw shaft running from
            // the movable one, a solid knob. The PNG twin's geometry at
            // tree scale, drawn from what it names - original.
            let jaw = Vec2::Y * 3.5;
            for dx in [-4.0, -0.5] {
                let p = c + Vec2::X * dx;
                shapes.push(Shape::line_segment([p - jaw, p + jaw], s));
            }
            shapes.push(Shape::line_segment(
                [c + Vec2::X * -0.5, c + Vec2::X * 2.8],
                s,
            ));
            shapes.push(Shape::circle_filled(c + Vec2::X * 4.0, 1.3, pal.text_dim));
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
        Kind::Setup(active) => {
            shapes.push(Shape::circle_stroke(c, 5.0, Stroke::new(1.5, pal.text_dim)));
            if active {
                shapes.push(Shape::circle_filled(c, 2.8, pal.accent));
            }
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
pub(super) fn eye_glyph(shapes: &mut Vec<Shape>, r: Rect, hidden: bool, pal: &Tokens) {
    let s = Stroke::new(1.0, if hidden { pal.text_faint } else { pal.text_dim });
    shapes.push(Shape::rect_stroke(r, 4.0, s, egui::StrokeKind::Middle));
    if hidden {
        shapes.push(Shape::line_segment([r.left_top(), r.right_bottom()], s));
        shapes.push(Shape::line_segment([r.left_bottom(), r.right_top()], s));
    } else {
        shapes.push(Shape::circle_filled(r.center(), 2.2, pal.text_dim));
    }
}
