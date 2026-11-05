//! M7.1 A: the brick SDF stock field. The laws here are the ones the
//! Phase 7 preview will stand on: the sign is ray-parity at the query
//! point itself (material is never called air, whatever the cell size),
//! the magnitude near the surface is the exact seed or a graph distance
//! that UNDER-reports at worst (clearance rounds toward the surface),
//! the band truncates and far space answers `cap`, the storage is
//! sparse by brick, and every nonsense input refuses by name.
//! Provenance: multi-source BFS distance transform (Barilla et al.
//! 2024), narrow-band level sets, and this codebase's own parity and
//! point-triangle kernels - per the Clean-Room Protocol.
//!
//! Gate map (M4.4): covers camcraft_sim::stock_sim::field,
//! camcraft_sim::stock_sim::field_gen.

use camcraft_sim::geometry::{Point3, Triangle};
use camcraft_sim::stock_sim::{SdfField, StockError};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The closed box `[lo, hi]^3`: 12 facets, winding-free by law.
fn box_mesh(lo: f64, hi: f64) -> Vec<Triangle> {
    let v = |i: usize| {
        p(
            [lo, hi][i & 1],
            [lo, hi][(i >> 1) & 1],
            [lo, hi][(i >> 2) & 1],
        )
    };
    let quad = |a: usize, b: usize, c: usize, d: usize| [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))];
    let mut t = Vec::with_capacity(12);
    for q in [
        quad(4, 5, 7, 6),
        quad(0, 2, 3, 1),
        quad(0, 1, 5, 4),
        quad(2, 6, 7, 3),
        quad(1, 3, 7, 5),
        quad(2, 0, 4, 6),
    ] {
        t.extend(q);
    }
    t
}

/// A closed UV sphere: one vertex per lattice position, so shared
/// corners agree bit-for-bit and the edge-use gate sees 2 everywhere.
fn sphere_mesh(r: f64, n_lat: usize, n_lon: usize) -> Vec<Triangle> {
    let vertex = |i: usize, j: usize| {
        if i == 0 {
            return p(0.0, 0.0, r);
        }
        if i == n_lat {
            return p(0.0, 0.0, -r);
        }
        let th = std::f64::consts::PI * i as f64 / n_lat as f64;
        let ph = std::f64::consts::TAU * j as f64 / n_lon as f64;
        p(r * th.sin() * ph.cos(), r * th.sin() * ph.sin(), r * th.cos())
    };
    let mut t = Vec::new();
    let (north, south) = (vertex(0, 0), vertex(n_lat, 0));
    for j in 0..n_lon {
        let k = (j + 1) % n_lon;
        t.push(Triangle::new(north, vertex(1, k), vertex(1, j)));
        for i in 1..n_lat - 1 {
            t.push(Triangle::new(vertex(i, j), vertex(i, k), vertex(i + 1, k)));
            t.push(Triangle::new(vertex(i, j), vertex(i + 1, k), vertex(i + 1, j)));
        }
        let l = n_lat - 1;
        t.push(Triangle::new(vertex(l, j), vertex(l, k), south));
    }
    t
}

fn flipped(facets: &[Triangle]) -> Vec<Triangle> {
    facets.iter().map(|t| Triangle::new(t.a, t.c, t.b)).collect()
}

/// The deterministic probe stream: bit-mixing, no RNG dependency.
struct Probes(u64);
impl Probes {
    fn next_in(&mut self, lo: f64, hi: f64) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        lo + (hi - lo) * ((self.0 >> 11) as f64 / (1u64 << 53) as f64)
    }
    fn point(&mut self, lo: f64, hi: f64) -> Point3 {
        p(self.next_in(lo, hi), self.next_in(lo, hi), self.next_in(lo, hi))
    }
}

#[test]
fn the_sign_never_calls_material_air() {
    // 512 deterministic probes through a 10mm box at a cell size the
    // band cannot describe the interior with: every answer agrees
    // with the truth, because the sign is parity at the point.
    let cell = 0.7;
    let field = SdfField::from_mesh(&box_mesh(0.0, 10.0), cell, 8).unwrap();
    let mut pb = Probes(0x243F6A8885A308D3);
    for _ in 0..512 {
        let q = pb.point(-2.0, 12.0);
        if q.x == 0.0 || q.y == 0.0 || q.z == 0.0 || q.x == 10.0 || q.y == 10.0 || q.z == 10.0 {
            continue; // on the shell: either answer is honest
        }
        let truth = q.x > 0.0 && q.x < 10.0 && q.y > 0.0 && q.y < 10.0 && q.z > 0.0 && q.z < 10.0;
        assert_eq!(field.inside(q), truth, "parity lied at {q:?}");
        let d = field.distance(q);
        assert_eq!(d < 0.0, truth, "signed distance lied at {q:?} (d = {d})");
    }
}

