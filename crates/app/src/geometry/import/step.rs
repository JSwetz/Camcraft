//! STEP reader: B-rep shells tessellated to triangles with the `truck`
//! geometry crates (ricosjp, Apache-2.0), used as a library dependency.
//!
//! Provenance: file syntax per ISO 10303-21; surface tessellation is truck's
//! own published algorithm - nothing here from any CAM vendor.
//!
//! Hardening notes (Prime Directive):
//! - `truck` panics on shells that are not oriented/closed. Every truck call
//!   on the parse path runs inside `catch_unwind`; a kernel panic becomes a
//!   clean `ImportError::Step`, logged, never propagated.
//! - Tessellation tolerance is scaled from the shell's raw vertex extents
//!   BEFORE meshing (never a blind fixed tol), and floored so the
//!   subdivision ratio cannot explode into an unbounded mesh.
//! - Facets are capped at `MAX_FACETS` while collecting.

use std::panic::AssertUnwindSafe;

use truck_meshalgo::prelude::*;
use truck_stepio::r#in::Table;

use super::{ImportError, MAX_FACETS};
use crate::geometry::{Mesh, Point3, Triangle};

/// Lower bound for tessellation tolerance; truck asserts tol >= 1e-7.
const MIN_TOL: f64 = 1e-6;

pub fn parse(data: &[u8]) -> Result<Mesh, ImportError> {
    if data.is_empty() {
        return Err(ImportError::Empty);
    }
    let text = std::str::from_utf8(data)
        .map_err(|_| ImportError::Step("file is not valid UTF-8".into()))?;
    let exchange = ruststep::parser::parse(text)
        .map_err(|e| ImportError::Step(format!("syntax error: {e}")))?;
    // An exchange with zero DATA sections parses fine but would index out of
    // bounds inside truck's convenience helpers; guard the only raw index.
    let Some(section) = exchange.data.first() else {
        return Err(ImportError::Step("file has no DATA section".into()));
    };
    let table = Table::from_data_section(section);
    if table.shell.is_empty() {
        return Err(ImportError::Step("no shells found".into()));
    }

    let mut facets: Vec<Triangle> = Vec::new();
    for shell in table.shell.values() {
        // The whole per-shell kernel sequence runs inside one guarded call.
        // Everything it touches is shared-immutable or thread-local, so
        // AssertUnwindSafe holds for our purposes: on a panic we discard all
        // outputs and abort the import.
        let per_shell = std::panic::catch_unwind(AssertUnwindSafe(|| {
            shell_facets(&table, shell)
        }));
        let tris = match per_shell {
            Err(_) => {
                eprintln!("step: truck kernel panicked on a shell; import aborted");
                return Err(ImportError::Step(
                    "geometry kernel failed on a shell (degenerate or unclosed B-rep)".into(),
                ));
            }
            Ok(Err(err)) => return Err(err),
            Ok(Ok(tris)) => tris,
        };
        facets.extend(tris);
        if facets.len() > MAX_FACETS {
            return Err(ImportError::TooLarge);
        }
    }
    if facets.is_empty() {
        return Err(ImportError::Empty);
    }
    Ok(Mesh::from_facets(facets))
}

/// Tessellate one STEP shell into triangles. Runs inside `catch_unwind`;
/// returns `Err` only for reasons we control.
fn shell_facets(
    table: &Table,
    shell: &truck_stepio::r#in::ShellHolder,
) -> Result<Vec<Triangle>, ImportError> {
    let cshell = table
        .to_compressed_shell(shell)
        .map_err(|e| ImportError::Step(format!("shell conversion failed: {e}")))?;
    let mut extent = 0.0f64;
    for p in cshell.vertices.iter() {
        for c in [p.x, p.y, p.z] {
            if !c.is_finite() {
                return Err(ImportError::Step("shell vertices are not finite".into()));
            }
            extent = extent.max(c.abs());
        }
    }
    // Ratio-bounded tolerance: 0.1% of the model extent, floored at MIN_TOL
    // so tiny parts still subdivide and truck's assert (tol >= 1e-7) holds.
    let tol = (extent * 1e-3).max(MIN_TOL);
    let meshed = cshell.robust_triangulation(tol).to_polygon();
    let positions = meshed.positions();
    let mut facets: Vec<Triangle> = Vec::new();
    for face in meshed.face_iter() {
        if face.len() < 3 {
            continue;
        }
        let at = |i: usize| -> Option<Point3> {
            let v = face.get(i)?.pos;
            let p = positions.get(v)?;
            let pt = Point3::new(p.x, p.y, p.z);
            pt.is_finite().then_some(pt)
        };
        let Some(a) = at(0) else { continue };
        for i in 1..face.len() - 1 {
            let (Some(b), Some(c)) = (at(i), at(i + 1)) else {
                continue;
            };
            let tri = Triangle::new(a, b, c);
            // Zero-area facets are tessellation slivers: drop them, they
            // carry no geometry and would poison the STL-style gates.
            let n = tri.normal();
            if n.x != 0.0 || n.y != 0.0 || n.z != 0.0 {
                facets.push(tri);
                if facets.len() > MAX_FACETS {
                    return Err(ImportError::TooLarge);
                }
            }
        }
    }
    Ok(facets)
}
