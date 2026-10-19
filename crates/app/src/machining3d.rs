//! Mesh-driven 3D pocketing (M2.1a): the level ladder meets the model.
//!
//! Provenance: Z-level pocketing is standard roughing practice, specced
//! from published CAM literature and general machining knowledge; the
//! region algebra is our Clipper2 wrapper and per-level sectioning our
//! marching-slice cutter (`geometry::slice`). The helical entry follows
//! RS-274's G2/G3-with-Z interpolation vocabulary.
//!
//! ## The level law (why every cut is legal by construction)
//!
//! The model is the FINAL part: inside the pocket footprint, material
//! below the model surface must stay; material above it must go. A
//! cutter standing at level z therefore cuts the footprint minus the
//! model's cross-section strictly above z:
//!
//! ```text
//! cut_target(z) = footprint ∖ slice(model, z + ε)
//! ```
//!
//! `slice(model, z+ε)` = columns whose solid reaches above the plane -
//! the per-level islands, computed, not picked. A column whose surface
//! T(x,y) ≤ z is fair game; the ladder then leaves a staircase of at
//! most `stepdown` above T on anything raised (the rest-material bullet,
//! M2.1's simulation work, is what reads that staircase later; a flat
//! floor at `bottom_z` comes out exact, the last level snapped there by
//! the same `cut_levels` law the 2.5-D pockets use). The ε probe means a
//! shelf within one lattice micron of the plane is cut tangentially -
//! quantization noise, never a silent gouge: the reach law verifies the
//! chords at their Z.
//!
//! Entries at a level's first pass: `Plunge` mirrors the 2-D pocket
//! precedent (an axial dive the flutes are built for); `Ramp` feeds the
//! diagonal from the previous level's plane, earned only when that
//! chord stays inside the previous level's cutter-center area - the
//! deepest cut is the most crowded one, so a chord legal there is
//! legal at every plane the descent crosses; `Helix` orbits a circle of
//! `helix_radius` (≤ tool radius, so successive turns overlap) about a
//! scanline midpoint whose room fits the widened disc at both planes,
//! then links along this level's floor to the first pass. Pass ends hug
//! the walls, so an orbit about one is never legal - the dive lives
//! mid-row instead. A diagonal or an orbit into a wall is a gouge, and
//! a fallback that costs a slower dive is never one. The first level
//! always plunges: its plane is the surface itself.
//!
//! ## The rest pass (M2.1b)
//!
//! `pocket3d_with_rest` runs the ladder, simulates it against the mesh
//! with the dexel grid (`stock_sim`), then runs a half-stepdown ladder
//! aimed at the columns the sim says still stand:
//! `target(z) = footprint ∖ slice(z+ε) ∩ {sim_top > z}`. Entries there
//! are always plunges - the nesting law (a deeper plane's island is a
//! superset of a shallower one's) means a column clear at z is clear at
//! every plane above z, so a dive into rest crosses no part wall. The
//! staircase bound halves per pass, toward the grid's own resolution;
//! the closing report is the sim's verdict, never a second surface.
//!
//! ## Feed modulation (M2.1d)
//!
//! `optimize_feed` re-reads every planned planar feed against the same
//! dexel field and scales its feedrate by the engagement it actually
//! meets: the median engagement over the path's loaded bites is the
//! programmed feed, and bites heavier than the median are slowed
//! toward it, down to `FFO_FLOOR` of it; bites the sim reads as
//! engaging nothing (air, zero-depth skin) stay at the programmed
//! feed and hold no vote in the median. Lighter bites never run
//! faster than the programmed feed - the op's feed *is* the tool
//! limit this build models, and speeding past it needs a
//! spindle/torque curve we do not carry (provenance: the
//! constant-chip-thickness relation feed ∝ 1/engagement from
//! machining-handbook side-milling theory; no vendor implementation
//! inspected). Geometry is byte-identical; only `Move::feed` values
//! change.

