//! Not noisy: every committed 2D golden program whose kind answers to a
//! depth law (contour, face, pocket - the model law with its solid law
//! b2; drill, peck, tap - containment's solid law) is regenerated, proved
//! byte-identical to its golden, and verified against the finished part
//! it was cut for, modelled as an import would hand it over (round
//! holes as inscribed 32/128-gons). The pinned verdict is what the
//! per-plane verifier gave before the solid law, warning for warning:
//! the solid law adds none.
//! (Slot and trace cut their source - no depth law runs on them; the
//! 3D finish goldens verify as `Mode::Envelope`, where the solid law
//! stands down by mode gate - their verdict path is untouched; the
//! arc-planes golden is a posting fixture with no part.)
//!
//! Provenance: our own harness over our own goldens; no vendor material.
//!
//! Gate map (solid law): covers app::verify (engine b2, solid_hits),
//! app::machining2d (contour, face, pocket), app::drilling (drill).

mod common;

use common::gen_to_cltext;

use camcraft_app::cl::Toolpath;
use camcraft_app::drilling::{CycleChoice, CycleParams, Hole, drill};
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::geometry::{Point3, Triangle};
use camcraft_app::machining2d::{ContourParams, FaceParams, Lead, PocketParams, PocketStrategy, Side, contour, face, pocket};
use camcraft_app::tools::{Tool, ToolKind};
use camcraft_app::verify::{Cutter, Mode, SolidLaw, ToolGeom, VerifyContext, Warning, solid_hits, verify};

type P2 = (f64, f64);

