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
//! - **The ball atom is exact, not sampled** (`cut_ball`): a sphere's
//!   bottom above a cell is a convex function of the chord parameter,
//!   so its minimum is at a window end or at one stationary point that
//!   a sign-guarded quadratic finds in closed form. Endpoints alone
//!   would over-report rest (and could hide a gouge); the full law is
//!   cheap and never guesses.
//! - **Part surface is read from the mesh**, facet-plane interpolation
//!   at cell centers over each facet's XY bounding box: the highest
//!   surface covering a cell is that column's part top (`-inf` when no
//!   facet covers it - air everywhere along the column). Rest material
//!   and gouges are both measured against this, so the sim's verdict
//!   uses the model, not a second hand-entered surface.
//! - **The program sim chains ops, never re-plans them** (M2.3): the
//!   same conservative atoms, fed the cached paths in program order,
//!   so the grid answers "what does this whole NC program leave
//!   standing" - the stock-comparison deviation and its color map read
//!   that answer, and the post gate judges it once at the end.
//! - **Deviation is located, not just counted** (M2.3): the program
//!   report carries the XY and the sim's own Z of its worst rest and
//!   worst gouge cells, because "the toolpath is trusted before the
//!   machine sees it" means a human can point at where the program is
//!   trusted to be right, or wrong. Columns that answer no verdict
//!   (air in the stock, or air in the model) carry NaN, never a zero
//!   dressed up as a clean bill of health.

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
                // Parameter interval where the disc reaches q: the
                // shared reach law (see `reach`), exact on the segment.
                let Some((t0, t1)) = reach(q.x - a.x, q.y - a.y, dx, dy, r) else {
                    continue;
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

    /// The ball-nose atom: a sphere of radius `r` whose CENTER travels
    /// the straight, Z-linear chord `a -> b`. Above a cell center `q`,
    /// at instant `t`, the sphere's bottom sits at
    /// `z(t) - sqrt(r^2 - d(t)^2)` with `d` the horizontal distance
    /// from `q` to the center path; the cut is the minimum of that
    /// over the reach window. Endpoints do NOT suffice here (unlike
    /// `cut_chord`, where the bottom is constant along the chord): the
    /// bottom function is convex in `t`, so its minimum is at a window
    /// end or at its single interior stationary point - `f'(t) = k +
    /// (t*D^2 - w.D)/sqrt(r^2 - d^2)`, whose squaring gives the
    /// quadratic below, and the unsquared law keeps only roots where
    /// `(w.D - t*D^2)` shares `k`'s sign. The XY-nearest instant `t_c`
    /// is always evaluated too, so a root lost to rounding degrades
    /// toward the chord's mid-sagitta, never past an endpoint.
    pub fn cut_ball(&mut self, a: Point3, b: Point3, r: f64) {
        if !(a.is_finite() && b.is_finite()) || !(r.is_finite() && r >= 0.0) {
            return; // the cut_chord gate: validation lives upstream
        }
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let dd = dx * dx + dy * dy;
        let k = b.z - a.z;
        let fx = |v: f64| ((v - self.x0) / self.cell).floor();
        let fy = |v: f64| ((v - self.y0) / self.cell).floor();
        let lo_x = fx(a.x.min(b.x) - r).max(0.0) as usize;
        let hi_x = fx(a.x.max(b.x) + r).clamp(0.0, self.nx as f64 - 1.0) as usize;
        let lo_y = fy(a.y.min(b.y) - r).max(0.0) as usize;
        let hi_y = fy(a.y.max(b.y) + r).clamp(0.0, self.ny as f64 - 1.0) as usize;
        if lo_x >= self.nx || lo_y >= self.ny {
            return;
        }
        let r2 = r * r;
        for iy in lo_y..=hi_y {
            for ix in lo_x..=hi_x {
                let q = self.center(ix, iy);
                let (wx, wy) = (q.x - a.x, q.y - a.y);
                let Some((t0, t1)) = reach(wx, wy, dx, dy, r) else {
                    continue;
                };
                let f = |t: f64| {
                    let ex = wx - dx * t;
                    let ey = wy - dy * t;
                    a.z + k * t - (r2 - (ex * ex + ey * ey)).max(0.0).sqrt()
                };
                let t_c = if dd > f64::EPSILON {
                    (wx * dx + wy * dy) / dd
                } else {
                    0.0
                };
                let mut cut = f(t0).min(f(t1)).min(f(t_c.clamp(t0, t1)));
                if dd > f64::EPSILON {
                    let wd = wx * dx + wy * dy;
                    let s = dd + k * k;
                    let qa = dd * s;
                    let qb = -2.0 * wd * s;
                    let qc = wd * wd + k * k * (wx * wx + wy * wy - r2);
                    let mut disc = qb * qb - 4.0 * qa * qc;
                    if disc < 0.0 && disc > -qb * qb * 1e-12 {
                        disc = 0.0; // a planar double root erased by rounding
                    }
                    if disc >= 0.0 && disc.is_finite() {
                        let sd = disc.sqrt();
                        for t in [(-qb - sd) / (2.0 * qa), (-qb + sd) / (2.0 * qa)] {
                            if t > t0 && t < t1 && (wd - t * dd) * k >= 0.0 {
                                cut = cut.min(f(t));
                            }
                        }
                    }
                }
                let col = ix + iy * self.nx;
                if cut < self.top[col] {
                    self.top[col] = cut;
                }
            }
        }
    }

    /// Ball-nose version of `subtract`: moves carry TIP positions (the
    /// whole codebase stores toolpaths tip-first), so the CENTER path
    /// is the tip path lifted by `r` in Z and the swept solid is a
    /// sphere at every instant. Arcs reduce to chords under the same
    /// uniform-Z helix law as `subtract`.
    pub fn subtract_ball(&mut self, m: &Move, r: f64, chord_tol: f64) {
        if m.kind == MoveKind::Rapid || !m.is_finite() {
            return;
        }
        match m.arc {
            None => self.cut_ball(
                Point3::new(m.from.x, m.from.y, m.from.z + r),
                Point3::new(m.to.x, m.to.y, m.to.z + r),
                r,
            ),
            Some(arc) => {
                let a2 = Point2::new(m.from.x, m.from.y);
                let b2 = Point2::new(m.to.x, m.to.y);
                let cs = crate::cl::arc_chords(a2, b2, arc, chord_tol);
                let n = cs.len().max(1) as f64;
                let dz = m.to.z - m.from.z;
                for (j, (ca, cb)) in cs.into_iter().enumerate() {
                    let za = m.from.z + dz * (j as f64 / n) + r;
                    let zb = m.from.z + dz * ((j + 1) as f64 / n) + r;
                    self.cut_ball(
                        Point3::new(ca.x, ca.y, za),
                        Point3::new(cb.x, cb.y, zb),
                        r,
                    );
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

/// The parameter interval of a chord `0 -> d` whose tool disc of
/// radius `r` reaches a point offset `w` from the start - the reach
/// law shared by the flat and ball kernels so both inherit one
/// phantom rule: the window is on the SEGMENT, not the infinite line,
/// and an interval that falls entirely outside `[0, 1]` stays empty
/// (clamping it open would invent a cut at the nearest endpoint - the
/// exact bug the M2.1b phantom test pins). A degenerate `d` (plunge)
/// is its own interval, gated by distance.
pub fn reach(wx: f64, wy: f64, dx: f64, dy: f64, r: f64) -> Option<(f64, f64)> {
    let l2 = dx * dx + dy * dy;
    if l2 <= f64::EPSILON {
        if wx.hypot(wy) > r {
            None
        } else {
            Some((0.0, 1.0))
        }
    } else {
        let t_c = (wx * dx + wy * dy) / l2;
        let cross = wx * dy - wy * dx;
        let d_perp2 = (cross * cross) / l2;
        if d_perp2 > r * r {
            return None;
        }
        let half = (r * r - d_perp2).sqrt() / l2.sqrt();
        let lo = (t_c - half).max(0.0);
        let hi = (t_c + half).min(1.0);
        if lo > hi {
            None
        } else {
            Some((lo, hi))
        }
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

/// The envelope verdict for a finishing path (M2.2a), on a stated
/// lattice, measured not assumed. Two truths, each read by the law
/// that is exact for it. Gouge is Dexel truth: every ball's lowest
/// point over a column, and a floor below the model surface is depth
/// removed that must not be. Rest is distance truth at RIDGES: a
/// model surface point that stands within the pass's promise band
/// (`r + tol + floor` of at least two chords the center path ran
/// over), and how far outside every ball it stood. Two band contacts
/// is exactly what "this op rolled a stepdown ladder over you"
/// promises, so what stands outside both is scallop left on the
/// pass's own word. A point with one graze (a lone ring's annulus,
/// the box's footprint square) or none (a flat shelf, the crown
/// above the top ring, untouched walls) was never promised anything
/// by this pass: its leftover is rest machining's jurisdiction
/// (M2.3), law there, not a silence here. Gating rest by "this pass
/// cut this column" instead was tried and is dead code - a ball
/// reaching a column's surface is the same event as a center standing
/// within `r + floor` of that point (prove it: `cut <= t + floor`
/// rearranges to `dist^2 <= (r + floor)^2`), so it can never report
/// above tolerance. Proved, not observed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EnvelopeReport {
    /// Worst depth by which the swept envelope cut BELOW the model
    /// surface (0.0 when nothing was cut too deep).
    pub gouge_mm: f64,
    /// Worst distance the model surface stood OUTSIDE the swept
    /// envelope at a ridge the pass promised to roll over (two band
    /// contacts; single grazes and untouched columns are law, not
    /// this number). M2.2d: the union with the solid truth - material
    /// standing hidden under an occluding top grows this number too,
    /// measured by the same distance law. The union is in the FIELD
    /// only; the ridge predicate above is untouched by it.
    pub rest_mm: f64,
    /// The lattice's cell size, so a caller can state the
    /// quantization floor of these numbers honestly.
    pub cell: f64,
    /// XY of the largest single deviation.
    pub at: Point2,
}

pub fn envelope_scan(
    facets: &[Triangle],
    moves: &[Move],
    r: f64,
    tol: f64,
) -> Result<EnvelopeReport, StockError> {
    if !(r.is_finite() && r > 0.0) {
        return Err(StockError::Invalid(format!(
            "envelope ball radius {r} is not finite-positive"
        )));
    }
    if !tol.is_finite() {
        return Err(StockError::Invalid(format!(
            "envelope tolerance {tol} is not finite"
        )));
    }
    let tol = tol.max(0.0);
    let (mut bx0, mut by0, mut bx1, mut by1) =
        (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for t in facets {
        for v in [t.a, t.b, t.c] {
            if !(v.x.is_finite() && v.y.is_finite() && v.z.is_finite()) {
                return Err(StockError::Invalid("model has a non-finite vertex".into()));
            }
            bx0 = bx0.min(v.x);
            bx1 = bx1.max(v.x);
            by0 = by0.min(v.y);
            by1 = by1.max(v.y);
        }
    }
    if facets.is_empty() || bx1 <= bx0 || by1 <= by0 {
        return Ok(EnvelopeReport::default()); // no footprint to measure on
    }
    // One eighth of the tool radius sees a scallop the tolerance must
    // keep; the floor and the span growth keep the grid honest on
    // dust motes and stadiums alike (MAX_CELLS discipline, applied
    // before allocation, never after).
    let mut cell = (r * 0.125).max(1e-4);
    cell = cell.max((bx1 - bx0) / 1023.0).max((by1 - by0) / 1023.0);
    let margin = r + cell;
    let (x0, y0) = (bx0 - margin, by0 - margin);
    let nx = (((bx1 - bx0 + 2.0 * margin) / cell).ceil() as usize).max(1);
    let ny = (((by1 - by0 + 2.0 * margin) / cell).ceil() as usize).max(1);
    if nx > MAX_CELLS_PER_AXIS || ny > MAX_CELLS_PER_AXIS {
        return Err(StockError::Invalid(format!(
            "envelope grid would be {nx}x{ny} cells; at most {MAX_CELLS_PER_AXIS} per axis"
        )));
    }
    // The grid starts as ALL AIR: the sweep writes every cut it makes
    // and nothing else, so a finite column top after the sweep is
    // exactly "the path reached this column". Dexel::block refuses
    // non-finite tops (a stock surface must be real), hence the
    // literal - same shape, stated reason.
    let mut dex = Dexel {
        x0,
        y0,
        cell,
        nx,
        ny,
        stock_z: f64::INFINITY,
        top: vec![f64::INFINITY; nx * ny],
    };
    for m in moves {
        dex.subtract_ball(m, r, (cell * 0.25).max(1e-9));
    }
    let part = part_surface(facets, x0, y0, cell, nx, ny);
    let floor = cell * cell / (2.0 * r); // the sagitta one cell can hide
    let mut rep = EnvelopeReport {
        cell,
        at: Point2::new(0.0, 0.0),
        ..Default::default()
    };
    let mut worst = 0.0f64;
    // Gouge: the Dexel truth. A column's floor is the lowest point any
    // ball held over it; below the surface means below means removed.
    for iy in 0..ny {
        for ix in 0..nx {
            let k = ix + iy * nx;
            let (cut, t) = (dex.top[k], part[k]);
            if !cut.is_finite() || !t.is_finite() {
                continue; // never swept, or nothing there to deviate from
            }
            let dev = t - cut;
            if dev > rep.gouge_mm {
                rep.gouge_mm = dev;
                if dev > worst {
                    worst = dev;
                    rep.at = dex.center(ix, iy);
                }
            }
        }
    }
    // Rest: the distance truth at ridges - surface points the center
    // path passed by twice inside the promise band. The band is what
    // the stepdown law swore, so a point outside every ball there is
    // this path's own failure, while a once-grazed or never-touched
    // point is another op's job (the law above).
    let band = r + tol + floor;
    let segs = center_segments(moves, r, (cell * 0.25).max(1e-9));
    // A far segment's 3D distance cannot stand inside the band, so
    // its XY window rejects it before the distance is paid. A
    // column's dmin only feeds a report when two segments proved
    // near, and those are never the skipped ones - the filter is
    // exact, not an approximation.
    let boxes: Vec<(f64, f64, f64, f64)> = segs
        .iter()
        .map(|(a, b)| (a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y)))
        .collect();
    for iy in 0..ny {
        for ix in 0..nx {
            let t = part[ix + iy * nx];
            if !t.is_finite() {
                continue; // no model surface at this column
            }
            let q = dex.center(ix, iy);
            let (mut dmin, mut near) = (f64::INFINITY, 0u8);
            for ((a, b), (x0, x1, y0, y1)) in segs.iter().zip(&boxes) {
                if q.x < x0 - band
                    || q.x > x1 + band
                    || q.y < y0 - band
                    || q.y > y1 + band
                {
                    continue;
                }
                let d = dist_pt_seg3(q.x, q.y, t, *a, *b);
                if d <= band {
                    near = near.saturating_add(1);
                }
                if d < dmin {
                    dmin = d;
                }
            }
            if near < 2 {
                continue; // one graze or none: no promise kept or broken here
            }
            let rest = dmin - r - floor;
            if rest > rep.rest_mm {
                rep.rest_mm = rest;
                if rest > worst {
                    worst = rest;
                    rep.at = q;
                }
            }
        }
    }
    // Solid truth behind the height field (M2.2d): material tops the
    // visible surface OCCLUDES - a ceiling's shadow over a floor -
    // judged by this same distance law. The union lives in the FIELD:
    // the ridge numbers above stand as computed, predicate untouched.
    let (hidden, hidden_at) = hidden_rest(facets, &segs, &part, x0, y0, cell, nx, ny, r, floor);
    if hidden > rep.rest_mm {
        rep.rest_mm = hidden;
        rep.at = hidden_at;
    }
    Ok(rep)
}

/// The hidden-rest scan (M2.2d), paying M2.2b's open debt. Per
/// column the mesh's SOLID truth is paired (`solid_rest` law (a));
/// every material top below the height field's own top minus the
/// quantization floor is occluded material - invisible to a
/// Z-following measure by construction. Each occluded top is then
/// judged by the ridge law's own distance truth: nearest ball
/// center minus the ball minus the floor, so the two reports are
/// one yardstick reading two depths, never two predicates. An odd
/// column is an open shell - unknown, and unknown never reports
/// material (law (b)). Work is binned facet-outer (the
/// `part_surface` shape); over budget the answer is the stated
/// zero - the height field's debt stays on the books, never
/// guessed at.
#[allow(clippy::too_many_arguments)] // the scan's whole geometry frame, threaded once from envelope_scan
fn hidden_rest(
    facets: &[Triangle],
    segs: &[(Point3, Point3)],
    part: &[f64],
    x0: f64,
    y0: f64,
    cell: f64,
    nx: usize,
    ny: usize,
    r: f64,
    floor: f64,
) -> (f64, Point2) {
    const BUDGET_CELLS: usize = 1_200_000; // ~30 MB of bin headers
    const BUDGET_CLAIMS: usize = 40_000_000; // plane claims within them
    if segs.is_empty() || facets.is_empty() || nx * ny > BUDGET_CELLS {
        return (0.0, Point2::new(0.0, 0.0));
    }
    let mut bins: Vec<Vec<crate::solid_rest::Claim>> = (0..nx * ny).map(|_| Vec::new()).collect();
    let mut claims = 0usize;
    for t in facets {
        let (a, b, c) = (t.a, t.b, t.c);
        if !(a.is_finite() && b.is_finite() && c.is_finite()) {
            continue;
        }
        let fx = |v: f64| ((v - x0) / cell).floor();
        let i0 = fx(a.x.min(b.x).min(c.x)).max(0.0) as usize;
        let i1 = fx(a.x.max(b.x).max(c.x)).clamp(0.0, nx as f64 - 1.0) as usize;
        let j0 = fy0(a.y.min(b.y).min(c.y), y0, cell).max(0.0) as usize;
        let j1 = fy0(a.y.max(b.y).max(c.y), y0, cell).clamp(0.0, ny as f64 - 1.0) as usize;
        if i0 >= nx || j0 >= ny {
            continue;
        }
        for iy in j0..=j1 {
            for ix in i0..=i1 {
                let (cx, cy) = (x0 + (ix as f64 + 0.5) * cell, y0 + (iy as f64 + 0.5) * cell);
                if let Some(cl) = crate::solid_rest::plane_claim(&a, &b, &c, cx, cy) {
                    bins[ix + iy * nx].push(cl);
                    claims += 1;
                    if claims > BUDGET_CLAIMS {
                        return (0.0, Point2::new(0.0, 0.0)); // over budget: the stated zero
                    }
                }
            }
        }
    }
    let mut best = 0.0;
    let mut at = Point2::new(0.0, 0.0);
    for iy in 0..ny {
        for ix in 0..nx {
            let bin = &bins[ix + iy * nx];
            if bin.is_empty() {
                continue;
            }
            let t = part[ix + iy * nx];
            if !t.is_finite() {
                continue; // the field sees nothing here; nor has this scan's ground
            }
            let crate::solid_rest::Crossings::Paired(z) =
                crate::solid_rest::crossings_from_claims(bin)
            else {
                continue; // open shell: unknown never reports material
            };
            // Interval tops, high to low: the highest is the visible
            // surface; every lower one hides under a top above it.
            for c in z.as_chunks::<2>().0 {
                let hi = c[1];
                if hi >= t - floor {
                    continue; // not occluded: the ridge law already owns this top
                }
                let (cx, cy) = (x0 + (ix as f64 + 0.5) * cell, y0 + (iy as f64 + 0.5) * cell);
                let mut dmin = f64::INFINITY;
                for (sa, sb) in segs {
                    let d = dist_pt_seg3(cx, cy, hi, *sa, *sb);
                    if d < dmin {
                        dmin = d;
                    }
                }
                let rest = (dmin - r - floor).max(0.0);
                if rest > best {
                    best = rest;
                    at = Point2::new(cx, cy);
                }
            }
        }
    }
    (best, at)
}

/// The ball-center polyline the path sweeps: rapids excluded, arcs
/// reduced to their sagitta chords with Z interpolated, everything
/// lifted by `r` so the stored tip coordinates read as centers - the
/// exact discretization `subtract_ball` cuts with, so a chord the
/// Dexel honoured is a chord the distance law sees.
fn center_segments(moves: &[Move], r: f64, chord_tol: f64) -> Vec<(Point3, Point3)> {
    let mut segs = Vec::new();
    for m in moves {
        if m.kind == MoveKind::Rapid || !m.is_finite() {
            continue;
        }
        match m.arc {
            None => segs.push((
                Point3::new(m.from.x, m.from.y, m.from.z + r),
                Point3::new(m.to.x, m.to.y, m.to.z + r),
            )),
            Some(arc) => {
                let a2 = Point2::new(m.from.x, m.from.y);
                let b2 = Point2::new(m.to.x, m.to.y);
                let cs = crate::cl::arc_chords(a2, b2, arc, chord_tol);
                let n = cs.len().max(1) as f64;
                let dz = m.to.z - m.from.z;
                for (j, (ca, cb)) in cs.into_iter().enumerate() {
                    segs.push((
                        Point3::new(ca.x, ca.y, m.from.z + dz * (j as f64 / n) + r),
                        Point3::new(cb.x, cb.y, m.from.z + dz * ((j + 1) as f64 / n) + r),
                    ));
                }
            }
        }
    }
    segs
}

/// 3D point-to-segment distance - the honest "did any ball of the
/// sweep reach this surface point" test a per-column floor cannot
/// answer on tilted walls.
fn dist_pt_seg3(px: f64, py: f64, pz: f64, a: Point3, b: Point3) -> f64 {
    let (ex, ey, ez) = (b.x - a.x, b.y - a.y, b.z - a.z);
    let len2 = ex * ex + ey * ey + ez * ez;
    let t = if len2 > 0.0 {
        ((px - a.x) * ex + (py - a.y) * ey + (pz - a.z) * ez) / len2
    } else {
        0.0
    }
    .clamp(0.0, 1.0);
    let (dx, dy, dz) = (px - a.x - ex * t, py - a.y - ey * t, pz - a.z - ez * t);
    dx.hypot(dy).hypot(dz)
}

// ---- M2.3 stage A: the program-level sim ------------------------------

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

/// What the finished program leaves, per column, against the model's
/// own surface: `top - part` where both stand (positive is rest,
/// negative is gouge), NaN where either is air. The aggregates locate
/// themselves - a worst value with no XY cannot be shown, jumped to,
/// or argued with, and "trusted before the machine sees it" is a
/// claim about a WHERE.
#[derive(Clone, Debug, PartialEq)]
pub struct Deviation {
    pub cells_x: usize,
    pub cells_y: usize,
    pub cell: f64,
    pub tol: f64,
    pub dev: Vec<f64>,
    pub rest_cells: usize,
    pub gouge_cells: usize,
    pub rest_volume: f64,
    pub gouge_volume: f64,
    pub max_rest: f64,
    pub max_gouge: f64,
    /// The sim's own surface at the worst cell - the material that
    /// stands, or the dent that was cut.
    pub max_rest_at: Option<Point3>,
    pub max_gouge_at: Option<Point3>,
}

impl Deviation {
    /// The program-level verdict: nothing stands over tolerance and
    /// nothing is cut below it. The color map's classes (NaN
    /// unjudged, |v| <= tol clean, positive rest, negative gouge)
    /// are this law read one column at a time by the viewer.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.max_rest <= self.tol && self.max_gouge <= self.tol
    }
}

/// Measure the grid against the model on the grid's own lattice.
pub fn deviate(dex: &Dexel, facets: &[Triangle], tol: f64) -> Result<Deviation, StockError> {
    if !tol.is_finite() {
        return Err(StockError::Invalid(format!("tolerance {tol} is not finite")));
    }
    let tol = tol.max(0.0);
    let part = part_surface(facets, dex.x0, dex.y0, dex.cell, dex.nx, dex.ny);
    let a2 = dex.cell * dex.cell;
    let mut d = Deviation {
        cells_x: dex.nx,
        cells_y: dex.ny,
        cell: dex.cell,
        tol,
        dev: vec![f64::NAN; dex.nx * dex.ny],
        rest_cells: 0,
        gouge_cells: 0,
        rest_volume: 0.0,
        gouge_volume: 0.0,
        max_rest: 0.0,
        max_gouge: 0.0,
        max_rest_at: None,
        max_gouge_at: None,
    };
    for iy in 0..dex.ny {
        for ix in 0..dex.nx {
            let k = ix + iy * dex.nx;
            let (sim, p) = (dex.top[k], part[k]);
            if !(sim.is_finite() && p.is_finite()) {
                continue; // NaN stays: no material, or nothing to judge against
            }
            let v = sim - p;
            d.dev[k] = v;
            if v > tol {
                d.rest_cells += 1;
                d.rest_volume += v * a2;
                if v > d.max_rest {
                    d.max_rest = v;
                    d.max_rest_at = Some(Point3::new(dex.center(ix, iy).x, dex.center(ix, iy).y, sim));
                }
            } else if v < -tol {
                d.gouge_cells += 1;
                d.gouge_volume += -v * a2;
                if -v > d.max_gouge {
                    d.max_gouge = -v;
                    d.max_gouge_at = Some(Point3::new(dex.center(ix, iy).x, dex.center(ix, iy).y, sim));
                }
            }
        }
    }
    Ok(d)
}

// ---- M2.3 stage B: the grid as a viewer can show it --------------------

/// Per-column color classes of the deviation field: 0.0 clean (within
/// tolerance, and the untouched flat top of a program that simply owns
/// that column), 1.0 rest, 2.0 gouge, 3.0 unjudged - NaN gets its own
/// class because a verdict the sim cannot make must not render as the
/// absence of a problem. The four numbers are the kernel's side of a
/// colormap; the viewer picks the paints (and says what they mean).
#[must_use]
pub fn color_classes(dev: &Deviation) -> Vec<f32> {
    dev.dev
        .iter()
        .map(|v| {
            if v.is_nan() {
                3.0
            } else if *v > dev.tol {
                1.0
            } else if *v < -dev.tol {
                2.0
            } else {
                0.0
            }
        })
        .collect()
}

/// A sheet through the material columns of the grid: one top quad per
/// standing column, and a vertical wall across every step to a lower
/// neighbor. Air neighbors leave the edge open - the grid stores tops,
/// never bottoms, and a sheet that grew a floor it does not know would
/// be the grid asserting stock it never saw. A wall stands for its
/// step, so it carries the class of the upper column: the one that
/// reads too high, or was cut too deep, is the column the verdict is
/// about. Positions are world mm on the grid's own lattice.
#[derive(Debug)]
pub struct SimSurface {
    pub verts: Vec<Point3>,
    /// One class per vertex, from `color_classes`' law - the shader's
    /// color map reads it exactly like `sel` reads selection.
    pub vcls: Vec<f32>,
    /// Triangle indices into `verts`, front face by right-hand rule.
    pub tris: Vec<[u32; 3]>,
}

impl Dexel {
    /// Build the sheet. `classes` is `color_classes(&deviate(..))` (or
    /// all-zeros for a plain stock surface); a column whose entry is
    /// missing reads as class 0 - the surface never invents a verdict.
    #[must_use]
    pub fn surface(&self, classes: &[f32]) -> SimSurface {
        let at = |ix: usize, iy: usize| -> f64 { self.top[ix + iy * self.nx] };
        let cls = |ix: usize, iy: usize| -> f32 {
            classes.get(ix + iy * self.nx).copied().unwrap_or(0.0)
        };
        let mut s = SimSurface { verts: Vec::new(), vcls: Vec::new(), tris: Vec::new() };
        let quad = |s: &mut SimSurface, a: Point3, b: Point3, c: Point3, ca: f32, cb: f32, cc: f32| {
            let n = s.verts.len() as u32;
            s.verts.extend([a, b, c]);
            s.vcls.extend([ca, cb, cc]);
            s.tris.push([n, n + 1, n + 2]);
        };
        for iy in 0..self.ny {
            for ix in 0..self.nx {
                let z = at(ix, iy);
                if !z.is_finite() {
                    continue; // no material stood here: no sheet
                }
                let c = cls(ix, iy);
                let x0 = self.x0 + ix as f64 * self.cell;
                let y0 = self.y0 + iy as f64 * self.cell;
                let (x1, y1) = (x0 + self.cell, y0 + self.cell);
                // Top: two CCW triangles seen from +Z, one cell.
                quad(
                    &mut s,
                    Point3::new(x0, y0, z),
                    Point3::new(x1, y0, z),
                    Point3::new(x1, y1, z),
                    c,
                    c,
                    c,
                );
                quad(
                    &mut s,
                    Point3::new(x0, y0, z),
                    Point3::new(x1, y1, z),
                    Point3::new(x0, y1, z),
                    c,
                    c,
                    c,
                );
                // East wall: this column stands above a finite lower one.
                if ix + 1 < self.nx {
                    let zn = at(ix + 1, iy);
                    if zn.is_finite() && zn < z - 1e-12 {
                        let (a, b) = (Point3::new(x1, y0, z), Point3::new(x1, y1, z));
                        let (d, e) = (Point3::new(x1, y0, zn), Point3::new(x1, y1, zn));
                        quad(&mut s, a, d, b, c, c, c);
                        quad(&mut s, b, d, e, c, c, c);
                    }
                }
                // South wall.
                if iy > 0 {
                    let zn = at(ix, iy - 1);
                    if zn.is_finite() && zn < z - 1e-12 {
                        let (a, b) = (Point3::new(x0, y0, z), Point3::new(x1, y0, z));
                        let (d, e) = (Point3::new(x0, y0, zn), Point3::new(x1, y0, zn));
                        quad(&mut s, a, d, b, c, c, c);
                        quad(&mut s, b, d, e, c, c, c);
                    }
                }
            }
        }
        s
    }
}

// ---- M2.3 stage B: the document's program, simulated -------------------

/// The deviation verdict tolerance: a column reading within this of the
/// model is clean for the color map. Deliberately a display law, not a
/// re-use of any operation's authoring tolerance - the op's tolerance is
/// what its generator promised to hold; the map answers "what stands
/// after the WHOLE program", and per-op tolerances would make one
/// tolerance field edit re-color columns another op authored. 10 um is
/// the finest scallop the shipped lattice can resolve visually.
pub const DEV_TOL: f64 = 0.01;

/// One run of the program sim: the per-column verdict and the sheet
/// that paints it. Held by the UI as view state (the `Sim`/`PostView`
/// exemption in AGENTS.md) - it is frozen evidence of one specific
/// program state, like `PostView`'s program text, so an edit that
/// makes it stale never silently re-runs: re-arming re-judges.
#[derive(Debug)]
pub struct ProgramSim {
    pub dev: Deviation,
    pub surface: SimSurface,
}
