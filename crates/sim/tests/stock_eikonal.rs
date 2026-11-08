//! #22 (M7.1 / M13.1): one eikonal sweep re-normalizes the render
//! lattice's band before the render pass. The sweep must bring the
//! band's |grad phi| back to one after CSG-style cuts bent it, must
//! never move the surface (every texel keeps its sign, every texel at
//! the zero set keeps its value), must agree with the exact distance of
//! a plain cut within a cell, and must leave out-of-band texels alone.
//! Provenance: Zhao's fast sweeping (Math. Comp. 2005) checked against
//! closed-form box/cylinder distances; nothing from vendor binaries.
//!
//! Gate map (M4.4): covers camcraft_sim::stock_sim::preview::eikonal
//! (FieldLattice::renormalize) over pack_field.

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::{cube, Point3};
use camcraft_sim::stock_sim::preview::{pack_field, FieldLattice};
use camcraft_sim::stock_sim::{Backend, SimOp, Stock, FIELD_BAND};

const CELL: f64 = 0.5;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn feed(from: Point3, to: Point3) -> Move {
    Move { kind: MoveKind::Feed, from, to, feed: Some(500.0), arc: None, axis: None }
}

/// The 10 x 10 x 5 box stock.
fn box_stock() -> Stock {
    let c = cube(p(0.0, 0.0, 0.0), p(10.0, 10.0, 5.0));
    Stock::from_facets(Backend::Field { band: FIELD_BAND }, &c.facets, CELL).expect("box field")
}

/// A flat r-2 plunge at (5, 5) to z = 2: a cylinder pocket.
fn pocket(stock: &mut Stock) {
    let mv = [feed(p(5.0, 5.0, 8.0), p(5.0, 5.0, 2.0))];
    stock.simulate(&[SimOp { moves: &mv, radius: 2.0, ball: false, cone: None, corner: None }]).expect("cut");
}

/// Composed texel value, `max(mesh, cut)`: what the march reads at a centre.
fn composed(l: &FieldLattice) -> Vec<f32> {
    l.mesh.iter().zip(&l.cut).map(|(m, c)| m.max(*c)).collect()
}

fn centre(l: &FieldLattice, i: usize) -> Point3 {
    let [nx, ny, _] = l.meta.dims.map(|d| d as usize);
    let (x, y, z) = (i % nx, (i / nx) % ny, i / (nx * ny));
    let c = |k: usize, a: usize| (l.meta.origin[a] as f64 + k as f64 + 0.5) * l.meta.cell;
    p(c(x, 0), c(y, 1), c(z, 2))
}

/// Mean | |grad phi| - 1 | over interior band texels (central
/// differences, every 6-neighbour in band and of the same sign), plus
/// the share within 0.1 of one, and the count measured.
fn gradient_deviation(l: &FieldLattice) -> (f64, f64, usize) {
    let v = composed(l);
    let [nx, ny, nz] = l.meta.dims.map(|d| d as usize);
    let (cap, h) = (l.meta.cap as f32, l.meta.cell);
    let idx = |x: usize, y: usize, z: usize| (z * ny + y) * nx + x;
    let (mut sum, mut good, mut n) = (0.0, 0usize, 0usize);
    for z in 1..nz - 1 {
        for y in 1..ny - 1 {
            for x in 1..nx - 1 {
                let c = v[idx(x, y, z)];
                let nb = [
                    v[idx(x - 1, y, z)], v[idx(x + 1, y, z)],
                    v[idx(x, y - 1, z)], v[idx(x, y + 1, z)],
                    v[idx(x, y, z - 1)], v[idx(x, y, z + 1)],
                ];
                let band = |w: f32| w.abs() < cap - 1e-3 && (w < 0.0) == (c < 0.0) && w != 0.0;
                if !band(c) || !nb.iter().all(|w| band(*w)) {
                    continue;
                }
                // Off the medial axis only: where the two one-sided
                // differences of an axis disagree by half a slope, the
                // texel straddles a kink and |grad| is undefined there.
                let kink = (0..3).any(|k| {
                    let (bk, fw) = (f64::from(c - nb[2 * k]) / h, f64::from(nb[2 * k + 1] - c) / h);
                    (fw - bk).abs() > 0.5
                });
                if kink {
                    continue;
                }
                let g = |a: f32, b: f32| f64::from(b - a) / (2.0 * h);
                let m = (g(nb[0], nb[1]).powi(2) + g(nb[2], nb[3]).powi(2) + g(nb[4], nb[5]).powi(2)).sqrt();
                sum += (m - 1.0).abs();
                good += usize::from((m - 1.0).abs() < 0.1);
                n += 1;
            }
        }
    }
    (sum / n.max(1) as f64, good as f64 / n.max(1) as f64, n)
}