use crate::cl::{lattice, MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::geometry::slice::{self, SliceError};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::{
    chord_clears, chord_crosses_path, cut_levels, plan_passes, GenError, PocketStrategy,
};
use crate::stock_sim::{self, Dexel, StockError, StockReport};

/// The sim's chord budget for arc moves - the sagitta law's own number,
/// the same one `Move::chords` is tested at. Finer than the lattice by
/// four decades, coarser than any gouge tolerance: chords under-sweep
/// the arc, which the stock_sim header makes conservative by law.
const CHORD_TOL: f64 = 0.01;

impl From<StockError> for GenError {
    fn from(e: StockError) -> Self {
        match e {
            StockError::Geo(g) => GenError::Geo(g),
            StockError::Invalid(s) => GenError::Invalid(s),
        }
    }
}

/// How the cutter first engages material at a level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Entry3D {
    #[default]
    /// Axial dive at the pass entry, the 2-D pocket motion.
    Plunge,
    /// One straight diagonal feed from the previous level's plane down
    /// to this one (first level: falls back to a plunge - there is no
    /// open floor to start a ramp from).
    Ramp,
    /// Helical interpolation: orbit while descending one stepdown (the
    /// widened arc law, see `cl.rs`). Air orbits above stock are never
    /// posted: the first level's dive is the surface itself.
    Helix,
}

/// Everything `pocket3d` needs, all numbers resolved (heights are the
/// operation layer's business, same seam as `machining2d`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pocket3DParams {
    pub tool_diameter: f64,
    pub stepover: f64,
    pub stock_to_leave: f64,
    pub clearance_z: f64,
    pub top_z: f64,
    pub bottom_z: f64,
    pub stepdown: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    pub climb: bool,
    pub strategy: PocketStrategy,
    /// Between-ring hop plane within one level (same law as 2-D).
    pub retract_z: Option<f64>,
    pub entry: Entry3D,
    /// Helix centerline circle radius (mm); validated ≤ tool radius so
    /// successive turns overlap and the dive cuts a clean bore.
    pub helix_radius: f64,
    /// The `Adaptive` strategy's engagement bound, in degrees of
    /// instant cutter contact (ignored by the other strategies, still
    /// validated finite). Drives the ring step through
    /// [`engagement_step`] - the whole of constant-engagement
    /// roughing's geometry law (M2.1c).
    pub max_engagement: f64,
}

/// The engagement law, in one line of trigonometry: a pass that bites
/// a wall face radially by `a` with a disc of radius `r` contacts it
/// over a half-angle `theta` with `a = r (1 - cos theta)` - the sagitta
/// of the contact arc. Inverting it gives the biggest ring step whose
/// contact stays under a chosen angle: `a = r (1 - cos theta_max)`.
/// Published trochoidal/HEM literature is all computations of this
/// same disc-against-wall geometry; this is our own reading of it,
/// pinned by `the_engagement_law_is_the_sagitta` (M2.1c).
#[must_use]
pub fn engagement_step(tool_r: f64, max_engagement_deg: f64) -> f64 {
    tool_r * (1.0 - max_engagement_deg.to_radians().cos())
}

