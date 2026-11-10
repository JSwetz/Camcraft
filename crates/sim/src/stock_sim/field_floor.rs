//! A field column's FLOOR (#637 step B, option 2): the lowest face of the
//! stock under a column, read exactly from the shell - for the column
//! body's commonest re-clamp, a part flipped onto a face an earlier setup
//! left uncut underneath.
//!
//! The down-march's exactness rests on `cut_ups` (a downward ray's entry
//! into a cut); an upward read has no such companion yet, so this read is
//! exact ONLY where no cut reaches the floor: there the floor is the
//! shell's own lowest crossing on the column (a mesh plane, bit-exact).
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
    /// The lowest face of the stock under the column, exact.
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
        // A cell the swept tool actually removed (claim > 0: inside the
        // tool) at or next to the floor means a cut reached it. A band
        // claim near the floor that removed nothing there does not (a
        // shallow slot's band reaching down is not a cut floor).
        let (ix, iy) = (cell_of(x, self.cell), cell_of(y, self.cell));
        let iz = cell_of(z, self.cell);
        for dz in -1..=1 {
            if self.cut_at([ix, iy, iz.saturating_add(dz)]) > 0.0 {
                return FloorRead::CutFromBelow;
            }
        }
        FloorRead::Floor(z)
    }
}
