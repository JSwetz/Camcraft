//! The machine overview as label/value rows (#745): one structured
//! reading the Machine page and the machine editor's header both draw
//! as a property grid, so neither re-derives the facts from
//! [`Machine::summary`]'s one-liner. Unstated limits read "not stated",
//! never a bare `?` or an invented number.
//!
//! Provenance: our own layout; field semantics from the .mch reader's
//! own types, general machine-tool practice for the wording.

use super::def::{Kind, Machine};
use crate::geometry::units::{self, Unit};

fn rpm(x: f64) -> String {
    format!("{x:.0} rpm")
}

impl Machine {
    /// Label/value rows for a property grid, lengths in `unit` (rotary
    /// strokes stay degrees). Labels are stable: Kind, Machine, Spindle,
    /// Axes, Tool changer, Coolant, Workspace.
    #[must_use]
    pub fn rows(&self, unit: Unit) -> Vec<(&'static str, String)> {
        let kind = self
            .kind
            .as_deref()
            .and_then(Kind::from_word)
            .map_or_else(|| "not stated".to_owned(), |k| cap(k.label()));
        let name = format!("{} {}", self.vendor, self.model);
        let name = if name.trim().is_empty() { "not stated".to_owned() } else { name.trim().to_owned() };
        let mut spindle = match (self.spindle_min, self.spindle_max) {
            (Some(lo), Some(hi)) => format!("{} - {}", rpm(lo), rpm(hi)),
            (None, Some(hi)) => format!("up to {} (min not stated)", rpm(hi)),
            (Some(lo), None) => format!("from {} (max not stated)", rpm(lo)),
            (None, None) => "speed not stated".to_owned(),
        };
        if let Some(p) = self.spindle_power {
            spindle.push_str(&format!(" \u{b7} {p:.1} kW"));
        }
        let axes = if self.axes.is_empty() {
            "not stated".to_owned()
        } else {
            self.axes
                .iter()
                .map(|a| match a.travel {
                    Some((lo, hi)) if a.rotary => format!("{} {lo:.0} \u{2026} {hi:.0} deg", a.name),
                    Some((lo, hi)) => format!(
                        "{} {} \u{2026} {} {}",
                        a.name,
                        units::len(lo, unit, 0),
                        units::len(hi, unit, 0),
                        unit.suffix()
                    ),
                    None => format!("{} stroke not stated", a.name),
                })
                .collect::<Vec<_>>()
                .join(" \u{b7} ")
        };
        let changer = self.tool_count.map_or_else(|| "not stated".to_owned(), |n| format!("{n} slots"));
        let coolant = if self.coolants.is_empty() {
            "not stated".to_owned()
        } else {
            self.coolants.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
        };
        vec![
            ("Kind", kind),
            ("Machine", name),
            ("Spindle", spindle),
            ("Axes", axes),
            ("Tool changer", changer),
            ("Coolant", coolant),
            ("Workspace", cap(super::kind(Some(self)).label())),
        ]
    }
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str())
}
