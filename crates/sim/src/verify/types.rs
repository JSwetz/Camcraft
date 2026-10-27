//! Split from `verify.rs` by M5.0.1; structure only.


use crate::geo2d::Region;
pub use crate::tools::ToolGeom;
/// What the path means, which decides which laws apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Pocket/slot: material is removed from inside the region.
    Area,
    /// Contour/face: the path trims the region's boundary.
    Boundary,
    /// Trace: the cutter center runs on the boundary itself; laws that
    /// assume tangency do not apply.
    Engrave,
    /// Ball-rolling 3-D finish (waterline): the centers sit exactly r
    /// off the model at each tangent point and nearer than r to the
    /// section silhouettes elsewhere, so the tangency gouge law would
    /// flag every ring. It stands down in favor of the measured sweep
    /// envelope below, gated by `finish_tol`.
    Envelope,
    /// The turned family (M5.2): the work spins, the tool traverses
    /// the meridian. The XY-plane laws below read a lathe's chords
    /// as degenerate - radial plunges "cross" at the axis, tangency
    /// fills the section - so (a) and (b) stand down in favor of the
    /// turning branch's meridian scan (motion's `turning::revolve`),
    /// which judges the revolved cut volume exactly. The reach laws
    /// stay: they are pose-independent.
    Revolve,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VerifyContext<'a> {
    pub region: &'a Region,
    /// The stock prism footprint; pass it when part islands must be
    /// checked too - otherwise the region stands in for it.
    pub stock: Option<&'a Region>,
    /// The mesh a model-driven op cuts to (M2.1's 3-D pocket): the cut
    /// target changes with depth, so the static region cannot be the
    /// whole law. Given, the gouge law checks each move against the
    /// model's cross-section strictly above that move's deepest plane
    /// (the level law's own islands), and the reach laws sample those
    /// sections' walls as the islands they are. Every 2.5-D op passes
    /// `None` - its laws are byte-identical to what they always were.
    pub model: Option<&'a [crate::geometry::Triangle]>,
    pub tool: &'a ToolGeom,
    pub mode: Mode,
    /// Top of the stock column in Z (the surface the tool starts from).
    pub stock_top_z: f64,
    /// Safe plane: rapids must live at or above it.
    pub clearance_z: f64,
    /// Finish allowance already baked into the path offsets.
    pub stock_to_leave: f64,
    /// Radial tolerance for the gouge law.
    pub gouge_tol: f64,
    /// Margin for "below the clearance plane" and "moved in XY".
    pub z_tol: f64,
    /// The deviation budget the measured envelope of `Mode::Envelope`
    /// is judged against. `None` everywhere the envelope law never
    /// runs - which keeps every existing op's verdict byte-identical,
    /// the same discipline as `model`.
    pub finish_tol: Option<f64>,
    /// Workholding hardware, in world space (M3.2 stage B). Fixtures
    /// are NOT cut material and never gouge-relevant; they participate
    /// in exactly two laws: (1) the shank/holder band-reach laws, where
    /// a vise jaw in the band's path is a crash just as uncut material
    /// in it is, and (2) the rapid law, where a rapid whose path dips
    /// into a fixture box is a crash - steel is never kerf. With the
    /// slice empty every law is byte-identical to what it always was,
    /// the same `None` discipline as `model`.
    pub fixtures: &'a [crate::fixture::FixtureBox],
    /// The spindle-housing band the machine file derives (M3.2 stage
    /// B), as `(radius, length)` standing on top of the holder - the
    /// offset comes from the tool stack, so the caller cannot get out
    /// of agreement with it. `None` when the `.mch` states no body
    /// geometry - the unstated-limit law - which keeps every
    /// pre-stage-B verdict byte-identical. It joins the fixture band
    /// law only: against stock the housing still stands down by the
    /// same kerf reasoning as the holder band.
    pub spindle: Option<(f64, f64)>,
}

