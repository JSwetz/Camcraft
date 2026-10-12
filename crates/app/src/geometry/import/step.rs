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
//! - Tessellation is ADAPTIVE: each shell is meshed coarse first, then refined
//!   toward a quality target only while the projected facet count stays inside
//!   a memory budget. A model that cannot tessellate at target quality still
//!   imports - at reduced fidelity, logged - instead of spiking memory and
//!   being thrown away.
//! - Facet collection is capped per shell while extracting, as a last line of
//!   defense on top of the tolerance schedule.

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
        if facets.len() >= MAX_FACETS {
            return Err(ImportError::TooLarge);
        }
        let budget = MAX_FACETS - facets.len();
        // The whole per-shell kernel sequence runs inside one guarded call.
        // Everything it touches is shared-immutable or thread-local, so
        // AssertUnwindSafe holds for our purposes: on a panic we discard all
        // outputs and abort the import.
        let per_shell = std::panic::catch_unwind(AssertUnwindSafe(|| {
            shell_facets(&table, shell, budget)
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
    }
    if facets.is_empty() {
        return Err(ImportError::Empty);
    }
    Ok(Mesh::from_facets(facets))
}

/// Tessellate one STEP shell into triangles, staying inside `budget` facets.
/// Runs inside `catch_unwind`; returns `Err` only for reasons we control.
fn shell_facets(
    table: &Table,
    shell: &truck_stepio::r#in::ShellHolder,
    budget: usize,
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
    // One tessellation pass: mesh the whole shell at `tol`, extract
    // triangles, stop with `TooLarge` as soon as `budget` is crossed. A
    // closure so the heavily-aliased CompressedShell type is inferred
    // instead of spelled out.
    let triangulate = |tol: f64| -> Result<Vec<Triangle>, ImportError> {
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
                    if facets.len() > budget {
                        return Err(ImportError::TooLarge);
                    }
                }
            }
        }
        Ok(facets)
    };
    // Quality target: ~0.1% of the model extent, floored so tiny parts still
    // subdivide and truck's assert (tol >= 1e-7) holds.
    let tol_target = (extent * 1e-3).max(MIN_TOL);
    // Start 250x coarser than the target - the first pass then costs a few
    // dozen triangles per face no matter how big the model, so no shell can
    // silently allocate a monster mesh before we have measured it.
    let mut tol = (extent * 0.25).max(tol_target);
    let mut refined = loop {
        match triangulate(tol) {
            Ok(tris) => break tris,
            // Face count alone can overflow the budget; coarsen up to a
            // bounded cap. tol grows strictly each retry, so this ends.
            Err(ImportError::TooLarge) => {
                if tol >= extent * 16.0 {
                    return Err(ImportError::TooLarge);
                }
                tol = (tol * 4.0).min(extent * 16.0);
            }
            Err(err) => return Err(err),
        }
    };
    // Refine toward the target: halving the tolerance at most quadruples the
    // count, so a pass is taken only while the projected mesh stays within
    // budget/2. The loop ends at the target, at the budget, or at the pass
    // cap - never on a panicking or unbounded path.
    for _ in 0..24 {
        if tol <= tol_target || refined.len() > budget / 8 {
            break;
        }
        tol = (tol * 0.5).max(tol_target);
        match triangulate(tol) {
            Ok(tris) => refined = tris,
            // Should not happen (the projection bounds it); if truck's
            // density does not follow the tol^-2 law, keep the last mesh
            // that fit rather than failing the import.
            Err(ImportError::TooLarge) => break,
            Err(err) => return Err(err),
        }
    }
    if tol > tol_target {
        eprintln!(
            "step: tessellated at reduced fidelity (tol {tol:e}, target {tol_target:e}, {} facets) to bound memory",
            refined.len()
        );
    }
    Ok(refined)
}
