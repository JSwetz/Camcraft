//! Dexel (Z-map) stock simulation for 3-D roughing (M2.1b).
//!
//! Provenance: the dexel/Z-map material model is classic published CAM
//! simulation literature (a column-wise height field of remaining
//! material, subtracted per swept tool disc); the region algebra is our
//! Clipper2 wrapper. Provenance also by omission: this grid is the
//! VERIFICATION layer, never the source of truth - operation geometry
//! and tolerance laws keep evaluating against the mesh itself (the
//! ROADMAP's standing rule for any discretized stock).
//!
//! ## The laws here
//!
//! - **Column model.** Every cell holds one number: the Z of the top of
//!   the material standing at that XY. This is exact for everything
//!   this codebase cuts - a Z-plunge world whose tools never machine
//!   under an overhang - and it is the same discretization that makes
//!   `machining3d::material_above` answerable per plane. Undercuts
//!   would need a voxel octree; they need it nowhere yet.
//! - **Subtraction is conservative.** A move removes, per cell, the
//!   LOWEST Z the tool bottom reaches while its disc touches the cell
//!   center - computed from the exact interval of the move that is in
//!   reach, not from the endpoints. Arcs ride their `chords` (the same
//!   sagitta law as everywhere else): chords sweep a hair LESS than
//!   the true arc, so a sim reports a touch more rest than exists and
//!   never a gouge the tool did not commit. A silent wrong answer is
//!   the one failure mode; over-reporting rest is honest noise.
//! - **Rapids touch nothing** (RS-274 G0 practice: no material contacts
//!   a tool moving at rapid). Feeds and plunges cut.
//! - **Part surface is read from the mesh**, facet-plane interpolation
//!   at cell centers over each facet's XY bounding box: the highest
//!   surface covering a cell is that column's part top (`-inf` when no
//!   facet covers it - air everywhere along the column). Rest material
//!   and gouges are both measured against this, so the sim's verdict
//!   uses the model, not a second hand-entered surface.

use crate::cl::{Move, MoveKind};
use crate::geo2d::{GeoError, Loop, Point2, Region};
use crate::geometry::{Point3, Triangle};

/// What the sim refuses, in the generator's vocabulary.
#[derive(thiserror::Error, Debug, PartialEq)]
pub enum StockError {
    #[error("invalid stock simulation setup: {0}")]
    Invalid(String),
    #[error(transparent)]
    Geo(#[from] GeoError),
}

/// Hard ceiling on grid cells (both axes): a model of a stadium at 1 um
/// cells would otherwise allocate until the machine stops. Refusing
/// with the numbers named is the clean answer; generators pick cells
/// from tool sizes and never approach it.
const MAX_CELLS_PER_AXIS: usize = 4096;

/// The height field: material top per cell, row-major from (`x0`, `y0`)
/// at the lower-left corner, cell centers at `x0 + (i + 0.5) * cell`.
#[derive(Clone, Debug, PartialEq)]
pub struct Dexel {
    pub x0: f64,
    pub y0: f64,
    pub cell: f64,
    pub nx: usize,
    pub ny: usize,
    /// The surface the whole grid started at: removal is counted from
    /// here, so a column's removed depth is one subtraction, not a
    /// history.
    pub stock_z: f64,
    /// Top of the material standing at each column.
    pub top: Vec<f64>,
}

impl Dexel {
    /// A stock block `x0..x1 × y0..y1 × ..top_z` on the given cell size:
    /// every column's top starts at the stock surface.
    pub fn block(x0: f64, y0: f64, x1: f64, y1: f64, top_z: f64, cell: f64) -> Result<Self, StockError> {
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
        Ok(Dexel {
            x0,
            y0,
            cell,
            nx,
            ny,
            stock_z: top_z,
            top: vec![top_z; nx * ny],
        })
    }

    #[must_use]
    pub fn center(&self, ix: usize, iy: usize) -> Point2 {
        Point2::new(
            self.x0 + (ix as f64 + 0.5) * self.cell,
            self.y0 + (iy as f64 + 0.5) * self.cell,
        )
    }

    #[must_use]
    pub fn col(&self, q: Point2) -> Option<usize> {
        let ix = (q.x - self.x0) / self.cell;
        let iy = (q.y - self.y0) / self.cell;
        if ix < 0.0 || iy < 0.0 || ix >= self.nx as f64 || iy >= self.ny as f64 {
            return None;
        }
        Some(ix as usize + (iy as usize) * self.nx)
    }