impl Pocket3DParams {
    fn validate(&self) -> Result<(), GenError> {
        let mut finite = [
            self.tool_diameter,
            self.stepover,
            self.stock_to_leave,
            self.clearance_z,
            self.top_z,
            self.bottom_z,
            self.stepdown,
            self.feed,
            self.plunge_feed,
            self.helix_radius,
            self.max_engagement,
        ]
        .into_iter()
        .all(f64::is_finite);
        finite &= self.retract_z.is_none_or(f64::is_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.tool_diameter <= 0.0 || self.stepdown <= 0.0 {
            return Err(GenError::Invalid("diameter and stepdown must be positive".into()));
        }
        if self.stepover <= 0.0 || self.stepover > self.tool_diameter {
            return Err(GenError::Invalid(
                "stepover must be positive and within the tool diameter".into(),
            ));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        if self.stock_to_leave < 0.0 {
            return Err(GenError::Invalid("stock to leave must not be negative".into()));
        }
        if self.top_z < self.bottom_z {
            return Err(GenError::Invalid("cut top below cut bottom".into()));
        }
        if self.clearance_z < self.top_z {
            return Err(GenError::Invalid("clearance plane below the first cut level".into()));
        }
        if self.retract_z.is_some_and(|rz| rz < self.top_z) {
            return Err(GenError::Invalid("retract plane sits inside the material".into()));
        }
        if self.entry == Entry3D::Helix {
            let r = self.tool_diameter * 0.5;
            if self.helix_radius <= 0.0 || self.helix_radius > r {
                return Err(GenError::Invalid(
                    "helix radius must be positive and at most the tool radius".into(),
                ));
            }
        }
        if self.strategy == PocketStrategy::Adaptive {
            // The law's own bounds: a zero angle is a zero step (an
            // infinite ladder), and past 90 degrees the sagitta passes
            // one radius - past where consecutive rings still overlap,
            // which is where the waterline reach law already says
            // rings stop covering honestly.
            if self.max_engagement <= 0.0 || self.max_engagement > 90.0 {
                return Err(GenError::Invalid(
                    "engagement angle must be in (0, 90] degrees".into(),
                ));
            }
        }
        Ok(())
    }

    /// What actually spaces the passes at a level: the engagement-law
    /// step under `Adaptive`, the stated stepover otherwise. Everything
    /// that reads the tiling - the pass plan and the sim's cell size -
    /// asks this one function, so a grid can never be coarser than the
    /// law's own bands.
    fn tiling_step(&self) -> f64 {
        match self.strategy {
            PocketStrategy::Adaptive => engagement_step(self.tool_diameter * 0.5, self.max_engagement),
            _ => self.stepover,
        }
    }
}

/// Half-open slices sit exactly on vertices for tessellated steps; this
/// is the same lattice the weld uses, so a grazing plane nudges rather
/// than lies.
const EPS: f64 = 1e-6;

fn from_slice(e: SliceError) -> GenError {
    match e {
        SliceError::Geo(g) => GenError::Geo(g),
        other => GenError::Invalid(other.to_string()),
    }
}

/// The model's cross-section strictly above `z`, nudge-retried when a
/// plane grazes a tessellated shelf (reading an errored slice as
/// no-island would be a silent gouge - the retry lands above the shelf
/// instead, failing safe). Public because it is the islands law itself:
/// the generator reads it to shape each level, the verify pass to prove
/// them, and the tests to pin the monotonicity the ladder assumes -
/// one law, three readers (M2.1a).
pub fn material_above(facets: &[Triangle], z: f64) -> Result<Region, GenError> {
    let mut err = None;
    for k in 0..3 {
        let probe = z + f64::from(k + 1) * EPS;
        match slice::slice(facets, probe) {
            Ok(r) => return Ok(r),
            Err(e) => err = Some(e),
        }
    }
    Err(err.map_or(
        GenError::Invalid("cannot section the model".into()),
        from_slice,
    ))
}

/// Would a feed chord `a -> b` along this level's plane properly cross
/// the level's own planned paths? `chord_clears` proves the chord stays
/// inside the cutter-center area, but on a holed area (an island, a
/// boss) a chord from one family's ring to another's crosses every ring
/// in between, and `verify` reports each such crossing at the cut depth.
/// A chord that fails here is flown over instead: rapids, traverse,
/// plunge. This is the same law `waterline_passes` chains its rings by,
/// applied where the ladder joins one planned pass to the next.
fn chord_hits_plan(a: Point2, b: Point2, passes: &[Vec<Point2>]) -> bool {
    // The planned passes store their full vertex chains - a waterline
    // ring repeats its entry point at the end, so walking consecutive
    // pairs covers its closing leg too - so consecutive segments, never
    // a phantom close, is the exact set of this level's cut paths.
    passes.iter().any(|p| chord_crosses_path(a, b, p, false))
}

/// The helix's dive point: the midpoint of the longest straight run in
/// the level plan that has room for the widened disc at both planes it
/// crosses (the leg regions, each the level's center eroded by the
/// helix radius plus a micron's insurance against buffer-of-buffer
/// quantization). A row's midpoint is the furthest from every wall a
/// planar pass offers; the scanline ends sit on the boundary by
/// construction. First choice is the longest run - the most room, and
/// the row a climb pass would most want to start mid-span on.
fn dive_point(passes: &[Vec<Point2>], leg_prev: &Region, leg_here: &Region) -> Option<Point2> {
    let mut best: Option<(f64, Point2)> = None;
    for pass in passes {
        for pair in pass.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let len = (b.x - a.x).hypot(b.y - a.y);
            if len <= 0.0 || best.is_some_and(|(l, _)| l >= len) {
                continue;
            }
            let mid = Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
            if leg_prev.contains(mid) && leg_here.contains(mid) {
                best = Some((len, mid));
            }
        }
    }
    best.map(|(_, d)| d)
}

/// The per-level cutter plan and its walk, level after level - the 3-D
/// sibling of `machining2d::pocket`'s run, with the passes recomputed
/// from the model at every z (that is the whole difference: 2-D knows
/// its plan once, 3-D's plan IS the ladder).
pub fn pocket3d(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
) -> Result<Toolpath, GenError> {
    guard_input(facets, footprint, p)?;
    let zs = cut_levels(p.top_z, p.bottom_z, p.stepdown);
    ladder(facets, footprint, p, &zs, None, home(p))
}

/// The rest-material sibling (M2.1b): the same level ladder, then a
/// second, half-stepdown pass aimed at what the simulation says is
/// still standing. The coarse ladder's staircase is the acknowledged
/// price of planes - at most `stepdown` above a raised surface - and
/// this closes it toward stepdown/2 per round the caller dares to run.
/// Entries here are always plain plunges, and the superset law is why:
/// a column the coarse ladder cleared at z is clear at every plane
/// above z, so a vertical dive into rest material can never cross part
/// geometry. The report is the sim's verdict on the COMBINED path,
/// measured against the mesh's own surface.
pub fn pocket3d_with_rest(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
) -> Result<(Toolpath, StockReport), GenError> {
    guard_input(facets, footprint, p)?;
    let r = p.tool_diameter * 0.5;
    let zs = cut_levels(p.top_z, p.bottom_z, p.stepdown);
    let coarse = ladder(facets, footprint, p, &zs, None, home(p))?;

    // What the coarse ladder leaves standing, column by column.
    let (mut dex, _) = sim_of(facets, footprint, p)?;
    for m in &coarse.moves {
        dex.subtract(m, r, CHORD_TOL);
    }

    // The rest ladder: same laws, half the step, rest-shaped targets.
    let zs_r = cut_levels(p.top_z, p.bottom_z, p.stepdown * 0.5);
    let start = coarse.moves.last().map_or_else(|| home(p), |m| m.to);
    let mut moves = coarse.moves;
    match ladder(facets, footprint, p, &zs_r, Some(&dex), start) {
        Ok(rp) => moves.extend(rp.moves),
        // Nothing stood above any rest plane: the coarse path was
        // already clean at lattice scale. A success, not a refusal.
        Err(GenError::Empty(_)) => {}
        Err(e) => return Err(e),
    }
    let path = Toolpath {
        moves,
        cycles: Vec::new(),
    };
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }

