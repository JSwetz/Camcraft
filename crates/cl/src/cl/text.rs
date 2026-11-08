//! The CL text dump (`; HSMW-CL vN` files, the `tests/golden/*.cl`
//! format): a reader and a writer over [`Toolpath`], so the headless
//! binaries (M13.3) can take a CLFile in and put one out without the
//! app. Provenance: our own line grammar (docs/CNC-INTERMEDIATE.md and
//! the golden writer in crates/app/tests/common), RS-274 move words
//! and arc-center letters; microns for lengths, millidegrees for pose
//! letters, mm/min x1000 for feeds.

use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::cycle::{Cycle, CycleKind};
use super::moves::{Move, MoveKind};
use super::path::Toolpath;
use super::plane::ArcPlane;
use super::pose::{angles_to_axis, axis_to_angles, is_vertical};

/// The dump's fixed-point unit: one micron, in millimetres.
pub const TEXT_SCALE: f64 = 0.001;

/// A line the reader could not take, by 1-based line number.
#[derive(Clone, Debug, PartialEq)]
pub struct TextError {
    pub line: usize,
    pub why: String,
}

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.why)
    }
}

impl std::error::Error for TextError {}

const CYCLE_KINDS: [(&str, CycleKind); 7] = [
    ("Spot", CycleKind::Spot),
    ("Drill", CycleKind::Drill),
    ("Peck", CycleKind::Peck),
    ("Deep", CycleKind::Deep),
    ("Tap", CycleKind::Tap),
    ("Bore", CycleKind::Bore),
    ("BackBore", CycleKind::BackBore),
];

fn q(mm: f64) -> i64 {
    (mm / TEXT_SCALE).round() as i64
}

/// Write `t` in the dump grammar - byte-for-byte the golden writer's
/// output. A non-finite coordinate is refused (it has no micron).
pub fn to_text(t: &Toolpath) -> Result<String, String> {
    if !t.all_finite() {
        return Err("path carries a non-finite coordinate".into());
    }
    let version = if !t.is_prismatic() {
        4
    } else if !t.cycles.is_empty() {
        3
    } else if t.has_arcs() {
        2
    } else {
        1
    };
    let mut s = format!("; HSMW-CL v{version} scale={TEXT_SCALE}mm moves={}\n", t.len());
    match t.start() {
        Some(p) => s.push_str(&format!("START {} {} {}\n", q(p.x), q(p.y), q(p.z))),
        None => s.push_str("START\n"),
    }
    for m in &t.moves {
        let mut k = match m.kind {
            MoveKind::Rapid => "Rapid",
            MoveKind::Feed => "Feed",
            MoveKind::Plunge => "Plunge",
        };
        let mut arc = String::new();
        if let Some(a) = m.arc {
            k = "Arc";
            let (l0, l1) = a.plane.center_letters();
            let sense = if a.ccw { 1 } else { -1 };
            arc = format!(" {l0}{} {l1}{} S{sense}", q(a.center.x), q(a.center.y));
        }
        s.push_str(&format!("{k} {} {} {}", q(m.to.x), q(m.to.y), q(m.to.z)));
        if let Some(f) = m.feed {
            s.push_str(&format!(" F{}", (f * 1000.0).round() as i64));
        }
        s.push_str(&arc);
        if let Some(a) = m.axis.filter(|a| !is_vertical(a)) {
            let [da, db, dc] = axis_to_angles(a);
            let md = |d: f64| (d * 1000.0).round() as i64;
            s.push_str(&format!(" A{} B{} C{}", md(da), md(db), md(dc)));
        }
        s.push('\n');
    }
    for c in &t.cycles {
        let kind = CYCLE_KINDS.iter().find(|(_, k)| *k == c.kind).map_or("Drill", |(w, _)| *w);
        s.push_str(&format!(
            "; CYCLE {kind} X{} Y{} RZ{} BZ{} ENTRY{} PECK{} DWELL{} AT{}\n",
            q(c.xy.x),
            q(c.xy.y),
            q(c.r_z),
            q(c.bottom_z),
            q(c.entry_z),
            q(c.peck),
            (c.dwell * 1000.0).round() as i64,
            c.first_move,
        ));
    }
    Ok(s)
}

/// An integer word in microns (or thousandths), as millimetres.
fn num(w: &str) -> Result<f64, String> {
    match w.parse::<i64>() {
        Ok(n) => Ok(n as f64 * TEXT_SCALE),
        Err(_) => Err(format!("not an integer: {w:?}")),
    }
}

