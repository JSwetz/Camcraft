//! One cached path as overlay lines, with its warned moves marked
//! (#261): moves a verify warning names draw in the palette's gouge
//! colour instead of their kind colour, and each folded warning's first
//! move gets a small 3-axis cross in the same colour, so a warned plunge
//! (whose house red sits near the gouge red) still reads as warned.
//! Provenance: our own display choice over our own warning list; split
//! out of `interact::toolpath_lines` (the move loop, unchanged for a
//! path with no warnings). No vendor material consulted.

use camcraft_app::cl::MoveKind;
use camcraft_app::schema::Toolpath;
use camcraft_app::warned_moves::{move_focus, warned_mask};

use crate::egui_app::colorvision::Semantic;
use crate::interact::{MAX_OVERLAY_VERTICES, arc_segments, chevrons, push_line};
use crate::operations::fold_warnings_located;

/// The marker cross's half-arm, in chevron arms.
const MARK_ARMS: f64 = 1.5;

/// The colour one move draws in: the gouge colour when warned, else its
/// kind's - scaled by `dim` (unselected paths under "show all").
#[must_use]
pub(crate) fn move_colour(pal: &Semantic, kind: MoveKind, warned: bool, dim: f32) -> [f32; 3] {
    let base = if warned {
        pal.gouge
    } else {
        match kind {
            MoveKind::Rapid => pal.rapid,
            MoveKind::Feed => pal.feed,
            MoveKind::Plunge => pal.plunge,
        }
    };
    [base[0] * dim, base[1] * dim, base[2] * dim]
}

fn over_budget(out: &mut Vec<[f32; 6]>) {
    eprintln!(
        "overlay: the cached paths outgrew the {MAX_OVERLAY_VERTICES}-vertex \
         display budget; the rest is truncated (the full path still posts)"
    );
    out.truncate(out.len() & !1);
}

/// Append `cache`'s lines to `out`. Returns false once the vertex
/// budget is spent (the caller stops drawing further paths); the
/// truncation is logged, never silent.
pub(crate) fn cache_lines(out: &mut Vec<[f32; 6]>, cache: &Toolpath, pal: &Semantic, dim: f32, arm: f64) -> bool {
    let moves = &cache.path.moves;
    let warned = warned_mask(&cache.warnings, moves.len());
    for (i, m) in moves.iter().enumerate() {
        let c = move_colour(pal, m.kind, warned.get(i).copied().unwrap_or(false), dim);
        if m.arc.is_some() {
            // Arcs draw as chords (one law, one tolerance story) and
            // take no chevrons: the curve itself reads as direction.
            let cs = arc_segments(m);
            if out.len() + 2 * cs.len() + 2 > MAX_OVERLAY_VERTICES {
                over_budget(out);
                return false;
            }
            for (a, b) in cs {
                push_line(out, a, b, c);
            }
            continue;
        }
        if out.len() + 10 > MAX_OVERLAY_VERTICES {
            // A pathological cache (millions of file-loaded moves)
            // must not allocate millions of segments either.
            over_budget(out);
            return false;
        }
        push_line(out, m.from, m.to, c);
        if m.kind == MoveKind::Feed {
            chevrons(out, m.from, m.to, c, arm);
        }
    }
    if warned.is_empty() {
        return true;
    }
    let c = move_colour(pal, MoveKind::Feed, true, dim);
    let r = arm * MARK_ARMS;
    for at in fold_warnings_located(&cache.warnings).into_iter().filter_map(|(_, at)| at) {
        let Some(q) = move_focus(moves, at) else { continue };
        if out.len() + 6 > MAX_OVERLAY_VERTICES {
            over_budget(out);
            return false;
        }
        for axis in 0..3 {
            let (mut a, mut b) = (q, q);
            match axis {
                0 => (a.x, b.x) = (q.x - r, q.x + r),
                1 => (a.y, b.y) = (q.y - r, q.y + r),
                _ => (a.z, b.z) = (q.z - r, q.z + r),
            }
            push_line(out, a, b, c);
        }
    }
    true
}

