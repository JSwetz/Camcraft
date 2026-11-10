//! The machine KIND: what a `.mch` declares itself to be (mill, lathe,
//! mill-turn, plasma), and the axis-inventory inference for files that
//! declare nothing.
//!
//! Provenance: general machine-tool practice (a mill moves a cutter in
//! X/Y/Z, a lathe turns the part under an X/Z tool, a mill-turn does
//! both); the plasma kind (#669) from Tormach's public 1300PL
//! Operator's Manual UM10720 v1223A §2 (an X/Y gantry whose Z is the
//! torch lifter) - behaviour only, our own words. Split out of `def`
//! (#669) along the kind seam, so `def` stays the plain data.

use super::def::{AxisLimits, Machine};

/// What a machine is, inferred from its axis inventory (M3.2 stage E -
/// the workspaces-by-kind law, refined by M5.1 stage A). Provenance:
/// general machine-tool practice - a mill moves a cutter in X, Y and
/// Z; a lathe turns the part and moves a tool in X and Z only; a
/// mill-turn does both, which an ordinary file reveals as a Y axis
/// with a TURNING rotary among its axes: one whose direction is the Z
/// column (a C table) or unstated. A rotary that states a LEANING
/// direction (an A or B head or trunnion) tilts a milling cutter and
/// turns nothing - a Y machine with only those is still a [`Kind::Mill`],
/// five axes of it. The inference stays deliberately the minimal
/// honest one: anything the axis list cannot decide - an unnamed file,
/// a file with no X/Z pair - reads [`Kind::Mill`], because a mill is
/// what every assumption in this codebase currently is, and an
/// invented lathe workspace would be the silent wrong answer.
///
/// [`Kind::Plasma`] (#669) is DECLARED only, never inferred: a plasma
/// table's X/Y/Z inventory is a mill's, so a file that does not say
/// "plasma" reads as the mill its axes describe. The declaration
/// carries the plasma rules (no spindle, no rotary, Z is the torch
/// lifter - `plasma::refusals`), so the word is never decoration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Mill,
    Lathe,
    MillTurn,
    /// A plasma cutting table (#669): X and Y cut, Z lifts the torch
    /// (the control owns it - pierce height, cut height, THC), and no
    /// spindle turns.
    Plasma,
}

impl Kind {
    /// The word the pages and tips use for the kind.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Kind::Mill => "milling",
            Kind::Lathe => "turning",
            Kind::MillTurn => "mill-turning",
            Kind::Plasma => "plasma cutting",
        }
    }

    /// The word a machine file writes for this kind - the canonical
    /// spelling the editor's combo emits, the subset `from_word`
    /// accepts.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Kind::Mill => "mill",
            Kind::Lathe => "lathe",
            Kind::MillTurn => "mill-turn",
            Kind::Plasma => "plasma",
        }
    }

    /// Read a declared kind word (M3.3 stage C): case- and
    /// punctuation-tolerant over the public vocabulary (a mill is
    /// also "milling", a lathe also "turning", and the hybrid's
    /// hyphen is optional). `None` = not a kind word; the caller
    /// refuses rather than guessing, because a typo'd declaration
    /// that silently inferred would be the declaration law undone.
    #[must_use]
    pub fn from_word(w: &str) -> Option<Kind> {
        let n: String = w
            .chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect();
        match n.as_str() {
            "mill" | "milling" => Some(Kind::Mill),
            "lathe" | "turning" | "turn" => Some(Kind::Lathe),
            "millturn" | "turnmill" | "millturning" | "turningmill" => Some(Kind::MillTurn),
            "plasma" | "plasmacutting" | "plasmatable" => Some(Kind::Plasma),
            _ => None,
        }
    }
}

/// The geometric tilt question (M5.1 stage A): can this machine lean
/// its tool? `machine.rtcp`'s law, restated for the checker's parsed
/// axes - true iff some rotary states a direction that is not the Z
/// column (z² < 0.99·(x²+y²+z²)). A C table is not tilt; a trunnion
/// or a tilting head is. An UNSTATED direction tilts nothing this
/// app may verify against: capability must be claimed, never
/// assumed.
#[must_use]
pub fn can_tilt(machine: &Machine) -> bool {
    machine.axes.iter().any(|a| a.rotary && is_tilt_axis(a))
}

/// Is `a` a rotary whose stated direction LEANS (off the Z column)?
fn is_tilt_axis(a: &AxisLimits) -> bool {
    a.axis
        .is_some_and(|d| d[2] * d[2] < 0.99 * (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]))
}

/// The machine's kind (M3.3 stage C): what the file DECLARES wins;
/// a silent file falls to the axis-inventory law of M3.2 stage E
/// (below), which the same files always produced. An unparseable
/// declaration cannot arrive here - `parse_mch` refuses it at the
/// front door and `is_sane` gates hand-edited project files - so by
/// the time a Machine exists, a stated kind is a known word.
///
/// Rotary axes alone do not make a mill-turn: a turning center with
/// a C axis still has no Y, and the file says "lathe". And (M5.1 A)
/// a TILTING rotary makes no more a mill-turn than a table does:
/// A and B heads and trunnions lean a milling cutter without ever
/// spinning a part for turning. The turning question is the same
/// geometry `can_tilt` answers, inverted - a rotary counts toward
/// "turning" unless its stated direction says lean; unstated
/// rotaries stay counted, the stage-E law for the files that
/// predate the vectors.
#[must_use]
pub fn kind(machine: Option<&Machine>) -> Kind {
    let Some(m) = machine else {
        return Kind::Mill; // the unstated machine is today's document
    };
    if let Some(w) = &m.kind
        && let Some(k) = Kind::from_word(w)
    {
        return k;
    }
    let has = |n: &str| m.axes.iter().any(|a| a.name.eq_ignore_ascii_case(n));
    let stated = |a: &AxisLimits| {
        a.rotary || ["A", "B", "C"].iter().any(|r| a.name.eq_ignore_ascii_case(r))
    };
    let turning = m.axes.iter().any(|a| stated(a) && !is_tilt_axis(a));
    match (has("Y"), has("X"), has("Z"), turning) {
        (true, _, _, true) => Kind::MillTurn,
        (true, _, _, false) => Kind::Mill,
        (false, true, true, _) => Kind::Lathe,
        _ => Kind::Mill,
    }
}

