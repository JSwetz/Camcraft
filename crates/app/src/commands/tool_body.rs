//! SetToolBody (#368): attach, replace or clear a tool's modelled body
//! as one undoable command. The mesh is read, placed and judged at
//! construction (AGENTS.md: malformed input never enters the history);
//! `apply`/`invert` only land the snapshot. Our own command model, the
//! snapshot-and-restore inverse every sibling uses.

use std::path::Path;

use flecs_ecs::prelude::World;

use camcraft_tools::spun::{SpunPoint, SpunProfile};

use crate::commands::{Command, CommandError};
use crate::geometry::Triangle;
use crate::model;
use crate::schema::ToolBody;
use crate::tool_body;

/// A body as the caller states it: model-frame facets, where they came
/// from, and the numeric placement (`tool_body::place`).
pub struct ToolBodySpec {
    pub source: String,
    pub facets: Vec<Triangle>,
    /// The model direction that is the tool axis, pointing from the tip
    /// toward the spindle.
    pub axis: [f64; 3],
    /// The model origin's height above the tool tip, along that axis.
    pub offset: f64,
}

/// Set (`Some`) or clear (`None`) a tool's body.
pub struct SetToolBody {
    name: String,
    prev: Option<ToolBody>,
    next: Option<ToolBody>,
}

impl SetToolBody {
    /// Place and judge `spec` against tool `name` now; `None` clears.
    pub fn new(world: &World, name: &str, spec: Option<ToolBodySpec>) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let tool = model::tool_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a tool")))?;
        let next = match spec {
            None => None,
            Some(s) => {
                let placed = tool_body::place(&s.facets, s.axis, s.offset).map_err(CommandError::Invalid)?;
                let body = tool_body::build(&s.source, s.axis, s.offset, placed)
                    .map_err(|e| CommandError::Invalid(format!("tool '{name}': {e}")))?;
                tool_body::fits(&tool, &body).map_err(|e| CommandError::Invalid(format!("tool '{name}': {e}")))?;
                Some(body)
            }
        };
        Ok(SetToolBody { name: name.to_string(), prev: tool_body::body_of(world, &entity), next })
    }

    /// Set (`Some`) a drawn spun holder profile (#93) as the tool's
    /// body, or clear it (`None`, back to the holder cylinder). The
    /// outline is validated here and refused by name ("holder profile:
    /// point 3 ..."), then judged against the tool like any body.
    pub fn spun(world: &World, name: &str, points: Option<Vec<SpunPoint>>) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let tool = model::tool_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a tool")))?;
        let next = match points {
            None => None,
            Some(p) => {
                let spun = SpunProfile::new(p)
                    .map_err(|e| CommandError::Invalid(format!("tool '{name}': holder profile: {e}")))?;
                let body = tool_body::build_spun(spun);
                tool_body::fits(&tool, &body).map_err(|e| CommandError::Invalid(format!("tool '{name}': {e}")))?;
                Some(body)
            }
        };
        Ok(SetToolBody { name: name.to_string(), prev: tool_body::body_of(world, &entity), next })
    }

    /// Read a STEP / STL / OBJ through the model importer and place it.
    pub fn from_file(world: &World, name: &str, path: &Path, axis: [f64; 3], offset: f64) -> Result<Self, CommandError> {
        let mesh = crate::geometry::import::load_model(path)?;
        let source = path.file_name().map_or_else(|| path.display().to_string(), |f| f.to_string_lossy().into_owned());
        Self::new(world, name, Some(ToolBodySpec { source, facets: mesh.facets.to_vec(), axis, offset }))
    }
}

impl Command for SetToolBody {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            tool_body::set_body(world, &entity, self.next.as_ref());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            tool_body::set_body(world, &entity, self.prev.as_ref());
        }
    }
}
