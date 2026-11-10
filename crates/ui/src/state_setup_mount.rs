//! The machine and post verbs, routed to whoever owns the mount (#343).
//! The ribbon, the Stock page's machine/post groups and a setup's own
//! Machine / Post pages all queue the same `CommandKind`s; they land on
//! the ACTIVE setup's own machine/post (`SetSetupMachine` / `SetSetupPost`)
//! when it owns one, and on the document's (`LoadMachine` / `SelectPost`)
//! when it follows the document - so a verb never edits a mount the
//! active setup does not use. Every arm is one undoable command, and a
//! refusal names itself on the status line.
//! Provenance: our own routing law (issue #343); no vendor material consulted.

use std::path::Path;

use camcraft_app::commands::{CommandError, LoadMachine, SelectPost, SetSetupMachine, SetSetupPost};
use camcraft_app::machine::library::{self, MachineSource};
use camcraft_app::machine::Machine;
use camcraft_app::schema::PostRef;
use camcraft_app::{machine, posting, setup, setup_mount};

use super::{ErrorScope, State};

/// Which command a routed machine verb builds.
enum MachineVerb<'a> {
    Builtin(&'a str),
    File(&'a Path),
    Mount(String, Option<Box<Machine>>),
}

impl State {
    /// The active setup when it owns its machine: the setup the machine
    /// verbs edit. `None` - they edit the document's.
    #[must_use]
    pub fn machine_owner(&self) -> Option<String> {
        let active = setup::active(&self.world)?;
        setup_mount::own_machine(&self.world, Some(&active)).map(|_| active)
    }

    /// The active setup when it owns its post.
    #[must_use]
    pub fn post_owner(&self) -> Option<String> {
        let active = setup::active(&self.world)?;
        setup_mount::own_post(&self.world, Some(&active)).map(|_| active)
    }

    fn route_machine(&mut self, verb: MachineVerb<'_>, done: &str, refused: &str) -> bool {
        let executed: Result<(), CommandError> = match self.machine_owner() {
            Some(s) => match verb {
                MachineVerb::Builtin(n) => SetSetupMachine::builtin(&self.world, &s, n),
                MachineVerb::File(p) => SetSetupMachine::from_file(&self.world, &s, p),
                MachineVerb::Mount(p, m) => SetSetupMachine::new(&self.world, &s, Some((p, m.map(|b| *b)))),
            }
            .map(|c| self.history.execute(&self.world, c)),
            None => match verb {
                MachineVerb::Builtin(n) => LoadMachine::builtin(&self.world, n),
                MachineVerb::File(p) => LoadMachine::from_file(&self.world, p),
                MachineVerb::Mount(p, m) => LoadMachine::new(&self.world, p, m.map(|b| *b)),
            }
            .map(|c| self.history.execute(&self.world, c)),
        };
        match executed {
            Ok(()) => {
                self.doc_version += 1;
                self.machine_editor = None;
                let whose = self.machine_owner().map_or_else(String::new, |s| format!(" ({s})"));
                self.status = format!("{done}{whose}");
                true
            }
            // Refused edits keep the editor open: the draft is someone's work.
            Err(err) => {
                self.refuse(ErrorScope::Setup, format!("{refused}: {err}"));
                false
            }
        }
    }

    /// Mount a shipped library machine by name.
    pub(super) fn mount_builtin(&mut self, name: &str) {
        let _ = self.route_machine(MachineVerb::Builtin(name), &format!("machine: {name} (built-in)"), "machine not loaded");
    }

    /// Mount a `.mch` file.
    pub(super) fn mount_file(&mut self, path: &Path) {
        if self.route_machine(MachineVerb::File(path), "machine loaded", "machine not loaded") {
            let summary = machine::of(&self.world).1.map(|m| m.summary()).unwrap_or_default();
            self.status = format!("machine: {summary}");
        }
    }

    /// State the absence of a machine.
    pub(super) fn mount_clear(&mut self) {
        let _ = self.route_machine(MachineVerb::Mount(String::new(), None), "machine cleared", "machine not cleared");
    }

    /// The machine editor's Apply.
    pub(super) fn mount_edited(&mut self, path: String, m: Machine) {
        let _ = self.route_machine(MachineVerb::Mount(path, Some(Box::new(m))), "machine edited", "machine edit refused");
    }

    /// The Machines pane's "Use for this setup" (#384): the catalogue
    /// machine is read and validated again (a refused file never
    /// mounts), then routed like every machine verb - onto the active
    /// setup's own machine when it has one, else the document's.
    pub(super) fn mount_catalogue(&mut self, source: &MachineSource) {
        match library::read_for_assign(source) {
            Ok((path, m)) => {
                let done = format!("machine: {path}");
                let _ = self.route_machine(MachineVerb::Mount(path, Some(Box::new(m))), &done, "machine not assigned");
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("machine not assigned: {err}")),
        }
    }

    /// Choose the post script (M3.3 A), for the active setup's own post
    /// when it has one. Validation lives in the command constructors, so
    /// a script that cannot run is refused at the pick, never at the post.
    pub(super) fn choose_post(&mut self, next: PostRef) {
        let executed = match self.post_owner() {
            Some(s) => SetSetupPost::new(&self.world, &s, Some(next)).map(|c| self.history.execute(&self.world, c)),
            None => SelectPost::new(&self.world, next).map(|c| self.history.execute(&self.world, c)),
        };
        match executed {
            Ok(()) => {
                self.doc_version += 1;
                self.status = format!("post: {}", posting::post_label(&self.world));
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("post not selected: {err}")),
        }
    }

    /// A setup takes its own machine (a copy of the one it mounts now, so
    /// nothing it cuts changes) or follows the document's again.
    pub(super) fn own_machine(&mut self, setup: &str, own: bool) {
        let next = own.then(|| machine::of_setup(&self.world, Some(setup)));
        match SetSetupMachine::new(&self.world, setup, next) {
            Ok(c) => {
                self.history.execute(&self.world, c);
                self.doc_version += 1;
                self.status = if own {
                    format!("{setup} has its own machine")
                } else {
                    format!("{setup} follows the document's machine")
                };
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("machine not changed: {err}")),
        }
    }

    /// A setup takes its own post (a copy of the one it posts with now)
    /// or follows the document's again.
    pub(super) fn own_post(&mut self, setup: &str, own: bool) {
        let next = own.then(|| setup_mount::post_of(&self.world, Some(setup)));
        match SetSetupPost::new(&self.world, setup, next) {
            Ok(c) => {
                self.history.execute(&self.world, c);
                self.doc_version += 1;
                self.status = if own {
                    format!("{setup} has its own post")
                } else {
                    format!("{setup} follows the document's post")
                };
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("post not changed: {err}")),
        }
    }
}
