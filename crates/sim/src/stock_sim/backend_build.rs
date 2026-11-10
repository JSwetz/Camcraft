//! Backend-independent stock construction.
//!
//! Provenance: the existing Dexel and brick-SDF construction laws in
//! `backend`; constructors and their refusal checks moved unchanged.

use super::backend::{Backend, Stock};
use super::dexel::Dexel;
use super::field::SdfField;
use super::kernel::{MAX_CELLS_PER_AXIS, StockError};
use crate::geometry::{Point3, Triangle};
use crate::stock::{stock_mesh, StockShape};

impl Stock {
    /// The stock block, either arm. Every refusal is `Dexel::block`'s
    /// own word, gate for gate (the backend-independence law).
    pub fn block(b: Backend, x0: f64, y0: f64, x1: f64, y1: f64, top_z: f64, cell: f64) -> Result<Self, StockError> {
        match b {
            Backend::Column => Ok(Stock::Column(Dexel::block(x0, y0, x1, y1, top_z, cell)?)),
            Backend::Field { band } => Ok(Stock::Field(block_field(x0, y0, x1, y1, top_z, cell, band)?)),
        }
    }

    /// Parametric stock: box and cylinder on either arm, model stock
    /// through `from_facets` (the refusal says so on both).
    pub fn from_shape(b: Backend, shape: &StockShape, cell: f64) -> Result<Self, StockError> {
        match b {
            Backend::Column => Ok(Stock::Column(Dexel::from_shape(shape, cell)?)),
            Backend::Field { band } => {
                use crate::stock::StockShape as S;
                match shape {
                    S::Box { dx, dy, dz } => {
                        if !(dx.is_finite() && dy.is_finite() && dz.is_finite() && *dx > 0.0 && *dy > 0.0 && *dz > 0.0)
                        {
                            return Err(StockError::Invalid("box stock needs finite positive dx, dy, dz".into()));
                        }
                        Ok(Stock::Field(block_field(
                            -dx / 2.0,
                            -dy / 2.0,
                            dx / 2.0,
                            dy / 2.0,
                            *dz,
                            cell,
                            band,
                        )?))
                    }
                    S::Cylinder { diameter, height } => {
                        if !(diameter.is_finite() && height.is_finite() && *diameter > 0.0 && *height > 0.0)
                        {
                            return Err(StockError::Invalid("cylinder stock needs finite positive diameter, height".into()));
                        }
                        // The column arm validates the cell and the
                        // circumscribed square's size inside `block`;
                        // the field says the same two refusals here.
                        if !cell.is_finite() || cell <= 0.0 {
                            return Err(StockError::Invalid("stock box and cell must be finite, box non-empty, cell positive".into()));
                        }
                        let n = (*diameter / cell).ceil() as usize;
                        if n == 0 || n > MAX_CELLS_PER_AXIS {
                            return Err(StockError::Invalid(format!(
                                "grid would be {n}x{n} cells; at most {MAX_CELLS_PER_AXIS} per axis"
                            )));
                        }
                        let facets = stock_mesh(shape)
                            .ok_or_else(|| StockError::Invalid("cylinder stock meshed empty - the field refuses instead of guessing".into()))?;
                        Ok(Stock::Field(SdfField::from_mesh(&facets, cell, band)?))
                    }
                    S::FromModel { .. } => Err(StockError::Invalid(
                        "model stock builds from its facets: Dexel::from_facets".into(),
                    )),
                }
            }
        }
    }

    /// Stock that IS a mesh. The column arm's gates repeat verbatim
    /// on the field arm; beyond them the field adds its one honest
    /// demand - a closed shell, because parity cannot answer an open
    /// edge - and refuses by name rather than guess a sign.
    pub fn from_facets(b: Backend, facets: &[Triangle], cell: f64) -> Result<Self, StockError> {
        match b {
            Backend::Column => Ok(Stock::Column(Dexel::from_facets(facets, cell)?)),
            Backend::Field { band } => {
                if !(cell.is_finite() && cell > 0.0) {
                    return Err(StockError::Invalid(format!("cell {cell} is not finite-positive")));
                }
                let (mut bx0, mut by0) = (f64::INFINITY, f64::INFINITY);
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
                    }
                }
                if facets.is_empty() || bx1 <= bx0 || by1 <= by0 {
                    return Err(StockError::Invalid(
                        "stock mesh is empty or flat in XY: nothing to simulate against".into(),
                    ));
                }
                let nx = (((bx1 - bx0) / cell).ceil() as usize + 2).max(1);
                let ny = (((by1 - by0) / cell).ceil() as usize + 2).max(1);
                if nx > MAX_CELLS_PER_AXIS || ny > MAX_CELLS_PER_AXIS {
                    return Err(StockError::Invalid(format!(
                        "grid would be {nx}x{ny} cells; at most {MAX_CELLS_PER_AXIS} per axis"
                    )));
                }
                Ok(Stock::Field(SdfField::from_mesh(facets, cell, band)?))
            }
        }
    }
}

/// The box gate, `Dexel::block`'s own words, then a solid whose floor
/// lies far enough under the stock that a program of the stock's own
/// scale marches through material to its last cell - the reach a
/// semi-infinite column takes as given, bought here with ten spans of
/// mesh, which the sparse bricks charge nothing outside the band.
fn block_field(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    top_z: f64,
    cell: f64,
    band: u32,
) -> Result<SdfField, StockError> {
    if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite() && top_z.is_finite())
        || x1 <= x0
        || y1 <= y0
        || !(cell.is_finite() && cell > 0.0)
    {
        return Err(StockError::Invalid("stock box and cell must be finite, box non-empty, cell positive".into()));
    }
    let nx = ((x1 - x0) / cell).ceil() as usize;
    let ny = ((y1 - y0) / cell).ceil() as usize;
    if nx == 0 || ny == 0 || nx > MAX_CELLS_PER_AXIS || ny > MAX_CELLS_PER_AXIS {
        return Err(StockError::Invalid(format!(
            "grid would be {nx}x{ny} cells; at most {MAX_CELLS_PER_AXIS} per axis"
        )));
    }
    let span = (x1 - x0).max(y1 - y0).max(cell);
    // Tessellated to the cell (M7.3): ten spans of side as one quad
    // out-ran the per-facet seed ceiling and refused a block the
    // column builds - a backend-dependent refusal, the law above's
    // exact violation.
    // #633: the box's distance is closed-form - no band fill along the
    // ten-span walls (the BFS made a 120 mm block take a minute).
    SdfField::from_box(Point3::new(x0, y0, top_z - 10.0 * span), Point3::new(x1, y1, top_z), cell, band)
}