fn golden(name: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name);
    std::fs::read_to_string(&p).expect("golden").replace("\r\n", "\n")
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<P2> {
    vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

fn circle(c: P2, r: f64, n: usize) -> Vec<P2> {
    (0..n).map(|k| {
        let t = std::f64::consts::TAU * k as f64 / n as f64;
        (c.0 + r * t.cos(), c.1 + r * t.sin())
    }).collect()
}

fn region(loops: &[Vec<P2>]) -> Region {
    Region::new(loops.iter().map(|l| Loop::new(l.iter().map(|&(x, y)| Point2::new(x, y)).collect()).expect("loop")).collect())
        .expect("region")
}

/// Walls over a ccw loop, facing out of it (`out`) or into it (a hole).
fn walls(l: &[P2], z0: f64, z1: f64, out: bool, f: &mut Vec<Triangle>) {
    let mut v = l.to_vec();
    if !out {
        v.reverse();
    }
    for k in 0..v.len() {
        let (a, b) = (v[k], v[(k + 1) % v.len()]);
        let q = |p: P2, z: f64| Point3::new(p.0, p.1, z);
        f.push(Triangle::new(q(a, z0), q(b, z0), q(b, z1)));
        f.push(Triangle::new(q(a, z0), q(b, z1), q(a, z1)));
    }
}

/// A horizontal face: the convex ccw loop `o`, less the convex ccw loop
/// `i` inside it. Each sector between two rays from `i`'s centroid
/// through consecutive `i` vertices is a convex polygon (inner edge, ray,
/// outer boundary with its corners, ray), fanned; every triangle is
/// turned ccw and the total checked against the area difference - the
/// harness proves its own fixture tiles the face.
fn cap(o: &[P2], i: Option<&[P2]>, z: f64, up: bool, f: &mut Vec<Triangle>) {
    let area = |l: &[P2]| (0..l.len()).map(|k| { let (a, b) = (l[k], l[(k + 1) % l.len()]); a.0 * b.1 - b.0 * a.1 }).sum::<f64>() * 0.5;
    let p = |q: P2| Point3::new(q.0, q.1, z);
    let mut tris: Vec<[P2; 3]> = Vec::new();
    match i {
        None => (1..o.len() - 1).for_each(|k| tris.push([o[0], o[k], o[k + 1]])),
        Some(i) => {
            let c = i.iter().fold((0.0, 0.0), |s, q| (s.0 + q.0, s.1 + q.1));
            let c = (c.0 / i.len() as f64, c.1 / i.len() as f64);
            let tau = std::f64::consts::TAU;
            let ang = |q: P2| (q.1 - c.1).atan2(q.0 - c.0).rem_euclid(tau);
            // Where the ray from c through q leaves `o`.
            let hit = |q: P2| -> P2 {
                let (dx, dy) = (q.0 - c.0, q.1 - c.1);
                let mut best = f64::INFINITY;
                for k in 0..o.len() {
                    let (a, b) = (o[k], o[(k + 1) % o.len()]);
                    let (ex, ey) = (b.0 - a.0, b.1 - a.1);
                    let den = dx * ey - dy * ex;
                    if den.abs() < 1e-15 {
                        continue;
                    }
                    let t = ((a.0 - c.0) * ey - (a.1 - c.1) * ex) / den;
                    let u = ((a.0 - c.0) * dy - (a.1 - c.1) * dx) / den;
                    if t > 0.0 && (-1e-12..=1.0 + 1e-12).contains(&u) {
                        best = best.min(t);
                    }
                }
                (c.0 + dx * best, c.1 + dy * best)
            };
            for k in 0..i.len() {
                let (ia, ib) = (i[k], i[(k + 1) % i.len()]);
                let (t0, mut t1) = (ang(ia), ang(ib));
                if t1 <= t0 {
                    t1 += tau;
                }
                let mut corners: Vec<(f64, P2)> = o.iter().map(|&q| {
                    let mut t = ang(q);
                    if t <= t0 {
                        t += tau;
                    }
                    (t, q)
                }).filter(|&(t, _)| t < t1).collect();
                corners.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut poly = vec![ia, hit(ia)];
                poly.extend(corners.into_iter().map(|(_, q)| q));
                poly.push(hit(ib));
                poly.push(ib);
                for j in 1..poly.len() - 1 {
                    tris.push([ia, poly[j], poly[j + 1]]);
                }
            }
        }
    }
    let want = area(o) - i.map_or(0.0, area);
    let mut got = 0.0;
    for [a, b, c] in tris {
        let s = ((b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)) * 0.5;
        if s.abs() < 1e-12 {
            continue; // a sliver where a ray meets a corner
        }
        let (b, c) = if s > 0.0 { (b, c) } else { (c, b) };
        got += s.abs();
        f.push(if up { Triangle::new(p(a), p(b), p(c)) } else { Triangle::new(p(a), p(c), p(b)) });
    }
    assert!((got - want).abs() < 1e-6 * want.abs().max(1.0), "cap tiles its face: {got} vs {want}");
}

/// A plate `o` from z0 to z1 with at most one hole `h` from `hz` (z0
/// for through) to the top; a blind hole gets its floor.
fn plate(o: &[P2], h: Option<(&[P2], f64)>, z0: f64, z1: f64, f: &mut Vec<Triangle>) {
    walls(o, z0, z1, true, f);
    plate_cell(o, h, z0, z1, f);
}

/// One cell of a plate's faces (caps, hole walls and floor) - cells
/// tiling a plate share its outer walls, never walls of their own, so
/// the shell stays one closed surface.
fn plate_cell(o: &[P2], h: Option<(&[P2], f64)>, z0: f64, z1: f64, f: &mut Vec<Triangle>) {
    match h {
        None => {
            cap(o, None, z0, false, f);
            cap(o, None, z1, true, f);
        }
        Some((h, hz)) => {
            cap(o, Some(h), z1, true, f);
            walls(h, hz, z1, false, f);
            if hz > z0 {
                cap(h, None, hz, true, f);
                cap(o, None, z0, false, f);
            } else {
                cap(o, Some(h), z0, false, f);
            }
        }
    }
}

fn mill(d: f64) -> ToolGeom {
    ToolGeom { radius: d * 0.5, nose_radius: 0.0, flute_len: 25.0, shank_radius: d * 0.5, shank_len: 30.0, holder_radius: 16.0, holder_len: 40.0 }
}

fn cx<'a>(region: &'a Region, model: Option<&'a [Triangle]>, tool: &'a ToolGeom, mode: Mode, top: f64, clear: f64) -> VerifyContext<'a> {
    VerifyContext {
        region,
        stock: None,
        model,
        tool,
        cutter: None,
        mode,
        stock_top_z: top,
        clearance_z: clear,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
    }
}

/// The model kinds' verdict, as the operation layer reads it.
fn section_law(t: &Toolpath, region: &Region, part: &[Triangle], tool: &ToolGeom, mode: Mode, top: f64, clear: f64) -> Vec<Warning> {
    verify(t, &cx(region, Some(part), tool, mode, top, clear))
}

/// The containment kinds' depth verdict: the solid law with the tool's
/// own tip, hole walls excused (their region laws read the exact rims,
/// untouched by the solid law and not judged here).
fn contain_law(t: &Toolpath, part: &[Triangle], tool: &Tool) -> Vec<Warning> {
    let law = SolidLaw { step: 0.25, allow: 0.01, hole_walls: true };
    solid_hits(t, part, &Cutter::of(tool), &law)
}

fn contour_params(d: f64, lead: Lead, side: Side) -> ContourParams {
    ContourParams {
        tool_diameter: d,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side,
        lead,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    }
}

