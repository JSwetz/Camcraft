//! Swept-tool subtraction on the brick SDF field (M7.1 B): the
//! max-law layer, the closed-form tool solids, and the tube walk.
//! Split from `field.rs` by the 400-line law; the storage and the
//! sign live there, the construction in `field_gen.rs`, the solid's
//! own geometry in `field_solid.rs`, the ray-entry companion layer
//! (M7.1 C) in `field_ray.rs`.
//!
//! Provenance: SDF boolean subtraction (the distance-function CSG of
//! the published implicit-surface literature: stock minus tool is
//! `max(phi_stock, -phi_tool)`), the tool solids of RS-274 world
//! machining (capsule, cylinder, truncated cone - the same three the
//! dexel atoms sweep), and the chord reduction of this codebase's own
//! `arc_chords`. Nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **Subtraction is a running max, so it commutes.** One layer of
//!   cut values rides the bricks: `phi_stock(p) = max(phi_mesh(p),
//!   cut(p))`, `cut` = the max over all moves of the negated tool
//!   distance of the move that reached deepest here. Max is
//!   associative and commutative, so a replay in any order answers
//!   identically - the ROADMAP's "sequential cuts accumulate as a
//!   running max, order-independent by construction", made literal.
//!   (The exact engine this mirrors, the dexel, looks order-dependent
//!   only because a column stores one Z; its removal is a min over
//!   bottoms, which commutes the same way.)
//! - **The tool solid is read exactly, never voxelized.** A move
//!   sweeps the solid along a segment; the solid of every instant is
//!   convex and the sweep is a Minkowski sum, so the distance at a
//!   query point is a 1-D CONVEX minimum over the move parameter -
//!   golden section to far below a cell. The ball closes further: its
//!   sweep is a capsule with a closed form. This is the ROADMAP's
//!   inverted-trajectory law: "does the solid, moved BACKWARD along
//!   this segment, contain p", with the containment made exact.
//! - **The conservative direction is the same one everywhere.** The
//!   numerical minimum of a convex function is an upper bound of the
//!   true distance, so the cut never over-claims removal (ghost rest,
//!   the blessed over-report, never a false clean); and the max
//!   layering keeps the field's sign promise: material answers as
//!   material unless the tool demonstrably reached it, whatever the
//!   cell size or band width.
//! - **Tool conventions mirror the dexel atoms value for value.**
//!   Moves carry TIP positions: a ball's center path is the tip path
//!   lifted by `r` along the tool axis (the +Z lift when the move
//!   carries no pose stamp - M5.1's `axis` field IS the inverted
//!   trajectory input); a cone's apex is its tip (virtual sharp);
//!   arcs reduce to chords with Z interpolated uniformly (the helix
//!   law everywhere else uses); and rapids touch nothing (RS-274 G0).
//!   The flat endmill's flank is finite and explicit (`flank`): a
//!   cylinder, not the dexel's infinite column - above the flank the
//!   field shows rest where the column law shows air, and the
//!   reconciliation at declared tolerance (stage C) passes a flank
//!   that spans the stock to agree.
//! - **Total input.** A non-finite move endpoint, a non-positive or
//!   non-finite tool parameter, a nonsense axis, or a move whose
//!   swept window - clipped to the region a cut can matter in (M7.3,
//!   `field_batch`) - would span more than `MAX_CELLS_PER_MOVE` cells
//!   refuses by name before a single brick is written.

use super::field::{cell_of, SdfField};
use super::field_solid::{add, dist_pt_para, dot, pose_dist, scale, sub, tool_kind, RevKind, ToolKind};
use super::kernel::StockError;
use super::scan::dist_pt_seg3;

use crate::cl::{Move, MoveKind};
use crate::geo2d::Point2;
use crate::geometry::Point3;

/// Refusal ceiling for one chord's window scan (the seed law's
/// `MAX_CELLS_PER_FACET` in the cutter's clothing).
const MAX_CELLS_PER_MOVE: u64 = 1 << 22;

/// Golden-section iterations: the interval shrinks by 4.6e-9 per run,
/// far under any cell the field can ask about.
const GS_ITERS: u32 = 32;

/// The golden-section contraction ratio, (sqrt(5) - 1) / 2.
const PHI: f64 = 0.6180339887498949;

/// The golden section's final bracket, as a fraction of t's unit
/// interval, with room to spare: PHI^GS_ITERS is ~2e-7.
const GS_RESIDUAL: f64 = 1e-6;

