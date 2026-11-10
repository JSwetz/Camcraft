//! The document's unit choice - the plain type, no ECS.
//!
//! Provenance: the international inch (exactly 25.4 mm) and the
//! internal-metric pattern of CAD/CAM systems and G-code (RS-274
//! `G20`/`G21`); general published practice, nothing copied.
//!
//! Crate-boundary home (M4.1 R0/R2, docs/M4.1-CRATE-SPLIT.md): the enum
//! and its display formatters are pure data and belong to the geometry
//! crate; only the flecs singleton glue lives on in the app crate's
//! `units` module (which re-exports all of this).

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

    /// The volume suffix (stock blocks, removed metal).
    #[must_use]
    pub fn vol_suffix(self) -> &'static str {
        match self {
            Unit::Millimeters => "mm³",
            Unit::Inches => "in³",
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

/// A stored millimeter value as the number the widget shows.
#[must_use]
pub fn to_display(v_mm: f64, unit: Unit) -> f64 {
    match unit {
        Unit::Millimeters => v_mm,
        Unit::Inches => v_mm / MM_PER_INCH,
    }
}

/// The shown number back to storage. Inches snap onto the micron lattice
/// (the same quantum as `cl::lattice`) so `0.5 in` is exactly `12.7 mm`
/// and no stored value ever carries float-conversion dust. Millimeter
/// input is already storage: identity, preserving what typing has always
/// done in mm mode.
#[must_use]
pub fn from_display(v_shown: f64, unit: Unit) -> f64 {
    match unit {
        Unit::Millimeters => v_shown,
        Unit::Inches => (v_shown * MM_PER_INCH * 1e6).round() / 1e6,
    }
}

/// Display string for a length, without the suffix (sites add their own).
/// The precision law: the site's millimeter decimals are kept verbatim
/// in mm mode - every pre-existing string stays byte-identical - and
/// inches get two more, because a mil (0.001 in) is finer than a
/// hundredth of a millimeter and an inch readout must not lose
/// information the mm one carried.
#[must_use]
pub fn len(v_mm: f64, unit: Unit, mm_prec: usize) -> String {
    match unit {
        Unit::Millimeters => format!("{v_mm:.mm_prec$}"),
        Unit::Inches => format!("{:.prec$}", v_mm / MM_PER_INCH, prec = mm_prec + 2),
    }
}

/// [`len`] with an explicit sign - for offsets and echoes that have
/// always read `+0.00`/`-0.00`.
#[must_use]
pub fn len_signed(v_mm: f64, unit: Unit, mm_prec: usize) -> String {
    match unit {
        Unit::Millimeters => format!("{v_mm:+.mm_prec$}"),
        Unit::Inches => format!("{:+.prec$}", v_mm / MM_PER_INCH, prec = mm_prec + 2),
    }
}

/// A feed rate stored in mm/min. mm mode keeps the bare `Display` the
/// strings have always used; inches divide and settle for two decimals
/// (whole in/min steps are coarse enough for any spindle).
#[must_use]
pub fn feed(v_mmmin: f64, unit: Unit) -> String {
    match unit {
        Unit::Millimeters => format!("{v_mmmin}"),
        Unit::Inches => format!("{:.2}", v_mmmin / MM_PER_INCH),
    }
}

/// A volume in mm^3 (a stock block, the metal a program removed). The
/// inch cube is ~16 000 mm^3, so like [`area`] the inch side needs more
/// decimals than the +2 [`len`] rule: three extra holds the millimeter
/// reading without turning a small cut into a string of zeros.
#[must_use]
pub fn vol(v_mm3: f64, unit: Unit, mm_prec: usize) -> String {
    match unit {
        Unit::Millimeters => format!("{v_mm3:.mm_prec$}"),
        Unit::Inches => format!(
            "{:.prec$}",
            v_mm3 / (MM_PER_INCH * MM_PER_INCH * MM_PER_INCH),
            prec = mm_prec + 3
        ),
    }
}

/// An area in mm^2 (face readouts); inches divide twice, so the +2
/// precision rule from [`len`] would understate it - three extra
/// decimals carry the same information at the 645x scale.
#[must_use]
pub fn area(v_mm2: f64, unit: Unit) -> String {
    match unit {
        Unit::Millimeters => format!("{v_mm2:.1}"),
        Unit::Inches => format!("{:.4}", v_mm2 / (MM_PER_INCH * MM_PER_INCH)),
    }
}
