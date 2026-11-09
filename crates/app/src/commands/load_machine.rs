//! LoadMachine: set the document's machine definition (M2.3 stage C).
//!
//! Setup furniture, like `SetUnits`: no geometry changes, the command
//! writes only the world singleton and its inverse writes the previous
//! value back. The `.mch` text is parsed at command construction -
//! like every other command, a bad file is a refusal at the front
//! door, never a half-written setup.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::machine::{self, Machine};

#[derive(Debug)]
pub struct LoadMachine {
    prev: (String, Option<Machine>),
    next: (String, Option<Machine>),
}

impl LoadMachine {
    /// Mount a parsed machine, loaded from `path` for display. Passing
    /// `None` clears the setup - the checker then checks nothing, by
    /// the unstated-limit law, and that is stated, not silent.
    pub fn new(world: &World, path: String, machine: Option<Machine>) -> Result<Self, CommandError> {
        if let Some(m) = &machine
            && !m.is_sane()
        {
            return Err(CommandError::Invalid(
                "machine states a limit that cannot be meant".to_owned(),
            ));
        }
        let prev = machine::of(world);
        if prev.0 == path && prev.1 == machine {
            return Err(CommandError::Invalid(if machine.is_none() {
                "no machine is loaded".to_owned()
            } else {
                "that machine is already loaded".to_owned()
            }));
        }
        Ok(Self {
            prev,
            next: (path, machine),
        })
    }

    /// Read and parse a `.mch` file, then mount it. The refusal names
    /// the file: a machine that loads from nowhere is the silent wrong
    /// answer wearing a setup change.
    pub fn from_file(world: &World, path: &std::path::Path) -> Result<Self, CommandError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| CommandError::Invalid(format!("machine file {}: {e}", path.display())))?;
        let m = crate::machine::parse_mch(&text).map_err(CommandError::Invalid)?;
        Self::new(world, path.display().to_string(), Some(m))
    }

    /// Mount a built-in machine from the shipped library (M3.3 stage
    /// C) by NAME - a stale or typo'd name refuses instead of quietly
    /// mounting a different machine than the pane advertised. The
    /// mount path records the library, not a file that exists.
    pub fn builtin(world: &World, name: &str) -> Result<Self, CommandError> {
        let m = match crate::posting::MACHINES.iter().find(|(n, _)| *n == name) {
            Some((_, text)) => crate::machine::parse_mch(text).map_err(CommandError::Invalid)?,
            None => {
                return Err(CommandError::Invalid(format!(
                    "machine {name} is not in the built-in library"
                )));
            }
        };
        Self::new(world, crate::posting::builtin_path(name), Some(m))
    }

    /// Mount a machine picked in the Machines pane's catalogue (issue
    /// #384) - the pane's "Use for this setup". The file is read and
    /// VALIDATED again here, not trusted from the pane's cached entry:
    /// a file that changed on disk since the list was read, or one with
    /// refusals, is refused at the front door by its first refusal,
    /// because the forgiving reader would otherwise mount a machine
    /// whose stated limits it quietly dropped.
    pub fn from_catalogue(
        world: &World,
        source: &crate::machine::library::MachineSource,
    ) -> Result<Self, CommandError> {
        use crate::machine::library::MachineSource;
        let text: String = match source {
            MachineSource::Builtin(name) => match crate::posting::MACHINES.iter().find(|(n, _)| n == name) {
                Some((_, text)) => (*text).to_owned(),
                None => {
                    return Err(CommandError::Invalid(format!(
                        "machine {name} is not in the built-in library"
                    )));
                }
            },
            MachineSource::File(path) => std::fs::read_to_string(path)
                .map_err(|e| CommandError::Invalid(format!("machine file {}: {e}", path.display())))?,
        };
        let refusals = crate::machine::validate::validate_text(&text);
        if let Some(first) = refusals.first() {
            return Err(CommandError::Invalid(format!(
                "{} fails validation - {first}",
                source.mount_path()
            )));
        }
        let m = crate::machine::parse_mch(&text).map_err(CommandError::Invalid)?;
        Self::new(world, source.mount_path(), Some(m))
    }
}

impl Command for LoadMachine {
    fn apply(&self, world: &World) {
        machine::set(world, &self.next.0, self.next.1.as_ref());
    }

    fn invert(&self, world: &World) {
        machine::set(world, &self.prev.0, self.prev.1.as_ref());
    }
}
