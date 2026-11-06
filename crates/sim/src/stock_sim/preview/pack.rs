//! The lattice, flattened: what the GPU trace binds instead of the
//! field's sparse bricks. Sphere tracing on the compute pass needs
//! one trilinear read per sample, so the CPU side materializes the
//! exact scalars `march::Eye::dist` derives at its corners - the
//! signed band magnitude and the clamped cut layer, per cell - and
//! the kernel interpolates them the same z-then-x-then-y way. The
//! pack adds no law: it is the field's own values, indexed densely,
//! with one documented rounding (f64 -> f32 texels).
//!
//! Size gate: the pack is dense where the field is sparse, so a
//! lattice over `MAX_TEXELS` cells answers `None` and the caller
//! keeps the CPU march (or the sheet) - a viewport that refuses to
//! pack still never renders a black stock.

use std::collections::HashMap;

use crate::geometry::Point3;
use crate::stock_sim::field::{cell_of, center_of, signed, SdfField};

/// Dense-lattice budget, texels per layer: 8M f32 texels = 32 MB,
/// the point where a desktop upload costs a frame and a laptop
/// starts paging. Above it the CPU march is the honest viewport.
pub const MAX_TEXELS: usize = 1 << 23;

/// Where the lattice lives and how to read it: cell size, the index
/// of texel (0,0,0), the per-axis texel count, the shell's AABB and
/// the cap the far field answers with. `world` (the trace's box) is
/// derived by the caller as `solid` expanded by `cap + cell` - the
/// same slab `preview()` hands `march::trace`.
#[derive(Clone, Debug)]
pub struct LatticeMeta {
    pub cell: f64,
    pub origin: [i32; 3],
    pub dims: [u32; 3],
    pub solid: ([f64; 3], [f64; 3]),
    pub cap: f64,
}

impl LatticeMeta {
    /// The trace's world box: `solid` padded by `cap + cell`, the same
    /// slab `preview()` hands `march::trace`.
    #[must_use]
    pub fn world(&self) -> ([f64; 3], [f64; 3]) {
        let pad = self.cap + self.cell;
        let (lo, hi) = self.solid;
        (
            [lo[0] - pad, lo[1] - pad, lo[2] - pad],
            [hi[0] + pad, hi[1] + pad, hi[2] + pad],
        )
    }

    /// World size gate: a region that cannot fit the budget.
    fn fits(&self) -> bool {
        let [a, b, c] = self.dims;
        (a as usize)
            .checked_mul(b as usize)
            .and_then(|n| n.checked_mul(c as usize))
            .is_some_and(|n| n <= MAX_TEXELS)
    }
}

/// The machined stock as three layers of texels: the signed mesh
/// magnitude and the clamped cut (whose max the march traces), kept
/// separate so the kernel's trilinear matches `Eye::dist` term by
/// term rather than blending a value the march never sampled.
#[derive(Clone, Debug)]
pub struct FieldLattice {
    pub meta: LatticeMeta,
    pub mesh: Vec<f32>,
    pub cut: Vec<f32>,
}

/// The deviation target's signed distance per cell - what the GPU
/// classifies with, the lattice's answer to the CPU path's exact
/// `true_distance`. Near a surface the band magnitudes are exact,
/// so the two agree there; far from it the class gate sits wider
/// than either says anything interesting.
#[derive(Clone, Debug)]
pub struct DistanceLattice {
    pub meta: LatticeMeta,
    pub d: Vec<f32>,
}

/// The cell span the pack covers: the shell's cells plus one ring
/// of margin, so a trilinear at any point inside `solid` reads four
/// real neighbours and the clamp-to-edge corners only answer where
/// the march's own `cap` would stand.
fn region(f: &SdfField) -> Option<LatticeMeta> {
    let b = f.bounds();
    if !(b.min.is_finite() && b.max.is_finite()) {
        return None;
    }
    let cell = f.cell();
    // The band's own horizon: values stored past the shell reach
    // `cap + 2 cells` and everything beyond answers cap by the
    // field's law, so texels clamped at this margin read the same
    // constants the march's unallocated-brick path returns.
    let m = (f.cap() / cell).ceil() as i32 + 2;
    let lo = [
        cell_of(b.min.x, cell).checked_sub(m)?,
        cell_of(b.min.y, cell).checked_sub(m)?,
        cell_of(b.min.z, cell).checked_sub(m)?,
    ];
    let hi = [
        cell_of(b.max.x, cell).checked_add(m)?,
        cell_of(b.max.y, cell).checked_add(m)?,
        cell_of(b.max.z, cell).checked_add(m)?,
    ];
    let dims = [
        u32::try_from(hi[0] - lo[0] + 1).ok()?,
        u32::try_from(hi[1] - lo[1] + 1).ok()?,
        u32::try_from(hi[2] - lo[2] + 1).ok()?,
    ];
    let meta = LatticeMeta {
        cell,
        origin: lo,
        dims,
        solid: ([b.min.x, b.min.y, b.min.z], [b.max.x, b.max.y, b.max.z]),
        cap: f.cap(),
    };
    meta.fits().then_some(meta)
}

