//! `list_features` (#180): a model's pickable features - faces with their
//! boundary loops, tangent edge chains, welded smooth faces and corner
//! vertices - each under the stable geometry-hash id the pick tools take.
//!
//! Provenance: our own read-only view over `geometry::feature`'s topology
//! (this repo's M1.5 / M13.4 design); no vendor material consulted.

use super::pick::{feature_id, req_model};
use super::tools::req_str;
use crate::geometry::feature::{self, FaceKind, Features, LoopKind};
use crate::geometry::{transform_point, Point3};
use crate::model;
use crate::operation::FeatureSel;
use crate::schema::Location;
use crate::script::{fnum, Env};

use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// Rows per feature kind unless `limit` says otherwise: enough for any
/// machined part an agent reasons about, small enough for one reply.
const DEFAULT_LIMIT: usize = 500;
/// The ceiling `limit` may ask for.
const MAX_LIMIT: usize = 20_000;

const KINDS: [&str; 4] = ["faces", "chains", "patches", "vertices"];

fn world_pt(loc: &Location, p: [f64; 3]) -> Point3 {
    transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot)
}

fn pt(p: Point3) -> Value {
    json!([fnum(p.x), fnum(p.y), fnum(p.z)])
}

/// World bounds of local points; `null` for none.
fn bounds(loc: &Location, pts: impl IntoIterator<Item = [f64; 3]>) -> Value {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut any = false;
    for p in pts {
        let w = world_pt(loc, p);
        for (i, v) in [w.x, w.y, w.z].into_iter().enumerate() {
            if v.is_finite() {
                lo[i] = lo[i].min(v);
                hi[i] = hi[i].max(v);
                any = true;
            }
        }
    }
    if !any || lo.iter().chain(hi.iter()).any(|v| !v.is_finite()) {
        return Value::Null;
    }
    json!({ "min": [fnum(lo[0]), fnum(lo[1]), fnum(lo[2])], "max": [fnum(hi[0]), fnum(hi[1]), fnum(hi[2])] })
}

/// A local direction turned into the world frame (rigid placement).
fn world_dir(loc: &Location, n: [f64; 3]) -> [f64; 3] {
    let o = world_pt(loc, [0.0; 3]);
    let t = world_pt(loc, n);
    [t.x - o.x, t.y - o.y, t.z - o.z]
}

/// Which way a face looks, in the words a 2.5D pick is chosen by.
fn facing(n: [f64; 3]) -> &'static str {
    const LEVEL: f64 = 0.999_999;
    if n[2] >= LEVEL {
        "up"
    } else if n[2] <= -LEVEL {
        "down"
    } else if n[2].abs() < 1e-6 {
        "side"
    } else {
        "inclined"
    }
}

/// Feature edge -> tangent chain id, keyed by its welded end pair, so a
/// face's boundary ring can name the chains that run it.
fn edge_chains(f: &Features) -> HashMap<(u32, u32), u64> {
    let mut out = HashMap::new();
    for c in feature::chains(f) {
        for &ei in &c.edges {
            if let Some(e) = f.edges.get(ei as usize) {
                out.insert((e.a.min(e.b), e.a.max(e.b)), c.id);
            }
        }
    }
    out
}

fn loop_view(lp: &feature::Loop, by_edge: &HashMap<(u32, u32), u64>, loc: &Location, f: &Features) -> Value {
    let closed = !matches!(lp.kind, LoopKind::Open);
    let n = lp.verts.len();
    let spans = if closed { n } else { n.saturating_sub(1) };
    let mut ids: Vec<u64> = Vec::new();
    let mut complete = n >= 2;
    for k in 0..spans {
        let (Some(&a), Some(&b)) = (lp.verts.get(k), lp.verts.get((k + 1) % n.max(1))) else {
            complete = false;
            continue;
        };
        match by_edge.get(&(a.min(b), a.max(b))) {
            Some(id) if !ids.contains(id) => ids.push(*id),
            Some(_) => {}
            None => complete = false,
        }
    }
    let chains: Vec<String> = ids.into_iter().map(|id| feature_id(FeatureSel::Chain(id))).collect();
    json!({
        "kind": match lp.kind { LoopKind::Outer => "outer", LoopKind::Hole => "hole", LoopKind::Open => "open" },
        "vertices": n,
        "bounds": bounds(loc, lp.verts.iter().filter_map(|&v| f.verts.get(v as usize).copied())),
        // Picking every chain listed picks this ring; `complete` is false
        // when some span of it is no feature edge (then pick the face).
        "chains": chains,
        "complete": complete,
    })
}

