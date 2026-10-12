//! ImportModel: parse a mesh file at construction time, spawn a named model
//! entity on apply, remove it on invert.

use std::path::Path;

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::geometry::{Mesh, import};
use crate::model;

pub struct ImportModel {
    pub name: String,
    pub mesh: Mesh,
}

impl ImportModel {
    /// Parse + validate now; a corrupt file is a construction error and
    /// never reaches the history.
    pub fn new(path: &Path) -> Result<Self, CommandError> {
        // IGES needs a translation kernel we do not have; refuse by
        // extension, not by a failed read, so the user gets the honest
        // diagnostic. STEP imports via the truck tessellator (mesh only).
        match path.extension().and_then(|e| e.to_str()) {
            Some(ext) if matches!(ext.to_ascii_lowercase().as_str(), "iges" | "igs") => {
                return Err(CommandError::UnsupportedType(ext.to_ascii_uppercase()));
            }
            _ => {}
        }
        let mesh = import::load_model(path)?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("model")
            .to_string();
        Ok(Self::from_mesh(&name, mesh))
    }

    pub fn from_mesh(name: &str, mesh: Mesh) -> Self {
        ImportModel {
            name: name.to_string(),
            mesh,
        }
    }
}

impl Command for ImportModel {
    fn apply(&self, world: &World) {
        if model::find_by_name(world, &self.name).is_some() {
            // A redo path: the entity was deleted out from under us or this
            // exact apply is replaying; make it idempotent by re-finding.
            // Fresh-conflict protection lives at construction sites, not
            // here, because construction cannot see the world.
            return;
        }
        let entity = model::spawn(world, &self.name);
        model::set_facets(world, &entity, self.mesh.facets.clone());
    }

    fn invert(&self, world: &World) {
        // Invert is name-keyed, not handle-keyed: after any interleaving of
        // undo/redo the entity to remove is the one that carries this name.
        // Finding nothing (a logic bug elsewhere) must no-op, not crash.
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::remove(world, &entity);
        }
    }
}