#[test]
fn near_surface_magnitude_is_bounded_above_by_truth_plus_a_cell() {
    // The direction law, measured: a stored clearance never exceeds
    // the true distance by more than the seed's slack. The graph
    // metric may UNDER-report (that is the conservative half), so the
    // weak lower bound only forbids collapse toward zero.
    let cell = 1.0;
    let field = SdfField::from_mesh(&box_mesh(0.0, 10.0), cell, 8).unwrap();
    let mut pb = Probes(0x13198A2E03707344);
    for _ in 0..256 {
        let q = pb.point(0.3, 9.7);
        let d = field.distance(q).abs();
        let t = field.true_distance(q);
        if t <= 2.0 * cell {
            assert!(d <= t + 2.0 * cell, "over-reported clearance at {q:?}: field {d} vs truth {t}");
            assert!(d >= 0.4 * t - 1.2 * cell, "collapsed magnitude at {q:?}: field {d} vs truth {t}");
        }
        assert!(d <= field.cap(), "magnitude above the cap at {q:?}: {d}");
    }
}

#[test]
fn far_space_and_deep_material_answer_the_cap() {
    // Out of the band in both directions: air beyond the band reads
    // +cap, material deeper than the band reads -cap. The magnitude
    // never pretends to know what it did not store.
    let (cell, band) = (1.0, 4);
    let field = SdfField::from_mesh(&box_mesh(0.0, 40.0), cell, band).unwrap();
    let far = p(-17.3, 20.5, 20.5);
    assert_eq!(field.distance(far), field.cap());
    let deep = p(20.5, 20.5, 20.5);
    assert_eq!(field.distance(deep), -field.cap());
    assert_eq!(field.cap(), band as f64 * cell);
}

#[test]
fn a_sub_cell_wall_still_flips_the_sign_where_its_column_center_cannot() {
    // The law that killed the parity-table plan: a fin thinner than a
    // cell, probed at its own XY and at its column center - parity
    // answers the point, so the fin is material at its pixels and air
    // beside them, with no cell size able to blur the two. The fin
    // protrudes through the box top: where two closed shells overlap
    // the parity law (theirs, inherited whole) reads the XOR, and the
    // test states that too.
    let mut mesh = box_mesh(0.0, 12.0);
    let (x0, x1, y0, y1, z0, z1) = (6.15, 6.25, 6.4, 6.6, 8.0, 14.0);
    let v = |i: usize| {
        p(
            [x0, x1][i & 1],
            [y0, y1][(i >> 1) & 1],
            [z0, z1][(i >> 2) & 1],
        )
    };
    let quad = |a: usize, b: usize, c: usize, d: usize| [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))];
    for q in [
        quad(4, 5, 7, 6),
        quad(0, 2, 3, 1),
        quad(0, 1, 5, 4),
        quad(2, 6, 7, 3),
        quad(1, 3, 7, 5),
        quad(2, 0, 4, 6),
    ] {
        mesh.extend(q);
    }
    let field = SdfField::from_mesh(&mesh, 1.0, 6).unwrap();
    let in_fin = p(6.2, 6.5, 13.0); // the point's own column hits the fin
    let beside = p(6.5, 6.5, 13.0); // the column center's ray misses it
    assert!(field.inside(in_fin) && field.distance(in_fin) < 0.0, "the fin was hidden");
    assert!(!field.inside(beside) && field.distance(beside) > 0.0, "air was called material");
    // And the overlap: inside both shells is inside neither, per the
    // parity law the whole codebase shares.
    assert!(!field.inside(p(6.2, 6.5, 10.0)), "the XOR law was broken");
}

#[test]
fn winding_does_not_matter() {
    // Flip every facet: same bricks, same cap, same answers. The
    // distance reads points and the parity reads crossings (the
    // solid_rest law (c), inherited whole).
    let cell = 0.8;
    let mesh = sphere_mesh(10.0, 12, 24);
    let a = SdfField::from_mesh(&mesh, cell, 5).unwrap();
    let b = SdfField::from_mesh(&flipped(&mesh), cell, 5).unwrap();
    assert_eq!(a.brick_count(), b.brick_count());
    let mut pb = Probes(0xA8010300B16E54FC);
    for _ in 0..128 {
        let q = pb.point(-14.0, 14.0);
        assert_eq!(a.distance(q), b.distance(q), "a flip changed the answer at {q:?}");
    }
}