/// (golden, verdict) for every depth-law golden.
fn verdicts() -> Vec<(&'static str, Vec<Warning>)> {
    let mut out = Vec::new();
    let mut check = |name: &'static str, t: &Toolpath, w: Vec<Warning>| {
        assert_eq!(gen_to_cltext(t), golden(name), "{name}: the regenerated program is the golden");
        out.push((name, w));
    };
    // The plate with a round bore (contour_disc_hole, tangential lead).
    let bore6 = circle((20.0, 10.0), 6.0, 128);
    let mut part = Vec::new();
    plate(&rect(0.0, 0.0, 40.0, 20.0), Some((&bore6, -4.0)), -4.0, 0.0, &mut part);
    let reg = region(&[rect(0.0, 0.0, 40.0, 20.0), bore6.clone()]);
    let g = mill(6.0);
    for (name, lead) in [("contour_disc_hole.cl", Lead::Arc { radius: 3.0 }), ("contour_tangential_lead.cl", Lead::Tangential { radius: 3.0 })] {
        let t = contour(&reg, &contour_params(6.0, lead, Side::Auto)).expect("contour");
        check(name, &t, section_law(&t, &reg, &part, &g, Mode::Boundary, 0.0, 5.0));
    }
    // The plate with a square hole, sharp and blended.
    let sq = rect(16.0, 6.0, 24.0, 14.0);
    let mut part = Vec::new();
    plate(&rect(0.0, 0.0, 40.0, 20.0), Some((&sq, -4.0)), -4.0, 0.0, &mut part);
    let reg = region(&[rect(0.0, 0.0, 40.0, 20.0), sq.clone()]);
    for (name, blend) in [("contour_plate_with_hole.cl", false), ("contour_plate_with_hole_blended.cl", true)] {
        let t = contour(&reg, &ContourParams { blend_turns: blend, ..contour_params(6.0, Lead::Arc { radius: 3.0 }, Side::Auto) }).expect("contour");
        check(name, &t, section_law(&t, &reg, &part, &g, Mode::Boundary, 0.0, 5.0));
    }
    // The R12 bore walked from the inside, arc entry.
    let bore12 = circle((20.0, 10.0), 12.0, 128);
    let mut part = Vec::new();
    plate(&rect(0.0, -10.0, 40.0, 30.0), Some((&bore12, -4.0)), -4.0, 0.0, &mut part);
    let reg = region(std::slice::from_ref(&bore12));
    let t = contour(&reg, &ContourParams { entry: camcraft_app::machining3d::Entry3D::Arc, ..contour_params(6.0, Lead::Arc { radius: 3.0 }, Side::Inside) })
        .expect("contour");
    check("contour_bore_arc_entry.cl", &t, section_law(&t, &reg, &part, &g, Mode::Boundary, 0.0, 5.0));
    // The faced 40 x 40 block, finished at Z -2.
    let sq40 = rect(0.0, 0.0, 40.0, 40.0);
    let mut part = Vec::new();
    plate(&sq40, None, -12.0, -2.0, &mut part);
    let reg = region(std::slice::from_ref(&sq40));
    let g10 = mill(10.0);
    for (name, blend, top, step) in [
        ("face_square_40x40.cl", false, None, None),
        ("face_square_40x40_blended.cl", true, None, None),
        ("face_square_40x40_stepped.cl", false, Some(2.0), Some(1.5)),
    ] {
        let p = FaceParams { holes: Default::default(), tool_diameter: 10.0, stepover: 8.0, clearance_z: 5.0, cut_z: -2.0, top_z: top, stepdown: step, feed: 500.0, plunge_feed: 100.0, climb: true, blend_turns: blend };
        let t = face(&reg, &p).expect("face");
        check(name, &t, section_law(&t, &reg, &part, &g10, Mode::Boundary, top.unwrap_or(-2.0), 5.0));
    }
    // The 30 x 30 pocket to Z -4 around a 10 x 10 island standing to Z 0.
    let (outer, pk, isl) = (rect(-10.0, -10.0, 40.0, 40.0), rect(0.0, 0.0, 30.0, 30.0), rect(10.0, 10.0, 20.0, 20.0));
    let mut part = Vec::new();
    walls(&outer, -10.0, 0.0, true, &mut part);
    cap(&outer, None, -10.0, false, &mut part);
    cap(&outer, Some(&pk), 0.0, true, &mut part);
    walls(&pk, -4.0, 0.0, false, &mut part);
    cap(&pk, Some(&isl), -4.0, true, &mut part);
    walls(&isl, -4.0, 0.0, true, &mut part);
    cap(&isl, None, 0.0, true, &mut part);
    let reg = region(&[pk.clone(), isl.clone()]);
    let g8 = mill(8.0);
    for (name, strategy, blend) in [
        ("pocket_zigzag_plate_with_island.cl", PocketStrategy::Zigzag, false),
        ("pocket_zigzag_plate_with_island_blended.cl", PocketStrategy::Zigzag, true),
        ("pocket_waterline_plate_with_island.cl", PocketStrategy::Waterline, false),
    ] {
        let p = PocketParams {
            links: camcraft_app::links::Links::PLANAR,
            tool_diameter: 8.0,
            stepover: 3.0,
            stock_to_leave: 0.0,
            clearance_z: 5.0,
            retract_z: None,
            top_z: 0.0,
            bottom_z: -4.0,
            stepdown: 2.0,
            feed: 400.0,
            plunge_feed: 80.0,
            climb: true,
            strategy,
            entry: camcraft_app::machining3d::Entry3D::Plunge,
            helix_radius: 1.0,
            optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
            blend_turns: blend,
        };
        let t = pocket(&reg, &p).expect("pocket");
        check(name, &t, section_law(&t, &reg, &part, &g8, Mode::Area, 0.0, 5.0));
    }
    // The drilling goldens: their plates, holes inscribed 32-gons.
    let tool = |kind: ToolKind, d: f64, tip: f64, pitch: f64| Tool {
        kind,
        diameter: d,
        corner_r: 0.0,
        tip_angle: tip,
        pitch,
        nose_radius: 0.0,
        width: 0.0,
        flute_len: 40.0,
        shank_d: d.min(10.0),
        shank_len: 30.0,
        holder_d: 20.0,
        holder_len: 40.0,
        material_group: String::from("steel"),
        rpm: 500.0,
        feed: 200.0,
        tool_material: None,
        flutes: None,
        center_cutting: true,
        orientation: None,
    };
    let hole = |x: f64, y: f64, r: f64, floor: f64, through: bool| Hole { xy: Point2::new(x, y), radius: r, entry_z: 0.0, floor_z: floor, through, stepped: false };
    let cycle = |holes: &[Hole], tool: &Tool, clear: f64| {
        drill(&CycleParams { clearance_z: clear, r_z: 0.0, hop_z: None, bottom_z: None, cycle: CycleChoice::Auto, peck: 0.0, dwell: 0.0, feed: 200.0, plunge_feed: 60.0, tool, holes })
            .expect("drill")
    };
    let dr6 = tool(ToolKind::Drill, 6.0, 118.0, 0.0);
    let (ha, hb) = (circle((10.0, 10.0), 3.0, 32), circle((30.0, 10.0), 3.0, 32));
    let mut part = Vec::new();
    walls(&rect(0.0, 0.0, 40.0, 20.0), -4.0, 0.0, true, &mut part);
    plate_cell(&rect(0.0, 0.0, 20.0, 20.0), Some((&ha, -4.0)), -4.0, 0.0, &mut part);
    plate_cell(&rect(20.0, 0.0, 40.0, 20.0), Some((&hb, -3.0)), -4.0, 0.0, &mut part);
    let t = cycle(&[hole(10.0, 10.0, 3.0, -4.0, true), hole(30.0, 10.0, 3.0, -3.0, false)], &dr6, 5.0);
    check("drill_plate.cl", &t, contain_law(&t, &part, &dr6));
    let hp = circle((20.0, 10.0), 3.0, 32);
    let mut part = Vec::new();
    plate(&rect(0.0, 0.0, 40.0, 20.0), Some((&hp, -23.0)), -23.0, 0.0, &mut part);
    let t = cycle(&[hole(20.0, 10.0, 3.0, -23.0, true)], &dr6, 6.0);
    check("drill_peck.cl", &t, contain_law(&t, &part, &dr6));
    // The tapped hole: the finished part's thread stands at the tap's
    // own diameter (the golden's 2.5 mm hole record is the tap DRILL's).
    let tap = tool(ToolKind::Tap, 10.0, 0.0, 1.5);
    let ht = circle((15.0, 15.0), 5.0, 32);
    let mut part = Vec::new();
    plate(&rect(0.0, 0.0, 30.0, 30.0), Some((&ht, -12.0)), -20.0, 0.0, &mut part);
    let t = cycle(&[hole(15.0, 15.0, 2.5, -12.0, false)], &tap, 5.0);
    check("tap_plate.cl", &t, contain_law(&t, &part, &tap));
    out
}

#[test]
fn every_depth_law_golden_verifies_exactly_as_before() {
    let got = verdicts();
    for (name, w) in &got {
        eprintln!("{name}: {w:?}");
    }
    assert_eq!(got.len(), 14, "every depth-law golden is judged");
    // The verifier's verdict on these programs, recorded at the base
    // before the solid law - and unchanged by it, warning for warning.
    for (name, w) in got {
        let want: Vec<Warning> = match name {
            // Law (a), the XY crossing law the solid law never touches:
            // the same three move pairs before and after.
            "contour_disc_hole.cl" | "contour_tangential_lead.cl" => {
                [[17, 19], [40, 42], [63, 65]].into_iter().map(|moves| Warning::Crossing { moves }).collect()
            }
            _ => Vec::new(),
        };
        assert_eq!(w, want, "{name}");
    }
}
