//! `SetSetupRelative` (#316): place a setup relative to another - "this
//! setup is that one flipped over X / turned 90° about Z / turned by
//! these angles" - or unlink it. The turn is resolved into the setup's
//! part alignment (#326) through `AlignPartToSetup`, so there is still
//! one transform per setup; the link only makes a later change of the
//! other setup's alignment carry into this one
//! ([`crate::setup_relative::carried`]).
//!
//! Provenance: our own design; re-clamping a part flipped or turned
//! between setups is plain shop practice, the algebra standard rigid-
//! body composition. No vendor implementation inspected.
//!
//! ## The laws this command keeps
//!
//! - **Validated and snapshotted at construction.** Both setups live,
//!   not the same setup, no loop of links, finite angles, and a change
//!   from what is held. `apply`/`invert` are pure writes.
//! - **The work offset is not touched.** The turn is on the part; the
//!   `G54`..`G59` word and the WCS origin stay as they are.
//! - **Unlinking keeps the part where it is.** Only the link goes; the
//!   alignment stays, so nothing moves and no path goes stale.

use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::World;

use crate::commands::{AlignPartToSetup, Command, CommandError};
use crate::model;
use crate::schema::SetupDef;
use crate::setup;
use crate::setup_relative;

/// Link (or unlink) a setup to the setup it is a re-clamp of.
pub struct SetSetupRelative {
    setup: String,
    prev_link: Option<String>,
    next_link: Option<String>,
    /// The alignment change the turn needs (`None`: the part already sits
    /// that way).
    align: Option<AlignPartToSetup>,
}

impl SetSetupRelative {
    /// Place `name` relative to `parent`, turned by `deg` (about X, then
    /// Y, then Z, degrees) in `parent`'s frame about the parts' centre.
    /// All zero is "the same as `parent`".
    pub fn link(world: &World, name: &str, parent: &str, deg: [f64; 3]) -> Result<Self, CommandError> {
        let prev_link = held_link(world, name)?;
        if !setup::exists(world, parent) {
            return Err(CommandError::NotFound(parent.to_string()));
        }
        if parent == name {
            return Err(CommandError::Invalid(format!("setup '{name}' cannot be placed relative to itself")));
        }
        if setup_relative::links_back_to(world, parent, name) {
            return Err(CommandError::Invalid(format!(
                "'{parent}' is already placed relative to '{name}' - linking back would make a loop"
            )));
        }
        let rot = setup_relative::rotation(deg).map_err(CommandError::Invalid)?;
        let next = setup_relative::relative_alignment(world, parent, rot).map_err(CommandError::Invalid)?;
        let held = crate::setup_align::stored(world, name);
        let align = if setup_relative::same(held, next) {
            None
        } else {
            Some(AlignPartToSetup::new(world, name, next)?)
        };
        if align.is_none() && prev_link.as_deref() == Some(parent) {
            return Err(CommandError::Invalid(format!("setup '{name}' already sits that way relative to '{parent}'")));
        }
        Ok(SetSetupRelative { setup: name.to_string(), prev_link, next_link: Some(parent.to_string()), align })
    }

    /// Unlink `name`: the part stays where it is.
    pub fn unlink(world: &World, name: &str) -> Result<Self, CommandError> {
        let prev_link = held_link(world, name)?;
        if prev_link.is_none() {
            return Err(CommandError::Invalid(format!("setup '{name}' is not placed relative to another setup")));
        }
        Ok(SetSetupRelative { setup: name.to_string(), prev_link, next_link: None, align: None })
    }

    fn write(&self, world: &World, link: Option<String>) {
        if let Some(entity) = model::find_by_name(world, &self.setup) {
            let view = entity.entity_view(world);
            if let Some(mut def) = view.try_get::<&SetupDef>(|d| d.clone()) {
                def.relative_to = link;
                view.set(def);
            }
        }
    }
}

/// The link a live setup holds now.
fn held_link(world: &World, name: &str) -> Result<Option<String>, CommandError> {
    let entity = model::find_by_name(world, name).ok_or_else(|| CommandError::NotFound(name.to_string()))?;
    entity
        .entity_view(world)
        .try_get::<&SetupDef>(|d| d.relative_to.clone())
        .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a setup")))
}

impl Command for SetSetupRelative {
    fn apply(&self, world: &World) {
        self.write(world, self.next_link.clone());
        if let Some(a) = &self.align {
            a.apply(world);
        }
    }

    fn invert(&self, world: &World) {
        if let Some(a) = &self.align {
            a.invert(world);
        }
        self.write(world, self.prev_link.clone());
    }
}
