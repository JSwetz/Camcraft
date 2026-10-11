//! STL reader: binary and ASCII, sniffed per public-format rules.
//!
//! Binary layout (public): 80-byte header, u32 LE facet count, then
//! count x [12 x f32 LE (normal + 3 vertices) + u16 attribute].
//! ASCII layout: `solid`/`facet normal`/`outer loop`/`vertex`/`endloop`/
//! `endfacet`/`endsolid` token lines.

use super::{ImportError, MAX_FACETS};
use crate::geometry::{Mesh, Point3, Triangle};

const HEADER_BYTES: usize = 80;
const COUNT_BYTES: usize = 4;
const FACET_BYTES: usize = 50;

pub fn parse(data: &[u8]) -> Result<Mesh, ImportError> {
    if data.is_empty() {
        return Err(ImportError::Empty);
    }
    if looks_binary(data) {
        parse_binary(data)
    } else {
        parse_ascii(data)
    }
}

/// A leading `solid` is ambiguous (binary headers can carry it too): binary
/// is chosen only when the declared count matches the byte size exactly.
/// Anything without the `solid` prefix is binary - the parser then reports
/// precise size errors rather than the ASCII parser reporting garbage.
fn looks_binary(data: &[u8]) -> bool {
    if !data.starts_with(b"solid") {
        return true;
    }
    match read_count(data) {
        Some(count) => match (count as usize).checked_mul(FACET_BYTES) {
            Some(body) => data.len() == HEADER_BYTES + COUNT_BYTES + body,
            None => false,
        },
        None => false,
    }
}