/// A box cut by three overlapping sweeps: a cylinder pocket, a ball
/// slot across it and a flat slot along the other side - the max of
/// several SDFs, the composition that bends the band.
fn csg_lattice() -> FieldLattice {
    let mut s = box_stock();
    pocket(&mut s);
    let slot = [feed(p(1.0, 5.0, 3.5), p(9.0, 5.0, 3.5))];
    let side = [feed(p(2.0, 1.5, 4.0), p(2.0, 8.5, 2.5))];
    s.simulate(&[
        SimOp { moves: &slot, radius: 1.5, ball: true, cone: None, corner: None },
        SimOp { moves: &side, radius: 1.0, ball: false, cone: None, corner: None },
    ])
    .expect("cuts");
    pack_field(s.field().expect("field arm")).expect("packs")
}

#[test]
fn csg_cuts_bend_the_band_and_one_sweep_straightens_it() {
    let mut l = csg_lattice();
    let (before, good_before, n0) = gradient_deviation(&l);
    let stats = l.renormalize().expect("sweep");
    let (after, good_after, n1) = gradient_deviation(&l);
    eprintln!("deviation {before:.3} -> {after:.3}; within 0.1: {good_before:.2} -> {good_after:.2}; texels {n0} -> {n1}; {stats:?}");
    assert!(n1 > 1000, "the band is measured over real texels: {n1}");
    assert!(after < 0.08, "mean | |grad| - 1 | after the sweep: {after}");
    assert!(after < 0.5 * before, "the sweep must at least halve the deviation: {before} -> {after}");
    assert!(good_after > 0.75 && good_before < 0.1, "most band texels read a unit gradient: {good_before} -> {good_after}");
    assert_eq!(stats.updates, stats.solved * 8, "one cycle of eight orders, bounded: {stats:?}");
}

#[test]
fn the_surface_never_moves() {
    let mut l = csg_lattice();
    let old = composed(&l);
    let stats = l.renormalize().expect("sweep");
    let new = composed(&l);
    let [nx, ny, nz] = l.meta.dims.map(|d| d as usize);
    let idx = |x: usize, y: usize, z: usize| (z * ny + y) * nx + x;
    for (i, (a, b)) in old.iter().zip(&new).enumerate() {
        assert_eq!(*a < 0.0, *b < 0.0, "texel {i} at {:?} flipped sign: {a} -> {b}", centre(&l, i));
    }
    // Every lattice edge that crosses the zero set keeps both values,
    // so the crossing a trilinear or an iso extraction reads is equal.
    let mut crossings = 0;
    for z in 0..nz {
        for y in 0..ny {
            for x in 0..nx {
                let i = idx(x, y, z);
                for j in [(x + 1 < nx).then(|| idx(x + 1, y, z)), (y + 1 < ny).then(|| idx(x, y + 1, z)), (z + 1 < nz).then(|| idx(x, y, z + 1))]
                    .into_iter()
                    .flatten()
                {
                    if (old[i] < 0.0) != (old[j] < 0.0) {
                        crossings += 1;
                        assert_eq!(old[i].to_bits(), new[i].to_bits(), "zero-set texel {i} moved");
                        assert_eq!(old[j].to_bits(), new[j].to_bits(), "zero-set texel {j} moved");
                    }
                }
            }
        }
    }
    assert!(crossings > 500 && stats.frozen > 0, "the cut box has a surface: {crossings} {stats:?}");
}

