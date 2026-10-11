//! Mesh file importers (STL, OBJ). Public API: `load_model` / `parse_*`.
//!
//! All parsers take a byte slice so the size is known before anything is
//! allocated; the facet/vertex caps below are enforced pre-allocation.

pub mod obj;
pub mod stl;

use std::path::{Path, PathBuf};

use crate::geometry::Mesh;

/// Hard cap on facets per model (5M triangles ~ 84 + 50*cap bytes binary).
pub const MAX_FACETS: usize = 5_000_000;
/// Hard cap on OBJ vertex definitions.
pub const MAX_VERTICES: usize = 5_000_000;
/// Cap on the size of any file we are willing to read into memory.
pub const MAX_FILE_BYTES: u64 = 1 << 30;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("file is empty")]
    Empty,
    #[error("model exceeds the import limit")]
    TooLarge,
    #[error("corrupt STL: {0}")]
    Stl(String),
    #[error("corrupt OBJ: {0}")]
    Obj(String),
    #[error("imported mesh contains non-finite coordinates")]
    NonFinite,
    #[error("unsupported file type: {0}")]
    Unsupported(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Stl,
    Obj,
}

/// Load a model from disk, dispatching on extension.
pub fn load_model(path: &Path) -> Result<Mesh, ImportError> {
    let format = format_from_path(path)?;
    let meta = std::fs::metadata(path)?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(ImportError::TooLarge);
    }
    let data = std::fs::read(path)?;
    parse_model(&data, format)
}

pub fn parse_model(data: &[u8], format: Format) -> Result<Mesh, ImportError> {
    let mesh = match format {
        Format::Stl => stl::parse(data)?,
        Format::Obj => obj::parse(data)?,
    };
    // Belt-and-gate: each importer checks finiteness at the parse site, and
    // nothing enters the world without passing here either.
    if !mesh.all_finite() {
        return Err(ImportError::NonFinite);
    }
    Ok(mesh)
}

fn format_from_path(path: &Path) -> Result<Format, ImportError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "stl" => Ok(Format::Stl),
        "obj" => Ok(Format::Obj),
        _ => Err(ImportError::Unsupported(
            path.file_name()
                .map(PathBuf::from)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<unknown>".to_string()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_extension_is_an_error_not_a_panic() {
        let err = load_model(Path::new("thing.step")).err();
        assert!(matches!(err, Some(ImportError::Unsupported(_))));
    }
}
