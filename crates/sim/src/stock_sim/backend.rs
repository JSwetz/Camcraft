//! The stock seam (M7.1 C): one API, two engines.
//!
//! Provenance: the column arm is the dexel that has answered every
//! verdict since M2.1; the field arm is the brick SDF of M7.1 A/B,
//! its heights read by the ray march in `march`. Nothing from vendor
//! binaries.
//!
//! ## The laws here
//!
//! - **A verdict says the same thing either way.** `deviate`,
//!   `measure`, and `surface` answer in HEIGHTS on the stock's own XY
//!   lattice - the column engine's vocabulary - so a `Deviation` or
//!   `StockReport` can be compared value for value across arms, and
//!   every aggregate (rest, gouge, their locations, NaN for the
//!   columns that judge nothing) is the same law computed twice.
//! - **Refusals are backend-independent.** Every gate a column arm
//!   refuses through, the field arm refuses through with the same
//!   words, checked the same order before a single cell is written;
//!   a program that will not run runs nowhere. The field's two extra
//!   truths - watertightness (the parity needs it) and a chord whose
//!   window would span the grid - refuse BY NAME and are honest
//!   over-strictness, never a silent pass.
//! - **The flat tool's flank spans the verdict world** (see
//!   `spanning_flank`): the column atom is an infinite cylinder, and
//!   the field makes that law exact below the world's ceiling by
//!   giving the solid a flank that reaches past anything a verdict
//!   can stand on. Where the spanning law cannot reach (a path below
//!   the march floor, the truncated cone's chimney above its max
//!   radius) the field shows MORE rest than the column - the blessed
//!   direction: rest over-reported is noise, rest hidden is a gouge
//!   waiting.
//! - **The column arm is what ships.** App and motion still build
//!   `Dexel` directly; routing those call sites through this seam is
//!   M7.3's retirement march, with the parity suite here as its
//!   safety law. Until then the field is the verifiable second
//!   opinion, which is exactly what a new engine should be.

use super::cut::{check_ops, SimOp};
use super::dexel::Dexel;
use super::deviate::{deviate, Deviation, SimSurface};
use super::field::SdfField;
use super::field_cut::SdfTool;
use super::kernel::{measure as column_measure, part_surface, MAX_CELLS_PER_AXIS, StockError, StockReport};
use super::march;
use super::march_verdicts;

use crate::geo2d::{Point2, Region};
use crate::geometry::{Point3, Triangle};
use crate::stock::{stock_mesh, StockShape};

/// Which engine a stock is built on. `Column` is the dexel height
/// field; `Field` is the brick SDF, `band` cells wide on each side of
/// the shell (a memory policy, not a precision one - the march reads
/// only signs and step bounds, so `band` 1 answers the same verdicts
/// at more rebuild cost, and 4 is the shipped default).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Column,
    Field { band: u32 },
}

/// The shipped field band, in cells (see `Backend::Field`).
pub const FIELD_BAND: u32 = 4;

/// The field's declared verdict epsilon, in mm of column height: the
/// march's solver floor (golden-section and bisection residuals, ~1e-7
/// per column) with headroom. Two engines agree when every column's
/// deviation lands within it and every verdict word matches (M7.3).
pub const FIELD_VERDICT_EPS: f64 = 1e-5;