/// The tool solids of the field's vocabulary - the same three the
/// dexel sweeps (`Dexel::subtract`, `subtract_ball`, `subtract_cone`).
#[derive(Clone, Copy, Debug)]
pub enum SdfTool {
    /// Ball nose of radius `r`; moves carry tip positions and the
    /// center rides `r` above along the axis. The bare sphere.
    Ball { r: f64 },
    /// Ball nose on a shank of its own radius, `shank` long above the
    /// center (M7.3): the capsule the column's ball atom implies -
    /// it removes everything above the ball's bottom - made finite.
    Shanked { r: f64, shank: f64 },
    /// Flat endmill: disc of `r`, flank of `flank` along the axis.
    Flat { r: f64, flank: f64 },
    /// Taper: cone of maximum cutting radius `r`, apex at the tip
    /// (virtual sharp), side slope `cot` = cotangent of the half
    /// angle from the axis, as the dexel's cone atom reads it.
    Taper { r: f64, cot: f64 },
}

/// One chord's swept solid, prepped for the walk (`field_ray` reads
/// it too - the ray-entry layer is this layer's companion).
pub(crate) enum Sweep {
    /// The capsule of a ball center path.
    Ball { c0: Point3, c1: Point3, r: f64 },
    /// The ball center path swept up the axis by `up` (the shank),
    /// dilated by `r`: a parallelogram's offset, closed form.
    Shanked { c0: Point3, c1: Point3, up: [f64; 3], r: f64 },
    /// The Minkowski sum of a tip path and a revolved profile.
    Revolve { a: Point3, b: Point3, kind: RevKind },
}

impl Sweep {
    /// Radius of a ball this solid fits in around any anchor point:
    /// the quick reject's slack. NOT the window's dilation - a
    /// spanning flank (M7.1 C) is a cylinder's bounding radius
    /// inflated into every axis, which would refuse payables.
    fn extent(&self) -> f64 {
        match self {
            Sweep::Ball { r, .. } => *r,
            Sweep::Shanked { r, up, .. } => r + dot(*up, *up).sqrt(),
            Sweep::Revolve { kind, .. } => match kind {
                RevKind::Flat { r, flank } => r.hypot(*flank),
                RevKind::Taper { r, cot } => r.hypot(r * cot),
            },
        }
    }

    /// The solid's own reach: (lateral, along-axis). A vertical tool
    /// is r wide and flank tall - the honest box the window dilates
    /// by, axis component for axis component. The along-axis reach
    /// runs one way, from the tip UP the axis (M7.3: a flank never
    /// reaches below its own tip); the ball's path is its center
    /// path, so its solid is the path dilated by `r` and nothing more.
    fn reach(&self) -> (f64, f64) {
        match self {
            Sweep::Ball { r, .. } => (*r, 0.0),
            Sweep::Shanked { r, up, .. } => (*r, dot(*up, *up).sqrt()),
            Sweep::Revolve { kind, .. } => match kind {
                RevKind::Flat { r, flank } => (*r, *flank),
                RevKind::Taper { r, cot } => (*r, r * cot),
            },
        }
    }

    /// The path endpoints the solid rides (tip path, or center path
    /// for the ball).
    fn path(&self) -> (Point3, Point3) {
        match self {
            Sweep::Ball { c0, c1, .. } | Sweep::Shanked { c0, c1, .. } => (*c0, *c1),
            Sweep::Revolve { a, b, .. } => (*a, *b),
        }
    }

    /// The lattice window this sweep can reach (`window_of`).
    pub(crate) fn window(&self, cell: f64, cap: f64, ax: [f64; 3]) -> [(i32, i32); 3] {
        window_of(self, cell, cap, ax)
    }

    /// How far `dist` may sit ABOVE the true distance: the golden
    /// section's final bracket times the path length (the anchor
    /// moves `|b - a|` per unit t, and one pose's distance is
    /// 1-Lipschitz in its anchor); the capsule is closed-form.
    pub(crate) fn residual(&self) -> f64 {
        match self {
            Sweep::Ball { .. } | Sweep::Shanked { .. } => 0.0,
            Sweep::Revolve { a, b, .. } => {
                let d = sub(*b, *a);
                dot(d, d).sqrt() * GS_RESIDUAL
            }
        }
    }

    /// A cheap LOWER bound of `dist` (a stadium, not the solid), used
    /// to skip window corners before the honest evaluation.
    pub(crate) fn stadium(&self, q: Point3) -> f64 {
        let (p0, p1) = self.path();
        dist_pt_seg3(q.x, q.y, q.z, p0, p1) - self.extent()
    }

