//! SetWcsOrigin: place the active setup's one work coordinate system
//! (#342) - its origin and orientation.
//!
//! A setup has exactly one WCS; which `G54`..`G59` word the post writes
//! for it is the setup's label (`SetSetupWorkOffset`), never chosen here.
//! The `Wcs` tag distinguishes the entity from a model that happens to
//! share the name. Provenance: published G-code standards (RS-274 / ISO
//! 6983 work-offset semantics) and our own design notes only.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::geometry::normalize_quat;
use crate::model;
use crate::schema::{Location, Wcs};

pub struct SetWcsOrigin {
    /// The entity this command places - the active setup's WCS at
    /// construction time (`G54` in a document without setups,
    /// `Setup2/WCS` when the active setup owns its own). The G-code word
    /// is never stored here: it is the setup's label at post time.
    entity_name: String,
    location: Location,
    /// Where the WCS sat before; `None` means the entity did not exist
    /// yet, so the inverse removes it.
    prev: Option<Location>,
}

impl SetWcsOrigin {
    /// Validates against the world now: a finite rigid transform and no
    /// name collision with a model (AGENTS.md: validate early, never fail
    /// deep).
    pub fn new(world: &World, location: Location) -> Result<Self, CommandError> {
        let mut location = location;
        if !location.rot.iter().all(|v| v.is_finite()) {
            return Err(CommandError::Invalid(
                "WCS rotation must be finite (check the angle inputs)".into(),
            ));
        }
        location.rot = normalize_quat(location.rot).ok_or_else(|| {
            CommandError::Invalid("WCS rotation is degenerate (zero norm)".into())
        })?;
        if !location.pos.is_finite() {
            return Err(CommandError::Invalid("WCS placement must be finite".into()));
        }
        let active = crate::setup::active(world);
        let entity_name = crate::setup::wcs_entity_name(world, active.as_deref());
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

    /// The entity this command places.
    #[must_use]
    pub fn entity_name(&self) -> &str {
        &self.entity_name
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