    // The verdict on the whole job: re-run the sim from raw stock over
    // the combined path, so the report describes the machining, not
    // one of its legs.
    let (mut dex, part) = sim_of(facets, footprint, p)?;
    for m in &path.moves {
        dex.subtract(m, r, CHORD_TOL);
    }
    Ok((path, stock_sim::measure(&dex, &part, footprint, EPS)))
}

/// How far below the programmed feed the modulation may take a planar
/// cut move. One quarter is a handbook-style floor for a roughing
/// bite: below it the chip is too thin to evacuate in steel and the
/// slow feed buys nothing the floor pass will not cut anyway. It is a
/// law constant, not a parameter - a path with bites needing less
/// than this wants a smaller stepover, said the strategy, not a
/// crawl said the feed.
pub const FFO_FLOOR: f64 = 0.25;

/// Radial engagement of one planar feed move, as a fraction of the
/// tool diameter (0 = air, 1 = full bite): the perpendicular chord
/// through the move's midpoint, sampled at `2 * HALF + 1` points,
/// read against the sim's standing-material field BEFORE the move
/// cuts it. Sampling the instantaneous field is the whole point - a
/// static per-level region would report the entire pocket cross-section
/// as material and slow every ring to the floor. A move too short to
/// have a direction is treated as a full bite: slowing down is the
/// safe read of an ambiguity. Grid-quantized by construction (the
/// same dexel law everywhere: measure, never derive, from the sim).
///
/// The chord is sampled at two stations - mid-move and move-end - and
/// averaged: the trapezoid of the swept side width. A midpoint-only
/// chord misreads a stepover link, whose fresh material lies *ahead*
/// along travel rather than beside it, as zero engagement; averaging
/// the two stations puts a half-engaged link at ~0.5, which is what
/// it cuts. A move that engages nothing at either station - the
/// zero-depth skin of the first level, air moves - reads exactly 0
/// and is held at the programmed feed (there is no load to pay for).
fn engagement(dex: &Dexel, from: Point2, to: Point2, z: f64, r: f64) -> f64 {
    const HALF: i32 = 8; // r/8 chord resolution, one lattice step per side
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len <= EPS {
        return 1.0;
    }
    // Chords normal to travel: width of cut is measured across the
    // feed direction, along it the tool is a line.
    let (nx, ny) = (-dy / len, dx / len);
    let stations = [
        Point2::new((from.x + to.x) * 0.5, (from.y + to.y) * 0.5),
        Point2::new(from.x + dx * 0.75, from.y + dy * 0.75),
    ];
    let mut acc = 0.0;
    for s in stations {
        let mut hit = 0usize;
        let mut total = 0usize;
        for k in -HALF..=HALF {
            let q = Point2::new(
                s.x + nx * (k as f64) * r / HALF as f64,
                s.y + ny * (k as f64) * r / HALF as f64,
            );
            total += 1;
            if let Some(i) = dex.col(q)
                && dex.top[i] > z + EPS
            {
                hit += 1;
            }
        }
        acc += (hit as f64 / total as f64).clamp(0.0, 1.0);
    }
    (acc * 0.5).clamp(0.0, 1.0)
}

