//! Split from `stock_sim.rs` by M5.0.1; structure only.

use super::kernel::{MAX_CELLS_PER_AXIS, StockError, part_surface};
use super::dexel::Dexel;

use crate::cl::Move;
use crate::geometry::Triangle;
/// One operation's claim on the stock: its cached moves and the tool
/// that cut them. `ball` picks the exact sphere atom for a ball nose
/// and leaves the flat-disc atom for everything whose final hole or
/// groove IS its cylinder (end mills, drills, taps); a bull nose's
/// corner fillet is swept here as if it were flat-bottomed at the
/// corner's lowest point, which over-reports removal beside a fillet
/// wall - the law above's one known-biased atom, stated at the type
/// so no caller mistakes a fillet's clean grid for a proven fillet.
#[derive(Clone, Copy, Debug)]
pub struct SimOp<'a> {
    pub moves: &'a [Move],
    pub radius: f64,
    pub ball: bool,
}

/// The lattice law, stated once (and the same numbers `envelope_scan`
/// has always used): an eighth of the smallest tool radius sees every
/// scallop the tolerance must keep, and the span growth keeps the
/// grid honest on dust motes and stadiums alike.
#[must_use]
pub fn auto_cell(min_radius: f64, span_x: f64, span_y: f64) -> f64 {
    (min_radius * 0.125)
        .max(1e-4)
        .max(span_x / 1023.0)
        .max(span_y / 1023.0)
}

impl Dexel {
    /// The stock solid as a grid. Box and cylinder are analytic - the
    /// cylinder's columns outside its own circle start as air, so a
    /// sweep never finds material in the machine's world beside the
    /// round. `FromModel` carries its facets in the world, not the
    /// enum, so it builds through `from_facets` - a refusal with the
    /// right answer in its words, never a silent box.
    pub fn from_shape(shape: &crate::stock::StockShape, cell: f64) -> Result<Self, StockError> {
        use crate::stock::StockShape as S;
        match shape {
            S::Box { dx, dy, dz } => {
                if !(dx.is_finite() && dy.is_finite() && dz.is_finite() && *dx > 0.0 && *dy > 0.0 && *dz > 0.0)
                {
                    return Err(StockError::Invalid("box stock needs finite positive dx, dy, dz".into()));
                }
                Self::block(-dx / 2.0, -dy / 2.0, dx / 2.0, dy / 2.0, *dz, cell)
            }
            S::Cylinder { diameter, height } => {
                if !(diameter.is_finite() && height.is_finite() && *diameter > 0.0 && *height > 0.0)
                {
                    return Err(StockError::Invalid("cylinder stock needs finite positive diameter, height".into()));
                }
                let mut dex = Self::block(
                    -diameter / 2.0,
                    -diameter / 2.0,
                    diameter / 2.0,
                    diameter / 2.0,
                    *height,
                    cell,
                )?;
                for iy in 0..dex.ny {
                    for ix in 0..dex.nx {
                        let c = dex.center(ix, iy);
                        if c.x.hypot(c.y) > *diameter / 2.0 {
                            dex.top[ix + iy * dex.nx] = f64::NEG_INFINITY;
                        }
                    }
                }
                Ok(dex)
            }
            S::FromModel { .. } => Err(StockError::Invalid(
                "model stock builds from its facets: Dexel::from_facets".into(),
            )),
        }
    }

    /// Stock that IS a mesh (stock-from-model): every column starts at
    /// the mesh's own top, air where no facet covers - `part_surface`
    /// read once as an initial state instead of a verdict. The one
    /// cell of margin keeps a sweep that starts exactly on the bbox
    /// edge inside the grid that must judge it.
    pub fn from_facets(facets: &[Triangle], cell: f64) -> Result<Self, StockError> {
        if !(cell.is_finite() && cell > 0.0) {
            return Err(StockError::Invalid(format!("cell {cell} is not finite-positive")));
        }
        let (mut bx0, mut by0, mut bz1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY);
        let (mut bx1, mut by1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for t in facets {
            for v in [t.a, t.b, t.c] {
                if !(v.x.is_finite() && v.y.is_finite() && v.z.is_finite()) {
                    return Err(StockError::Invalid("stock mesh has a non-finite vertex".into()));
                }
                bx0 = bx0.min(v.x);
                bx1 = bx1.max(v.x);
                by0 = by0.min(v.y);
                by1 = by1.max(v.y);
                bz1 = bz1.max(v.z);
            }
        }
        if facets.is_empty() || bx1 <= bx0 || by1 <= by0 {
            return Err(StockError::Invalid(
                "stock mesh is empty or flat in XY: nothing to simulate against".into(),
            ));
        }
        let (x0, y0) = (bx0 - cell, by0 - cell);
        let nx = (((bx1 - bx0) / cell).ceil() as usize + 2).max(1);
        let ny = (((by1 - by0) / cell).ceil() as usize + 2).max(1);
        if nx > MAX_CELLS_PER_AXIS || ny > MAX_CELLS_PER_AXIS {
            return Err(StockError::Invalid(format!(
                "grid would be {nx}x{ny} cells; at most {MAX_CELLS_PER_AXIS} per axis"
            )));
        }
        Ok(Dexel {
            x0,
            y0,
            cell,
            nx,
            ny,
            stock_z: if bz1.is_finite() { bz1 } else { 0.0 },
            top: part_surface(facets, x0, y0, cell, nx, ny),
        })
    }
}

/// Run the program: each op's moves, in order, through the same grid.
/// Refusals happen BEFORE any subtraction, so a bad op cannot leave a
/// half-run grid that looks like a sim.
pub fn simulate(dex: &mut Dexel, ops: &[SimOp]) -> Result<(), StockError> {
    for (i, op) in ops.iter().enumerate() {
        if !(op.radius.is_finite() && op.radius > 0.0) {
            return Err(StockError::Invalid(format!(
                "op {i} tool radius {} is not finite-positive",
                op.radius
            )));
        }
        if !op.moves.iter().all(|m| m.is_finite()) {
            return Err(StockError::Invalid(format!(
                "op {i} carries a non-finite move - the sim refuses instead of cutting nothing silently"
            )));
        }
    }
    let chord_tol = (dex.cell * 0.25).max(1e-9); // envelope_scan's law
    for op in ops {
        for m in op.moves {
            if op.ball {
                dex.subtract_ball(m, op.radius, chord_tol);
            } else {
                dex.subtract(m, op.radius, chord_tol);
            }
        }
    }
    Ok(())
}
