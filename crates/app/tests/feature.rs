//! Feature edges and face clustering: crease/boundary extraction off a
//! triangle soup, the planar faces they bound, and the position-keyed
//! ids that survive STEP ordering and re-tessellation.
//! Lifted from `crates/ui/src/viewport.rs` (its `edge_tests`) in M1.5
//! step 1, re-based on the 1 µm f64 lattice, plus the lattice pins the
//! old f32 version could not express; clustering added in step 2.
//!
//! Gate map (M4.4): covers app::geometry, app::model, app::world.

use hsmworks_app::geometry::cube;
use hsmworks_app::geometry::feature::{
    chains, digest_of, feature_edges, features_of_facets, patch_rings, patches, Chain, EdgeKind,
    FaceKind, Features, LoopKind,
};
use hsmworks_app::geometry::{Mesh, Point3, Triangle};
use hsmworks_app::model;
use hsmworks_app::world::new_world;

fn flat(mesh: &Mesh) -> Vec<[f64; 3]> {
    mesh.facets
        .iter()
        .flat_map(|t| [[t.a.x, t.a.y, t.a.z], [t.b.x, t.b.y, t.b.z], [t.c.x, t.c.y, t.c.z]])
        .collect()
}

fn flat_row() -> Vec<[f64; 3]> {
    // A 2x1 rectangle tiled with four coplanar triangles: no interior
    // seam may be drawn, all border edges must be.
    let quad = |x0: f64| {
        [
            [[x0, 0.0, 0.0], [x0 + 1.0, 1.0, 0.0], [x0 + 1.0, 0.0, 0.0]],
            [[x0, 0.0, 0.0], [x0, 1.0, 0.0], [x0 + 1.0, 1.0, 0.0]],
        ]
    };
    quad(0.0).into_iter().chain(quad(1.0)).flatten().collect()
}

fn kinds(edges: &[hsmworks_app::geometry::feature::FeatureEdge]) -> Vec<EdgeKind> {
    edges.iter().map(|e| e.kind).collect()
}

#[test]
fn coplanar_seam_is_not_an_edge() {
    // The 2x1 rectangle: 6 border edges, but neither the internal
    // vertical nor either diagonal (all coplanar seams).
    let edges = feature_edges(&flat_row());
    assert_eq!(edges.len(), 6);
    assert!(
        kinds(&edges).iter().all(|k| *k == EdgeKind::Boundary),
        "the visible edges are rims, not creases"
    );
}

#[test]
fn closed_cube_shows_its_twelve_box_edges() {
    let edges = feature_edges(&flat(&cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0))));
    assert_eq!(edges.len(), 12, "a cube is 12 crease edges, no seams");
    assert!(
        kinds(&edges).iter().all(|k| *k == EdgeKind::Crease),
        "every box edge is a 90-degree bend"
    );
}

#[test]
fn empty_and_degenerate_inputs_stay_finite() {
    assert!(feature_edges(&[]).is_empty());
    let nan = [
        [f64::NAN, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0],
    ];
    // Non-finite corners never alias the lattice; their edges are drawn
    // as suspect, never dropped, never a panic.
    let e = feature_edges(&nan);
    assert_eq!(e.len(), 3);
}

#[test]
fn sub_micron_jitter_welds_the_seam() {
    // Two coplanar triangles sharing the (0,0)-(1,0) seam, the far side
    // jittered by 0.4 µm: inside the 1 µm quantum, so the seam welds and
    // (being coplanar) stays hidden; only the 4 rim edges remain.
    let d = 4e-7f64;
    let pts = [
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.5, -1.0, 0.0]],
        [[1.0 + d, 0.0, 0.0], [d, 0.0, 0.0], [0.5, 1.0, 0.0]],
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<[f64; 3]>>();
    assert_eq!(feature_edges(&pts).len(), 4);
    // Past the quantum (4 µm) the corners are distinct vertices: the seam
    // counts as two unshared rim edges, so six lines are emitted instead.
    let d = 4e-6f64;
    let pts = [
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.5, -1.0, 0.0]],
        [[1.0 + d, 0.0, 0.0], [d, 0.0, 0.0], [0.5, 1.0, 0.0]],
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<[f64; 3]>>();
    let edges = feature_edges(&pts);
    assert_eq!(edges.len(), 6);
    assert!(kinds(&edges).iter().all(|k| *k == EdgeKind::Boundary));
}