/// The M2.1d feed pass: run the finished path through the sim once,
/// measuring each planar feed move's engagement against the material
/// standing the instant before it, then scale its feed by
/// `median / engagement`, clamped to `[FFO_FLOOR, 1]` of the
/// programmed feed. The median is the self-calibration: whatever the
/// strategy's nominal load, that is what runs at the programmed feed,
/// and only the peaks (full bites, corners, the first ring of a
/// level) are slowed. Never faster, never past the programmed feed -
/// within tool limits, where the op's feed word is the limit. The
/// geometry is untouched: same moves, same order, same points; a
/// law test pins that. Vertical feeds (plunges, ramps, helices) keep
/// their `plunge_feed`/`feed` as assigned - axial engagement is a
/// different curve we do not model here (decision, RECOMMENDED).
pub fn optimize_feed(
    path: &Toolpath,
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
) -> Result<Toolpath, GenError> {
    p.validate()?;
    let r = p.tool_diameter * 0.5;
    let (mut dex, _) = sim_of(facets, footprint, p)?;
    let mut moves = path.moves.clone();
    // (index, engagement) of the planar feed moves, measured before
    // the sim advances past them.
    let mut bites: Vec<(usize, f64)> = Vec::new();
    for (i, m) in moves.iter().enumerate() {
        if m.kind == MoveKind::Feed && m.arc.is_none() && (m.to.z - m.from.z).abs() <= EPS {
            bites.push((
                i,
                engagement(
                    &dex,
                    Point2::new(m.from.x, m.from.y),
                    Point2::new(m.to.x, m.to.y),
                    m.to.z,
                    r,
                ),
            ));
        }
        dex.subtract(m, r, CHORD_TOL);
    }
    if !bites.is_empty() {
        // The anchor is the median of the *loaded* bites only: an
        // air cut or a zero-depth skin reading 0 must not drag the
        // whole job's reference load down to nothing (that would
        // silently switch the optimization off - the failure mode a
        // first-run histogram caught). Zero bites stay at the
        // programmed feed below, which is the honest answer: no
        // load, no reason to slow.
        let mut ae: Vec<f64> = bites.iter().map(|&(_, a)| a).filter(|a| *a > EPS).collect();
        if !ae.is_empty() {
            ae.sort_by(|a, b| a.total_cmp(b));
            // Middle of the sorted run; an even count straddles the
            // two central samples and averages them - one value, no
            // branch.
            let mid = ae.len() / 2;
            let median = if ae.len() % 2 == 1 {
                ae[mid]
            } else {
                (ae[mid - 1] + ae[mid]) * 0.5
            };
            for &(i, a) in &bites {
                let ratio = if a <= EPS { 1.0 } else { (median / a).clamp(FFO_FLOOR, 1.0) };
                moves[i].feed = Some(p.feed * ratio);
            }
        }
    }
    let out = Toolpath {
        moves,
        cycles: path.cycles.clone(),
    };
    if !out.all_finite() {
        return Err(GenError::Invalid("feed modulation broke the finite horizon".into()));
    }
    Ok(out)
}

fn home(p: &Pocket3DParams) -> Point3 {
    Point3::new(0.0, 0.0, p.clearance_z)
}

fn guard_input(facets: &[Triangle], footprint: &Region, p: &Pocket3DParams) -> Result<(), GenError> {
    p.validate()?;
    if facets.is_empty() {
        return Err(GenError::Empty("no model facets to section".into()));
    }
    if footprint.is_empty() {
        return Err(GenError::Empty("empty pocket footprint".into()));
    }
    Ok(())
}