/// Which machine element struck a fixture (M3.2 stage B). Serde on it
/// for the same reason as on [`Warning`]: the pair is persisted with the
/// cached path, so the names are a file-format contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixturePart {
    /// The shank band of the tool assembly.
    Shank,
    /// The holder band.
    Holder,
    /// The spindle housing band the machine file's `spindle` geometry
    /// adds above the holder - present only when the `.mch` states both
    /// body diameter and length (the unstated-limit law again).
    Spindle,
    /// The tool body on a rapid move.
    Rapid,
    /// The flute column itself, on a cutting move whose bands cleared:
    /// the edge milling into clamping steel - the worst thing this law
    /// can report, and named as itself, never as a "shank".
    Flute,
}

/// Serde derives: the operation layer persists warnings with the cached
/// path (project v3 `OperationDto.cache`), so the enum is also a file
/// format - variant names are that contract, keep them stable.
/// (`Eq` is deliberately absent - the envelope variant carries
/// measurements, and nothing in this codebase needs warnings to be
/// hashable; `PartialEq` is all the dedup and assert sites use.)
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Warning {
    /// Feed moves `moves[0]` and `moves[1]` cross in XY at shared depth.
    Crossing { moves: [usize; 2] },
    /// Move `move`'s cutter center is inside the finish surface.
    Gouge { at: usize },
    /// Move `move` carries shank/holder through material; `part` marks
    /// whether the offended material is the part/an island (never to be
    /// cut) rather than stock the op simply has not cleared yet.
    ShankHit { at: usize, part: bool },
    /// The assembly swept through material DURING a turn (M5.1 stage
    /// G): the move commands a new pose, and somewhere in the swing -
    /// with the table partway along and the head partway round, in
    /// either family's order of arrival - a band meets steel, though
    /// both endpoint poses pass law (c) clean. Advisory, one per
    /// move, at most: moves law (c) already convicted are not
    /// re-tried here, and `part` reads like (c)'s.
    SweepHit { at: usize, part: bool },
    /// Rapid `move` travels under the clearance plane through stock.
    RapidInStock { at: usize },
    /// Move `move` runs the named `fixture_part` of the machine through
    /// a fixture (M3.2 stage B): a band collision on a cutting move, or
    /// the tool body itself on a rapid. At most one per move - the
    /// first, worst band.
    FixtureHit {
        at: usize,
        fixture_part: FixturePart,
    },
    /// Defensive: a non-finite coordinate (generators gate this out).
    NonFinite { at: usize },
    /// A 3-D finish's measured ball envelope misses the model:
    /// `gouge_mm` below it (worst), `rest_mm` above it (worst),
    /// reported at the cutting move nearest the deviation's XY.
    Envelope {
        at: usize,
        gouge_mm: f64,
        rest_mm: f64,
    },
}

impl core::fmt::Display for Warning {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Warning::Crossing { moves } => write!(
                f,
                "feed moves {} and {} cross in XY at shared depth",
                moves[0], moves[1]
            ),
            Warning::Gouge { at } => {
                write!(f, "move {at} cuts past the finish surface")
            }
            Warning::ShankHit { at, part } => write!(
                f,
                "shank/holder at move {at} collides with the {}",
                if *part { "part or an island" } else { "uncut stock" }
            ),
            Warning::SweepHit { at, part } => write!(
                f,
                "shank swept through the {} mid-turn at move {at}",
                if *part { "part or an island" } else { "uncut stock" }
            ),
            Warning::RapidInStock { at } => {
                write!(f, "rapid move {at} travels below the clearance plane inside stock")
            }
            Warning::FixtureHit { at, fixture_part } => write!(
                f,
                "{} at move {at} strikes a fixture",
                match fixture_part {
                    FixturePart::Shank => "shank",
                    FixturePart::Holder => "holder",
                    FixturePart::Spindle => "spindle housing",
                    FixturePart::Rapid => "rapid",
                    FixturePart::Flute => "flute",
                }
            ),
            Warning::NonFinite { at } => write!(f, "move {at} has a non-finite coordinate"),
            Warning::Envelope {
                at,
                gouge_mm,
                rest_mm,
            } => write!(
                f,
                "finish envelope near move {at}: {gouge_mm:.4} mm gouge, {rest_mm:.4} mm rest"
            ),
        }
    }
}