fn cycle(words: &str, moves: usize) -> Result<Cycle, String> {
    let mut it = words.split_whitespace();
    let Some(kw) = it.next() else {
        return Err("CYCLE needs a kind".into());
    };
    let Some(&(_, kind)) = CYCLE_KINDS.iter().find(|(w, _)| *w == kw) else {
        return Err(format!("unknown cycle kind {kw:?}"));
    };
    let mut c = Cycle {
        kind,
        xy: Point2::new(0.0, 0.0),
        r_z: 0.0,
        bottom_z: 0.0,
        entry_z: 0.0,
        peck: 0.0,
        dwell: 0.0,
        first_move: 0,
    };
    for w in it {
        let split = w.find(|ch: char| ch == '-' || ch.is_ascii_digit()).unwrap_or(w.len());
        let (key, val) = w.split_at(split);
        let v = num(val)?;
        match key {
            "X" => c.xy.x = v,
            "Y" => c.xy.y = v,
            "RZ" => c.r_z = v,
            "BZ" => c.bottom_z = v,
            "ENTRY" => c.entry_z = v,
            "PECK" => c.peck = v,
            "DWELL" => c.dwell = v,
            "AT" => {
                let at = val.parse::<usize>().map_err(|_| format!("AT {val:?}"))?;
                if at > moves {
                    return Err(format!("AT{at} past the {moves} moves read so far"));
                }
                c.first_move = at;
            }
            k => return Err(format!("unknown cycle word {k:?}")),
        }
    }
    Ok(c)
}

/// Read a dump back into a path. Every coordinate must be an integer
/// word; an unknown token, a missing coordinate or an arc without its
/// center pair is an error naming the line - never a guess.
pub fn from_text(text: &str) -> Result<Toolpath, TextError> {
    let mut moves: Vec<Move> = Vec::new();
    let mut cycles = Vec::new();
    let mut at: Option<Point3> = None;
    for (i, raw) in text.lines().enumerate() {
        let fail = |why: String| TextError { line: i + 1, why };
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix(';') {
            if let Some(c) = rest.trim_start().strip_prefix("CYCLE") {
                cycles.push(cycle(c, moves.len()).map_err(fail)?);
            }
            continue;
        }
        let mut it = line.split_whitespace();
        let head = it.next().unwrap_or("");
        let words: Vec<&str> = it.collect();
        let xyz = |w: &[&str]| -> Result<Point3, String> {
            if w.len() < 3 {
                return Err(format!("{head} needs X Y Z"));
            }
            Ok(Point3::new(num(w[0])?, num(w[1])?, num(w[2])?))
        };
        if head == "START" {
            at = if words.is_empty() { None } else { Some(xyz(&words).map_err(fail)?) };
            continue;
        }
        let kind = match head {
            "Rapid" => MoveKind::Rapid,
            "Feed" | "Arc" => MoveKind::Feed,
            "Plunge" => MoveKind::Plunge,
            other => return Err(fail(format!("unknown record {other:?}"))),
        };
        let to = xyz(&words).map_err(fail)?;
        let Some(from) = at else {
            return Err(fail("a move before any START position".into()));
        };
        let (mut feed, mut centre, mut sense, mut ang) = (None, Vec::new(), None, [None; 3]);
        for w in words.iter().skip(3) {
            let (k, v) = w.split_at(w.chars().next().map_or(0, char::len_utf8));
            let k = k.chars().next().unwrap_or(' ');
            let n = num(v).map_err(fail)?;
            match k {
                'F' => feed = Some(n),
                'I' | 'J' | 'K' => centre.push((k, n)),
                'S' => sense = Some(n > 0.0),
                'A' => ang[0] = Some(n),
                'B' => ang[1] = Some(n),
                'C' => ang[2] = Some(n),
                _ => return Err(fail(format!("unknown word {w:?}"))),
            }
        }
        let mut m = Move::new(kind, from, to, feed);
        if head == "Arc" {
            let ([(a, u), (b, v)], Some(ccw)) = (centre.as_slice(), sense) else {
                return Err(fail("Arc needs two center words and S".into()));
            };
            let Some(plane) = ArcPlane::from_letters(*a, *b) else {
                return Err(fail(format!("center words {a}{b} name no plane")));
            };
            m = m.with_arc_in(Point2::new(*u, *v), ccw, plane);
        } else if !centre.is_empty() || sense.is_some() {
            return Err(fail(format!("{head} carries arc words")));
        }
        if let [Some(a), Some(b), Some(c)] = ang {
            m = m.attach_axis(angles_to_axis([a, b, c]));
        } else if ang.iter().any(Option::is_some) {
            return Err(fail("a pose needs all of A, B and C".into()));
        }
        moves.push(m);
        at = Some(to);
    }
    let mut t = Toolpath::new(moves);
    t.cycles = cycles;
    Ok(t)
}