    /// The distance from `q` to the swept solid: negative inside,
    /// and every evaluation of the convex 1-D minimum is an UPPER
    /// bound of the truth (the conservative half of the law above).
    pub(crate) fn dist(&self, ax: [f64; 3], q: Point3) -> f64 {
        match self {
            Sweep::Ball { c0, c1, r } => dist_pt_seg3(q.x, q.y, q.z, *c0, *c1) - r,
            Sweep::Shanked { c0, c1, up, r } => dist_pt_para(q, *c0, sub(*c1, *c0), *up) - r,
            Sweep::Revolve { a, b, kind } => {
                let d = sub(*b, *a);
                if dot(d, d) == 0.0 {
                    return pose_dist(ax, *a, q, kind); // a station: one pose
                }
                if let Some(t) = upright_flat_foot(ax, *a, d, q, kind) {
                    // Exact, then nudged a rounding-scale hair UP: the
                    // golden section's conservative direction kept, so
                    // a tie on the solid's face never claims removal.
                    let v = pose_dist(ax, add(*a, scale(d, t)), q, kind);
                    return v + 1e-12 + 1e-15 * v.abs();
                }
                // Golden section on the convex map t -> phi(q - (a + t*d)).
                let (mut lo, mut hi) = (0.0f64, 1.0f64);
                let mut c = hi - PHI * (hi - lo);
                let mut dd = lo + PHI * (hi - lo);
                let mut fc = pose_dist(ax, add(*a, scale(d, c)), q, kind);
                let mut fd = pose_dist(ax, add(*a, scale(d, dd)), q, kind);
                for _ in 0..GS_ITERS {
                    if fc < fd {
                        hi = dd;
                        dd = c;
                        fd = fc;
                        c = hi - PHI * (hi - lo);
                        fc = pose_dist(ax, add(*a, scale(d, c)), q, kind);
                    } else {
                        lo = c;
                        c = dd;
                        fc = fd;
                        dd = lo + PHI * (hi - lo);
                        fd = pose_dist(ax, add(*a, scale(d, dd)), q, kind);
                    }
                }
                fc.min(fd)
            }
        }
    }
}

impl SdfField {
    /// Subtract one CL move: the tool rides the move's path (its
    /// chords, arcs reduced under the uniform-Z helix law) and the
    /// cut layer takes the running max. Rapids touch nothing; invalid
    /// input refuses before any write.
    pub fn subtract(
        &mut self,
        m: &Move,
        tool: &SdfTool,
        chord_tol: f64,
    ) -> Result<(), StockError> {
        self.subtract_batch(std::slice::from_ref(m), tool, chord_tol)
    }

    /// Subtract a batch of moves cut by one tool - the same answer as
    /// one `subtract` per move, bit for bit (both layers commute), at
    /// one visit per dirty brick for the whole batch (M7.3's cost law,
    /// `field_batch`). Every move is validated and every chord's
    /// window proved payable BEFORE any write, so a refusal leaves the
    /// field exactly as it stood.
    pub fn subtract_batch(
        &mut self,
        moves: &[Move],
        tool: &SdfTool,
        chord_tol: f64,
    ) -> Result<(), StockError> {
        let kind = tool_kind(tool)?;
        if !chord_tol.is_finite() || chord_tol < 0.0 {
            return Err(StockError::Invalid(format!(
                "field subtract: chord tolerance {chord_tol} is not a finite non-negative number"
            )));
        }
        let mut sweeps: Vec<([f64; 3], Sweep)> = Vec::new();
        for m in moves {
            if !m.is_finite() {
                return Err(StockError::Invalid(
                    "field subtract: the move has a non-finite endpoint".into(),
                ));
            }
            if m.kind == MoveKind::Rapid {
                continue; // rapids touch nothing (RS-274 G0, the dexel's law)
            }
            let ax = match m.axis {
                None => [0.0, 0.0, 1.0],
                Some(a) => {
                    let nn = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
                    if !a[0].is_finite() || !a[1].is_finite() || !a[2].is_finite() || nn == 0.0 {
                        return Err(StockError::Invalid(format!(
                            "field subtract: axis {:?} is not a finite nonzero direction",
                            a
                        )));
                    }
                    [a[0] / nn, a[1] / nn, a[2] / nn]
                }
            };
            for (a, b) in chords(m, chord_tol) {
                let sw = match &kind {
                    ToolKind::Ball(r) => {
                        let off = scale(ax, *r);
                        Sweep::Ball { c0: add(a, off), c1: add(b, off), r: *r }
                    }
                    ToolKind::Shanked { r, shank } => {
                        let off = scale(ax, *r);
                        Sweep::Shanked { c0: add(a, off), c1: add(b, off), up: scale(ax, *shank), r: *r }
                    }
                    ToolKind::Flat { r, flank } => Sweep::Revolve {
                        a,
                        b,
                        kind: RevKind::Flat { r: *r, flank: *flank },
                    },
                    ToolKind::Taper { r, cot } => Sweep::Revolve {
                        a,
                        b,
                        kind: RevKind::Taper { r: *r, cot: *cot },
                    },
                };
                // The ceiling bounds the WORK: the window as clipped
                // to the cut region, the only cells the walk can visit.
                let work = self.clipped(sw.window(self.cell, self.cap, ax)).map_or(0, window_cells);
                if work > MAX_CELLS_PER_MOVE {
                    return Err(StockError::Invalid(format!(
                        "field subtract: a chord's swept window spans more than {MAX_CELLS_PER_MOVE} cells at cell size {} - the move is too long for one field update; segment it",
                        self.cell
                    )));
                }
                sweeps.push((ax, sw));
            }
        }
        self.cut_batch(&sweeps);
        Ok(())
    }
}

