//! SetToolBody (#368): attach, replace or clear a tool's modelled body
//! as one undoable command - a milling body along the spindle axis, or
//! (#369) a lathe body in the turning tool frame, with (#396) its
//! optional insert zone. The mesh is read, placed and judged at
//! construction (AGENTS.md: malformed input never enters the history);
//! `apply`/`invert` only land the snapshot. Our own command model, the
//! snapshot-and-restore inverse every sibling uses.

use std::path::Path;

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::geometry::Triangle;
use crate::model;
use crate::schema::ToolBody;
use crate::tool_body;
use camcraft_tools::lathe_holder::LathePlacement;
use camcraft_tools::lathe_insert_zone::InsertZone;

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

/// A lathe-held body as the caller states it (#369): model-frame
/// facets, where they came from, and the turning-frame placement (the
/// model directions that are radial and along the spindle, the model
/// point that is the tip - `camcraft_tools::lathe_holder`).
pub struct LatheBodySpec {
    pub source: String,
    pub facets: Vec<Triangle>,
    pub place: LathePlacement,
    /// #396: the insert plate the mesh models, a box in the turning
    /// tool frame the holder judge leaves out; `None` judges it all.
    pub zone: Option<InsertZone>,
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

    /// Read a STEP / STL / OBJ through the model importer and place it.
    pub fn from_file(world: &World, name: &str, path: &Path, axis: [f64; 3], offset: f64) -> Result<Self, CommandError> {
        let mesh = crate::geometry::import::load_model(path)?;
        let source = path.file_name().map_or_else(|| path.display().to_string(), |f| f.to_string_lossy().into_owned());
        Self::new(world, name, Some(ToolBodySpec { source, facets: mesh.facets.to_vec(), axis, offset }))
    }

    /// Place and judge a lathe body (#369) against tool `name` now;
    /// `None` clears. Refused by name: no such tool, no lathe steel, a
    /// placement that is no frame, a mesh that is no body.
    pub fn lathe(world: &World, name: &str, spec: Option<LatheBodySpec>) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let tool = model::tool_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a tool")))?;
        let invalid = |e: String| CommandError::Invalid(format!("tool '{name}': {e}"));
        let next = match spec {
            None => None,
            Some(s) => {
                crate::tool_body_lathe::fits(&tool, s.zone.as_ref()).map_err(invalid)?;
                let placed = crate::tool_body_lathe::place(&s.facets, &s.place).map_err(invalid)?;
                let body = crate::tool_body_lathe::build(&s.source, s.place, s.zone, placed).map_err(invalid)?;
                tool_body::fits(&tool, &body).map_err(invalid)?;
                Some(body)
            }
        };
        Ok(SetToolBody { name: name.to_string(), prev: tool_body::body_of(world, &entity), next })
    }

    /// Read a STEP / STL / OBJ through the model importer and place it
    /// as a lathe body.
    pub fn lathe_from_file(world: &World, name: &str, path: &Path, place: LathePlacement) -> Result<Self, CommandError> {
        let mesh = crate::geometry::import::load_model(path)?;
        let source = path.file_name().map_or_else(|| path.display().to_string(), |f| f.to_string_lossy().into_owned());
        Self::lathe(world, name, Some(LatheBodySpec { source, facets: mesh.facets.to_vec(), place, zone: None }))
    }

    /// Set (`Some`) or clear (`None`) the insert zone (#396) of tool
    /// `name`'s lathe body, keeping its mesh and placement. Refused by
    /// name: no such tool, no lathe body on it, a zone its inserts
    /// could not fill or that holds the whole body.
    pub fn insert_zone(world: &World, name: &str, zone: Option<InsertZone>) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let tool = model::tool_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a tool")))?;
        let invalid = |e: String| CommandError::Invalid(format!("tool '{name}': {e}"));
        let prev = tool_body::body_of(world, &entity);
        let Some((source, place, facets)) = prev.as_ref().and_then(|b| match b.frame {
            tool_body::BodyFrame::Lathe { place, .. } => Some((b.source.clone(), place, b.facets.to_vec())),
            tool_body::BodyFrame::Spindle { .. } => None,
        }) else {
            return Err(invalid("it carries no lathe body - an insert zone is part of one".to_owned()));
        };
        crate::tool_body_lathe::fits(&tool, zone.as_ref()).map_err(invalid)?;
        let body = crate::tool_body_lathe::build(&source, place, zone, facets).map_err(invalid)?;
        Ok(SetToolBody { name: name.to_string(), prev, next: Some(body) })
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
