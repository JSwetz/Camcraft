//! The document's unit choice - the plain type, no ECS.
//!
//! Provenance: the international inch (exactly 25.4 mm) and the
//! internal-metric pattern of CAD/CAM systems and G-code (RS-274
//! `G20`/`G21`); general published practice, nothing copied.
//!
//! Crate-boundary home (M4.1 R0, docs/M4.1-CRATE-SPLIT.md): this enum is
//! pure data and belongs to the geometry crate; the flecs singleton glue
//! and the display formatters live on in the app crate's `units`
//! module.

/// Exact by the 1959 international agreement.
pub const MM_PER_INCH: f64 = 25.4;

/// The document's display unit. Copy, total, and boring on purpose:
/// nothing in this enum is geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Unit {
    #[default]
    Millimeters,
    Inches,
}

impl Unit {
    /// The word the UI's radio shows.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Unit::Millimeters => "Millimeters",
            Unit::Inches => "Inches",
        }
    }

    /// The length suffix for display strings.
    #[must_use]
    pub fn suffix(self) -> &'static str {
        match self {
            Unit::Millimeters => "mm",
            Unit::Inches => "in",
        }
    }

    /// The feed suffix for labels and strings.
    #[must_use]
    pub fn feed_suffix(self) -> &'static str {
        match self {
            Unit::Millimeters => "mm/min",
            Unit::Inches => "in/min",
        }
    }

    /// The area suffix (face readouts).
    #[must_use]
    pub fn area_suffix(self) -> &'static str {
        match self {
            Unit::Millimeters => "mm²",
            Unit::Inches => "in²",
        }
    }

    /// The project-file spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Unit::Millimeters => "mm",
            Unit::Inches => "in",
        }
    }

    /// Parse a project-file spelling; exact and case-sensitive (the wire
    /// is written by [`Unit::as_str`], never by hand-guessing humans).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "mm" => Some(Unit::Millimeters),
            "in" => Some(Unit::Inches),
            _ => None,
        }
    }
}

