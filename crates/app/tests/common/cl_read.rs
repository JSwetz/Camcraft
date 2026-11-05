//! Reading a committed golden CL dump back into moves (M7.3): the
//! heavy-lane engine suites replay the goldens' own bytes, so what
//! the two stock engines judge is exactly what the generators were
//! pinned to emit. The format is ours (`gen_to_cltext`, the
//! tests/golden/CLAUDE.md grammar); `; CYCLE` lines are posting
//! intent and carry no motion, so they are skipped. A posed move
//! (`A`/`B`/`C` words) is refused: the goldens replayed here are
//! prismatic, and a pose read wrongly would be a silent lie.

use camcraft_app::cl::{ArcMove, Move, MoveKind};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::Point3;

use super::CL_SCALE;

/// The committed golden's text, by file name.
pub fn golden_text(name: &str) -> Result<String, String> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(name);
    std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Parse a CL dump into moves, `from` chained from START (or from the
/// first move's own destination when START is bare).
pub fn parse_cl(text: &str) -> Result<Vec<Move>, String> {
    let mm = |w: &str| -> Result<f64, String> {
        w.parse::<i64>()
            .map(|v| v as f64 * CL_SCALE)
            .map_err(|e| format!("bad number {w:?}: {e}"))
    };
    let mut at: Option<Point3> = None;
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let w: Vec<&str> = line.split_whitespace().collect();
        if w[0] == "START" {
            if w.len() >= 4 {
                at = Some(Point3::new(mm(w[1])?, mm(w[2])?, mm(w[3])?));
            }
            continue;
        }
        if w.len() < 4 {
            return Err(format!("line {}: too few words: {line}", n + 1));
        }
        let to = Point3::new(mm(w[1])?, mm(w[2])?, mm(w[3])?);
        let kind = match w[0] {
            "Rapid" => MoveKind::Rapid,
            "Feed" | "Arc" => MoveKind::Feed,
            "Plunge" => MoveKind::Plunge,
            k => return Err(format!("line {}: unknown move kind {k}", n + 1)),
        };
        let (mut feed, mut ci, mut cj, mut sense) = (None, None, None, None);
        for t in &w[4..] {
            let (key, val) = t.split_at(1);
            match key {
                "F" => feed = Some(mm(val)?), // x1000 mm/min: the same scale
                "I" => ci = Some(mm(val)?),
                "J" => cj = Some(mm(val)?),
                "S" => sense = Some(val == "1"),
                "A" | "B" | "C" => {
                    return Err(format!(
                        "line {}: a posed move - this reader replays prismatic goldens only",
                        n + 1
                    ))
                }
                _ => return Err(format!("line {}: unknown word {t}", n + 1)),
            }
        }
        let arc = if w[0] == "Arc" {
            match (ci, cj, sense) {
                (Some(x), Some(y), Some(ccw)) => Some(ArcMove { center: Point2::new(x, y), ccw }),
                _ => return Err(format!("line {}: an Arc without I/J/S", n + 1)),
            }
        } else {
            None
        };
        let from = at.unwrap_or(to);
        out.push(Move { kind, from, to, feed, arc, axis: None });
        at = Some(to);
    }
    Ok(out)
}
