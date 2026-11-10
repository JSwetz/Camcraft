//! Fixture commands: tag, place, and unmake the workholding hardware a
//! part rides on (M3.2 stage B).
//!
//! Provenance: our own design; a fixture being an imported solid the
//! shop bolts down is plain machine-shop practice. Like every sibling
//! here, construction reads the world, validates, and snapshots;
//! `apply` and `invert` are pure writes, so undo/redo replay bytes and
//! never re-derive.
//!
//! ## The laws these commands keep
//!
//! - **A fixture is a model wearing a marker.** So there are two ways to
//!   make one: import a solid and [`TagFixture`] it (the UI path - STEP
//!   parsing runs on the import worker, never in a command
//!   constructor), or [`AddFixtureModel`], which parses synchronously at
//!   construction for headless callers and tests. Both land on the same
//!   components every model uses: `Facets`, `Location`, and the
//!   `FixtureDef` binding. No command here builds geometry.
//! - **The marker is the only fixture state.** The binding is a
//!   declared setup (`None` = first/implicit, the stage-A law); the pose
//!   is the entity's `Location` like any part's. [`UntagFixture`]
//!   demotes to a plain model - the geometry stays, the collision law
//!   stops seeing it; [`MoveFixture`] is the same move any model
//!   receives, gated to fixture-bearing entities because the page that
//!   offers it shows only those.
//! - **`Machine/Table` is prose, not an entity.** Every command here
//!   refuses that name with the honest reason: the table exists while a
//!   `.mch` says so, and its geometry is whatever the user machined
//!   against, not a thing to move or delete.
//! - **Deleting is the whole-entity inverse of having.** Everything the
//!   fixture owned - marker, placement, facets - is snapshotted at
//!   construction and restored verbatim by the inverse, so redo-after-
//!   undo reproduces bytes, not re-derivations.

use flecs_ecs::prelude::{Entity, World};

use crate::commands::{Command, CommandError};
use crate::fixture::{self, MACHINE_TABLE};
use crate::geometry::{normalize_quat, Triangle};
use crate::model;
use crate::schema::{FixtureDef, Location};
use crate::setup;

/// The placement validation every pose write shares, in the
/// `SetStock` voice: a unit quaternion (normalized at the chokepoint)
/// and a finite position, or the command never exists. Shared by every
/// pose-writing command (`MoveFixture`, `MoveModel`) so the placement
/// validation is one law, not two that drift.
pub(crate) fn check_location(location: Location, what: &str) -> Result<Location, CommandError> {
    let mut location = location;
    location.rot = normalize_quat(location.rot).ok_or_else(|| {
        CommandError::Invalid(format!("{what} rotation is not a quaternion"))
    })?;
    if !location.pos.is_finite() {
        return Err(CommandError::Invalid(format!(
            "{what} placement must be finite"
        )));
    }
    Ok(location)
}

/// The gate every fixture-named command runs first: the table row is
/// virtual, everything else must name a live entity.
fn require_writable(world: &World, name: &str) -> Result<Entity, CommandError> {
    if name == MACHINE_TABLE {
        return Err(CommandError::Invalid(
            "the machine table is derived from the machine file - it is not a fixture you can change".into(),
        ));
    }
    model::find_by_name(world, name).ok_or_else(|| CommandError::NotFound(name.to_string()))
}

/// Tag an existing model as a fixture of setup context `setup` (or
/// re-bind an existing fixture: the previous binding is snapshotted, so
/// undo restores it, not "untagged"). The model's geometry and pose are
/// already someone's business - `ImportModel`/`UpdateModel` own those
/// writes; this command touches only the marker.
pub struct TagFixture {
    name: String,
    prev: Option<FixtureDef>,
    next: FixtureDef,
}

impl TagFixture {
    pub fn new(world: &World, name: &str, setup_name: Option<&str>) -> Result<Self, CommandError> {
        let entity = require_writable(world, name)?;
        if !model::is_model(world, &entity) {
            return Err(CommandError::Invalid(format!(
                "'{name}' is setup furniture or CAM data, not a model to mark"
            )));
        }
        if let Some(s) = setup_name
            && !setup::exists(world, s)
        {
            return Err(CommandError::NotFound(s.to_string()));
        }
        let prev = fixture::def_of(world, &entity);
        Ok(TagFixture {
            name: name.to_string(),
            // A re-bind keeps whether the steel turns: that is
            // [`SetFixtureFixed`]'s write, not the binding's.
            next: FixtureDef {
                setup: setup_name.map(str::to_string),
                fixed: prev.as_ref().is_some_and(|d| d.fixed),
            },
            prev,
        })
    }
}

/// Declare whether a fixture stands still while a rotary turns the part
/// (a table, a tailstock: judged in the machine frame) or turns with it
/// (a chuck: judged in the path's own frame - the default) - M13.8 (2).
pub struct SetFixtureFixed {
    name: String,
    prev: FixtureDef,
    next: FixtureDef,
}

impl SetFixtureFixed {
    pub fn new(world: &World, name: &str, fixed: bool) -> Result<Self, CommandError> {
        let entity = require_writable(world, name)?;
        let prev = fixture::def_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a fixture")))?;
        let next = FixtureDef { fixed, ..prev.clone() };
        Ok(SetFixtureFixed { name: name.to_string(), prev, next })
    }
}

impl Command for SetFixtureFixed {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.next.clone());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.prev.clone());
        }
    }
}

