//! The operation list's Pause / Home entries (#155), as the UI queues
//! them: add one after a row (or at the active setup's end), change what
//! it does, suppress it. Each verb is one undoable app command; delete
//! rides `DeleteByName` and reorder `ReorderOperation`, as for an op.
//! Provenance: our own routing; no vendor material consulted.

use camcraft_app::commands::{AddProgramEntry, EditProgramEntry};
use camcraft_app::operation::AfterOperation;
use camcraft_app::program_entry;

use super::{ErrorScope, State};

/// One entry verb, carried by `CommandKind::ProgramEntry`.
#[derive(Clone, Debug, PartialEq)]
pub enum EntryVerb {
    /// Add an entry asking `action`: right after the row `after` names,
    /// or at the end of the active setup's list.
    Add { action: AfterOperation, after: Option<String> },
    /// What the machine does at the entry.
    SetAction { name: String, action: AfterOperation },
    /// Leave the entry out of the posted program, or put it back.
    Suppress { name: String, suppressed: bool },
}

impl State {
    pub(super) fn program_entry_verb(&mut self, verb: EntryVerb) {
        match verb {
            EntryVerb::Add { action, after } => match AddProgramEntry::new(&self.world, action, after.as_deref()) {
                Ok(command) => {
                    let name = command.name().to_owned();
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("{name} added: {}", program_entry::action_label(action));
                }
                Err(err) => self.refuse(ErrorScope::Operation, format!("entry not added: {err}")),
            },
            EntryVerb::SetAction { name, action } => match EditProgramEntry::with_action(&self.world, &name, action) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("{name}: {}", program_entry::action_label(action));
                }
                Err(err) => self.refuse(ErrorScope::Operation, format!("entry not changed: {err}")),
            },
            EntryVerb::Suppress { name, suppressed } => {
                match EditProgramEntry::with_suppressed(&self.world, &name, suppressed) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = if suppressed {
                            format!("{name} suppressed: left out of the posted program")
                        } else {
                            format!("{name} back in the posted program")
                        };
                    }
                    Err(err) => self.refuse(ErrorScope::Operation, format!("suppression not changed: {err}")),
                }
            }
        }
    }
}
