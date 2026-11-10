//! Column reads across the seam (M7.3): the per-point top and the
//! `above(z)` mask that the column engine's generator-side consumers
//! (the M2.1d feed-engagement sampler, the M2.1b/M3.1 D rest and
//! program masks) ask of a stock, answered by either arm.
//!
//! Provenance: the column vocabulary is this codebase's own dexel
//! (`Dexel::col`/`top_at`/`above`, M2.1b); the field arm reads the
//! same heights through the M7.1 C verdict march. Nothing from vendor
//! binaries.
//!
//! ## The laws here
//!
//! - **One quantization, both arms.** A point maps to its column by
//!   the column engine's own formula (`floor((q - x0) / cell)` on the
//!   stock's lattice), so where the two lattices coincide the two arms
//!   are asked about the SAME column - the parity suites can then only
//!   disagree on the height, never on which height was asked.
//! - **The lattice is resolved once.** The field's bounds walk its
//!   whole shell; a sampler asking 34 points per move must not pay
//!   that per point, so `Stock::columns` resolves it up front.
//! - **No verdict reads as no material.** A field column whose march
//!   cannot finish answers NaN, and every consumer's `top > z` test
//!   reads NaN as "nothing stands here" - the column arm's air
//!   (`-inf`) says the same, so both arms abstain alike.

use super::backend::Stock;
use super::dexel::{strip, Dexel};
use super::field::{center_of, SdfField};
use super::kernel::StockError;
use super::march::{lattice_of, march, world, Read};

use crate::geo2d::{Point2, Region};

/// What a column reader reads: a column store, or a field (#585 -
/// a bare `Dexel` the program-ahead rest grid still is, read through
/// the same seam a `Stock` is, until M7.3 retires it).
#[derive(Clone, Copy)]
enum Source<'a> {
    Grid(&'a Dexel),
    Field(&'a SdfField),
}

/// A stock's columns, lattice resolved: see the module laws.
pub struct Columns<'a> {
    stock: Source<'a>,
    x0: f64,
    y0: f64,
    cell: f64,
    nx: usize,
    ny: usize,
    i0: i32,
    j0: i32,
    z_hi: f64,
    z_lo: f64,
}

impl Stock {
    /// The column reader for this stock (see `Columns`). Refuses only
    /// where the field's lattice itself would (a grid past the per-axis
    /// ceiling) - the same refusal `deviate` gives on that arm.
    pub fn columns(&self) -> Result<Columns<'_>, StockError> {
        match self {
            Stock::Column(d) => Ok(Columns::of_grid(d)),
            Stock::Field(f) => {
                let lat = lattice_of(f)?;
                let (z_hi, z_lo) = world(f);
                Ok(Columns {
                    stock: Source::Field(f),
                    x0: lat.x0,
                    y0: lat.y0,
                    cell: lat.cell,
                    nx: lat.nx,
                    ny: lat.ny,
                    i0: lat.i0,
                    j0: lat.j0,
                    z_hi,
                    z_lo,
                })
            }
        }
    }
}

impl<'a> Columns<'a> {
    /// The reader over a bare column store (#585): the prior stock the
    /// operation layer still holds as a `Dexel`, read through the same
    /// vocabulary as a `Stock` - so its consumers port off `Dexel` now
    /// and take a field when the holder does (M7.3).
    #[must_use]
    pub fn of_grid(d: &'a Dexel) -> Self {
        Columns {
            stock: Source::Grid(d),
            x0: d.x0,
            y0: d.y0,
            cell: d.cell,
            nx: d.nx,
            ny: d.ny,
            i0: 0,
            j0: 0,
            z_hi: f64::NAN,
            z_lo: f64::NAN,
        }
    }
}