/// The sim for this operation: the footprint's XY box, cells fine
/// enough to see both the tool and the pass gaps (under `Adaptive`
/// those gaps are the engagement-law step, so the grid follows it -
/// `tiling_step`), the stock face at the cut top. A part that stands taller than the cut top is part
/// of the stock there (the setup's stock contains its model, or the
/// operation's span does not reach it - either way the column starts
/// full, never half-invented).
fn sim_of(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
) -> Result<(Dexel, Vec<f64>), GenError> {
    let (lo, hi) = footprint
        .bounds()
        .ok_or_else(|| GenError::Invalid("footprint has no bounds".into()))?;
    let cell = (p.tool_diameter * 0.5).min(p.tiling_step() * 0.5);
    let mut dex = Dexel::block(lo.x, lo.y, hi.x, hi.y, p.top_z, cell)?;
    let part = stock_sim::part_surface(facets, dex.x0, dex.y0, dex.cell, dex.nx, dex.ny);
    for (t, &pt) in dex.top.iter_mut().zip(&part) {
        if pt > *t {
            *t = pt;
        }
    }
    Ok((dex, part))
}

/// One ladder run: the level loop shared by both entry points. `zs` is
/// the plane list, `rest` the post-coarse state whose still-standing
/// columns the pass is aimed at (None: the pure level law), and
/// `start` where the builder begins - the home position for the coarse
/// run, the coarse path's end for the rest run, so the two legs chain
/// into one path.
fn ladder(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
    zs: &[f64],
    rest: Option<&Dexel>,
    start: Point3,
) -> Result<Toolpath, GenError> {
    let r = p.tool_diameter * 0.5;
    let hop_z = p.retract_z.unwrap_or(p.clearance_z).min(p.clearance_z);

    let mut b = PathBuilder::new(start);
    let step = p.tiling_step();
    // The last level that actually cut: its plane and center area, the
    // most crowded the entry motions of the next level can meet (a
    // deeper level's target only grows). None until one has cut.
    let mut prev_state: Option<(f64, Region)> = None;
    for (li, &z) in zs.iter().enumerate() {
        // The level law, in two region ops: islands from the model, the
        // cutter-center area by one erosion.
        let islands = material_above(facets, z)?;
        let target = footprint.difference(&islands)?;
        let target = match rest {
            // The level law on rest terms: only columns the simulated
            // state still has standing above this plane. The islands
            // subtraction already ran, and the nesting law keeps the
            // intersection honest: a part column above z is an island,
            // so the sim's coarser top cannot smuggle part material
            // into a rest target.
            None => target,
            Some(dex) => target.intersect(&dex.above(z)?)?,
        };
        if target.is_empty() {
            continue; // nothing above this plane: model material across
                      // the whole footprint, or nothing standing at all
        }
        let center = target.offset(-(r + p.stock_to_leave))?;
        if center.is_empty() {
            continue; // narrower than the tool at this level: the
                      // staircase (or a wall this tool cannot enter) -
                      // rest material's later problem, never a lie now
        }
        let passes = plan_passes(&center, step, p.climb, p.strategy)?;
        let Some(first_pass) = passes.first() else {
            continue;
        };
        let Some(&entry) = first_pass.first() else {
            continue;
        };
        // A level change always climbs to clearance first: the new
        // level engages material the previous one never touched, and
        // the entry motions below all start their bite from there (or
        // from the previous plane, when that is what legality asks).
        let at = b.at();
        if at.z != p.clearance_z {
            b.push(MoveKind::Rapid, Point3::new(at.x, at.y, p.clearance_z), None);
        }
        let leg = Point2::new(at.x, at.y);
        let prev_z = prev_state.as_ref().map_or(z, |(q, _)| *q);
        match p.entry {
            // Ramp and helix are the coarse ladder's motions: a rest
            // pass starts inside planes that are already cut, where a
            // diagonal would cross floors the coarse pass owns. The
            // vertical dive needs no such proof (the nesting law
            // carries it), so the rest ladder always takes the plunge
            // arm below.
            Entry3D::Ramp if li > 0 && rest.is_none() => {
                // The diagonal's legality is asked of the PREVIOUS
                // level's center: it is the deepest cut so far, its
                // islands stand above everything this descent passes,
                // and every shallower plane's target is a superset -
                // one chord test covers the whole interpolated move.
                let open = prev_state
                    .as_ref()
                    .is_some_and(|(_, pc)| chord_clears(pc, leg, entry));
                if open {
                    b.push(MoveKind::Rapid, Point3::new(leg.x, leg.y, prev_z), None);
                    b.push(MoveKind::Feed, Point3::new(entry.x, entry.y, z), Some(p.feed));
                } else {
                    b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, p.clearance_z), None);
                    b.push(
                        MoveKind::Plunge,
                        Point3::new(entry.x, entry.y, z),
                        Some(p.plunge_feed),
                    );
                }
            }
            Entry3D::Helix if li > 0 && rest.is_none() => {
                // Pass ends sit on the walls by construction, so the
                // orbit is never legal about one: the dive lives at the
                // midpoint of the level's longest row, where the
                // widened disc (tool radius + helix radius) has room.
                // Legality is the same superset law as the ramp's, at
                // the previous center; the link to the first pass then
                // runs along this level's own floor.
                let leg_prev = prev_state
                    .as_ref()
                    .map(|(_, pc)| pc.offset(-(p.helix_radius + 1e-3)))
                    .transpose()?;
                let leg_here = center.offset(-(p.helix_radius + 1e-3))?;
                let dive = leg_prev
                    .as_ref()
                    .and_then(|lp| dive_point(&passes, lp, &leg_here));
                if let Some(d) = dive {
                    if leg.x != d.x || leg.y != d.y {
                        b.push(MoveKind::Rapid, Point3::new(d.x, d.y, p.clearance_z), None);
                    }
                    b.push(MoveKind::Rapid, Point3::new(d.x, d.y, prev_z), None);
                    helix_down(&mut b, d, prev_z, z, p.helix_radius, p.feed);
                    if chord_clears(&center, d, entry) && !chord_hits_plan(d, entry, &passes) {
                        b.push(MoveKind::Feed, Point3::new(entry.x, entry.y, z), Some(p.feed));
                    } else {
                        b.push(MoveKind::Rapid, Point3::new(d.x, d.y, hop_z), None);
                        b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, hop_z), None);
                        b.push(
                            MoveKind::Plunge,
                            Point3::new(entry.x, entry.y, z),
                            Some(p.plunge_feed),
                        );
                    }
                } else {
                    b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, p.clearance_z), None);
                    b.push(
                        MoveKind::Plunge,
                        Point3::new(entry.x, entry.y, z),
                        Some(p.plunge_feed),
                    );
                }
            }
            // The first level's plane IS the stock surface: there is
            // nothing to ramp or spiral into, and a dive there cuts
            // air. One honest Plunge move says so.
            _ => {
                if leg.x != entry.x || leg.y != entry.y {
                    b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, p.clearance_z), None);
                }
                b.push(
                    MoveKind::Plunge,
                    Point3::new(entry.x, entry.y, z),
                    Some(p.plunge_feed),
                );
            }
        }
        walk_pass(&mut b, first_pass, z, p.feed, /*skip_first=*/ true);
        // Remaining passes: connect as far as the floor allows - and no
        // further than the level's own planned paths allow (the boss
        // bug: a chord can clear the cutter-center area yet cross every
        // ring scheduled under it, which `verify` then reports as a
        // Crossing at the cut depth; fly over it instead).
        for pass in &passes[1..] {
            let Some(&next_entry) = pass.first() else {
                continue;
            };
            let prev = b.at();
            let p2 = Point2::new(prev.x, prev.y);
            if chord_clears(&center, p2, next_entry) && !chord_hits_plan(p2, next_entry, &passes) {
                // Feed straight on at this level (the join chord the
                // test just proved).
                b.push(MoveKind::Feed, Point3::new(next_entry.x, next_entry.y, z), Some(p.feed));
            } else {
                b.push(MoveKind::Rapid, Point3::new(prev.x, prev.y, hop_z), None);
                b.push(MoveKind::Rapid, Point3::new(next_entry.x, next_entry.y, hop_z), None);
                b.push(
                    MoveKind::Plunge,
                    Point3::new(next_entry.x, next_entry.y, z),
                    Some(p.plunge_feed),
                );
            }
            walk_pass(&mut b, pass, z, p.feed, /*skip_first=*/ true);
        }
        // Hand this level's plan to the next one's entry legality: it
        // is the deepest cut so far, so its center bounds every plane
        // a descent from here will cross.
        prev_state = Some((z, center));
    }
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
    let path = b.finish();
    if path.moves.is_empty() {
        return Err(GenError::Empty(
            "nothing to cut: the model fills the pocket at every level".into(),
        ));
    }
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// Feed-walk a pass's points at plane `z` (the first point is the
/// current position; `skip_first` avoids a zero move). Ring closure is
/// a property of the point DATA, not a motion added here: a waterline
/// loop arrives with its start point repeated at the end (each ring is
/// emitted closed by `waterline_passes`, and a chain of rings carries a
/// closing point per ring), so the walk's final feed lands each ring
/// back on its own start. Zigzag rows arrive open and stay open. An
/// earlier draft closed passes here by feeding the walk's last point
/// back to the first; for a lone ring that was a zero-length duplicate
/// of the data's own closing, but for a chained multi-ring pass it
/// dragged a diagonal across every intermediate ring - the boss crossing
/// bug. The law is: walk the data, add nothing.
fn walk_pass(b: &mut PathBuilder, pass: &[Point2], z: f64, feed: f64, skip_first: bool) {
    for q in pass.iter().skip(if skip_first { 1 } else { 0 }) {
        b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), Some(feed));
    }
}