#[test]
fn out_of_band_texels_are_untouched_and_the_sweep_is_deterministic() {
    let mut l = csg_lattice();
    let (mesh0, cut0) = (l.mesh.clone(), l.cut.clone());
    let cap = l.meta.cap as f32;
    l.renormalize().expect("sweep");
    assert_eq!(l.cut, cut0, "the cut layer stays as packed");
    let old = mesh0.iter().zip(&cut0).map(|(m, c)| m.max(*c));
    for (i, (o, (m0, m1))) in old.zip(mesh0.iter().zip(&l.mesh)).enumerate() {
        if o.abs() >= cap {
            assert_eq!(m0.to_bits(), m1.to_bits(), "out-of-band texel {i} was written");
        }
        assert!(m1.abs() <= cap, "texel {i} past cap: {m1}");
    }
    let mut again = csg_lattice();
    again.renormalize().expect("sweep");
    assert_eq!(again.mesh, l.mesh, "same input, same bits");
}

#[test]
fn a_cylinder_pocket_reads_its_exact_distance_within_a_cell() {
    let mut s = box_stock();
    pocket(&mut s);
    let mut l = pack_field(s.field().expect("field arm")).expect("packs");
    l.renormalize().expect("sweep");
    let v = composed(&l);
    let (cap, cell) = (l.meta.cap, l.meta.cell);
    let (mut worst, mut n) = (0.0f64, 0usize);
    for (i, d) in v.iter().enumerate() {
        let q = centre(&l, i);
        let r = ((q.x - 5.0).powi(2) + (q.y - 5.0).powi(2)).sqrt();
        let in_box = (0.0..=10.0).contains(&q.x) && (0.0..=10.0).contains(&q.y) && (0.0..=5.0).contains(&q.z);
        // Closed forms: material inside the box (distance to the box's
        // faces or to the half-infinite r-2 cylinder above z = 2), and
        // pocket air below the top (the wall or the floor).
        let exact = if !in_box {
            continue;
        } else if r < 2.0 && q.z > 2.0 {
            (2.0 - r).min(q.z - 2.0)
        } else {
            let faces = q.x.min(10.0 - q.x).min(q.y).min(10.0 - q.y).min(q.z).min(5.0 - q.z);
            let cyl = ((r - 2.0).max(0.0).powi(2) + (2.0 - q.z).max(0.0).powi(2)).sqrt();
            -faces.min(cyl)
        };
        if exact.abs() > cap - 2.0 * cell {
            continue; // the cap clamp is the field's law, not the sweep's
        }
        worst = worst.max((f64::from(*d) - exact).abs());
        n += 1;
    }
    eprintln!("cylinder pocket: worst {worst:.3} over {n} texels (cell {cell})");
    assert!(n > 1000, "measured over the band: {n}");
    assert!(worst <= cell, "within one cell of the exact distance: {worst}");
}

#[test]
fn a_plane_reads_exact_after_the_sweep() {
    // The uncut box: under the top face, far from the others, the
    // distance is 5 - z; the BFS band under-reads it, the sweep does not.
    let mut l = pack_field(box_stock().field().expect("field arm")).expect("packs");
    l.renormalize().expect("sweep");
    let v = composed(&l);
    let mut n = 0;
    for (i, d) in v.iter().enumerate() {
        let q = centre(&l, i);
        let inner = q.x > 3.0 && q.x < 7.0 && q.y > 3.0 && q.y < 7.0 && q.z > 3.0 && q.z < 5.0;
        if inner {
            let exact = -(5.0 - q.z);
            assert!((f64::from(*d) - exact).abs() < 1e-4, "at {q:?}: {d} vs {exact}");
            n += 1;
        }
    }
    assert!(n > 0);
}

#[test]
fn a_mismatched_lattice_refuses_by_name() {
    let mut l = pack_field(box_stock().field().expect("field arm")).expect("packs");
    l.cut.pop();
    let before = l.mesh.clone();
    let e = l.renormalize().unwrap_err();
    assert!(format!("{e}").contains("eikonal"), "{e}");
    assert_eq!(l.mesh, before, "a refusal writes nothing");
}