/// Where each folded warning line of `cache` looks (#261), aligned with
/// [`crate::operations::fold_warnings`]: the midpoint of the group's
/// first move, `None` for a line that locates nothing or names a move
/// past the path's end.
#[must_use]
pub fn warning_focus(cache: &Toolpath) -> Vec<Option<[f64; 3]>> {
    fold_warnings_located(&cache.warnings)
        .into_iter()
        .map(|(_, at)| at.and_then(|i| move_focus(&cache.path.moves, i)).map(|q| [q.x, q.y, q.z]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use camcraft_app::cl::{Move, Toolpath as ClPath};
    use camcraft_app::geometry::Point3;
    use camcraft_app::verify::Warning;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    /// Rapid in, plunge, two feeds, rapid out: five moves.
    fn path() -> ClPath {
        let moves = vec![
            Move::new(MoveKind::Rapid, p(0.0, 0.0, 10.0), p(0.0, 0.0, 2.0), None),
            Move::new(MoveKind::Plunge, p(0.0, 0.0, 2.0), p(0.0, 0.0, -1.0), Some(100.0)),
            Move::new(MoveKind::Feed, p(0.0, 0.0, -1.0), p(30.0, 0.0, -1.0), Some(500.0)),
            Move::new(MoveKind::Feed, p(30.0, 0.0, -1.0), p(30.0, 30.0, -1.0), Some(500.0)),
            Move::new(MoveKind::Rapid, p(30.0, 30.0, -1.0), p(30.0, 30.0, 10.0), None),
        ];
        ClPath { moves, ..ClPath::default() }
    }

    /// The colour of the line whose first vertex sits at `a`.
    fn colours_at(out: &[[f32; 6]], a: Point3) -> Vec<[f32; 3]> {
        out.as_chunks::<2>().0.iter()
            .filter(|l| l[0][0] == a.x as f32 && l[0][1] == a.y as f32 && l[0][2] == a.z as f32)
            .map(|l| [l[0][3], l[0][4], l[0][5]])
            .collect()
    }

    #[test]
    fn a_clean_path_draws_exactly_as_before() {
        let pal = &Semantic::OFF;
        let mut out = Vec::new();
        assert!(cache_lines(&mut out, &Toolpath::new(path(), Vec::new()), pal, 1.0, 0.6));
        assert!(!out.iter().any(|v| v[3..6] == pal.gouge), "no gouge colour on a clean path");
        assert_eq!(colours_at(&out, p(30.0, 0.0, -1.0)).first(), Some(&pal.feed));
    }

    #[test]
    fn warned_moves_wear_the_gouge_colour_and_the_first_gets_a_cross() {
        let pal = &Semantic::OFF;
        // Move 3 is shank-hit; 99 (past the end) must be ignored, not indexed.
        let w = vec![Warning::ShankHit { at: 3, part: false }, Warning::Gouge { at: 99 }];
        let mut out = Vec::new();
        assert!(cache_lines(&mut out, &Toolpath::new(path(), w), pal, 1.0, 0.6));
        assert_eq!(colours_at(&out, p(30.0, 0.0, -1.0)).first(), Some(&pal.gouge), "move 3 is warned");
        assert_eq!(colours_at(&out, p(0.0, 0.0, -1.0)).first(), Some(&pal.feed), "move 2 is not");
        assert_eq!(colours_at(&out, p(0.0, 0.0, 2.0)).first(), Some(&pal.plunge));
        // The cross at move 3's midpoint (30, 15, -1): its X arm starts
        // 0.9 left of it (1.5 chevron arms of 0.6).
        let x_arm = colours_at(&out, p(30.0 - 0.9, 15.0, -1.0));
        assert_eq!(x_arm, vec![pal.gouge], "one cross, in the gouge colour");
    }

    #[test]
    fn each_folded_line_frames_its_first_moves_midpoint() {
        let w = vec![
            Warning::ShankHit { at: 3, part: false },
            Warning::ShankHit { at: 4, part: false },
            Warning::RapidInStock { at: 0 },
            Warning::Gouge { at: 99 },
            Warning::RestUnjudged { at: 2, cells: 1, claims: 1 },
        ];
        let f = warning_focus(&Toolpath::new(path(), w));
        assert_eq!(f, vec![Some([30.0, 15.0, -1.0]), Some([0.0, 0.0, 6.0]), None, None]);
    }

    #[test]
    fn the_mark_follows_the_colour_vision_palette_and_the_dim() {
        let pal = &Semantic::RED_GREEN;
        let w = vec![Warning::Gouge { at: 2 }];
        let mut out = Vec::new();
        assert!(cache_lines(&mut out, &Toolpath::new(path(), w), pal, 0.5, 0.6));
        let g = pal.gouge;
        assert_eq!(colours_at(&out, p(0.0, 0.0, -1.0)).first(), Some(&[g[0] * 0.5, g[1] * 0.5, g[2] * 0.5]));
    }
}