    /// Remove material down to the LOWEST Z of one straight, Z-linear
    /// chord whose disc of radius `r` touches the cell center - the
    /// atom every move reduces to. A zero-XY-length chord (a plunge)
    /// is its own interval. The reach interval is exact for a segment,
    /// so this is never an endpoint approximation: an oblique move
    /// cannot over-report its cut, and cannot under-report one either.
    pub fn cut_chord(&mut self, a: Point3, b: Point3, r: f64) {
        if !(a.is_finite() && b.is_finite()) || !(r.is_finite() && r >= 0.0) {
            return; // non-finite geometry cuts nothing; refusing lives
                    // in the callers' validation laws, not mid-sweep
        }
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let l2 = dx * dx + dy * dy;
        // Cells whose centers can be within r of the segment: the
        // segment bbox grown by r, clamped to the grid. Clamping is
        // harmless - every candidate still faces the exact reach test.
        let fx = |v: f64| ((v - self.x0) / self.cell).floor();
        let fy = |v: f64| ((v - self.y0) / self.cell).floor();
        let lo_x = fx(a.x.min(b.x) - r).max(0.0) as usize;
        let hi_x = fx(a.x.max(b.x) + r).clamp(0.0, self.nx as f64 - 1.0) as usize;
        let lo_y = fy(a.y.min(b.y) - r).max(0.0) as usize;
        let hi_y = fy(a.y.max(b.y) + r).clamp(0.0, self.ny as f64 - 1.0) as usize;
        if lo_x >= self.nx || lo_y >= self.ny {
            return;
        }
        for iy in lo_y..=hi_y {
            for ix in lo_x..=hi_x {
                let q = self.center(ix, iy);
                // Parameter interval where the disc reaches q:
                // |q - a - t*d| = r. Degenerate d (a plunge) is t = 0.
                let (t0, t1) = if l2 <= f64::EPSILON {
                    let d2 = (q.x - a.x).hypot(q.y - a.y);
                    if d2 > r {
                        continue;
                    }
                    (0.0, 1.0)
                } else {
                    let wx = q.x - a.x;
                    let wy = q.y - a.y;
                    let t_c = (wx * dx + wy * dy) / l2;
                    // Perpendicular offset from the line, for the
                    // half-width along t.
                    let cross = wx * dy - wy * dx;
                    let d_perp2 = (cross * cross) / l2;
                    if d_perp2 > r * r {
                        continue;
                    }
                    let half = (r * r - d_perp2).sqrt() / l2.sqrt();
                    // The reach set on the SEGMENT, not the infinite
                    // line: a cell whose window falls entirely before
                    // t = 0 or after t = 1 was never touched - an
                    // endpoint clamp would turn that empty window into
                    // a phantom cut at the nearest end.
                    let lo = (t_c - half).max(0.0);
                    let hi = (t_c + half).min(1.0);
                    if lo > hi {
                        continue;
                    }
                    (lo, hi)
                };
                // Z is linear along the chord, so its minimum over the
                // interval is at one of the interval ends.
                let za = a.z + (b.z - a.z) * t0;
                let zb = a.z + (b.z - a.z) * t1;
                let cut = za.min(zb);
                let k = ix + iy * self.nx;
                if cut < self.top[k] {
                    self.top[k] = cut;
                }
            }
        }
    }

    /// Subtract one move: rapids touch nothing, straight moves are one
    /// chord, arcs reduce to their sagitta chords with Z interpolated
    /// at each chord endpoint (a helix descends uniformly with its
    /// sweep - the same reading `sim::path_len` takes).
    pub fn subtract(&mut self, m: &Move, r: f64, chord_tol: f64) {
        if m.kind == MoveKind::Rapid || !m.is_finite() {
            return;
        }
        match m.arc {
            None => self.cut_chord(m.from, m.to, r),
            Some(arc) => {
                let a2 = Point2::new(m.from.x, m.from.y);
                let b2 = Point2::new(m.to.x, m.to.y);
                let cs = crate::cl::arc_chords(a2, b2, arc, chord_tol);
                let n = cs.len().max(1) as f64;
                let dz = m.to.z - m.from.z;
                for (k, (ca, cb)) in cs.into_iter().enumerate() {
                    let f0 = k as f64 / n;
                    let f1 = (k + 1) as f64 / n;
                    let za = m.from.z + dz * f0;
                    let zb = m.from.z + dz * f1;
                    self.cut_chord(
                        Point3::new(ca.x, ca.y, za),
                        Point3::new(cb.x, cb.y, zb),
                        r,
                    );
                }
            }
        }
    }

