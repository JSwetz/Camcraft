//! Split from `machining3d.rs` by M5.0.1; structure only.


use crate::machining2d::{
    GenError, PocketStrategy,
};
use crate::stock_sim::StockError;
/// The sim's chord budget for arc moves - the sagitta law's own number,
/// the same one `Move::chords` is tested at. Finer than the lattice by
/// four decades, coarser than any gouge tolerance: chords under-sweep
/// the arc, which the stock_sim header makes conservative by law.
pub(super) const CHORD_TOL: f64 = 0.01;

impl From<StockError> for GenError {
    fn from(e: StockError) -> Self {
        match e {
            StockError::Geo(g) => GenError::Geo(g),
            StockError::Invalid(s) => GenError::Invalid(s),
        }
    }
}

/// The entry family and the engagement law moved to the crate's
/// `entry` seam in M5.1 H, so the planar pocket can engage its levels
/// the way the 3-D ladder does; the paths callers always used stay
/// exactly that.
pub use crate::entry::{engagement_step, Entry3D};

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
    /// The `Adaptive` strategy's optimal load: radial width of cut as a
    /// fraction of the tool diameter (ae/D, M11.4 B - `crate::load`;
    /// ignored by the other strategies, still validated finite). The
    /// ring step is `optimal_load x D` - the whole of constant-engagement
    /// roughing's geometry law (M2.1c), the contact angle derived.
    pub optimal_load: f64,
}

impl Pocket3DParams {
    pub(super) fn validate(&self) -> Result<(), GenError> {
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
            self.optimal_load,
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
            // The law's own bounds: a zero load is a zero step (an
            // infinite ladder), and past half the diameter (90 degrees
            // of contact) the step passes one radius - past where consecutive rings still overlap,
            // which is where the waterline reach law already says
            // rings stop covering honestly.
            if !crate::load::load_in_door(self.optimal_load) {
                return Err(GenError::Invalid(
                    "optimal load must be in (0, 50] % of the tool diameter".into(),
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
    pub(super) fn tiling_step(&self) -> f64 {
        match self.strategy {
            PocketStrategy::Adaptive => crate::load::load_step(self.tool_diameter, self.optimal_load),
            _ => self.stepover,
        }
    }
}
