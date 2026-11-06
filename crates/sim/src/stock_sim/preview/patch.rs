//! Cut-layer patches (M13.1): what a playback picture re-sends when
//! only some bricks were cut since the last one. Provenance: plain
//! dirty-region tracking (the brick map's own granularity) over the
//! dense lattice `pack` defines; nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **A patch is the full pack, restricted.** Every texel a patch
//!   carries is `pack::cut_texel` at its cell - the same scalar the
//!   full pack writes - so a layer patched with every brick that
//!   changed equals a fresh `pack_cut` byte for byte.
//! - **Runs, not texels.** A brick is 64 x-rows of up to 8 texels in
//!   the dense layout; rows are sorted and merged (gaps up to
//!   `MERGE_GAP` texels are re-sent from current values, which is
//!   always correct) so the GPU sees few, larger writes.
//! - **Out-of-lattice bricks are nothing.** A brick outside the pack's
//!   extent moves no texel and contributes no run.

use std::collections::HashSet;

use super::pack::{cut_texel, LatticeMeta};
use crate::stock_sim::field::{SdfField, BRICK};

/// Texels between two runs that are re-sent rather than split.
const MERGE_GAP: usize = 16;

/// A sparse rewrite of a cut layer: ascending, disjoint texel runs and
/// their values, concatenated in run order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CutPatch {
    /// (first texel, texel count) per run.
    pub runs: Vec<(usize, usize)>,
    pub data: Vec<f32>,
}

impl CutPatch {
    /// Texels the patch carries.
    #[must_use]
    pub fn texels(&self) -> usize {
        self.data.len()
    }

    /// Bytes the patch uploads.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.data.len().saturating_mul(4)
    }

    /// Each run as (first texel, its values).
    pub fn chunks(&self) -> impl Iterator<Item = (usize, &[f32])> + '_ {
        let mut at = 0usize;
        self.runs.iter().filter_map(move |&(o, n)| {
            let s = self.data.get(at..at.checked_add(n)?)?;
            at += n;
            Some((o, s))
        })
    }

    /// True when every run lies inside a layer of `len` texels and the
    /// runs account for exactly the data carried.
    #[must_use]
    pub fn fits(&self, len: usize) -> bool {
        let sum = self.runs.iter().try_fold(0usize, |acc, &(o, n)| {
            (o.checked_add(n)? <= len).then_some(())?;
            acc.checked_add(n)
        });
        sum == Some(self.data.len())
    }

    /// Write the patch into a full layer; refuses (writing nothing) a
    /// patch that does not fit it.
    pub fn apply(&self, layer: &mut [f32]) -> Result<(), String> {
        if !self.fits(layer.len()) {
            return Err("cut patch: a run falls outside the layer".into());
        }
        for (o, s) in self.chunks() {
            if let Some(dst) = layer.get_mut(o..o + s.len()) {
                dst.copy_from_slice(s);
            }
        }
        Ok(())
    }
}

