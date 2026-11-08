//! The cutting fluid an operation asks for, as the stream carries it
//! (issue #112). Provenance: the coolant families of public machine-tool
//! handbooks (flood, mist, through-the-tool delivery) and the RS-274 /
//! ISO 6983 miscellaneous-function convention (M7 mist, M8 flood, M9
//! off); our own names and stream spelling, no vendor material.
//!
//! The stream law: flood is what a section states by saying nothing -
//! every stream written before the field existed, and every legacy
//! `.cl`, posts exactly as it always did. Any other ask rides one
//! `PARAM Coolant <word>` ahead of the section; the post looks the
//! word's code up in the mounted machine's coolant list (the machine
//! is truth) and refuses by name when the machine states none.

use serde::{Deserialize, Serialize};

/// One operation's coolant choice. `Flood` is the default: it is what
/// every post emitted before the choice existed.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum CoolantMode {
    /// No cutting fluid: the section switches nothing on.
    Off,
    /// Flood coolant - the machine's `Flood` entry (M8 on most controls).
    #[default]
    Flood,
    /// Mist (air-carried fluid) - the machine's `Mist` entry (M7 on most).
    Mist,
    /// Fluid through the tool's own bore - the machine's
    /// `Through-spindle` / `Through-tool` entry; only machines that state
    /// one can post it.
    ThroughTool,
}

impl CoolantMode {
    /// Every mode, in picker order.
    pub const ALL: [CoolantMode; 4] = [
        CoolantMode::Off,
        CoolantMode::Flood,
        CoolantMode::Mist,
        CoolantMode::ThroughTool,
    ];

    /// The picker label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            CoolantMode::Off => "Off",
            CoolantMode::Flood => "Flood",
            CoolantMode::Mist => "Mist",
            CoolantMode::ThroughTool => "Through tool",
        }
    }

    /// The `PARAM Coolant` word the stream carries, or `None` for flood
    /// (the stream's silent default - see the module law).
    #[must_use]
    pub fn param_word(self) -> Option<&'static str> {
        match self {
            CoolantMode::Off => Some("off"),
            CoolantMode::Flood => None,
            CoolantMode::Mist => Some("mist"),
            CoolantMode::ThroughTool => Some("through-tool"),
        }
    }

    /// Whether this is the default (serde's skip test: an untouched
    /// operation writes no key, so older files round-trip unchanged).
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == CoolantMode::Flood
    }
}
