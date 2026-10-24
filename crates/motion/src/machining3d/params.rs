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
    pub(super) fn tiling_step(&self) -> f64 {
        match self.strategy {
            PocketStrategy::Adaptive => engagement_step(self.tool_diameter * 0.5, self.max_engagement),
            _ => self.stepover,
        }
    }
}