/// The exact minimizer of `t -> pose_dist` for an upright flat tool
/// on the two moves 2.5-D paths are made of (M7.3's per-cell cost):
/// on a level move `zl` is fixed and the distance only grows with the
/// radial offset, so the minimum sits at the XY foot of the path; on
/// a plunge `rho` is fixed and the box distance is convex in `zl`
/// with its minimum at mid-flank. `None` sends every other move to
/// the golden section.
fn upright_flat_foot(ax: [f64; 3], a: Point3, d: [f64; 3], q: Point3, kind: &RevKind) -> Option<f64> {
    let RevKind::Flat { flank, .. } = kind else {
        return None;
    };
    if ax != [0.0, 0.0, 1.0] {
        return None;
    }
    if d[2] == 0.0 {
        let dd = d[0] * d[0] + d[1] * d[1];
        let t = ((q.x - a.x) * d[0] + (q.y - a.y) * d[1]) / dd;
        return t.is_finite().then(|| t.clamp(0.0, 1.0));
    }
    if d[0] == 0.0 && d[1] == 0.0 {
        let t = (q.z - a.z - 0.5 * flank) / d[2];
        return t.is_finite().then(|| t.clamp(0.0, 1.0));
    }
    None
}

/// The lattice window one sweep can reach: the path's AABB swept
/// along the tool axis by the solid's axial reach (one way: tip to
/// top, M7.3), then dilated by the lateral reach, the cap, and one
/// cell. A spanning flank reaches far up its axis and not one micron
/// sideways or below its tip; dilating every axis by the bounding
/// radius (the M7.1 B law), or the axial reach both ways (M7.1 C),
/// made a vertical tool's window as deep as it is tall.
fn window_of(sw: &Sweep, cell: f64, cap: f64, ax: [f64; 3]) -> [(i32, i32); 3] {
    let (p0, p1) = sw.path();
    let (lat, axial) = sw.reach();
    let span = |lo: f64, hi: f64, u: usize| {
        let e = axial * ax[u];
        let pad = lat + cap + cell;
        (
            cell_of(lo.min(hi) + e.min(0.0) - pad, cell),
            cell_of(lo.max(hi) + e.max(0.0) + pad, cell),
        )
    };
    [
        span(p0.x, p1.x, 0),
        span(p0.y, p1.y, 1),
        span(p0.z, p1.z, 2),
    ]
}

/// Cells in a chord's window: saturating, so an absurd dilation
/// answers absurd (and refuses), never an overflowed count.
fn window_cells(w: [(i32, i32); 3]) -> u64 {
    let cnt = |s: (i32, i32)| (s.1 as i64 - s.0 as i64 + 1).max(0) as u64;
    cnt(w[0])
        .saturating_mul(cnt(w[1]))
        .saturating_mul(cnt(w[2]))
}

/// The move's path as straight chords: one segment for a linear move,
/// the sagitta chords with Z interpolated uniformly for an arc (the
/// helix law `Dexel::subtract` reads).
fn chords(m: &Move, chord_tol: f64) -> Vec<(Point3, Point3)> {
    match m.arc {
        None => vec![(m.from, m.to)],
        Some(arc) => {
            let a2 = Point2::new(m.from.x, m.from.y);
            let b2 = Point2::new(m.to.x, m.to.y);
            let cs = crate::cl::arc_chords(a2, b2, arc, chord_tol);
            let n = cs.len().max(1) as f64;
            let dz = m.to.z - m.from.z;
            cs.into_iter()
                .enumerate()
                .map(|(j, (ca, cb))| {
                    (
                        Point3::new(ca.x, ca.y, m.from.z + dz * (j as f64 / n)),
                        Point3::new(cb.x, cb.y, m.from.z + dz * ((j + 1) as f64 / n)),
                    )
                })
                .collect()
        }
    }
}
