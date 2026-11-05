//! What the program leaves, as a mesh (M7.2 D): the traced stock's
//! field iso-extracted (`stock_sim::iso`) and written as binary STL.
//! Provenance: the binary STL layout (80-byte header, u32 count, 50
//! bytes a facet - the 3D Systems format as publicly documented) and
//! the M7.1 field; our own code, nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **The mesh is a cache of the field, never a second truth.** It is
//!   derived from the same `FieldView` the viewport traces, and keyed by
//!   it (`MeshCache`): a new view - any cut appended, any edit re-armed
//!   - is a new mesh; nothing reads the triangles back as material.
//! - **A hole is a refusal.** An extraction that is not watertight is
//!   never written: the file would claim a solid it is not.

use std::path::Path;
use std::sync::Arc;

use crate::geometry::Triangle;
use crate::stock_sim::iso;
use crate::stock_view::FieldView;

/// The machined stock's closed surface, extracted at the view's cell.
pub fn machined_mesh(fv: &FieldView) -> Result<Vec<Triangle>, String> {
    let field = fv.stock.field().ok_or("the stock carries no field to extract")?;
    let tris = iso::extract(field).map_err(|e| e.to_string())?;
    if tris.is_empty() {
        return Err("the program leaves no material - nothing to export".into());
    }
    let open = iso::open_edges(&tris);
    if open > 0 {
        return Err(format!("the extracted surface has {open} open edges - refused, not written"));
    }
    Ok(tris)
}

/// Binary STL bytes for `tris` (normals from the winding, f32).
#[must_use]
pub fn stl_bytes(tris: &[Triangle]) -> Vec<u8> {
    let mut b = Vec::with_capacity(84 + tris.len() * 50);
    let mut header = [0u8; 80];
    let tag = b"camcraft machined stock";
    header[..tag.len()].copy_from_slice(tag);
    b.extend_from_slice(&header);
    let n = u32::try_from(tris.len()).unwrap_or(u32::MAX);
    b.extend_from_slice(&n.to_le_bytes());
    for t in tris.iter().take(n as usize) {
        let nrm = t.normal();
        for p in [nrm, t.a, t.b, t.c] {
            for c in [p.x, p.y, p.z] {
                b.extend_from_slice(&(c as f32).to_le_bytes());
            }
        }
        b.extend_from_slice(&[0, 0]);
    }
    b
}

/// Extract and write `fv`'s machined stock to `path`; the facet count.
pub fn write_stl(fv: &FieldView, path: &Path) -> Result<usize, String> {
    let tris = machined_mesh(fv)?;
    if u32::try_from(tris.len()).is_err() {
        return Err("the mesh has more facets than binary STL can count".into());
    }
    std::fs::write(path, stl_bytes(&tris)).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(tris.len())
}

/// The mesh cache: one extraction per field view, rebuilt when the view
/// changes (identity of the `Arc` the viewport holds - the toolpath
/// cache's keyed law, applied to the field).
#[derive(Default)]
pub struct MeshCache {
    key: Option<Arc<FieldView>>,
    mesh: Option<Arc<Vec<Triangle>>>,
}

impl MeshCache {
    /// The mesh of `fv`, extracting only when `fv` is not the cached view.
    pub fn get(&mut self, fv: &Arc<FieldView>) -> Result<Arc<Vec<Triangle>>, String> {
        if let (Some(k), Some(m)) = (&self.key, &self.mesh)
            && Arc::ptr_eq(k, fv)
        {
            return Ok(Arc::clone(m));
        }
        self.key = None;
        self.mesh = None;
        let m = Arc::new(machined_mesh(fv)?);
        self.key = Some(Arc::clone(fv));
        self.mesh = Some(Arc::clone(&m));
        Ok(m)
    }

    /// Drop the cached view and mesh.
    pub fn clear(&mut self) {
        self.key = None;
        self.mesh = None;
    }
}
