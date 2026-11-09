//! `SetSetupWorkOffset` (#196, #342): label a setup's one WCS with the
//! `G54`..`G59` word the post writes for it, or clear the label back to
//! the `G54` default. Only the word changes - the setup's WCS origin, and
//! every height measured from it, stay where they are.
//!
//! Provenance: our own design; RS-274 / ISO 6983 work-offset selection
//! (one of six controller zero registers per clamping) is general
//! machine-shop practice - no vendor implementation inspected. Like its
//! setup siblings, construction validates and snapshots both values;
//! `apply`/`invert` are pure writes of the snapshot.

use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::model;
use crate::schema::{SetupDef, WCS_SLOTS};

/// Write a setup's work-offset word. `None` clears the label.
pub struct SetSetupWorkOffset {
    setup: String,
    prev: Option<usize>,
    next: Option<usize>,
}

impl SetSetupWorkOffset {
    pub fn new(world: &World, setup: &str, slot: Option<usize>) -> Result<Self, CommandError> {
        let entity = model::find_by_name(world, setup)
            .ok_or_else(|| CommandError::NotFound(setup.to_string()))?;
        let prev = entity
            .entity_view(world)
            .try_get::<&SetupDef>(|d| d.work_offset)
            .ok_or_else(|| CommandError::Invalid(format!("'{setup}' is not a setup")))?;
        if let Some(i) = slot
            && i >= WCS_SLOTS.len()
        {
            return Err(CommandError::Invalid(format!(
                "work offset #{i} does not exist - the slots are G54 to G59"
            )));
        }
        if prev == slot {
            return Err(CommandError::Invalid(format!(
                "setup '{setup}' already posts at {}",
                slot.and_then(|i| WCS_SLOTS.get(i)).copied().unwrap_or("G54")
            )));
        }
        Ok(SetSetupWorkOffset {
            setup: setup.to_string(),
            prev,
            next: slot,
        })
    }

    fn write(&self, world: &World, slot: Option<usize>) {
        let Some(entity) = model::find_by_name(world, &self.setup) else {
            return;
        };
        let view = entity.entity_view(world);
        if let Some(mut def) = view.try_get::<&SetupDef>(|d| d.clone()) {
            def.work_offset = slot;
            view.set(def);
        }
    }
}

impl Command for SetSetupWorkOffset {
    fn apply(&self, world: &World) {
        self.write(world, self.next);
    }

    fn invert(&self, world: &World) {
        self.write(world, self.prev);
    }
}
