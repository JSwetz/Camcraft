//! Project save/load: `.hsmproj`, JSON, version 2.
//!
//! Serialization goes through plain DTO structs (deliberately not flecs
//! reflection): the file format is our public persistence contract and must
//! stay stable regardless of ECS storage decisions. Entity order is by name,
//! so a save -> load -> save cycle is byte-identical.
//!
//! v2 added the rotation quaternion to every entity (rigid transforms, M1.2)
//! and the `kind`/`stock` fields for setup furniture; both are defaulted, so
//! a v2 file written before stock existed still parses. Parametric stock is
//! persisted as PARAMETERS only - `stock::stock_mesh` regenerates identical
//! facets in any build (see `stock.rs`), keeping files small and round-trips
//! exact. v1 files migrate on load: translation-only placements mean identity.

use std::path::Path;

use flecs_ecs::prelude::World;
use serde::{Deserialize, Serialize};

use crate::geometry::{IDENTITY_Q, Point3, Triangle};
use crate::model;
use crate::schema::{Location, StockDef, Wcs, STOCK_NAME, WCS_SLOTS};
use crate::stock::{self, StockShape};

pub const PROJECT_VERSION: u32 = 2;

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported project version {0} (this build writes {PROJECT_VERSION})")]
    UnsupportedVersion(u32),
    #[error("malformed project file: {0}")]
    Corrupt(String),
}

#[derive(Serialize, Deserialize)]
pub struct ProjectFile {
    pub version: u32,
    pub entities: Vec<EntityDto>,
}

/// What an entity is, in file terms. Absent means `Model` (the only kind
/// v1 and early v2 files could hold).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EntityKind {
    Model,
    Stock,
    Wcs,
}

/// The file mirror of `StockShape`, kept separate so the domain enum can
/// evolve without editing the persistence contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StockDto {
    Box { dx: f64, dy: f64, dz: f64 },
    Cylinder { diameter: f64, height: f64 },
    FromModel { source: String },
}

impl StockDto {
    fn from_shape(shape: &StockShape) -> Self {
        match shape {
            StockShape::Box { dx, dy, dz } => StockDto::Box {
                dx: *dx,
                dy: *dy,
                dz: *dz,
            },
            StockShape::Cylinder { diameter, height } => StockDto::Cylinder {
                diameter: *diameter,
                height: *height,
            },
            StockShape::FromModel { source } => StockDto::FromModel {
                source: source.clone(),
            },
        }
    }