/// Texel offset of a cell key, clamped into the lattice: reads past
/// the margin land on the shell-adjacent border texels, whose
/// magnitudes answer `cap` by the field's own out-of-band law.
fn slot(meta: &LatticeMeta, key: [i32; 3]) -> usize {
    let i = |axis: usize| {
        (key[axis] - meta.origin[axis]).clamp(0, meta.dims[axis] as i32 - 1) as usize
    };
    (i(2) * meta.dims[1] as usize + i(1)) * meta.dims[0] as usize + i(0)
}

/// Fill one layer with a per-cell scalar, scanning columns once for
/// the shell parity (the same acceleration `march::Eye` caches).
fn fill(meta: &LatticeMeta, mut value: impl FnMut(&mut HashMap<[i32; 2], Option<Vec<f64>>>, [i32; 3]) -> f64) -> Vec<f32> {
    let mut cols: HashMap<[i32; 2], Option<Vec<f64>>> = HashMap::new();
    let mut out = vec![0.0f32; (meta.dims[2] as usize) * (meta.dims[1] as usize) * (meta.dims[0] as usize)];
    for (k, iz) in (meta.origin[2]..meta.origin[2] + meta.dims[2] as i32).enumerate() {
        for (j, iy) in (meta.origin[1]..meta.origin[1] + meta.dims[1] as i32).enumerate() {
            for (i, ix) in (meta.origin[0]..meta.origin[0] + meta.dims[0] as i32).enumerate() {
                let v = value(&mut cols, [ix, iy, iz]);
                out[(k * meta.dims[1] as usize + j) * meta.dims[0] as usize + i] = v as f32;
            }
        }
    }
    out
}

/// The machined stock: `signed(mag, parity)` wearing the cut clamp,
/// layer by layer, exactly as `march::Eye::dist` composes them.
pub fn pack_field(f: &SdfField) -> Option<FieldLattice> {
    let meta = region(f)?;
    let mesh = fill(&meta, |cols, key| {
        signed(f.mag_at(key), parity(cols, f, key, meta.cell))
    });
    let floor = -4.0 * f.cap();
    let cut = fill(&meta, |_, key| {
        let c = f.cut_at(key);
        if c == f64::NEG_INFINITY {
            floor
        } else {
            c
        }
    });
    Some(FieldLattice { meta, mesh, cut })
}

/// The deviation target: signed distance per cell, mesh layer only
/// (a target carries no cuts - that is the `preview()` seam's law).
pub fn pack_target(f: &SdfField) -> Option<DistanceLattice> {
    let meta = region(f)?;
    let d = fill(&meta, |cols, key| {
        signed(f.mag_at(key), parity(cols, f, key, meta.cell))
    });
    Some(DistanceLattice { meta, d })
}

/// Column parity at a cell center: one `shell_crossings` scan per
/// column, kept in the caller's cache.
fn parity(
    cols: &mut HashMap<[i32; 2], Option<Vec<f64>>>,
    f: &SdfField,
    key: [i32; 3],
    cell: f64,
) -> bool {
    let ck = [key[0], key[1]];
    let e = cols.entry(ck).or_insert_with(|| f.shell_crossings(center_of(ck[0], cell), center_of(ck[1], cell)));
    SdfField::parity_of(e, center_of(key[2], cell))
}

/// Trilinear at normalized lattice coords, corner order and blend
/// order copied from `march::Eye::dist` (slot `dx + 2*dy + 4*dz`,
/// blend z, then x, then y). `g` answers the texel at a cell key.
fn trilinear(meta: &LatticeMeta, p: Point3, g: impl Fn([i32; 3]) -> f64) -> f64 {
    let cell = meta.cell;
    let (fx, fy, fz) = (p.x / cell - 0.5, p.y / cell - 0.5, p.z / cell - 0.5);
    let (ix, iy, iz) = (fx.floor() as i32, fy.floor() as i32, fz.floor() as i32);
    let (wx, wy, wz) = (fx - fx.floor(), fy - fy.floor(), fz - fz.floor());
    let lerp = |a: f64, b: f64, w: f64| a + (b - a) * w;
    let c = |dx: i32, dy: i32, dz: i32| g([ix + dx, iy + dy, iz + dz]);
    let m = |dx: i32, dy: i32| lerp(c(dx, dy, 0), c(dx, dy, 1), wz);
    lerp(lerp(m(0, 0), m(1, 0), wx), lerp(m(0, 1), m(1, 1), wx), wy)
}