fn read_count(data: &[u8]) -> Option<u32> {
    if data.len() < HEADER_BYTES + COUNT_BYTES {
        return None;
    }
    let b = &data[HEADER_BYTES..HEADER_BYTES + COUNT_BYTES];
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn f32_at(data: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

fn parse_binary(data: &[u8]) -> Result<Mesh, ImportError> {
    let count = match read_count(data) {
        Some(c) => c,
        None => return Err(ImportError::Stl("truncated header".into())),
    };
    if count as usize > MAX_FACETS {
        return Err(ImportError::TooLarge);
    }
    let count = count as usize;
    let body = match count.checked_mul(FACET_BYTES) {
        Some(b) => b,
        None => return Err(ImportError::TooLarge),
    };
    let expect = HEADER_BYTES + COUNT_BYTES + body;
    if data.len() != expect {
        return Err(ImportError::Stl(format!(
            "size {} does not match declared {count} facets (expected {expect} bytes)",
            data.len()
        )));
    }
    if count == 0 {
        return Err(ImportError::Empty);
    }

    let mut facets = Vec::with_capacity(count);
    for i in 0..count {
        let base = HEADER_BYTES + COUNT_BYTES + i * FACET_BYTES;
        let mut p = [0.0f32; 12];
        for (slot, chunk) in p.iter_mut().zip(0..12) {
            *slot = f32_at(data, base + chunk * 4);
        }
        // normal may be zero (writers often skip it) but must not be NaN
        if !p[..3].iter().all(|v| v.is_finite()) {
            return Err(ImportError::Stl(format!("facet {i}: non-finite normal")));
        }
        let a = Point3::new(p[3] as f64, p[4] as f64, p[5] as f64);
        let b = Point3::new(p[6] as f64, p[7] as f64, p[8] as f64);
        let c = Point3::new(p[9] as f64, p[10] as f64, p[11] as f64);
        if !(a.is_finite() && b.is_finite() && c.is_finite()) {
            return Err(ImportError::Stl(format!("facet {i}: non-finite vertex")));
        }
        let t = Triangle::new(a, b, c);
        if t.normal().x == 0.0 && t.normal().y == 0.0 && t.normal().z == 0.0 {
            return Err(ImportError::Stl(format!("facet {i} is degenerate")));
        }
        facets.push(t);
    }
    Ok(Mesh::from_facets(facets))
}

fn parse_ascii(data: &[u8]) -> Result<Mesh, ImportError> {
    let text = std::str::from_utf8(data)
        .map_err(|_| ImportError::Stl("not valid UTF-8".into()))?;

    let mut facets: Vec<Triangle> = Vec::new();
    let mut verts: [Option<Point3>; 3] = [None, None, None];
    let mut vi = 0usize; // number of vertices collected in current facet
    let mut state = State::Start;
    let mut declared_count = 0usize;

    for (ln, raw) in text.lines().enumerate() {
        let line = ln + 1;
        let mut tok = raw.split_whitespace();
        match tok.next() {
            None if matches!(state, State::Start) => continue,
            None => continue,
            Some(word) => match word {
                "solid" => {
                    if !matches!(state, State::Start) {
                        return Err(ImportError::Stl(format!("line {line}: nested solid")));
                    }
                    state = State::InSolid;
                }
                "facet" | "outer" | "endloop" | "endfacet" | "vertex"
                    if matches!(state, State::Start) =>
                {
                    return Err(ImportError::Stl(format!(
                        "line {line}: '{word}' before 'solid'"
                    )));
                }
                "facet" => {
                    expect_next(&mut tok, "normal", line)?;
                    read_f32_triple(&mut tok, line, "normal")?;
                    state = State::InFacet;
                    verts = [None, None, None];
                    vi = 0;
                }
                "outer" => {
                    expect_next(&mut tok, "loop", line)?;
                }
                "vertex" if matches!(state, State::InFacet) => {
                    let p = read_f64_triple(&mut tok, line, "vertex")?;
                    if vi < 3 {
                        verts[vi] = Some(p);
                        vi += 1;
                    } else {
                        return Err(ImportError::Stl(format!("line {line}: >3 vertices")));
                    }
                }
                "endloop" => {}
                "endfacet" => {
                    if !matches!(state, State::InFacet) {
                        return Err(ImportError::Stl(format!("line {line}: stray endfacet")));
                    }
                    let (Some(a), Some(b), Some(c)) = (verts[0], verts[1], verts[2]) else {
                        return Err(ImportError::Stl(format!(
                            "line {line}: facet has {vi} vertices"
                        )));
                    };
                    let t = Triangle::new(a, b, c);
                    let n = t.normal();
                    if n.x == 0.0 && n.y == 0.0 && n.z == 0.0 {
                        return Err(ImportError::Stl(format!(
                            "line {line}: facet is degenerate"
                        )));
                    }
                    facets.push(t);
                    declared_count += 1;
                    if declared_count > MAX_FACETS {
                        return Err(ImportError::TooLarge);
                    }
                    state = State::InSolid;
                }
                "endsolid" => {
                    if declared_count == 0 {
                        return Err(ImportError::Empty);
                    }
                    break;
                }
                other => {
                    return Err(ImportError::Stl(format!(
                        "line {line}: unknown token '{other}'"
                    )));
                }
            },
        }
    }
    // A missing `endsolid` on a complete solid is accepted (common writer
    // quirk); zero facets is not.
    if declared_count == 0 {
        return Err(ImportError::Empty);
    }
    Ok(Mesh::from_facets(facets))
}

enum State {
    Start,
    InSolid,
    InFacet,
}

fn expect_next<'a>(
    tok: &mut impl Iterator<Item = &'a str>,
    want: &str,
    line: usize,
) -> Result<(), ImportError> {
    match tok.next() {
        Some(w) if w == want => Ok(()),
        _ => Err(ImportError::Stl(format!("line {line}: expected '{want}'"))),
    }
}

fn read_f32_triple<'a>(
    tok: &mut impl Iterator<Item = &'a str>,
    line: usize,
    what: &str,
) -> Result<[f32; 3], ImportError> {
    let vals = read_float_triple::<f32>(tok, line, what)?;
    if !vals.iter().all(|v| v.is_finite()) {
        return Err(ImportError::Stl(format!(
            "line {line}: non-finite {what} value"
        )));
    }
    Ok(vals)
}

fn read_f64_triple<'a>(
    tok: &mut impl Iterator<Item = &'a str>,
    line: usize,
    what: &str,
) -> Result<Point3, ImportError> {
    let t = read_float_triple::<f64>(tok, line, what)?;
    let p = Point3::new(t[0], t[1], t[2]);
    if !p.is_finite() {
        return Err(ImportError::Stl(format!("line {line}: non-finite {what}")));
    }
    Ok(p)
}

/// Reads exactly 3 whitespace-separated floats from the caller's token
/// iterator.
fn read_float_triple<'a, T>(
    tok: &mut impl Iterator<Item = &'a str>,
    line: usize,
    what: &str,
) -> Result<[T; 3], ImportError>
where
    T: std::str::FromStr,
{
    Ok([
        parse_one(tok, line, what)?,
        parse_one(tok, line, what)?,
        parse_one(tok, line, what)?,
    ])
}

fn parse_one<'a, T>(
    tok: &mut impl Iterator<Item = &'a str>,
    line: usize,
    what: &str,
) -> Result<T, ImportError>
where
    T: std::str::FromStr,
{
    let word = tok
        .next()
        .ok_or_else(|| ImportError::Stl(format!("line {line}: expected a {what} value")))?;
    word.parse::<T>()
        .map_err(|_| ImportError::Stl(format!("line {line}: bad {what} value '{word}'")))
}