impl Command for TagFixture {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.next.clone());
        }
    }

    fn invert(&self, world: &World) {
        let Some(entity) = model::find_by_name(world, &self.name) else {
            return;
        };
        match &self.prev {
            Some(def) => {
                entity.entity_view(world).set(def.clone());
            }
            None => {
                let _ = entity
                    .entity_view(world)
                    .remove(world.component::<FixtureDef>());
            }
        }
    }
}

/// Untag: the entity stays, its geometry stays, the collision law stops
/// seeing it. Snapshots the binding so undo re-tags exactly.
pub struct UntagFixture {
    name: String,
    prev: FixtureDef,
}

impl UntagFixture {
    pub fn new(world: &World, name: &str) -> Result<Self, CommandError> {
        let entity = require_writable(world, name)?;
        let prev = fixture::def_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a fixture")))?;
        Ok(UntagFixture {
            name: name.to_string(),
            prev,
        })
    }
}

impl Command for UntagFixture {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            let _ = entity
                .entity_view(world)
                .remove(world.component::<FixtureDef>());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.prev.clone());
        }
    }
}

/// Move a fixture: the same rigid placement any model receives, named
/// for the page that offers it. A fixture that is not a tagged model is
/// refused - the alternative is silently moving a part someone else
/// calls workholding.
pub struct MoveFixture {
    name: String,
    prev: Location,
    next: Location,
    warning: Option<String>,
}

impl MoveFixture {
    /// A Fixtures-page move. Since #582 it carries a by-name warning when
    /// the moved fixture would sit in a part or the setup's stock - the
    /// same sampled check a locate runs, never a refusal.
    pub fn new(world: &World, name: &str, location: Location) -> Result<Self, CommandError> {
        let mut command = Self::locating(world, name, location)?;
        command.warning = crate::fixture_clash::after_fixture_move(world, name, command.next);
        Ok(command)
    }

    /// The move a `LocateFixture` makes: no check here, the locate runs
    /// its own (mate-worded) one.
    pub fn locating(world: &World, name: &str, location: Location) -> Result<Self, CommandError> {
        let entity = require_writable(world, name)?;
        if !model::is_fixture(world, &entity) {
            return Err(CommandError::Invalid(format!("'{name}' is not a fixture")));
        }
        let next = check_location(location, &format!("'{name}'"))?;
        let prev = model::transform_of(world, &entity).unwrap_or_default();
        Ok(MoveFixture {
            name: name.to_string(),
            prev,
            next,
            warning: None,
        })
    }

    /// The interpenetration warning (#582), if any.
    #[must_use]
    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }
}

impl Command for MoveFixture {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::set_transform(world, &entity, self.next);
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::set_transform(world, &entity, self.prev);
        }
    }
}

/// Delete a fixture solid outright - marker, pose, and geometry all
/// come back on undo. Everything the entity owned goes into the
/// snapshot; the inverse puts it back byte for byte.
pub struct DeleteFixture {
    name: String,
    def: FixtureDef,
    location: Location,
    facets: std::sync::Arc<[Triangle]>,
}

impl DeleteFixture {
    pub fn new(world: &World, name: &str) -> Result<Self, CommandError> {
        let entity = require_writable(world, name)?;
        let def = fixture::def_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a fixture")))?;
        Ok(DeleteFixture {
            name: name.to_string(),
            def,
            location: model::transform_of(world, &entity).unwrap_or_default(),
            facets: model::facets_of(world, &entity).unwrap_or_default(),
        })
    }
}

impl Command for DeleteFixture {
    fn apply(&self, world: &World) {
        if let Some(e) = model::find_by_name(world, &self.name) {
            model::remove(world, &e);
        }
    }

    fn invert(&self, world: &World) {
        if model::find_by_name(world, &self.name).is_some() {
            return; // already restored (idempotent redo path)
        }
        let entity = model::spawn(world, &self.name);
        model::set_transform(world, &entity, self.location);
        model::set_facets(world, &entity, self.facets.clone());
        entity.entity_view(world).set(self.def.clone());
    }
}

/// Headless convenience: parse a mesh file at construction (the law
/// that no UI command constructor reads disk is why the UI imports a
/// model on its worker and then [`TagFixture`]s it - this exists for
/// tests and batch scripts, where construction IS off the frame). The
/// new entity starts unplaced at the origin, revision 1, bound to no
/// setup (the implicit first).
pub struct AddFixtureModel {
    name: String,
    facets: std::sync::Arc<[Triangle]>,
}

impl AddFixtureModel {
    pub fn new(
        world: &World,
        name: &str,
        path: &std::path::Path,
    ) -> Result<Self, CommandError> {
        if name.trim().is_empty() {
            return Err(CommandError::Invalid(
                "fixture name must not be empty".into(),
            ));
        }
        if name == MACHINE_TABLE {
            return Err(CommandError::Invalid(
                "'Machine/Table' names the derived machine table".into(),
            ));
        }
        if model::find_by_name(world, name).is_some() {
            return Err(CommandError::NameConflict(name.to_string()));
        }
        let mesh = crate::geometry::import::load_model(path)?;
        Ok(AddFixtureModel {
            name: name.to_string(),
            facets: mesh.facets,
        })
    }
}

impl Command for AddFixtureModel {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            // Redo replay: make it idempotent by re-finding, the
            // ImportModel convention.
            if fixture::def_of(world, &entity).is_none() {
                entity.entity_view(world).set(FixtureDef::default());
            }
            return;
        }
        let entity = model::spawn(world, &self.name);
        model::set_facets(world, &entity, self.facets.clone());
        model::set_rev(world, &entity, 1);
        entity.entity_view(world).set(FixtureDef::default());
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::remove(world, &entity);
        }
    }
}
