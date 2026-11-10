//! A field column's FLOOR (#637 step B, option 2): the lowest face of the
//! stock under a column, read exactly from the shell - for the column
//! body's commonest re-clamp, a part flipped onto a face an earlier setup
//! left uncut underneath.
//!
//! The down-march's exactness rests on `cut_ups` (a downward ray's entry
//! into a cut); an upward read has no such companion yet, so this read is
//! exact ONLY where no cut reaches the floor: there the floor is the
//! shell's own lowest crossing on the column (a mesh plane, exact to
//! rounding).
//! A column whose floor a cut has claimed is refused by name
//! (`CutFromBelow`) - never a bracketed guess. A `cut_downs` companion
//! (#637, option 1) can lift that refusal later.
//!
//! Provenance: ray-parity crossings of a watertight shell (the
//! `solid_rest` law this codebase already uses); our own design.

use super::field::{cell_of, SdfField};

/// One column's floor read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FloorRead {
    /// The lowest face of the stock under the column, exact to rounding.
    Floor(f64),
    /// The column crosses no stock at all.
    NoMaterial,
    /// A cut has claimed the column near its floor: an upward read would
    /// not be exact, so none is given.
    CutFromBelow,
    /// The shell cannot be read on this column (an `Open` parity column).
    Unreadable,
}

impl SdfField {
    /// The floor under `(x, y)`: see the module law.
    #[must_use]
    pub fn floor_at(&self, x: f64, y: f64) -> FloorRead {
        if !(x.is_finite() && y.is_finite()) {
            return FloorRead::Unreadable;
        }
        let Some(cross) = self.shell_crossings(x, y) else {
            return FloorRead::Unreadable;
        };
        let Some(z) = cross.iter().copied().filter(|z| z.is_finite()).reduce(f64::min) else {
            return FloorRead::NoMaterial;
        };
        // A cut reached the floor if the swept tool came within one cell
        // of any cell centre in the 3x3x3 block around the floor point
        // (a claim is -d_tool at a centre: claim > -cell means the tool
        // passed within a cell of it). Every point of a cell lies within
        // sqrt(3)/2 cell of its centre, so this catches a point inside a
        // cut near its wall and a cut thinner than a cell (review of
        // #649) - conservatively: a tool that passed close without
        // reaching the floor is refused too, the safe direction. A cut a
        // full cell or more above the floor (a shallow slot) is not.
        let (ix, iy) = (cell_of(x, self.cell), cell_of(y, self.cell));
        let iz = cell_of(z, self.cell);
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let key = [ix.saturating_add(dx), iy.saturating_add(dy), iz.saturating_add(dz)];
                    if self.cut_at(key) > -self.cell {
                        return FloorRead::CutFromBelow;
                    }
                }
            }
        }
        FloorRead::Floor(z)
    }
}