fn faces_view(f: &Features, loc: &Location, limit: usize) -> (Vec<Value>, usize) {
    let by_edge = edge_chains(f);
    // Strip -> welded smooth face, so a strip names the logical face too.
    let mut patch_of: HashMap<u64, u64> = HashMap::new();
    for p in feature::patches(f) {
        for fid in p.faces {
            patch_of.insert(fid, p.id);
        }
    }
    let planar: Vec<&feature::Face> = f.faces.iter().filter(|fa| matches!(fa.kind, FaceKind::Planar)).collect();
    let rows = planar
        .iter()
        .take(limit)
        .map(|fa| {
            let n = world_dir(loc, fa.normal);
            let verts = fa.loops.iter().flat_map(|l| l.verts.iter()).filter_map(|&v| f.verts.get(v as usize).copied());
            json!({
                "id": feature_id(FeatureSel::Face(fa.id)),
                "normal": [fnum(n[0]), fnum(n[1]), fnum(n[2])],
                "facing": facing(n),
                "area": fnum(fa.area),
                "bounds": bounds(loc, verts),
                "patch": patch_of.get(&fa.id).map(|p| feature_id(FeatureSel::Patch(*p))),
                "loops": fa.loops.iter().map(|l| loop_view(l, &by_edge, loc, f)).collect::<Vec<_>>(),
            })
        })
        .collect();
    (rows, planar.len())
}

fn chains_view(f: &Features, loc: &Location, limit: usize) -> (Vec<Value>, usize) {
    let all = feature::chains(f);
    let rows = all
        .iter()
        .take(limit)
        .map(|c| {
            let pts = feature::chain_points(f, c);
            let length: f64 = pts
                .windows(2)
                .map(|w| {
                    let d = [w[1][0] - w[0][0], w[1][1] - w[0][1], w[1][2] - w[0][2]];
                    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
                })
                .filter(|l| l.is_finite())
                .sum();
            json!({
                "id": feature_id(FeatureSel::Chain(c.id)),
                "closed": c.closed,
                "edges": c.edges.len(),
                "length": fnum(length),
                "start": pts.first().map(|p| pt(world_pt(loc, *p))),
                "end": pts.last().map(|p| pt(world_pt(loc, *p))),
                "bounds": bounds(loc, pts.iter().copied()),
            })
        })
        .collect();
    (rows, all.len())
}

fn patches_view(f: &Features, limit: usize) -> (Vec<Value>, usize) {
    let all = feature::patches(f);
    let rows = all
        .iter()
        .take(limit)
        .map(|p| {
            let faces: Vec<String> = p.faces.iter().map(|id| feature_id(FeatureSel::Face(*id))).collect();
            json!({ "id": feature_id(FeatureSel::Patch(p.id)), "faces": faces })
        })
        .collect();
    (rows, all.len())
}

fn vertices_view(f: &Features, loc: &Location, limit: usize) -> (Vec<Value>, usize) {
    let all: Vec<Value> = feature::corner_verts(f)
        .iter()
        .filter_map(|&vi| {
            let id = feature::vertex_id(f, vi)?;
            let p = f.verts.get(vi as usize)?;
            Some(json!({ "id": feature_id(FeatureSel::Vertex(id)), "at": pt(world_pt(loc, *p)) }))
        })
        .collect();
    let total = all.len();
    (all.into_iter().take(limit).collect(), total)
}

fn wanted_kinds(a: &Map<String, Value>) -> Result<Vec<&'static str>, String> {
    match a.get("kinds") {
        None | Some(Value::Null) => Ok(KINDS.to_vec()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .and_then(|s| KINDS.iter().find(|k| **k == s).copied())
                    .ok_or_else(|| format!("\"kinds\" entries are {}; got {v}", KINDS.join(", ")))
            })
            .collect(),
        Some(other) => Err(format!("\"kinds\" must be an array, got {other}")),
    }
}

fn limit_of(a: &Map<String, Value>) -> Result<usize, String> {
    match a.get("limit") {
        None | Some(Value::Null) => Ok(DEFAULT_LIMIT),
        Some(v) => v
            .as_u64()
            .filter(|n| (1..=MAX_LIMIT as u64).contains(n))
            .map(|n| n as usize)
            .ok_or_else(|| format!("\"limit\" must be a whole number from 1 to {MAX_LIMIT}, got {v}")),
    }
}

pub(super) fn list_features(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "model")?;
    let kinds = wanted_kinds(a)?;
    let limit = limit_of(a)?;
    // #561: a fixture's faces are pickable too - `locate_part` mates them.
    let e = match model::find_by_name(&env.world, name) {
        Some(e) if model::is_fixture(&env.world, &e) => e,
        _ => req_model(env, name)?,
    };
    let w = &env.world;
    let f = model::features_of(w, &e).ok_or_else(|| format!("'{name}' has no geometry to pick from"))?;
    let loc = model::transform_of(w, &e).unwrap_or_default();
    let mut out = json!({ "model": name, "frame": "world", "truncated": false });
    let mut truncated = false;
    for k in kinds {
        let (rows, total) = match k {
            "faces" => faces_view(&f, &loc, limit),
            "chains" => chains_view(&f, &loc, limit),
            "patches" => patches_view(&f, limit),
            _ => vertices_view(&f, &loc, limit),
        };
        truncated |= rows.len() < total;
        out[k] = Value::Array(rows);
        out[format!("{k}_total")] = json!(total);
    }
    out["truncated"] = json!(truncated);
    Ok(out)
}
