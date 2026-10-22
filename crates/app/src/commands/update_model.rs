//! UpdateModel: replace a model entity's geometry with a new revision,
//! snapshotting the previous facets and revision counter at construction
//! (the DeleteEntity pattern) so undo restores them exactly.
//!
//! Provenance: our own design - the command pattern here mirrors the app's
//! existing invertible commands; the feature diff it can report comes from
//! `geometry::revision`. No HSMWorks internals involved.
//!
//! This is the undoable half of M3.1's "model update detection": the
//! re-import path (UI `finish_import`) routes a same-named model here, and
//! staleness of dependent operations falls out of the stamp - which digests
//! the geometry signature, not a facet count - with no extra marking pass.
//!
//! The propagation law (M3.1 B): a `FromModel` stock is a copy of the model
//! it names, so the copy is re-derived inside this same command. One user
//! action, one undo unit - the viewer, the simulator and every stock-based
//! operation read a material solid, never a stale shard.

use flecs_ecs::prelude::{Entity, World};

use crate::commands::{Command, CommandError};
use crate::geometry::{Mesh, revision};
use crate::model;
use crate::schema::{Facets, STOCK_NAME};
use crate::stock::StockShape;

/// The stock entity when - and only when - it is a `FromModel` copy of
/// `name` (M3.1 B's gate: an update never writes into a box stock or into
/// a copy of some other model).
fn stock_of(world: &World, name: &str) -> Option<Entity> {
    let entity = model::find_by_name(world, STOCK_NAME)?;
    matches!(model::stock_def_of(world, &entity),
        Some(StockShape::FromModel { source }) if source == name)
        .then_some(entity)
}

pub struct UpdateModel {
    pub name: String,
    pub mesh: Mesh,
    /// The component as it stood: `None` means the entity had no geometry
    /// at all (a spawned-empty model), and undo must not leave an empty
    /// `Facets` behind where there was no component.
    prev_facets: Option<Vec<crate::geometry::Triangle>>,
    prev_rev: u64,
    /// The derived stock's facets when a `FromModel` stock copied this
    /// model at construction (`Some`) - the propagation law's side of the
    /// snapshot, so undo trades model and material back together.
    prev_stock: Option<Vec<crate::geometry::Triangle>>,
}

impl UpdateModel {
    /// Snapshots the world now (construction time), so undo/redo are exact
    /// whatever happens between now and then. The target must exist and be
    /// a model: a missing name is `NotFound`, setup furniture is
    /// `Invalid` - an import must never overwrite the stock solid or a WCS
    /// slot just because someone named the file `G54.stl`.
    pub fn new(world: &World, name: &str, mesh: Mesh) -> Result<Self, CommandError> {
        if mesh.is_empty() {
            return Err(CommandError::Invalid(format!(
                "'{name}' would update the model to empty geometry"
            )));
        }
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        if !model::is_model(world, &entity) {
            return Err(CommandError::Invalid(format!(
                "'{name}' is setup furniture, not a model"
            )));
        }
        let prev_stock = stock_of(world, name).and_then(|s| model::facets_of(world, &s));
        Ok(UpdateModel {
            name: name.to_string(),
            mesh,
            prev_facets: model::facets_of(world, &entity),
            prev_rev: model::rev_of(world, &entity),
            prev_stock,
        })
    }

    /// What moved between the two revisions this command trades. Computed
    /// on demand (the message wants it once, at import); cached Features
    /// make both sides cheap after the first call anywhere.
    #[must_use]
    pub fn changes(&self) -> revision::FeatureChanges {
        revision::changes(
            &model::features_of_local(self.prev_facets.as_deref().unwrap_or(&[])),
            &model::features_of_local(&self.mesh.facets),
        )
    }
}

impl Command for UpdateModel {
    fn apply(&self, world: &World) {
        // The entity to update is the one that carries this name (the
        // name-keyed law). Re-spawn if a DeleteEntity interleaving deleted
        // it out from under a redo, mirroring ImportModel's idempotence.
        let entity = model::find_by_name(world, &self.name)
            .unwrap_or_else(|| model::spawn(world, &self.name));
        model::set_facets(world, &entity, self.mesh.facets.clone());
        model::set_rev(world, &entity, self.prev_rev + 1);
        // Propagation (M3.1 B): the derived stock's copy is local-frame
        // facets - the very mesh above (SetStock's copy law) - and the
        // stock's own placement is never touched: re-importing a model
        // updates the material it was cut from, it does not move the
        // setup. The re-check at apply time is for redo after a
        // DeleteEntity interleaving, exactly as the model re-spawn above.
        if self.prev_stock.is_some() && let Some(stock) = stock_of(world, &self.name) {
            model::set_facets(world, &stock, self.mesh.facets.clone());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            match &self.prev_facets {
                Some(tris) => model::set_facets(world, &entity, tris.clone()),
                None => {
                    // No facet component to restore: the id, not the Rust
                    // type, is what `remove` takes (flecs_ecs 0.2.2 law).
                    entity.entity_view(world).remove(world.component::<Facets>());
                }
            }
            model::set_rev(world, &entity, self.prev_rev);
        }
        if let Some(tris) = &self.prev_stock && let Some(stock) = stock_of(world, &self.name) {
            model::set_facets(world, &stock, tris.clone());
        }
    }
}
