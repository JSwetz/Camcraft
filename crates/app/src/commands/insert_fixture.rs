//! Insert a fixture library entry into a setup (#339 slice 6) in ONE
//! undo unit: a fresh model carrying the entry's facets, its datum
//! placed on the picked frame, wearing the `FixtureDef` of the setup.
//! The result is an ordinary tagged model - the project file saves and
//! reloads it like any imported fixture, with no new key.
//!
//! Placement: the entry's datum `D` is a frame in the mesh's own frame
//! and the pick `P` a frame in the document's world, so the model's
//! `Location` is `P ∘ D⁻¹` - the datum lands on the pick, axes and all.
//!
//! Provenance: our own composition of the import and tag commands;
//! rigid-frame composition from published quaternion algebra. No vendor
//! fixture material consulted.

use flecs_ecs::prelude::World;

use crate::commands::fixture::check_location;
use crate::commands::{Command, CommandError};
use crate::fixture::MACHINE_TABLE;
use crate::fixture_library::{FixtureEntry, check_entry};
use crate::geometry::{Point3, Triangle, quat_conjugate, quat_mul, quat_rotate_point};
use crate::model;
use crate::schema::{FixtureDef, Location};
use crate::setup;

/// `p ∘ d⁻¹` for unit rotations: the model pose that puts frame `d` of
/// the mesh on frame `p` of the world.
#[must_use]
pub fn place_datum(p: Location, d: Location) -> Location {
    let inv_rot = quat_conjugate(d.rot);
    let back = quat_rotate_point(inv_rot, d.pos);
    let inv_pos = Point3::new(-back.x, -back.y, -back.z);
    let r = quat_rotate_point(p.rot, inv_pos);
    Location { pos: Point3::new(r.x + p.pos.x, r.y + p.pos.y, r.z + p.pos.z), rot: quat_mul(p.rot, inv_rot) }
}

/// The first free model name for `base`: `base`, then `base (2)`, ...
/// (inserting the same vise twice is ordinary shop work).
#[must_use]
pub fn free_name(world: &World, base: &str) -> String {
    let taken = |n: &str| n == MACHINE_TABLE || model::find_by_name(world, n).is_some();
    if !taken(base) {
        return base.to_owned();
    }
    (2u32..)
        .map(|k| format!("{base} ({k})"))
        .find(|n| !taken(n))
        .unwrap_or_else(|| format!("{base} (copy)"))
}

pub struct InsertFixture {
    name: String,
    facets: std::sync::Arc<[Triangle]>,
    location: Location,
    def: FixtureDef,
}

impl InsertFixture {
    /// Insert `entry` as a fixture of setup context `setup_name` (`None`
    /// = the implicit first setup) with its datum at `pick`. Refused when
    /// the setup is gone, the pick is not a finite rigid frame, or the
    /// entry would not read back from a library.
    pub fn new(
        world: &World,
        entry: &FixtureEntry,
        setup_name: Option<&str>,
        pick: Location,
    ) -> Result<Self, CommandError> {
        check_entry(entry).map_err(CommandError::Invalid)?;
        if let Some(s) = setup_name
            && !setup::exists(world, s)
        {
            return Err(CommandError::NotFound(s.to_string()));
        }
        let pick = check_location(pick, "fixture datum")?;
        let datum = check_location(entry.datum, "library datum")?;
        let location = check_location(place_datum(pick, datum), "fixture")?;
        Ok(InsertFixture {
            name: free_name(world, &entry.name),
            facets: entry.facets.clone().into(),
            location,
            def: FixtureDef { setup: setup_name.map(str::to_owned), fixed: false },
        })
    }

    /// The model name the insert lands as.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The model pose the insert writes.
    #[must_use]
    pub fn location(&self) -> Location {
        self.location
    }
}

impl Command for InsertFixture {
    fn apply(&self, world: &World) {
        if model::find_by_name(world, &self.name).is_some() {
            // Redo replay over a re-found entity: the ImportModel convention.
            return;
        }
        let entity = model::spawn(world, &self.name);
        model::set_facets(world, &entity, self.facets.clone());
        model::set_rev(world, &entity, 1);
        model::set_transform(world, &entity, self.location);
        entity.entity_view(world).set(self.def.clone());
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::remove(world, &entity);
        }
    }
}
