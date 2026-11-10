//! `SetSetupPostOptions` (#389): give a setup its own post options, change
//! them, or let it follow the document's again (`None`). Construction
//! snapshots both values and refuses a no-op; `apply` / `invert` are pure
//! writes of `SetupDef::post_options`, so undo puts the previous choice
//! back exactly.
//!
//! Provenance: our own design (the `SetSetupPost` shape, #343); no vendor
//! implementation inspected.

use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::model;
use crate::schema::{PostOptions, SetupDef};

/// Write a setup's own post options; `None` follows the document's.
#[derive(Debug)]
pub struct SetSetupPostOptions {
    setup: String,
    prev: Option<PostOptions>,
    next: Option<PostOptions>,
}

impl SetSetupPostOptions {
    pub fn new(world: &World, setup: &str, next: Option<PostOptions>) -> Result<Self, CommandError> {
        let entity = model::find_by_name(world, setup).ok_or_else(|| CommandError::NotFound(setup.to_string()))?;
        let prev = entity
            .entity_view(world)
            .try_get::<&SetupDef>(|d| d.post_options)
            .ok_or_else(|| CommandError::Invalid(format!("'{setup}' is not a setup")))?;
        if prev == next {
            return Err(CommandError::Invalid(match next {
                None => format!("setup '{setup}' already follows the document's post options"),
                Some(_) => format!("setup '{setup}' already has those post options"),
            }));
        }
        Ok(Self { setup: setup.to_owned(), prev, next })
    }

    #[must_use]
    pub fn setup(&self) -> &str {
        &self.setup
    }
}

fn write(world: &World, setup: &str, value: Option<PostOptions>) {
    let Some(entity) = model::find_by_name(world, setup) else {
        return;
    };
    let view = entity.entity_view(world);
    if let Some(mut def) = view.try_get::<&SetupDef>(|d| d.clone()) {
        def.post_options = value;
        view.set(def);
    }
}

impl Command for SetSetupPostOptions {
    fn apply(&self, world: &World) {
        write(world, &self.setup, self.next);
    }

    fn invert(&self, world: &World) {
        write(world, &self.setup, self.prev);
    }
}
