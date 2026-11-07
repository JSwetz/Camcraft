//! What the machine does once an operation is done (issue #110): carry
//! on, an optional stop, a program stop, or a trip home. Provenance:
//! the RS-274 / ISO 6983 miscellaneous functions M0 (program stop) and
//! M1 (optional stop) and the reference-return idea of public control
//! programming manuals; our own names and stream spelling, no vendor
//! material.
//!
//! The stream law, the coolant's twin: carrying on is what a section
//! states by saying nothing - every stream written before the choice
//! existed posts exactly as it always did. Any other choice rides one
//! `PARAM AfterOperation <word>` ahead of the section; the post writes
//! its dialect's words after the section's spindle stop, and a dialect
//! that cannot honor the choice refuses by name.

use serde::{Deserialize, Serialize};

/// One operation's after-action. `None` is the default: the program
/// runs straight on to the next operation, as it always did.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum AfterOperation {
    /// Carry on to the next operation.
    #[default]
    None,
    /// Stop only when the operator has the optional-stop switch on (M1).
    OptionalStop,
    /// Always stop and wait for cycle start (M0).
    ProgramStop,
    /// Retract to the machine's safe Z, then move home in X and Y.
    GoHome,
}

impl AfterOperation {
    /// Every choice, in picker order.
    pub const ALL: [AfterOperation; 4] = [
        AfterOperation::None,
        AfterOperation::OptionalStop,
        AfterOperation::ProgramStop,
        AfterOperation::GoHome,
    ];

    /// The picker label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            AfterOperation::None => "Continue",
            AfterOperation::OptionalStop => "Optional stop",
            AfterOperation::ProgramStop => "Program stop",
            AfterOperation::GoHome => "Go home",
        }
    }

    /// The `PARAM AfterOperation` word the stream carries, or `None`
    /// for carrying on (the stream's silent default - see the module law).
    #[must_use]
    pub fn param_word(self) -> Option<&'static str> {
        match self {
            AfterOperation::None => None,
            AfterOperation::OptionalStop => Some("optional-stop"),
            AfterOperation::ProgramStop => Some("stop"),
            AfterOperation::GoHome => Some("home"),
        }
    }

    /// Whether this is the default (serde's skip test: an untouched
    /// operation writes no key, so older files round-trip unchanged).
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == AfterOperation::None
    }
}