/// The cut-layer texels of `bricks` (brick keys) over `meta`'s extent,
/// valued from `f` as it stands now.
#[must_use]
pub fn pack_cut_patch(f: &SdfField, meta: &LatticeMeta, bricks: &HashSet<[i32; 3]>) -> CutPatch {
    let dims = meta.dims.map(i64::from);
    let org = meta.origin.map(i64::from);
    let b = i64::from(BRICK);
    let mut rows: Vec<(usize, usize)> = Vec::new();
    for bk in bricks {
        let span = |a: usize| {
            let lo = i64::from(bk[a]) * b - org[a];
            (lo.max(0), (lo + b - 1).min(dims[a] - 1))
        };
        let (x, y, z) = (span(0), span(1), span(2));
        if x.0 > x.1 || y.0 > y.1 || z.0 > z.1 {
            continue;
        }
        for k in z.0..=z.1 {
            for j in y.0..=y.1 {
                let row = (k * dims[1] + j) * dims[0];
                if let (Ok(s), Ok(e)) = (usize::try_from(row + x.0), usize::try_from(row + x.1 + 1)) {
                    rows.push((s, e));
                }
            }
        }
    }
    rows.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (s, e) in rows {
        match merged.last_mut() {
            Some(last) if s <= last.1.saturating_add(MERGE_GAP) => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    let (dx, dy) = (meta.dims[0] as usize, meta.dims[1] as usize);
    let mut patch = CutPatch::default();
    for (s, e) in merged {
        patch.runs.push((s, e - s));
        for t in s..e {
            let (i, j, k) = (t % dx, (t / dx) % dy, t / (dx * dy));
            let key = [
                meta.origin[0] + i as i32,
                meta.origin[1] + j as i32,
                meta.origin[2] + k as i32,
            ];
            patch.data.push(cut_texel(f, key) as f32);
        }
    }
    patch
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::{Move, MoveKind};
    use crate::geometry::{cube, Point3};
    use crate::stock_sim::preview::{pack_cut, pack_field};
    use crate::stock_sim::{Backend, SimOp, Stock, FIELD_BAND};

    fn block() -> Stock {
        let c = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(30.0, 20.0, 10.0));
        Stock::from_facets(Backend::Field { band: FIELD_BAND }, &c.facets, 1.0).unwrap()
    }

    fn feed(from: Point3, to: Point3) -> Move {
        Move { kind: MoveKind::Feed, from, to, feed: Some(500.0), arc: None, axis: None }
    }

    fn cut(stock: &mut Stock, m: &[Move]) {
        stock
            .simulate_display(&[SimOp { moves: m, radius: 2.0, ball: false, cone: None, corner: None }])
            .unwrap();
    }

    fn bits(v: &[f32]) -> Vec<u32> {
        v.iter().map(|x| x.to_bits()).collect()
    }

    /// A zig-zag of passes, one per step: the moves a playback chunk cuts.
    fn steps() -> Vec<Move> {
        (0..8)
            .map(|n| {
                let y = 2.0 + 2.0 * f64::from(n);
                let (a, b) = if n % 2 == 0 { (2.0, 28.0) } else { (28.0, 2.0) };
                feed(Point3::new(a, y, 8.0 - 0.5 * f64::from(n)), Point3::new(b, y, 8.0 - 0.5 * f64::from(n)))
            })
            .collect()
    }

    #[test]
    fn patched_layers_equal_a_full_pack_byte_for_byte() {
        let mut stock = block();
        let meta = pack_field(stock.field().unwrap()).expect("the block packs").meta;
        let mut layer = pack_cut(stock.field().unwrap(), &meta);
        let mut checkpoint = None;
        for (n, m) in steps().iter().enumerate() {
            cut(&mut stock, std::slice::from_ref(m));
            let Stock::Field(f) = &mut stock else { panic!("a field stock") };
            let dirty = f.take_dirty_bricks();
            assert!(!dirty.is_empty(), "step {n}: a cutting pass dirties bricks");
            let p = pack_cut_patch(f, &meta, &dirty);
            assert!(p.texels() < layer.len(), "step {n}: the patch is smaller than the layer");
            p.apply(&mut layer).unwrap();
            assert_eq!(bits(&layer), bits(&pack_cut(f, &meta)), "step {n}: patched == full pack");
            if n == 3 {
                checkpoint = Some((stock.clone(), layer.clone()));
            }
        }
        // A backward seek restores a checkpoint: its layer is a full
        // repack, and patches resume on top of it.
        let (mut back, _) = checkpoint.expect("a checkpoint");
        let Stock::Field(f) = &mut back else { panic!("a field stock") };
        let _ = f.take_dirty_bricks(); // the restore's ledger is stale: full upload
        let mut layer = pack_cut(f, &meta);
        cut(&mut back, &steps()[4..6]);
        let Stock::Field(f) = &mut back else { panic!("a field stock") };
        let dirty = f.take_dirty_bricks();
        pack_cut_patch(f, &meta, &dirty).apply(&mut layer).unwrap();
        assert_eq!(bits(&layer), bits(&pack_cut(f, &meta)), "after a backward seek, patched == full pack");
    }

    #[test]
    fn nothing_cut_is_nothing_dirty() {
        let mut stock = block();
        assert!(stock.field().unwrap().dirty_bricks().is_empty(), "a fresh field is clean");
        // A pass far above the stock claims no cell.
        cut(&mut stock, &[feed(Point3::new(0.0, 0.0, 80.0), Point3::new(30.0, 20.0, 80.0))]);
        assert!(stock.field().unwrap().dirty_bricks().is_empty(), "an air move dirties nothing");
        let meta = pack_field(stock.field().unwrap()).expect("packs").meta;
        let p = pack_cut_patch(stock.field().unwrap(), &meta, &HashSet::new());
        assert!(p.runs.is_empty() && p.data.is_empty());
        // A repeated cut of the same pass changes nothing either.
        let m = steps();
        cut(&mut stock, &m[..1]);
        let Stock::Field(f) = &mut stock else { panic!("a field stock") };
        assert!(!f.take_dirty_bricks().is_empty());
        cut(&mut stock, &m[..1]);
        assert!(stock.field().unwrap().dirty_bricks().is_empty(), "a re-cut moves no value");
    }

    #[test]
    fn a_patch_that_does_not_fit_is_refused_whole() {
        let p = CutPatch { runs: vec![(0, 2), (8, 4)], data: vec![1.0; 6] };
        let mut short = vec![0.0f32; 10];
        assert!(p.apply(&mut short).is_err());
        assert!(short.iter().all(|&x| x == 0.0), "nothing written on refusal");
        let mut ok = vec![0.0f32; 12];
        p.apply(&mut ok).unwrap();
        assert_eq!(ok.iter().filter(|&&x| x == 1.0).count(), 6);
    }
}
