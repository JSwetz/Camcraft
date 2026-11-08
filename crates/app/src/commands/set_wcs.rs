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
    /// The entity this command places - the active setup's name for this
    /// slot at construction time (M3.2 A): `G54` in a legacy document,
    /// `Setup2/G54` when the active setup owns its furniture. The G-code
    /// word is never stored: it comes from the slot index in the
    /// operation's params at post time, and only the LOOKUP follows the
    /// setup (the one G-word law).
    entity_name: String,
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
        // #327: every refusal names the slot, so a script or the pane
        // reports which offset was refused.
        if !location.rot.iter().all(|v| v.is_finite()) {
            return Err(CommandError::Invalid(format!(
                "work offset {slot}: rotation must be finite (check the angle inputs)"
            )));
        }
        location.rot = normalize_quat(location.rot).ok_or_else(|| {
            CommandError::Invalid(format!(
                "work offset {slot}: rotation is degenerate (zero norm)"
            ))
        })?;
        if !location.pos.is_finite() {
            return Err(CommandError::Invalid(format!(
                "work offset {slot}: origin must be finite"
            )));
        }
        let active = crate::setup::active(world);
        let slot_ix = WCS_SLOTS
            .iter()
            .position(|s| *s == slot)
            .ok_or_else(|| CommandError::Invalid(format!("'{slot}' is not a work offset slot")))?;
        let entity_name = crate::setup::wcs_entity_name(world, active.as_deref(), slot_ix)
            .unwrap_or_else(|| slot.to_string());
        let prev = match model::find_by_name(world, &entity_name) {
            Some(entity) => {
                if !model::is_wcs(world, &entity) {
                    return Err(CommandError::NameConflict(entity_name));
                }
                model::transform_of(world, &entity)
            }
            None => None,
        };
        Ok(SetWcsOrigin {
            entity_name,
            location,
            prev,
        })
    }
}

impl Command for SetWcsOrigin {
    fn apply(&self, world: &World) {
        // Pure world write from stored fields: idempotent under redo.
        let entity = match model::find_by_name(world, &self.entity_name) {
            Some(entity) => entity,
            None => model::spawn(world, &self.entity_name),
        };
        entity.entity_view(world).add(Wcs); // tags add, not set (zero-sized)
        model::set_transform(world, &entity, self.location);
    }

    fn invert(&self, world: &World) {
        let Some(entity) = model::find_by_name(world, &self.entity_name) else {
            return; // already undone (idempotent undo path)
        };
        match self.prev {
            Some(location) => model::set_transform(world, &entity, location),
            None => model::remove(world, &entity), // we created it
        }
    }
}