    /// Rectangle regions of the cells whose top stands above `z` (the
    /// mask's horizontal runs merged into strips, the strips fused by
    /// the region algebra). This is a grid query answer in region
    /// clothing: cell-quantized by construction, and its consumer's
    /// tolerance laws - not this shape's corners - decide what is cut.
    pub fn above(&self, z: f64) -> Result<Region, StockError> {
        let mut acc: Option<Region> = None;
        for iy in 0..self.ny {
            let mut run: Option<usize> = None;
            for ix in 0..=self.nx {
                let over = ix < self.nx && self.top[ix + iy * self.nx] > z;
                match (run, over) {
                    (None, true) => run = Some(ix),
                    (Some(a), false) => {
                        let r = strip(self.x0, self.y0, self.cell, a, ix, iy)?;
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
}

/// One merged strip: cells `ia..ib` of row `iy`, as a closed rectangle.
fn strip(x0: f64, y0: f64, cell: f64, ia: usize, ib: usize, iy: usize) -> Result<Region, StockError> {
    let (a, b) = (
        Point2::new(x0 + ia as f64 * cell, y0 + iy as f64 * cell),
        Point2::new(x0 + ib as f64 * cell, y0 + (iy as f64 + 1.0) * cell),
    );
    let loop_ = Loop::new(vec![
        a,
        Point2::new(b.x, a.y),
        b,
        Point2::new(a.x, b.y),
    ])
    .map_err(StockError::Geo)?;
    Region::new(vec![loop_]).map_err(StockError::Geo)
}

/// The final part's top surface per cell: the highest facet plane
/// covering each cell center, `-inf` where no facet covers it (the
/// column is air). Facets whose XY projection is degenerate (vertical
/// walls) carry no surface height at any point - their caps do - and
/// are skipped by the signed-area test, exact by construction.
pub fn part_surface(
    facets: &[Triangle],
    x0: f64,
    y0: f64,
    cell: f64,
    nx: usize,
    ny: usize,
) -> Vec<f64> {
    let mut out = vec![f64::NEG_INFINITY; nx * ny];
    for t in facets {
        let (a, b, c) = (t.a, t.b, t.c);
        if !(a.is_finite() && b.is_finite() && c.is_finite()) {
            continue;
        }
        let det = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        if det.abs() <= f64::EPSILON {
            continue; // vertical (or degenerate) projection: no top
        }
        let minx = a.x.min(b.x).min(c.x);
        let maxx = a.x.max(b.x).max(c.x);
        let miny = a.y.min(b.y).min(c.y);
        let maxy = a.y.max(b.y).max(c.y);
        let fx = |v: f64| ((v - x0) / cell).floor();
        let i0 = fx(minx).max(0.0) as usize;
        let i1 = fx(maxx).clamp(0.0, nx as f64 - 1.0) as usize;
        let j0 = fy0(miny, y0, cell).max(0.0) as usize;
        let j1 = fy0(maxy, y0, cell).clamp(0.0, ny as f64 - 1.0) as usize;
        if i0 >= nx || j0 >= ny {
            continue;
        }
        for iy in j0..=j1 {
            for ix in i0..=i1 {
                let q = Point2::new(x0 + (ix as f64 + 0.5) * cell, y0 + (iy as f64 + 0.5) * cell);
                // Barycentric against the signed det (works for either
                // winding; the det sign cancels in both ratios).
                let u = ((q.x - a.x) * (c.y - a.y) - (q.y - a.y) * (c.x - a.x)) / det;
                let v = ((b.x - a.x) * (q.y - a.y) - (b.y - a.y) * (q.x - a.x)) / det;
                if u < -1e-9 || v < -1e-9 || u + v > 1.0 + 1e-9 {
                    continue;
                }
                let z = a.z + u * (b.z - a.z) + v * (c.z - a.z);
                let k = ix + iy * nx;
                if z > out[k] {
                    out[k] = z;
                }
            }
        }
    }
    out
}

fn fy0(v: f64, y0: f64, cell: f64) -> f64 {
    ((v - y0) / cell).floor()
}

/// The sim's verdict, all measured against the model's own surface:
/// rest (material still standing above where the part says it must
/// not), gouge (material removed from below it), and the gross number
/// the tool actually took out.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StockReport {
    pub removed_volume: f64,
    pub rest_volume: f64,
    pub rest_cells: usize,
    pub max_rest_height: f64,
    pub gouge_volume: f64,
    pub gouge_cells: usize,
    pub max_gouge_depth: f64,
}

/// One pass of the measuring law over a grid, its part surface, and
/// the region the operation cares about (columns outside the footprint
/// are the rest of the machine's world, not this operation's verdict).
#[must_use]
pub fn measure(dex: &Dexel, part: &[f64], footprint: &Region, tol: f64) -> StockReport {
    let a2 = dex.cell * dex.cell;
    let mut rep = StockReport::default();
    for iy in 0..dex.ny {
        for ix in 0..dex.nx {
            let k = ix + iy * dex.nx;
            if !footprint.contains(dex.center(ix, iy)) {
                continue;
            }
            let sim = dex.top[k];
            let ptop = part[k];
            if ptop.is_finite() {
                if sim > ptop + tol {
                    rep.rest_volume += (sim - ptop) * a2;
                    rep.rest_cells += 1;
                    rep.max_rest_height = rep.max_rest_height.max(sim - ptop);
                }
                if sim < ptop - tol {
                    rep.gouge_volume += (ptop - sim) * a2;
                    rep.gouge_cells += 1;
                    rep.max_gouge_depth = rep.max_gouge_depth.max(ptop - sim);
                }
            }
            // Removed volume is gross: what stood between the stock
            // surface and what the tool left standing, taken out by
            // the path that cut it - air columns included, because
            // the stock block had material there to take.
            rep.removed_volume += (dex.stock_z - sim).max(0.0) * a2;
        }
    }
    rep
}