/// One stock, one verdict API, two engines behind it.
#[derive(Clone, Debug)]
pub enum Stock {
    Column(Dexel),
    Field(SdfField),
}

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

    /// The engine's cell edge, in mm.
    #[must_use]
    pub fn cell(&self) -> f64 {
        match self {
            Stock::Column(d) => d.cell,
            Stock::Field(f) => f.cell(),
        }
    }

    #[must_use]
    pub fn column(&self) -> Option<&Dexel> {
        match self {
            Stock::Column(d) => Some(d),
            Stock::Field(_) => None,
        }
    }

    #[must_use]
    pub fn field(&self) -> Option<&SdfField> {
        match self {
            Stock::Field(f) => Some(f),
            Stock::Column(_) => None,
        }
    }

    /// The signed distance at `p`: NEGATIVE IN MATERIAL, positive in
    /// air, NaN where the engine cannot judge the point. The column
    /// arm answers the VERTICAL distance to its flat top (outside the
    /// grid there is no verdict); the field arm answers the true
    /// Euclidean distance to the remaining solid. Same sign law - the
    /// vocabulary differs, and callers that need a height ask `top`.
    #[must_use]
    pub fn distance(&self, p: Point3) -> f64 {
        match self {
            Stock::Column(d) => {
                if !p.is_finite() {
                    return f64::NAN;
                }
                match d.col(Point2::new(p.x, p.y)) {
                    None => f64::NAN, // outside the grid: no verdict, not air
                    Some(k) => p.z - d.top_at(k),
                }
            }
            Stock::Field(f) => f.distance(p),
        }
    }

    /// The Z of the top of remaining material at `(x, y)`: marched
    /// from the world's ceiling on the field arm, read from the tile
    /// on the column arm - the height a verdict is made of, NaN when
    /// no engine can say one (no column there, air, an unfinished
    /// march).
    #[must_use]
    pub fn top(&self, x: f64, y: f64) -> f64 {
        match self {
            Stock::Column(d) => match d.col(Point2::new(x, y)) {
                Some(k) if x.is_finite() && y.is_finite() => d.top_at(k),
                _ => f64::NAN,
            },
            Stock::Field(f) => march::column_top(f, x, y),
        }
    }

    /// Run the program. `check_ops` refuses the invalid before either
    /// arm writes a cell, so a bad op cannot leave half a stock; past
    /// that gate the column arm is `cut::simulate` unchanged and the
    /// field arm walks the same moves through the same three solids,
    /// with `spanning_flank` giving the flat tool - and the ball its
    /// shank (M7.3: the bare sphere left a waterline's stock top
    /// floating above the cut) - the column's reach, batched per
    /// brick (M7.3's cost law, `field_batch`).
    pub fn simulate(&mut self, ops: &[SimOp]) -> Result<(), StockError> {
        match self {
            Stock::Column(d) => super::cut::simulate(d, ops),
            Stock::Field(f) => {
                check_ops(ops)?;
                let chord_tol = (f.cell() * 0.25).max(1e-9);
                let flank = spanning_flank(f);
                for (i, op) in ops.iter().enumerate() {
                    let tool = if op.ball {
                        SdfTool::Shanked { r: op.radius, shank: flank }
                    } else if let Some(cot) = op.cone {
                        SdfTool::Taper { r: op.radius, cot }
                    } else if let Some(corner) = op.corner {
                        SdfTool::BullNose { r: op.radius, corner, flank }
                    } else {
                        SdfTool::Flat { r: op.radius, flank }
                    };
                    for chunk in op.moves.chunks(super::field_batch::BATCH_MOVES) {
                        f.subtract_batch(chunk, &tool, chord_tol).map_err(|e| named_op(i, e))?;
                    }
                }
                Ok(())
            }
        }
    }

    /// The program's verdict against the model, per column. The tol
    /// gate and every aggregate are `deviate`'s law read off the
    /// engine's own tops (the height-parity law above).
    pub fn deviate(&self, facets: &[Triangle], tol: f64) -> Result<Deviation, StockError> {
        match self {
            Stock::Column(d) => deviate(d, facets, tol),
            Stock::Field(f) => march_verdicts::deviate_field(f, facets, tol),
        }
    }

    /// One operation's report against the model within a footprint.
    /// The column arm is `kernel::measure` over its grid's part
    /// surface; the field arm marches both the stock's own top and
    /// what remains, so `removed_volume` means the same depth-times-
    /// area either way (for air columns the semi-infinite block's
    /// gross count is the column's, and the field counts only steel
    /// it found - the cylinder's corners are where that shows).
    #[must_use]
    pub fn measure(&self, model: &[Triangle], footprint: &Region, tol: f64) -> StockReport {
        match self {
            Stock::Column(d) => {
                let part = part_surface(model, d.x0, d.y0, d.cell, d.nx, d.ny);
                column_measure(d, &part, footprint, tol)
            }
            Stock::Field(f) => march_verdicts::measure_field(f, model, footprint, tol),
        }
    }

    /// The sheet the viewer paints, classed as `deviate` judged it.
    #[must_use]
    pub fn surface(&self, classes: &[f32]) -> SimSurface {
        match self {
            Stock::Column(d) => d.surface(classes),
            Stock::Field(f) => march_verdicts::sheet_field(f, classes),
        }
    }

    /// The machined volume as the eye sees it (M7.2): a sphere-traced
    /// frame of this stock, classed per pixel against `target` - the
    /// model's own field, so the deviation is located wherever the
    /// ray lands, not only at column tops. The field arm only: the
    /// column's vertical distance would let a lateral ray slip past
    /// its walls, so that arm keeps the sheet until M7.3 retires it,
    /// and says so by name.
    pub fn preview(
        &self,
        target: Option<&Stock>,
        cam: &super::preview::PreviewCamera,
        w: u32,
        h: u32,
        tol: f64,
    ) -> Result<super::preview::Preview, StockError> {
        let f = self
            .field()
            .ok_or_else(|| StockError::Invalid("the column arm previews through its sheet mesh; the ray march needs the field arm".into()))?;
        let mut tf = None;
        if let Some(t) = target {
            tf = Some(
                t.field()
                    .ok_or_else(|| StockError::Invalid("the deviation target must be a field: the model needs a signed distance, not a height field".into()))?,
            );
        }
        super::preview::preview(f, tf, cam, w, h, tol)
    }
}

/// The column engine's refusal of a turning program, said once.
pub const LATHE_COLUMN_REFUSAL: &str = "a revolved path is not judgeable in the mill's dexel frame - the column engine has no annular-sector sweep; the field arm cuts it (Stock::simulate_lathe)";

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
    let m = super::field_gen::box_shell(
        Point3::new(x0, y0, top_z - 10.0 * span),
        Point3::new(x1, y1, top_z),
        cell,
    );
    SdfField::from_mesh(&m, cell, band)
}

/// The flank that makes the field's flat tool reach what the column
/// atom reaches: every height a verdict can stand on sits within the
/// shell's diagonal plus two caps of the deepest tip position the
/// program can hold, so a solid that tall claims exactly the column's
/// removal below that world. Above it the two engines were never
/// going to agree (the column removes everything to the sky), and no
/// verdict lives there.
pub(crate) fn spanning_flank(f: &SdfField) -> f64 {
    let b = f.bounds();
    let (dx, dy, dz) = (b.max.x - b.min.x, b.max.y - b.min.y, b.max.z - b.min.z);
    (dx * dx + dy * dy + dz * dz).sqrt() + 2.0 * f.cap() + 4.0 * f.cell()
}

/// A field refusal, said with the op that caused it: the column
/// arm's gates are per-op by construction, and an error that cannot
/// say which op raised its hand is the opaque failure AGENTS.md
/// refuses.
pub(crate) fn named_op(i: usize, e: StockError) -> StockError {
    match e {
        StockError::Invalid(s) => StockError::Invalid(format!("op {i}: {s}")),
        other => other,
    }
}
