//! Per-axis direction (#373): a machine whose linear axis moves the
//! other way for a positive command.
//!
//! Provenance: ISO 841 / RS-274 name the right-handed program frame the
//! whole CAM side keeps (generation, verification and the stock sim never
//! change); a machine-tool handbook's axis-direction note is the source
//! for "this slide's + goes the other way". The grammar (`"direction": +1
//! | -1` on a linear part of the `.mch` file) and every name here are ours.
//!
//! The law: the machine file states WHICH axis is inverted; the program
//! frame stays standard. A stated travel is the machine's own stroke - in
//! the coordinates its controller reads - so an inverted axis maps it
//! into the program frame by negation ([`AxisLimits::program_travel`]).
//! The post translates an inverted X / Y into the controller's frame
//! (#374, `post::invert`); what it cannot translate - an inverted Z, any
//! other letter - is refused by name ([`Machine::inverted_post_refusal`]):
//! a standard-sign program on it would move the slide the wrong way.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::def::{AxisLimits, Machine};

/// Which way an axis moves for a positive command. Written `1` / `-1`
/// in a machine file and a project file; absent reads [`Standard`].
///
/// [`Standard`]: AxisDirection::Standard
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AxisDirection {
    /// `+1`: the axis's + is the program frame's + (every shipped machine).
    #[default]
    Standard,
    /// `-1`: the axis's + moves the other way.
    Inverted,
}

impl AxisDirection {
    /// Read a file's stated sign: exactly `1` or `-1`, nothing else.
    #[must_use]
    pub fn from_sign(sign: f64) -> Option<Self> {
        if sign == 1.0 {
            Some(Self::Standard)
        } else if sign == -1.0 {
            Some(Self::Inverted)
        } else {
            None
        }
    }

    /// The sign as a factor: `1.0` or `-1.0`.
    #[must_use]
    pub const fn sign(self) -> f64 {
        match self {
            Self::Standard => 1.0,
            Self::Inverted => -1.0,
        }
    }

    /// For `skip_serializing_if`: a standard axis writes nothing, so every
    /// file saved before #373 and every standard machine stays byte-equal.
    #[must_use]
    pub fn is_standard(&self) -> bool {
        *self == Self::Standard
    }
}

impl Serialize for AxisDirection {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i8(if self.is_standard() { 1 } else { -1 })
    }
}

impl<'de> Deserialize<'de> for AxisDirection {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let n = f64::deserialize(d)?;
        Self::from_sign(n).ok_or_else(|| {
            serde::de::Error::custom(format!("axis direction must be +1 or -1, not {n}"))
        })
    }
}

impl AxisLimits {
    /// The stated stroke in the PROGRAM frame (#373): the file's travel is
    /// the machine's own, so an inverted axis reaches program `v` when its
    /// controller reads `-v` - the band `[lo, hi]` becomes `[-hi, -lo]`.
    /// A standard axis returns the travel unchanged.
    #[must_use]
    pub fn program_travel(&self) -> Option<(f64, f64)> {
        let (lo, hi) = self.travel?;
        Some(match self.direction {
            AxisDirection::Standard => (lo, hi),
            AxisDirection::Inverted => (-hi, -lo),
        })
    }
}

impl Machine {
    /// The letters of the linear axes the file states inverted, in table
    /// order. Empty for every standard machine.
    #[must_use]
    pub fn inverted_axes(&self) -> Vec<&str> {
        self.axes
            .iter()
            .filter(|a| !a.rotary && a.direction == AxisDirection::Inverted)
            .map(|a| a.name.as_str())
            .collect()
    }

    /// The post's refusal for an inverted axis it cannot translate
    /// (#374): an inverted Z, or any letter but X / Y. `None` for every
    /// standard machine and for an inverted X and/or Y, which the post
    /// translates ([`crate::post::invert`]).
    #[must_use]
    pub fn inverted_post_refusal(&self) -> Option<String> {
        untranslated_inversion(&self.model, &self.inverted_axes())
    }
}

/// "machine M has an inverted X axis" / "... inverted X and Y axes".
#[must_use]
pub fn inverted_phrase(model: &str, letters: &[&str]) -> String {
    let Some((last, rest)) = letters.split_last() else {
        return format!("machine {model} has no inverted axis");
    };
    if rest.is_empty() {
        format!("machine {model} has an inverted {last} axis")
    } else {
        format!("machine {model} has inverted {} and {last} axes", rest.join(", "))
    }
}

/// The machine-level refusal (#374) for inverted letters the post does
/// not translate. An inverted X and/or Y is translated; an inverted Z is
/// not - tool-length compensation (G43), the canned cycles' R-plane
/// retract and every "up is away from the work" convention assume the
/// controller's +Z leaves the part - and neither is any other letter.
#[must_use]
pub fn untranslated_inversion(model: &str, letters: &[&str]) -> Option<String> {
    let bad: Vec<&str> = letters.iter().copied().filter(|l| *l != "X" && *l != "Y").collect();
    if bad.is_empty() {
        return None;
    }
    let why = if bad == ["Z"] {
        "tool-length compensation (G43) and the canned cycles' retract assume the controller's +Z leaves the work"
    } else {
        "the post translates an inverted X or Y only"
    };
    Some(format!(
        "{}; posting for an inverted {} is not supported yet - {why}",
        inverted_phrase(model, letters),
        bad.join(" / ")
    ))
}
