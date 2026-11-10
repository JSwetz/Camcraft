//! The locate verb's dispatch arm (#561): the Fixtures page and the Model
//! page queue `CommandKind::Locate`; it lands here as one undoable
//! `Locate` (`LocatePart` / `LocateFixture`). A refusal names itself on
//! the status line and keeps the draft so the picks can be fixed; an
//! interpenetration warning lands on the status line after the move.
//! Provenance: our own routing over our own commands; no vendor material.

use camcraft_app::commands::Locate;
use camcraft_app::fixture_locate::{LocateSpec, Mover};

use super::{ErrorScope, State};

impl State {
    pub(super) fn dispatch_locate(&mut self, spec: &LocateSpec, mover: Mover) {
        match Locate::new(&self.world, spec, mover) {
            Ok(command) => {
                let warning = command.warning().map(str::to_owned);
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                // The mates stay (hand check of #566): an Offset or Turn
                // edit re-solves from them and Apply lands it as one more
                // undo step; Stop locating ends the draft.
                if let Some(d) = self.locate_pick.as_mut() {
                    d.half = None;
                }
                let moved = match mover {
                    Mover::Part => format!("located {} in {}", spec.part, spec.fixture),
                    Mover::Fixture => format!("located {} on {}", spec.fixture, spec.part),
                };
                self.status = match warning {
                    Some(w) => format!("{moved} - warning: {w}"),
                    None => moved,
                };
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("not located: {err}")),
        }
    }
}