impl Columns<'_> {
    /// The lattice pitch (mm).
    #[must_use]
    pub fn cell(&self) -> f64 {
        self.cell
    }

    /// The centre of column `(ix, iy)`, on the reader's own lattice.
    #[must_use]
    pub fn center(&self, ix: usize, iy: usize) -> Point2 {
        Point2::new(self.x0 + (ix as f64 + 0.5) * self.cell, self.y0 + (iy as f64 + 0.5) * self.cell)
    }

    /// The lattice origin (the min corner of column (0, 0)).
    #[must_use]
    pub fn origin(&self) -> Point2 {
        Point2::new(self.x0, self.y0)
    }

    #[must_use]
    pub fn nx(&self) -> usize {
        self.nx
    }

    #[must_use]
    pub fn ny(&self) -> usize {
        self.ny
    }

    /// The column `q` falls in, by the column engine's formula; None
    /// off the lattice (and for a non-finite `q`).
    #[must_use]
    pub fn col(&self, q: Point2) -> Option<(usize, usize)> {
        let ix = (q.x - self.x0) / self.cell;
        let iy = (q.y - self.y0) / self.cell;
        // NaN fails every comparison below: refused, never index 0.
        if !(ix >= 0.0 && iy >= 0.0 && ix < self.nx as f64 && iy < self.ny as f64) {
            return None;
        }
        Some((ix as usize, iy as usize))
    }

    /// The top of the material standing in column `(ix, iy)`: the
    /// tile on the column arm, the march at the cell center on the
    /// field arm (NaN where it gives no verdict).
    #[must_use]
    pub fn top_at(&self, ix: usize, iy: usize) -> f64 {
        if ix >= self.nx || iy >= self.ny {
            return f64::NAN;
        }
        match self.stock {
            Source::Grid(d) => d.top_at(ix + iy * d.nx),
            Source::Field(f) => {
                let (x, y) = (
                    center_of(self.i0.saturating_add(ix as i32), self.cell),
                    center_of(self.j0.saturating_add(iy as i32), self.cell),
                );
                match march(f, &f.shell_crossings(x, y), x, y, self.z_hi, self.z_lo, true) {
                    Read::Top(z) => z,
                    _ => f64::NAN,
                }
            }
        }
    }

    /// The top over `q`'s column - `Dexel::col` then `top_at`, the
    /// pair the feed sampler reads.
    #[must_use]
    pub fn top_over(&self, q: Point2) -> Option<f64> {
        self.col(q).map(|(ix, iy)| self.top_at(ix, iy))
    }

    /// The cells whose top stands above `z`, as merged strips - the
    /// column arm is `Dexel::above` itself; the field arm reads every
    /// column's march and merges by the same strip law.
    pub fn above(&self, z: f64) -> Result<Region, StockError> {
        match self.stock {
            Source::Grid(d) => d.above(z),
            Source::Field(_) => {
                let mut tops = Vec::with_capacity(self.nx * self.ny);
                for iy in 0..self.ny {
                    for ix in 0..self.nx {
                        tops.push(self.top_at(ix, iy));
                    }
                }
                above_strips(self.x0, self.y0, self.cell, self.nx, self.ny, |k| tops[k], z)
            }
        }
    }
}

/// `Dexel::above`'s strip law over any column store (`top(k)` in the
/// row-major order the grid uses): the cells strictly above `z`,
/// horizontal runs merged into strips, the strips fused.
pub(super) fn above_strips(
    x0: f64,
    y0: f64,
    cell: f64,
    nx: usize,
    ny: usize,
    top: impl Fn(usize) -> f64,
    z: f64,
) -> Result<Region, StockError> {
    let mut acc: Option<Region> = None;
    for iy in 0..ny {
        let mut run: Option<usize> = None;
        for ix in 0..=nx {
            let over = ix < nx && top(ix + iy * nx) > z;
            match (run, over) {
                (None, true) => run = Some(ix),
                (Some(a), false) => {
                    let r = strip(x0, y0, cell, a, ix, iy)?;
                    acc = Some(match acc {
                        None => r,
                        Some(g) => g.union(&r).map_err(StockError::Geo)?,
                    });
                    run = None;
                }
                _ => {}
            }
        }
    }
    Ok(acc.unwrap_or_else(Region::empty))
}
