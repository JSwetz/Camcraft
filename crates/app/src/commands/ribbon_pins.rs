//! SetRibbonPins: pin or unpin operation kinds on the ribbon (issue #143).
//!
//! Pins are saved in the project file, so changing them changes the
//! document: it goes through the history like `SetTheme`, can be undone,
//! and counts as an unsaved change. The command writes only the
//! `RibbonPins` singleton (through `ribbon::pinned_set`) and its inverse
//! writes the previous list back. Provenance: our own command, mirroring
//! our own `theme.rs`; specced from issue #143 and AGENTS.md's
//! "Everything Is a Command".

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::operation::KIND_COUNT;
use crate::ribbon::{pinned_of, pinned_set};

pub struct SetRibbonPins {
    prev: Vec<usize>,
    next: Vec<usize>,
}

impl SetRibbonPins {
    /// Replace the pinned list with `next` (pin order kept). The list is
    /// cleaned the way `pinned_set` writes it - duplicates and indices
    /// past the kind table dropped - so `prev`/`next` are exactly what the
    /// world will hold. A list equal to the current one is refused, so a
    /// no-op never lands on the undo stack.
    pub fn new(world: &World, next: &[usize]) -> Result<Self, CommandError> {
        let mut clean: Vec<usize> = Vec::with_capacity(next.len());
        for i in next {
            if *i < KIND_COUNT && !clean.contains(i) {
                clean.push(*i);
            }
        }
        let prev = pinned_of(world);
        if prev == clean {
            return Err(CommandError::Invalid(
                "the ribbon pins are already set that way".to_owned(),
            ));
        }
        Ok(Self { prev, next: clean })
    }
}

impl Command for SetRibbonPins {
    fn apply(&self, world: &World) {
        pinned_set(world, &self.next);
    }

    fn invert(&self, world: &World) {
        pinned_set(world, &self.prev);
    }
}