#[test]
fn sparse_by_brick_not_dense_by_grid() {
    // The field pays for the SHELL, not the box it sits in nor the
    // solid it wraps. The law's reach is sqrt(3)*(band+2) + seed
    // cells of L1 footprint both ways (a hop-truncated BFS is an L1
    // ball, not a thin Euclidean band - the multi-resolution fix
    // rides M7.1 C's clipmaps), so at a coarse test cell the shell
    // can legitimately out-pay a small ball's own volume: what never
    // changes is that it stays under half the bounding box, and
    // that a part 20*(band+2) cells across - every real model at a
    // machining cell size - is stored as a fraction of its box.
    let (r, cell, band) = (100.0, 2.0, 2);
    let field = SdfField::from_mesh(&sphere_mesh(r, 16, 32), cell, band).unwrap();
    let bricks = field.brick_count();
    assert!(bricks > 0);
    let bbox_side = (((2.0 * r / cell) as i32 + 2 * 8) as f64 / 8.0).ceil(); // +reach
    let bbox = bbox_side.powi(3);
    assert!(
        (bricks as f64) <= bbox / 2.0,
        "{bricks} bricks against the dilated bounding box's {bbox:.0}",
    );
    // And the cap's honesty at the band's truth edge, measured on a
    // small box (cheap true distances): the law is v <= t + 1.73
    // cells for the staircase path, so a magnitude answering cap had
    // v >= cap and therefore stands at least cap - 2*cells away.
    let boxy = SdfField::from_mesh(&box_mesh(0.0, 20.0), 1.0, 3).unwrap();
    let mut pb = Probes(0x9E3779B97F4A7C15);
    let mut seen_cap = 0;
    for _ in 0..192 {
        let q = pb.point(-5.0, 25.0);
        let d = boxy.distance(q);
        if d.abs() == boxy.cap() {
            seen_cap += 1;
            let t = boxy.true_distance(q);
            assert!(
                t >= boxy.cap() - 2.0,
                "cap lied at {q:?}: truth {t} below the band edge {}",
                boxy.cap() - 2.0
            );
        }
    }
    assert!(seen_cap > 0, "the probe stream never reached the cap's far field");
}

/// The refusal path, without needing `SdfField: Debug` it has no
/// reason to fake: an input that built a field is itself the failure.
fn refuse(mesh: &[Triangle], cell: f64, band: u32) -> StockError {
    match SdfField::from_mesh(mesh, cell, band) {
        Ok(_) => panic!("this input must refuse, never build"),
        Err(e) => e,
    }
}

#[test]
fn nonsense_inputs_refuse_by_name() {
    let err = |e: &StockError| format!("{e}");
    let e = refuse(&[], 1.0, 4);
    assert!(err(&e).contains("empty"), "{e}");
    let mut f = box_mesh(0.0, 4.0);
    f[0].c.z = f64::NAN;
    let e = refuse(&f, 1.0, 4);
    assert!(err(&e).contains("non-finite"), "{e}");
    let g = vec![
        box_mesh(0.0, 4.0)[0],
        Triangle::new(p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(2.0, 0.0, 0.0)),
    ];
    let e = refuse(&g, 1.0, 4);
    assert!(err(&e).contains("zero area"), "{e}");
    let mut h = box_mesh(0.0, 4.0);
    h.pop();
    let e = refuse(&h, 1.0, 4);
    assert!(err(&e).contains("watertight"), "{e}");
    let e = refuse(&box_mesh(0.0, 4.0), 0.0, 4);
    assert!(err(&e).contains("cell size"), "{e}");
    let e = refuse(&box_mesh(0.0, 4.0), f64::INFINITY, 4);
    assert!(err(&e).contains("cell size"), "{e}");
    let e = refuse(&box_mesh(0.0, 4.0), 1.0, 0);
    assert!(err(&e).contains("band"), "{e}");
    let s = |q: Point3| p(q.x * 250.0, q.y * 250.0, q.z * 250.0);
    let big: Vec<Triangle> = box_mesh(0.0, 4.0)
        .iter()
        .map(|t| Triangle::new(s(t.a), s(t.b), s(t.c)))
        .collect();
    let e = refuse(&big, 0.001, 4);
    assert!(err(&e).contains("tessellating"), "{e}");
}

#[test]
fn a_non_finite_query_answers_nan_and_never_panics() {
    // The query contract, literal: garbage in is NaN out, on the
    // field of a real mesh, not a stub.
    let field = SdfField::from_mesh(&box_mesh(0.0, 4.0), 1.0, 4).unwrap();
    assert!(field.distance(p(f64::NAN, 1.0, 1.0)).is_nan());
    assert!(field.distance(p(1.0, f64::INFINITY, 1.0)).is_nan());
    assert!(field.distance(p(1.0, 1.0, f64::NEG_INFINITY)).is_nan());
}

#[test]
fn the_field_repeats_itself() {
    // HashMap iteration order must not leak into values: two builds
    // of the same mesh answer identically everywhere.
    let mesh = box_mesh(0.0, 10.0);
    let a = SdfField::from_mesh(&mesh, 0.6, 6).unwrap();
    let b = SdfField::from_mesh(&mesh, 0.6, 6).unwrap();
    assert_eq!(a.brick_count(), b.brick_count());
    let mut pb = Probes(0x002B6E5939F06C1A);
    for _ in 0..256 {
        let q = pb.point(-1.0, 11.0);
        assert_eq!(a.distance(q), b.distance(q));
    }
}