/// One helical dive between two planes: at least one full turn (split
/// into half-circle arcs so no move is a zero-sweep full circle), Z
/// descending linearly with the orbit. The builder must stand at `xy`
/// on plane `z0` (the caller's rapid to the dive point); an even
/// number of half turns ends back on `xy` at `z1` by construction -
/// each turn is two 180° arcs whose XY net is zero.
fn helix_down(b: &mut PathBuilder, xy: Point2, z0: f64, z1: f64, rh: f64, feed: f64) {
    let span = z0 - z1;
    if span <= 0.0 || rh <= 0.0 {
        return;
    }
    // At least one turn per dive; one more when the span is deeper than
    // the circle's own reach, so the bore's slope stays inside the tool
    // diameter's width.
    let turns = (span / (rh * 2.0)).ceil().max(1.0);
    let half_turns = ((turns * 2.0).ceil() as u32).max(2);
    // Orbit about the circle whose near side is the entry point: center
    // offset +rh in X. Endpoints sit lattice-snapped (the arc law wants
    // a 1 um center; the builder keeps the chord endpoints exact).
    let center = Point2::new(lattice(xy.x + rh), lattice(xy.y));
    for h in 0..half_turns {
        let frac = f64::from(h + 1) / f64::from(half_turns);
        let tz = z0 - span * frac;
        // Alternating far/near point of the circle; even counts close on
        // xy at the last leg.
        let px = if h % 2 == 0 { xy.x + 2.0 * rh } else { xy.x };
        // ccw: right -> left over the +Y side.
        b.push_arc(
            MoveKind::Feed,
            Point3::new(lattice(px), lattice(xy.y), lattice(tz)),
            Some(feed),
            center,
            true,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chord_over_a_step_reads_half_engagement() {
        // The chord law at its plainest: travel along a wall, half the
        // disc in standing material, half in air - the measurement says
        // 0.5 up to the 17-point quantization (8 hits, 9 misses).
        let mut dex = Dexel::block(0.0, 0.0, 16.0, 16.0, 10.0, 1.0).unwrap();
        for iy in 0..dex.ny {
            for ix in 0..dex.nx {
                dex.top[iy * dex.nx + ix] = if ix < 8 { 10.0 } else { 0.0 };
            }
        }
        let ae = engagement(&dex, Point2::new(8.0, 4.0), Point2::new(8.0, 12.0), 5.0, 8.0);
        assert!((ae - 0.5).abs() < 0.05, "half-bite read {ae}");
        // Pure air, and pure solid, at the same place.
        let out = dex.top.len();
        let _ = out;
        for t in dex.top.iter_mut() {
            *t = 0.0;
        }
        let ae = engagement(&dex, Point2::new(8.0, 4.0), Point2::new(8.0, 12.0), 5.0, 8.0);
        assert!(ae < 1e-9, "air must read zero, got {ae}");
        for (iy, t) in dex.top.chunks_mut(dex.nx).enumerate() {
            for (ix, c) in t.iter_mut().enumerate() {
                *c = if (iy * dex.nx + ix) < 8 * dex.nx { 10.0 } else { 0.0 };
                let _ = ix;
            }
        }
        let ae = engagement(&dex, Point2::new(8.0, 1.0), Point2::new(8.0, 2.0), 5.0, 0.5);
        assert!((ae - 1.0).abs() < 1e-9, "solid must read full, got {ae}");
    }

    #[test]
    fn a_zero_length_planar_move_reads_as_a_full_bite() {
        // The ambiguity law: no direction, no chord normal to measure -
        // the safe read is the heavy one.
        let dex = Dexel::block(0.0, 0.0, 4.0, 4.0, 10.0, 1.0).unwrap();
        let ae = engagement(&dex, Point2::new(2.0, 2.0), Point2::new(2.0, 2.0), 5.0, 1.0);
        assert!((ae - 1.0).abs() < 1e-12, "got {ae}");
    }
}