    fn to_shape(&self) -> StockShape {
        match self {
            StockDto::Box { dx, dy, dz } => StockShape::Box {
                dx: *dx,
                dy: *dy,
                dz: *dz,
            },
            StockDto::Cylinder { diameter, height } => StockShape::Cylinder {
                diameter: *diameter,
                height: *height,
            },
            StockDto::FromModel { source } => StockShape::FromModel {
                source: source.clone(),
            },
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct EntityDto {
    pub name: String,
    pub translation: [f64; 3],
    /// Unit quaternion (x, y, z, w). Defaulted so v1 files parse; the v1 -> v2
    /// migration below makes that choice explicit rather than implicit.
    #[serde(default = "identity_rotation")]
    pub rotation: [f64; 4],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facets: Option<Vec<[[f64; 3]; 3]>>, // a, b, c per triangle
    /// Absent means Model; only non-model entities carry the tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<EntityKind>,
    /// Stock parameters; present exactly when `kind == Some(Stock)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock: Option<StockDto>,
}

fn identity_rotation() -> [f64; 4] {
    IDENTITY_Q
}

fn corrupt(msg: impl Into<String>) -> ProjectError {
    ProjectError::Corrupt(msg.into())
}

fn facets_to_dto(tris: Vec<Triangle>) -> Vec<[[f64; 3]; 3]> {
    tris.into_iter()
        .map(|t| {
            [
                [t.a.x, t.a.y, t.a.z],
                [t.b.x, t.b.y, t.b.z],
                [t.c.x, t.c.y, t.c.z],
            ]
        })
        .collect()
}

fn dto_to_facets(v: Vec<[[f64; 3]; 3]>) -> Vec<Triangle> {
    v.into_iter()
        .map(|t| {
            Triangle::new(
                Point3::new(t[0][0], t[0][1], t[0][2]),
                Point3::new(t[1][0], t[1][1], t[1][2]),
                Point3::new(t[2][0], t[2][1], t[2][2]),
            )
        })
        .collect()
}

pub fn collect(world: &World) -> ProjectFile {
    let entities = model::named_entities(world)
        .into_iter()
        .map(|(name, entity)| {
            let loc = model::transform_of(world, &entity).unwrap_or_default();
            let kind = if model::is_stock(world, &entity) {
                Some(EntityKind::Stock)
            } else if model::is_wcs(world, &entity) {
                Some(EntityKind::Wcs)
            } else {
                None
            };
            let stock = model::stock_def_of(world, &entity).map(|s| StockDto::from_shape(&s));
            // Parametric stock regenerates its facets on load, and offsets
            // never have geometry, so neither writes a facet array.
            let facets = match (&kind, &stock) {
                (Some(EntityKind::Wcs), _) => None,
                (Some(EntityKind::Stock), Some(StockDto::Box { .. }))
                | (Some(EntityKind::Stock), Some(StockDto::Cylinder { .. })) => None,
                _ => model::facets_of(world, &entity).map(facets_to_dto),
            };
            EntityDto {
                name,
                translation: [loc.pos.x, loc.pos.y, loc.pos.z],
                rotation: loc.rot,
                facets,
                kind,
                stock,
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
    let text = serde_json::to_string_pretty(&file).map_err(|e| corrupt(e.to_string()))?;
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
    let file: ProjectFile = serde_json::from_str(text).map_err(|e| corrupt(e.to_string()))?;
    check_and_apply(world, file)
}

fn check_and_apply(world: &World, mut file: ProjectFile) -> Result<(), ProjectError> {
    // Migration hook: step UP one version at a time, each migration exact,
    // before any entity touches the world. Anything newer than this build
    // remains refused.
    if file.version == 1 {
        // v1 -> v2: entities gained `rotation`; v1 had translation-only
        // placements, so identity is the exact migration. Kind and stock
        // fields default (absent = model), so nothing else changes.
        for dto in &mut file.entities {
            dto.rotation = IDENTITY_Q;
        }
        file.version = PROJECT_VERSION;
    }
    if file.version != PROJECT_VERSION {
        return Err(ProjectError::UnsupportedVersion(file.version));
    }
    // Whole-file validation BEFORE any world write: a corrupt entity deeper
    // in the list must not leave a half-loaded document behind.
    for dto in &file.entities {
        validate_kind(dto, dto.kind.unwrap_or(EntityKind::Model))?;
    }
    for dto in file.entities {
        let kind = dto.kind.unwrap_or(EntityKind::Model);
        let entity = model::spawn(world, &dto.name);
        model::set_transform(
            world,
            &entity,
            Location {
                pos: Point3::new(dto.translation[0], dto.translation[1], dto.translation[2]),
                rot: dto.rotation,
            },
        );
        match kind {
            EntityKind::Model => {
                if let Some(tris) = dto.facets {
                    model::set_facets(world, &entity, dto_to_facets(tris));
                }
            }
            EntityKind::Stock => {
                // validate_kind already refused these; Result, never unwrap.
                let shape = dto
                    .stock
                    .ok_or_else(|| corrupt("stock entity without parameters"))?
                    .to_shape();
                let facets = match &shape {
                    StockShape::FromModel { .. } => {
                        dto_to_facets(dto.facets.ok_or_else(|| {
                            corrupt("from-model stock is missing its facets")
                        })?)
                    }
                    _ => stock::stock_mesh(&shape)
                        .filter(|f| !f.is_empty())
                        .ok_or_else(|| corrupt("stock mesh regeneration failed"))?,
                };
                model::set_facets(world, &entity, facets);
                entity
                    .entity_view(world)
                    .set(StockDef { shape: shape.clone() });
            }
            EntityKind::Wcs => {
                entity.entity_view(world).add(Wcs);
            }
        }
    }
    Ok(())
}

/// Structural checks that JSON typing cannot express. All run before this
/// entity reaches the world; a corrupt field never half-loads (Prime
/// Directive: validate early, fail cleanly).
fn validate_kind(dto: &EntityDto, kind: EntityKind) -> Result<(), ProjectError> {
    match kind {
        EntityKind::Model => {
            if dto.stock.is_some() {
                return Err(corrupt(format!("'{}' carries stock parameters", dto.name)));
            }
        }
        EntityKind::Stock => {
            if dto.name != STOCK_NAME {
                return Err(corrupt(format!(
                    "a stock entity must be named '{STOCK_NAME}', found '{}'",
                    dto.name
                )));
            }
            let Some(stock) = &dto.stock else {
                return Err(corrupt("stock entity without parameters"));
            };
            let shape = stock.to_shape();
            if !shape.is_sane() || !shape.is_bounded() {
                return Err(corrupt(format!("stock parameters are not usable: {stock:?}")));
            }
            match (&shape, &dto.facets) {
                // Parametric stock is regenerated, never stored as floats;
                // FromModel stock has nothing to regenerate from.
                (StockShape::Box { .. } | StockShape::Cylinder { .. }, Some(_)) => {
                    return Err(corrupt("parametric stock must not store facets"));
                }
                (StockShape::FromModel { .. }, None) => {
                    return Err(corrupt("from-model stock is missing its facets"));
                }
                (StockShape::FromModel { .. }, Some(tris)) if tris.is_empty() => {
                    return Err(corrupt("from-model stock has no geometry"));
                }
                _ => {}
            }
        }
        EntityKind::Wcs => {
            if !WCS_SLOTS.contains(&dto.name.as_str()) {
                return Err(corrupt(format!(
                    "'{}' is not a work offset slot (expected one of {})",
                    dto.name,
                    WCS_SLOTS.join(", ")
                )));
            }
            if dto.facets.is_some() || dto.stock.is_some() {
                return Err(corrupt(format!("'{}' is an offset, not a solid", dto.name)));
            }
        }
    }
    Ok(())
}
