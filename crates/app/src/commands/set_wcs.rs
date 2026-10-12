//! SetWcsOrigin: place a work coordinate system in one of the G54..G59
//! machine-offset slots.
//!
//! Slot names are the public RS-274/ISO 6983 work-offset codes; the entity
//! name IS the slot, and the `Wcs` tag distinguishes it from a model that
//! happens to share the name. Provenance: published G-code standards only.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::geometry::normalize_quat;
use crate::model;
use crate::schema::{Location, WCS_SLOTS, Wcs};

pub struct SetWcsOrigin {
    slot: String,
    location: Location,
    /// Where the slot pointed before; `None` means the entity did not exist
    /// yet, so the inverse removes it.
    prev: Option<Location>,
}

impl SetWcsOrigin {
    /// Validates against the world now: a known slot, a finite rigid
    /// transform, and no name collision with a model (AGENTS.md: validate
    /// early, never fail deep).
    pub fn new(world: &World, slot: &str, location: Location) -> Result<Self, CommandError> {
        if !WCS_SLOTS.contains(&slot) {
            return Err(CommandError::Invalid(format!(
                "'{slot}' is not a work offset slot (expected one of {})",
                WCS_SLOTS.join(", ")
            )));
        }
        let mut location = location;
        location.rot = normalize_quat(location.rot).ok_or_else(|| {
            CommandError::Invalid("work offset rotation is not a quaternion".into())
        })?;
        if !location.pos.is_finite() {
            return Err(CommandError::Invalid("work offset placement must be finite".into()));
        }
        let prev = match model::find_by_name(world, slot) {
            Some(entity) => {
                if !model::is_wcs(world, &entity) {
                    return Err(CommandError::NameConflict(slot.to_string()));
                }
                model::transform_of(world, &entity)
            }
            None => None,
        };
        Ok(SetWcsOrigin {
            slot: slot.to_string(),
            location,
            prev,
        })
    }
}

impl Command for SetWcsOrigin {
    fn apply(&self, world: &World) {
        // Pure world write from stored fields: idempotent under redo.
        let entity = match model::find_by_name(world, &self.slot) {
            Some(entity) => entity,
            None => model::spawn(world, &self.slot),
        };
        entity.entity_view(world).add(Wcs); // tags add, not set (zero-sized)
        model::set_transform(world, &entity, self.location);
    }

    fn invert(&self, world: &World) {
        let Some(entity) = model::find_by_name(world, &self.slot) else {
            return; // already undone (idempotent undo path)
        };
        match self.prev {
            Some(location) => model::set_transform(world, &entity, location),
            None => model::remove(world, &entity), // we created it
        }
    }
}
