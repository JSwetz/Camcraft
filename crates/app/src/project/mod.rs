//! Project save/load: `.hsmproj`, JSON, version 1.
//!
//! Serialization goes through plain DTO structs (deliberately not flecs
//! reflection): the file format is our public persistence contract and must
//! stay stable regardless of ECS storage decisions. Entity order is by name,
//! so a save -> load -> save cycle is byte-identical.

use std::path::Path;

use flecs_ecs::prelude::World;
use serde::{Deserialize, Serialize};

use crate::geometry::Point3;
use crate::model;

pub const PROJECT_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported project version {0} (this build writes {PROJECT_VERSION})")]
    UnsupportedVersion(u32),
    #[error("malformed project file: {0}")]
    Corrupt(#[from] serde_json::Error),
}

#[derive(Serialize, Deserialize)]
pub struct ProjectFile {
    pub version: u32,
    pub entities: Vec<EntityDto>,
}

#[derive(Serialize, Deserialize)]
pub struct EntityDto {
    pub name: String,
    pub translation: [f64; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facets: Option<Vec<[[f64; 3]; 3]>>, // a, b, c per triangle
}

pub fn collect(world: &World) -> ProjectFile {
    let entities = model::named_entities(world)
        .into_iter()
        .map(|(name, entity)| {
            let translation = model::location_of(world, &entity).unwrap_or(Point3::ZERO);
            EntityDto {
                name,
                translation: [translation.x, translation.y, translation.z],
                facets: model::facets_of(world, &entity).map(|tris| {
                    tris.into_iter()
                        .map(|t| [[t.a.x, t.a.y, t.a.z], [t.b.x, t.b.y, t.b.z], [t.c.x, t.c.y, t.c.z]])
                        .collect()
                }),
            }
        })
        .collect();
    ProjectFile {
        version: PROJECT_VERSION,
        entities,
    }
}

pub fn save(world: &World, path: &Path) -> Result<(), ProjectError> {
    let file = collect(world);
    let text = serde_json::to_string_pretty(&file)?;
    std::fs::write(path, text)?;
    Ok(())
}

/// Rebuild world content from a project file. Not a command: opening a
/// project replaces the document and clears the history (M1.2 wires that).
pub fn load(world: &World, path: &Path) -> Result<(), ProjectError> {
    let text = std::fs::read_to_string(path)?;
    open(world, &text)
}

pub fn open(world: &World, text: &str) -> Result<(), ProjectError> {
    let file: ProjectFile = serde_json::from_str(text)?;
    check_and_apply(world, file)
}

fn check_and_apply(world: &World, file: ProjectFile) -> Result<(), ProjectError> {
    if file.version != PROJECT_VERSION {
        return Err(ProjectError::UnsupportedVersion(file.version));
    }
    // Migration hook: future versions step down here, one migration per
    // version pair, before any entities touch the world.
    for dto in file.entities {
        let entity = model::spawn(world, &dto.name);
        model::set_location(
            world,
            &entity,
            Point3::new(dto.translation[0], dto.translation[1], dto.translation[2]),
        );
        if let Some(tris) = dto.facets {
            use crate::geometry::Triangle;
            let facets = tris
                .into_iter()
                .map(|v| {
                    Triangle::new(
                        Point3::new(v[0][0], v[0][1], v[0][2]),
                        Point3::new(v[1][0], v[1][1], v[1][2]),
                        Point3::new(v[2][0], v[2][1], v[2][2]),
                    )
                })
                .collect();
            model::set_facets(world, &entity, facets);
        }
    }
    Ok(())
}