#[test]
fn non_manifold_edge_is_emitted_once_and_tagged() {
    // Three facets share the (0,0)-(1,0) seam: suspect, but drawn - and
    // drawn once, not once per incident facet.
    let pts = [
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.5, -1.0, 0.0]],
        [[1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.5, 1.0, 0.0]],
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.5, 0.0, 1.0]],
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<[f64; 3]>>();
    let edges = feature_edges(&pts);
    let nman = edges.iter().filter(|e| e.kind == EdgeKind::NonManifold).count();
    assert_eq!(nman, 1, "the shared seam is one visible line");
    assert_eq!(edges.len(), 7, "plus the six remaining rims");
}

// ===================== face clustering (step 2) =====================

fn pt(p: [f64; 3]) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}

fn tris(pts: &[[f64; 3]]) -> Vec<Triangle> {
    pts
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| Triangle::new(pt(c[0]), pt(c[1]), pt(c[2])))
        .collect()
}

/// One planar quad as two triangles, ordered `a b c / a c d`.
fn quad(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> Vec<[f64; 3]> {
    vec![a, b, c, a, c, d]
}

/// An axis-aligned box's 12 facets, CCW seen from outside (mirrors
/// `geometry::cube`, but as raw rows so variants can reuse the corners).
fn box_rows(x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64) -> Vec<[f64; 3]> {
    let p = |i: usize| match i {
        0 => [x0, y0, z0],
        1 => [x1, y0, z0],
        2 => [x1, y1, z0],
        3 => [x0, y1, z0],
        4 => [x0, y0, z1],
        5 => [x1, y0, z1],
        6 => [x1, y1, z1],
        _ => [x0, y1, z1],
    };
    [(3, 2, 1, 0), (4, 5, 6, 7), (0, 1, 5, 4), (2, 3, 7, 6), (1, 2, 6, 5), (3, 0, 4, 7)]
        .into_iter()
        .flat_map(|(a, b, c, d)| quad(p(a), p(b), p(c), p(d)))
        .collect()
}

/// The same box with its top face split into four quads: 18 facets.
fn box_rows_subdivided_top(x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64) -> Vec<[f64; 3]> {
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let mut rows = vec![
        quad([x0, y0, z1], [cx, y0, z1], [cx, cy, z1], [x0, cy, z1]),
        quad([cx, y0, z1], [x1, y0, z1], [x1, cy, z1], [cx, cy, z1]),
        quad([x0, cy, z1], [cx, cy, z1], [cx, y1, z1], [x0, y1, z1]),
        quad([cx, cy, z1], [x1, cy, z1], [x1, y1, z1], [cx, y1, z1]),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<[f64; 3]>>();
    // Everything but the top: bottom and the four walls.
    let p = |i: usize| match i {
        0 => [x0, y0, z0],
        1 => [x1, y0, z0],
        2 => [x1, y1, z0],
        3 => [x0, y1, z0],
        4 => [x0, y0, z1],
        5 => [x1, y0, z1],
        6 => [x1, y1, z1],
        _ => [x0, y1, z1],
    };
    for (a, b, c, d) in [(3, 2, 1, 0), (0, 1, 5, 4), (2, 3, 7, 6), (1, 2, 6, 5), (3, 0, 4, 7)] {
        rows.extend(quad(p(a), p(b), p(c), p(d)));
    }
    rows
}

/// Faces whose plane is (near) horizontal at `z`, both windings accepted.
fn horizontal_faces(f: &Features) -> Vec<u64> {
    f.faces
        .iter()
        .filter(|x| x.kind == FaceKind::Planar && x.normal[2] > 0.5)
        .map(|x| x.id)
        .collect()
}

fn faces_at_z(f: &Features, z: f64) -> Vec<&hsmworks_app::geometry::feature::Face> {
    f.faces
        .iter()
        .filter(|x| x.kind == FaceKind::Planar && x.normal[2] > 0.5 && (x.offset - z).abs() < 1e-6)
        .collect()
}

#[test]
fn subdivided_planar_top_stays_one_face_with_the_same_id() {
    let plain = features_of_facets(&tris(&box_rows(0.0, 0.0, 0.0, 4.0, 4.0, 1.0)));
    let subdiv = features_of_facets(&tris(&box_rows_subdivided_top(0.0, 0.0, 0.0, 4.0, 4.0, 1.0)));
    assert_eq!(plain.faces.len(), 6);
    assert_eq!(subdiv.faces.len(), 6, "the 2x2 grid on top must not split the face");
    let top_plain = faces_at_z(&plain, 1.0);
    let top_subdiv = faces_at_z(&subdiv, 1.0);
    assert_eq!((top_plain.len(), top_subdiv.len()), (1, 1));
    assert_eq!(
        top_plain[0].id, top_subdiv[0].id,
        "face ids key on boundary corners, which interior tessellation cannot move"
    );
    assert_eq!(top_subdiv[0].facets.len(), 8, "but the facets are still all there");
    assert_eq!(top_subdiv[0].loops.len(), 1);
    // The ring walks every rim segment, midpoints included - identity
    // ignores them (face_id folds collinear runs), geometry keeps them.
    assert_eq!(top_subdiv[0].loops[0].verts.len(), 8);
}

#[test]
fn face_ids_survive_facet_reordering() {
    // STEP iterates its shape table in nondeterministic order; a
    // position-keyed id must not care.
    let rows = box_rows(0.0, 0.0, 0.0, 2.0, 3.0, 1.0);
    let mut back = rows.clone();
    back.reverse();
    let mut ids_a: Vec<u64> = features_of_facets(&tris(&rows)).faces.iter().map(|f| f.id).collect();
    let mut ids_b: Vec<u64> = features_of_facets(&tris(&back)).faces.iter().map(|f| f.id).collect();
    ids_a.sort_unstable();
    ids_b.sort_unstable();
    assert_eq!(ids_a, ids_b);
    // The content digest is order-free too - that's what keys the cache.
    assert_eq!(digest_of(&tris(&rows)), digest_of(&tris(&back)));
}

#[test]
fn plate_with_a_square_hole_yields_outer_and_hole_loops() {
    let (o0, o1, o2, o3) = ([0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]);
    let (i0, i1, i2, i3) = ([1.5, 1.5], [2.5, 1.5], [2.5, 2.5], [1.5, 2.5]);
    let on = |p: [f64; 2], z: f64| [p[0], p[1], z];
    let mut rows = Vec::new();
    // Top and bottom annuli (z=1 and z=0).
    for z in [1.0, 0.0] {
        rows.extend(quad(on(o0, z), on(o1, z), on(i1, z), on(i0, z)));
        rows.extend(quad(on(o1, z), on(o2, z), on(i2, z), on(i1, z)));
        rows.extend(quad(on(o2, z), on(o3, z), on(i3, z), on(i2, z)));
        rows.extend(quad(on(o3, z), on(o0, z), on(i0, z), on(i3, z)));
    }
    // Outer walls and hole walls (the hole walls face inward).
    rows.extend(quad(on(o0, 1.0), on(o1, 1.0), on(o1, 0.0), on(o0, 0.0)));
    rows.extend(quad(on(o1, 1.0), on(o2, 1.0), on(o2, 0.0), on(o1, 0.0)));
    rows.extend(quad(on(o2, 1.0), on(o3, 1.0), on(o3, 0.0), on(o2, 0.0)));
    rows.extend(quad(on(o3, 1.0), on(o0, 1.0), on(o0, 0.0), on(o3, 0.0)));
    rows.extend(quad(on(i1, 1.0), on(i0, 1.0), on(i0, 0.0), on(i1, 0.0)));
    rows.extend(quad(on(i2, 1.0), on(i1, 1.0), on(i1, 0.0), on(i2, 0.0)));
    rows.extend(quad(on(i3, 1.0), on(i2, 1.0), on(i2, 0.0), on(i3, 0.0)));
    rows.extend(quad(on(i0, 1.0), on(i3, 1.0), on(i3, 0.0), on(i0, 0.0)));

    let f = features_of_facets(&tris(&rows));
    // 2 annuli + 4 outer walls + 4 hole walls; adjacent walls meet at
    // 90 degrees, so each stays its own face.
    assert_eq!(f.faces.len(), 10);
    for z in [1.0, 0.0] {
        let tops = faces_at_z(&f, z);
        assert_eq!(tops.len(), 1, "the annulus at z={z} is one face");
        let loops = &tops[0].loops;
        assert_eq!(loops.len(), 2);
        let kinds: Vec<LoopKind> = loops.iter().map(|l| l.kind).collect();
        assert!(
            kinds.contains(&LoopKind::Outer) && kinds.contains(&LoopKind::Hole),
            "{kinds:?}"
        );
        for l in loops {
            assert_eq!(l.verts.len(), 4);
        }
        let hole = loops.iter().find(|l| l.kind == LoopKind::Hole).unwrap();
        for &v in &hole.verts {
            let p = f.verts[v as usize];
            let inside = (1.4..2.6).contains(&p[0]) && (1.4..2.6).contains(&p[1]);
            assert!(inside, "the hole ring sits on the hole, not the rim");
        }
    }
    // Top and bottom annuli are separate faces despite equal-area rings.
    let a = faces_at_z(&f, 1.0)[0].id;
    let b = faces_at_z(&f, 0.0)[0].id;
    assert_ne!(a, b, "one millimetre apart is two machinable planes, not one");
    assert_eq!(horizontal_faces(&f).len(), 2);
}

#[test]
fn coplanar_boxes_touching_at_a_vertex_stay_separate_faces() {
    let mut rows = box_rows(0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
    rows.extend(box_rows(1.0, 1.0, 1.0, 2.0, 2.0, 2.0));
    let f = features_of_facets(&tris(&rows));
    assert_eq!(f.faces.len(), 12, "two boxes, twelve faces, one shared corner");
    let touch = faces_at_z(&f, 1.0);
    assert_eq!(touch.len(), 2, "A's top and B's bottom are coplanar but unmerged");
    assert_ne!(touch[0].id, touch[1].id);
}

#[test]
fn absurd_soup_yields_degenerate_faces_and_never_panics() {
    let rows = vec![
        [f64::NAN, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        // Beyond the weld range: refused, never clamped onto real points.
        [1e300, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
        // One honest triangle among the garbage.
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
    ];
    let f = features_of_facets(&tris(&rows));
    assert_eq!(f.facet_face.len(), 3);
    assert_eq!(f.faces.len(), 3);
    let planar = f.faces.iter().filter(|x| x.kind == FaceKind::Planar).count();
    assert_eq!(planar, 1, "only the honest triangle earns a plane");
    let good = f.faces.iter().find(|x| x.kind == FaceKind::Planar).unwrap();
    assert_eq!(good.loops.len(), 1);
    assert_eq!(good.loops[0].kind, LoopKind::Outer);
    assert_eq!(good.loops[0].verts.len(), 3);
}

#[test]
fn features_of_caches_by_content_not_by_frame() {
    let world = new_world();
    let e = model::spawn(&world, "cached");
    model::set_facets(&world, &e, tris(&box_rows(0.0, 0.0, 0.0, 2.0, 2.0, 1.0)));
    let f1 = model::features_of(&world, &e);
    let f2 = model::features_of(&world, &e);
    assert!(std::sync::Arc::ptr_eq(&f1.unwrap(), &f2.unwrap()), "second ask is a cache hit");
    // Moving the model must NOT recompute: ids resolve in the local frame.
    model::set_location(&world, &e, Point3::new(250.0, -70.0, 3.0));
    let f3 = model::features_of(&world, &e).unwrap();
    let f2 = model::features_of(&world, &e).unwrap();
    assert!(std::sync::Arc::ptr_eq(&f2, &f3), "translation is not new content");
    // New facets are new content: a different Arc, different ids.
    model::set_facets(&world, &e, tris(&box_rows(0.0, 0.0, 0.0, 2.0, 2.0, 2.0)));
    let f4 = model::features_of(&world, &e).unwrap();
    assert!(!std::sync::Arc::ptr_eq(&f3, &f4));
    assert_eq!(f4.faces.len(), 6);
}

// ===================== edge chains (step 3) =====================

/// A flat n-sided fan disc at z=0, radius r (open underside - a face,
/// not a solid, is all the rim test needs).
fn ngon_rows(n: usize, r: f64) -> Vec<[f64; 3]> {
    let tau = std::f64::consts::TAU;
    let p = |i: usize| {
        let a = tau * i as f64 / n as f64;
        [r * a.cos(), r * a.sin(), 0.0]
    };
    let mut rows = Vec::new();
    for i in 0..n {
        rows.push([0.0, 0.0, 0.0]);
        rows.push(p(i));
        rows.push(p(i + 1));
    }
    rows
}

/// A SOLID n-gon prism: radius r, height h, outward winding. Unlike
/// `ngon_rows` it has wall facets, and walls are what flank a rim - the
/// evidence the smooth-band rule reads.
fn prism_rows(n: usize, r: f64, h: f64) -> Vec<[f64; 3]> {
    let tau = std::f64::consts::TAU;
    let p = |i: usize, z: f64| {
        let a = tau * i as f64 / n as f64;
        [r * a.cos(), r * a.sin(), z]
    };
    let mut rows = Vec::new();
    for i in 0..n {
        // Caps as fans, wound so the normal points out of the solid.
        rows.push([0.0, 0.0, h]);
        rows.push(p(i, h));
        rows.push(p(i + 1, h));
        rows.push([0.0, 0.0, 0.0]);
        rows.push(p(i + 1, 0.0));
        rows.push(p(i, 0.0));
        rows.extend(quad(p(i, 0.0), p(i + 1, 0.0), p(i + 1, h), p(i, h)));
    }
    rows
}

#[test]
fn ten_gon_rim_chains_closed_by_smooth_band() {
    // A solid 10-gon prism: 36-degree chord turns beat the plain 30
    // -degree tangent rule - a coarse STEP bore rim, where adaptive
    // tessellation puts a hard turn at the seam. But the wall strips
    // sit 36 degrees apart, inside one smooth patch, so the band rule
    // closes both rims into single pickable loops.
    let f = features_of_facets(&tris(&prism_rows(10, 12.0, 5.0)));
    let cs = chains(&f);
    let closed: Vec<&Chain> = cs.iter().filter(|c| c.closed).collect();
    assert_eq!(closed.len(), 2, "top and bottom rim each loop closed");
    for c in &closed {
        assert_eq!(c.edges.len(), 10);
    }
    assert_eq!(cs.len(), 12, "two rings plus ten singleton verticals");
    assert!(
        cs.iter()
            .filter(|c| !c.closed)
            .all(|c| c.edges.len() == 1 && !c.closed),
        "verticals never chain: the rim flanks in the cap's patch, the \
         vertical in the walls' - different patches, no band"
    );
}

#[test]
fn box_edges_never_chain() {
    let f = features_of_facets(&tris(&box_rows(0.0, 0.0, 0.0, 2.0, 3.0, 1.0)));
    let cs = chains(&f);
    assert_eq!(cs.len(), 12, "every box corner turns 90 degrees: nothing chains");
    assert!(cs.iter().all(|c| c.edges.len() == 1 && !c.closed));
}

#[test]
fn sixteen_gon_rim_is_one_closed_chain() {
    let rows = ngon_rows(16, 12.0);
    let f = features_of_facets(&tris(&rows));
    let cs = chains(&f);
    assert_eq!(cs.len(), 1);
    assert!(cs[0].closed, "22.5-degree chord turns pass the 30-degree rule");
    assert_eq!(cs[0].edges.len(), 16);
    // Chain ids must not care about STEP's facet order.
    let mut back = rows.clone();
    back.reverse();
    let cs2 = chains(&features_of_facets(&tris(&back)));
    assert_eq!(cs[0].id, cs2[0].id);
}

#[test]
fn collinear_crease_line_chains_into_one() {
    // A flat 3x1 sheet (three quads) meeting a standing sheet at 90
    // degrees along the y=0 edge: the seam is three collinear crease
    // edges and must read as one pickable chain.
    let mut rows = Vec::new();
    for i in 0..3 {
        let x0 = i as f64;
        rows.extend(quad(
            [x0, 0.0, 0.0],
            [x0 + 1.0, 0.0, 0.0],
            [x0 + 1.0, 1.0, 0.0],
            [x0, 1.0, 0.0],
        ));
        rows.extend(quad(
            [x0, 0.0, 0.0],
            [x0 + 1.0, 0.0, 0.0],
            [x0 + 1.0, 0.0, 1.0],
            [x0, 0.0, 1.0],
        ));
    }
    let f = features_of_facets(&tris(&rows));
    let cs = chains(&f);
    let crease: Vec<&Chain> = cs
        .iter()
        .filter(|c| c.edges.iter().all(|&i| f.edges[i as usize].kind == EdgeKind::Crease))
        .collect();
    assert_eq!(crease.len(), 1, "the seam is one chain, not three segments");
    assert_eq!(crease[0].edges.len(), 3);
    assert!(!crease[0].closed);
}

#[test]
fn coarse_nine_gon_fragments_the_documented_limit() {
    let f = features_of_facets(&tris(&ngon_rows(9, 12.0)));
    let cs = chains(&f);
    assert_eq!(
        cs.len(),
        9,
        "40-degree turns beat the 30-degree tangent rule, and a sheet \
         rim has no flanking facets to carry a smooth band: the \
         documented coarse-sheet limit stands. Tessellated SOLID rims \
         (the STEP bore case) do chain closed - see \
         ten_gon_rim_chains_closed_by_smooth_band."
    );
    assert!(cs.iter().all(|c| c.edges.len() == 1 && !c.closed));
}

// ===================== welded patches (M1.5.2 step 2) =====================

#[test]
fn welded_cylinder_walls_report_one_patch() {
    // The hand-test complaint: a raycast onto a faceted bore answers
    // with the strip under the cursor, not the wall. The weld is the
    // patch: faces sharing one `facet_patch` root, more than one of
    // them. The decagon's wall strips sit 36 degrees apart, inside one
    // smooth patch; the caps are single planar strips and stay plain
    // faces - nothing flat is re-keyed.
    let f = features_of_facets(&tris(&prism_rows(10, 12.0, 5.0)));
    let ps = patches(&f);
    assert_eq!(ps.len(), 1, "one weld: the wall, not the caps");
    let wall: Vec<u64> = f
        .faces
        .iter()
        .filter(|fa| fa.normal[2].abs() < 0.5)
        .map(|fa| fa.id)
        .collect();
    assert_eq!(wall.len(), 10, "ten vertical strips");
    assert_eq!(ps[0].faces.len(), 10);
    assert!(
        ps[0].faces.iter().all(|id| wall.contains(id)),
        "the patch is exactly the wall faces"
    );
    // The boundary: seams weld away, rims stay - two closed decagons.
    let rings = patch_rings(&f, ps[0].id).expect("resolvable by id");
    assert_eq!(rings.len(), 2, "top and bottom rim");
    assert!(
        rings.iter().all(|r| r.closed && r.verts.len() == 10),
        "each ring walks the ten rim vertices, closed"
    );
    // An ordinary box has no weld: every face is one planar strip.
    let b = features_of_facets(&tris(&box_rows(0.0, 0.0, 0.0, 2.0, 3.0, 1.0)));
    assert!(patches(&b).is_empty(), "a box re-keys nothing");
    assert!(patch_rings(&b, 0xdead_beef).is_none(), "no such weld");
}

#[test]
fn patch_ids_survive_a_reimport() {
    // The id is boundary segments and strip planes at lattice positions,
    // never face or facet indices - STEP may hand the shape table back in
    // any order and the weld keeps its name (the pick contract).
    let rows = prism_rows(10, 12.0, 5.0);
    let mut back = rows.clone();
    back.reverse();
    let a = patches(&features_of_facets(&tris(&rows)));
    let b = patches(&features_of_facets(&tris(&back)));
    assert_eq!(a.len(), 1);
    assert_eq!(b.len(), 1);
    assert_eq!(a[0].id, b[0].id, "reordered facets, same patch id");
    // A different weld on the same part is a different id.
    let t = patches(&features_of_facets(&tris(&prism_rows(10, 12.0, 6.0))));
    assert_eq!(t.len(), 1);
    assert_ne!(t[0].id, a[0].id, "the wall grew a millimeter: new geometry, new id");
}