impl FieldLattice {
    /// The marched distance at `p`, f32-rounded: the kernel's law
    /// stated in Rust (what it binds, and what the parity tests
    /// check the WGSL against).
    #[must_use]
    pub fn dist(&self, p: Point3) -> f64 {
        if !p.is_finite() {
            return f64::NAN;
        }
        // The march blends the layers THEN maxes; maxing per corner
        // first differs only where the two surfaces cross inside one
        // cell - the sub-cell slack the field header already names.
        let mesh = trilinear(&self.meta, p, |k| self.mesh[slot(&self.meta, k)] as f64);
        let cut = trilinear(&self.meta, p, |k| self.cut[slot(&self.meta, k)] as f64);
        mesh.max(cut)
    }
}

impl DistanceLattice {
    /// The target's signed distance at `p`, lattice-read.
    #[must_use]
    pub fn signed_at(&self, p: Point3) -> f64 {
        if !p.is_finite() {
            return f64::NAN;
        }
        trilinear(&self.meta, p, |k| self.d[slot(&self.meta, k)] as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::cube;
    use crate::stock_sim::backend::FIELD_BAND;

    fn box_field(lo: Point3, hi: Point3, cell: f64) -> SdfField {
        let f = cube(lo, hi);
        SdfField::from_mesh(&f.facets, cell, FIELD_BAND).unwrap()
    }

    /// A deterministic scatter - no rand dependency, same points
    /// every run, so a pass here is a pass forever.
    fn scatter(n: usize, seed: u64) -> Vec<Point3> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let r = |k: u32| ((s >> k) & 0xFFFFFF) as f64 / 0xFFFFFF as f64;
                Point3::new(r(0) * 12.0 - 1.0, r(8) * 12.0 - 1.0, r(16) * 12.0 - 1.0)
            })
            .collect()
    }

    #[test]
    fn the_pack_reproduces_the_march_law() {
        let f = box_field(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 5.0), 1.0);
        let mut eye = crate::stock_sim::preview::march::Eye {
            f: &f,
            cols: HashMap::new(),
            solid: (
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(10.0, 10.0, 5.0),
            ),
            world: (
                Point3::new(-6.0, -6.0, -6.0),
                Point3::new(16.0, 16.0, 11.0),
            ),
            cell: f.cell(),
            eps: f.cell() / 64.0,
        };
        let lat = pack_field(&f).expect("the box packs");
        for p in scatter(512, 7) {
            let a = eye.dist(p);
            let b = lat.dist(p);
            assert!(
                (a - b).abs() <= 1e-5 * f.cap().max(1.0) + 1e-9,
                "pack diverged at {p:?}: march {a}, lattice {b}"
            );
        }
    }

    #[test]
    fn cuts_ride_the_pack() {
        use crate::cl::{Move, MoveKind};
        use crate::geometry::cube;
        use crate::stock_sim::{Backend, SimOp, Stock};
        let c = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 5.0));
        let mut stock = Stock::from_facets(Backend::Field { band: FIELD_BAND }, &c.facets, 1.0).unwrap();
        let mv = vec![Move {
            kind: MoveKind::Feed,
            from: Point3::new(5.0, 5.0, 6.0),
            to: Point3::new(5.0, 5.0, 2.0),
            feed: Some(500.0),
            arc: None,
            axis: None,
        }];
        stock
            .simulate(&[SimOp { moves: &mv, radius: 2.0, ball: false, cone: None, corner: None }])
            .unwrap();
        let g = stock.field().unwrap();
        let lat = pack_field(g).expect("a cut field packs");
        let mut eye = super::super::march::Eye {
            f: g,
            cols: HashMap::new(),
            solid: (Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 5.0)),
            world: (Point3::new(-10.0, -10.0, -10.0), Point3::new(20.0, 20.0, 15.0)),
            cell: g.cell(),
            eps: g.cell() / 64.0,
        };
        // The cut layer rides the pack exactly as the march composes
        // it: pocket air answers lifted by the cut blend, everywhere
        // else the mesh term stands.
        for p in scatter(512, 11) {
            let a = eye.dist(p);
            let b = lat.dist(p);
            assert!(
                (a - b).abs() <= 1e-5 * g.cap().max(1.0) + 1e-9,
                "cut pack diverged at {p:?}: march {a}, lattice {b}"
            );
        }
    }

    #[test]
    fn the_gate_refuses_a_huge_region_by_size_not_by_crash() {
        // Two 1 mm cubes at opposite corners of a 220 mm span: the
        // sparse builder seeds only their ~24 facets (a long bar's
        // whole skin cost ~50-80 s in debug), while the dense region
        // still blows the budget - the pack, not the builder, is the
        // first thing to say no.
        let near = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 1.0, 1.0));
        let far = cube(Point3::new(220.0, 220.0, 220.0), Point3::new(221.0, 221.0, 221.0));
        let facets: Vec<_> = near.facets.iter().chain(far.facets.iter()).copied().collect();
        let f = SdfField::from_mesh(&facets, 1.0, FIELD_BAND).unwrap();
        assert!(pack_field(&f).is_none(), "~234^3 = 12.8M cells is past the 8M budget");
        assert!(pack_target(&f).is_none(), "the target pack shares the gate");
    }
}
