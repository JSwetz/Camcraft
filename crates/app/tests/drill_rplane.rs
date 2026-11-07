//! Issue #91: an axial drill whose R plane (Top of Part Z, where the
//! cycle's rapid ends) lies below the stock top over the hole - the
//! rapid down to R crosses uncut stock, a collision - must never post.
//! Provenance: our own fixtures, specced from the Prime Directive and the
//! shop rule "never rapid into uncut stock"; no vendor material.
//!
//! Gate map (M4.4): covers app::operation (drill_reach's R-plane law,
//! the Z arm of generate's Drill), app::posting (the gate).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOP: f64 = 20.0;
const FLOOR: f64 = 12.0;
const HALF: f64 = 2.05;

fn flip(t: &Triangle) -> Triangle {
    Triangle::new(t.a, t.c, t.b)
}

/// A 40 x 40 x [-10, 20] block with a square hole `2 HALF` wide from the
/// top, blind to `FLOOR`. Outward-wound.
fn block() -> Mesh {
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    let mut f = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, -10.0), Point3::new(20.0, 20.0, TOP));
    f.extend(outer.facets[4..12].iter().cloned());
    f.extend(outer.facets[0..2].iter().cloned());
    let o = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)];
    let i = [(-HALF, -HALF), (HALF, -HALF), (HALF, HALF), (-HALF, HALF)];
    let p = |(x, y): (f64, f64)| Point3::new(x, y, TOP);
    for k in 0..4 {
        let n = (k + 1) % 4;
        quad(p(o[k]), p(o[n]), p(i[n]), p(i[k]), &mut f);
    }
    let inner = cube(Point3::new(-HALF, -HALF, FLOOR), Point3::new(HALF, HALF, TOP));
    f.extend(inner.facets[4..12].iter().map(flip));
    f.extend(inner.facets[0..2].iter().map(flip));
    Mesh::from_facets(f)
}

/// The block under a 40 x 40 box stock whose top stands at `stock_top`.
fn world_with(stock_top: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("block", block()));
    let loc = Location { pos: Point3::new(0.0, 0.0, -10.0), ..Default::default() };
    let s = SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: stock_top + 10.0 }, loc);
    h.execute(&world, s.expect("stock"));
    let t = catalog().into_iter().find(|(n, _)| *n == "DR-0400").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "DR-0400", t).expect("tool"));
    (world, h)
}

fn params(cycle: CycleChoice) -> OpParams {
    let hole = Hole { xy: Point2::new(0.0, 0.0), radius: 2.0, entry_z: TOP, floor_z: FLOOR, through: false, stepped: false };
    OpParams::Drill {
        clearance_z: Height::fixed(TOP + 15.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: None,
        cycle,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![hole],
        cross: vec![],
    }
}

fn raw_op(p: OpParams) -> Operation {
    Operation { tool: "DR-0400".into(), src: SrcRef::Entity("block".into()), params: p, stamp: None, setup: None, area: Default::default() }
}

fn tmp_nc(tag: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-rplane-{}-{}.nc", tag, std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

const FANUC: &str = include_str!("../../../assets/posts/fanuc.cps");
const HEIDENHAIN: &str = include_str!("../../../assets/posts/heidenhain.cps");

/// Generate through the command path, then post through `cps`.
fn post_with(stock_top: f64, cycle: CycleChoice, cps: &str, tag: &str) -> Result<String, String> {
    let (world, mut h) = world_with(stock_top);
    let op = CreateOperation::new(&world, "D1", "DR-0400", SrcRef::Entity("block".into()), params(cycle));
    h.execute(&world, op.map_err(|e| format!("{e:?}"))?);
    h.execute(&world, GenerateOperation::new(&world, "D1").map_err(|e| format!("{e:?}"))?);
    let nc = tmp_nc(tag);
    let r = posting::post_script(&world, &nc, cps).map(|p| p.program);
    let _ = std::fs::remove_file(&nc);
    r
}

#[test]
fn an_r_plane_below_the_unfaced_stock_top_is_refused_by_name() {
    // Stock top Z 25 over a part top Z 20; R = Top of Part Z = 20: the
    // rapid 35 -> 20 crosses 5 mm of uncut stock. Before #91 this
    // posted G81 R20 (and the expanded chain's G00 Z+20) silently.
    for cycle in [CycleChoice::Drill, CycleChoice::Peck] {
        let (world, _h) = world_with(TOP + 5.0);
        let e = operation::generate(&world, &raw_op(params(cycle)), "").err().map(|e| e.to_string());
        let e = e.unwrap_or_else(|| panic!("{cycle:?}: a rapid down through 5 mm of stock generated"));
        for s in ["R plane Z 20.000", "below the stock top Z 25.000", "hole at (0.000, 0.000)", "cross uncut stock", "raise Top of Part Z"] {
            assert!(e.contains(s), "{cycle:?}: the refusal names '{s}': {e}");
        }
    }
}

#[test]
fn neither_post_releases_an_r_plane_below_the_stock_top() {
    for cycle in [CycleChoice::Drill, CycleChoice::Peck] {
        for (n, cps) in [("fanuc", FANUC), ("heidenhain", HEIDENHAIN)] {
            let r = post_with(TOP + 5.0, cycle, cps, &format!("low-{n}"));
            assert!(r.is_err(), "{cycle:?} {n}: posted through the stock: {r:?}");
        }
    }
}

#[test]
fn an_r_plane_at_the_stock_top_still_posts_canned_and_expanded() {
    for (cycle, word) in [(CycleChoice::Drill, "G81"), (CycleChoice::Peck, "G73")] {
        let canned = post_with(TOP, cycle, FANUC, "top-fanuc");
        let text = canned.unwrap_or_else(|e| panic!("{cycle:?}: the stock-top R plane posts: {e}"));
        assert!(text.contains(word) && text.contains("R20"), "{cycle:?}: the canned cycle at R20: {text}");
        let expanded = post_with(TOP, cycle, HEIDENHAIN, "top-heid");
        let text = expanded.unwrap_or_else(|e| panic!("{cycle:?}: the expanded chain posts: {e}"));
        assert!(text.contains("G00 X+0 Y+0 Z+20"), "{cycle:?}: the rapid onto R: {text}");
    }
}
