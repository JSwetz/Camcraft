//! Setup commands: create, delete, activate, and move operations
//! between setups (M3.2 stage A).
//!
//! Provenance: our own design; one clamping = one datum system is
//! standard machine-shop practice, specced from general machining
//! knowledge - no vendor implementation inspected. Like every sibling
//! here, construction reads the world, validates, and snapshots; `apply`
//! and `invert` are pure writes, so undo/redo replay bytes and never
//! re-derive.
//!
//! ## The laws these commands keep
//!
//! - **Materialization, never renaming.** Creating the first setup on a
//!   document that has none points it at the existing `Stock`/`G54`..
//!   entities ([`crate::setup`]'s implicit-setup law). Nothing is
//!   renamed: every cached stamp, path and saved file predating setups
//!   keeps resolving, byte for byte.
//! - **Datum transfer = clone, not alias.** Transferring a datum copies
//!   the source setup's stock (parameters, placement, facets) and its
//!   `G54` placement into the new setup's OWN entities. A shop then
//!   re-places the copy to describe the re-clamp - placement IS the
//!   flip (M3.3 adds the rotated form). `datum_from` records where the
//!   clone came from; it is sheet provenance, inert for every stamp.
//! - **A setup with operations is not deletable.** [`DeleteSetup`]
//!   refuses while any operation belongs to it ([`crate::setup::belongs`],
//!   by declaration or by the first/implicit position), naming the
//!   operation. Furniture the setup alone owns goes with it; names the
//!   legacy world or another setup also points at stay.
//! - **Activation is inert data** (AGENTS.md, M1.6): which setup is
//!   ACTIVE changes no path, no stamp, no staleness - it only decides
//!   where NEW things join. Commands write the singleton; readers
//!   (`setup::active`) fall back to the first setup when it names
//!   nothing that exists.
//! - **Moving an operation between setups keeps its stamp.** The
//!   digest covers the RESOLVED heights and the stock's identity, so
//!   the machinery that already exists says Ok when the two setups
//!   place their stock identically, and Stale when they do not - no
//!   forced invalidation either way.

use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::geometry::Triangle;
use crate::model;
use crate::schema::{
    Location, Operation, OperationMeta, SetupDef, StockDef, Wcs, STOCK_NAME, WCS_SLOTS,
};
use crate::setup;
use crate::stock::StockShape;

/// A setup's stock as the transfer copies it: entity name, parameters,
/// placement, and the local facets a `FromModel` stock mirrors.
struct StockSnap {
    name: String,
    shape: StockShape,
    location: Location,
    facets: std::sync::Arc<[Triangle]>,
}

/// Create a setup, optionally transferring a datum: cloning the named
/// source setup's stock and `G54`..`G59` placements into the new
/// setup's own entities. With no source the new setup starts with empty
/// furniture (lazily spawned by `SetStock`/`SetWcsOrigin` when used).
pub struct CreateSetup {
    name: String,
    def: SetupDef,
    /// Furniture the datum transfer clones in (owned names only - the
    /// materialization case clones nothing; it points at what exists).
    stock: Option<StockSnap>,
    wcs: Vec<(String, Location)>,
    /// The active setup before this command, restored by its inverse.
    prev_active: Option<String>,
}

impl CreateSetup {
    /// `name` defaults to the next free `SetupN`. `datum_from` names an
    /// existing setup to clone; `None` means a fresh, empty context -
    /// except on a document with no setups at all, where creation is by
    /// law the MATERIALIZATION and points at the legacy entities.
    pub fn new(
        world: &World,
        name: Option<&str>,
        datum_from: Option<&str>,
    ) -> Result<Self, CommandError> {
        let name = match name {
            Some(n) => {
                if n.trim().is_empty() {
                    return Err(CommandError::Invalid("setup name must not be empty".into()));
                }
                if n.contains('/') {
                    return Err(CommandError::Invalid(
                        "a setup name must not contain '/' - that separator belongs to setup furniture".into(),
                    ));
                }
                n.to_string()
            }
            None => setup::next_setup_name(world).ok_or_else(|| {
                CommandError::Invalid("no free SetupN name - this document is out of setups".into())
            })?,
        };
        if model::find_by_name(world, &name).is_some() {
            return Err(CommandError::NameConflict(name.clone()));
        }
        if STOCK_NAME == name || WCS_SLOTS.contains(&name.as_str()) {
            return Err(CommandError::Invalid(format!(
                "'{name}' is reserved for setup furniture"
            )));
        }
        let materializing = setup::names(world).is_empty();
        if materializing && datum_from.is_some() {
            return Err(CommandError::Invalid(
                "there is nothing to transfer a datum from yet".into(),
            ));
        }
        let def = if materializing {
            SetupDef {
                stock: STOCK_NAME.to_string(),
                wcs: WCS_SLOTS.iter().map(|s| s.to_string()).collect(),
                datum_from: String::new(),
            }
        } else {
            SetupDef {
                stock: format!("{name}/Stock"),
                wcs: WCS_SLOTS.iter().map(|w| format!("{name}/{w}")).collect(),
                datum_from: datum_from.unwrap_or("").to_string(),
            }
        };
        let mut stock = None;
        let mut wcs = Vec::new();
        if let Some(from) = datum_from {
            if !setup::exists(world, from) {
                return Err(CommandError::NotFound(from.to_string()));
            }
            if let Some((entity, shape)) = setup::stock_entity(world, Some(from))
                .and_then(|e| model::stock_def_of(world, &e).map(|s| (e, s)))
            {
                stock = Some(StockSnap {
                    name: format!("{name}/Stock"),
                    shape,
                    location: model::transform_of(world, &entity).unwrap_or_default(),
                    facets: model::facets_of(world, &entity).unwrap_or_default(),
                });
            }
            for i in 0..WCS_SLOTS.len() {
                let Some(entity) = setup::wcs_entity(world, Some(from), i) else {
                    continue;
                };
                if let (Some(wname), Some(loc)) = (
                    setup::wcs_entity_name(world, Some(name.as_str()), i),
                    model::transform_of(world, &entity),
                ) {
                    wcs.push((wname, loc));
                }
            }
        }
        Ok(CreateSetup {
            name,
            def,
            stock,
            wcs,
            prev_active: setup::active(world),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Command for CreateSetup {
    fn apply(&self, world: &World) {
        let entity = match model::find_by_name(world, &self.name) {
            Some(e) => e,
            None => model::spawn(world, &self.name),
        };
        entity.entity_view(world).set(self.def.clone());
        if let Some(s) = &self.stock {
            let e = match model::find_by_name(world, &s.name) {
                Some(e) => e,
                None => model::spawn(world, &s.name),
            };
            model::set_transform(world, &e, s.location);
            model::set_facets(world, &e, s.facets.clone());
            e.entity_view(world).set(StockDef {
                shape: s.shape.clone(),
            });
        }
        for (wname, loc) in &self.wcs {
            let e = match model::find_by_name(world, wname) {
                Some(e) => e,
                None => model::spawn(world, wname),
            };
            e.entity_view(world).add(Wcs);
            model::set_transform(world, &e, *loc);
        }
        // The new setup is the one you work in (module header: creation
        // activates; undo restores what was active).
        setup::set_active(world, Some(self.name.clone()));
    }

    fn invert(&self, world: &World) {
        setup::set_active(world, self.prev_active.clone());
        if let Some(s) = &self.stock
            && let Some(e) = model::find_by_name(world, &s.name)
        {
            model::remove(world, &e);
        }
        for (wname, _) in &self.wcs {
            if let Some(e) = model::find_by_name(world, wname) {
                model::remove(world, &e);
            }
        }
        if let Some(e) = model::find_by_name(world, &self.name) {
            model::remove(world, &e);
        }
    }
}

/// Delete a setup, its private furniture, and its claim on the program.
/// Refused while any operation still machines in it.
pub struct DeleteSetup {
    name: String,
    def: SetupDef,
    /// Furniture that goes with the setup: entities this setup alone
    /// names. Shared legacy names (`Stock`, `G54`..) and anything
    /// another setup points at are NOT deleted.
    stock: Option<StockSnap>,
    wcs: Vec<(String, Location)>,
    prev_active: Option<String>,
}

impl DeleteSetup {
    pub fn new(world: &World, name: &str) -> Result<Self, CommandError> {
        let entity = model::find_by_name(world, name)
            .ok_or_else(|| CommandError::NotFound(name.to_string()))?;
        let def = entity
            .entity_view(world)
            .try_get::<&SetupDef>(|d| d.clone())
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a setup")))?;
        // Refusal: an operation that would orphan its own program
        // position. `belongs` is the single predicate the order law uses.
        for (op_name, op_e) in setup::operation_order(world) {
            let declared = model::operation_of(world, &op_e).and_then(|o| o.setup);
            if setup::belongs(world, declared.as_deref(), name) {
                return Err(CommandError::Invalid(format!(
                    "'{op_name}' machines in '{name}' - move or delete it first"
                )));
            }
        }
        let owned = |n: &str| {
            n != STOCK_NAME && !WCS_SLOTS.contains(&n) && !setup::is_furniture_of_other(world, n, name)
        };
        let stock = if owned(&def.stock) {
            model::find_by_name(world, &def.stock).and_then(|e| {
                model::stock_def_of(world, &e).map(|shape| StockSnap {
                    name: def.stock.clone(),
                    shape,
                    location: model::transform_of(world, &e).unwrap_or_default(),
                    facets: model::facets_of(world, &e).unwrap_or_default(),
                })
            })
        } else {
            None
        };
        let mut wcs = Vec::new();
        for (i, wname) in def.wcs.iter().enumerate() {
            if i >= WCS_SLOTS.len() || !owned(wname) {
                continue;
            }
            if let Some(e) = model::find_by_name(world, wname)
                && model::is_wcs(world, &e)
                && let Some(loc) = model::transform_of(world, &e)
            {
                wcs.push((wname.clone(), loc));
            }
        }
        Ok(DeleteSetup {
            name: name.to_string(),
            def,
            stock,
            wcs,
            prev_active: setup::active(world),
        })
    }
}

impl Command for DeleteSetup {
    fn apply(&self, world: &World) {
        if let Some(s) = &self.stock
            && let Some(e) = model::find_by_name(world, &s.name)
        {
            model::remove(world, &e);
        }
        for (wname, _) in &self.wcs {
            if let Some(e) = model::find_by_name(world, wname) {
                model::remove(world, &e);
            }
        }
        if let Some(e) = model::find_by_name(world, &self.name) {
            model::remove(world, &e);
        }
        // The active-setup singleton is left pointing where it pointed:
        // `setup::active`'s existence fallback makes a deleted name read
        // as the first remaining setup, exactly the law.
    }

    fn invert(&self, world: &World) {
        let entity = model::spawn(world, &self.name);
        entity.entity_view(world).set(self.def.clone());
        if let Some(s) = &self.stock {
            let e = model::spawn(world, &s.name);
            model::set_transform(world, &e, s.location);
            model::set_facets(world, &e, s.facets.clone());
            e.entity_view(world).set(StockDef {
                shape: s.shape.clone(),
            });
        }
        for (wname, loc) in &self.wcs {
            let e = model::spawn(world, wname);
            e.entity_view(world).add(Wcs);
            model::set_transform(world, &e, *loc);
        }
        setup::set_active(world, self.prev_active.clone());
    }
}

/// Choose the ACTIVE setup: where new operations and new furniture
/// join. Inert for every stamp (module header) - this command moves a
/// cursor, not geometry.
pub struct ActivateSetup {
    name: Option<String>,
    prev: Option<String>,
}

impl ActivateSetup {
    pub fn new(world: &World, name: Option<&str>) -> Result<Self, CommandError> {
        if let Some(n) = name
            && !setup::exists(world, n)
        {
            return Err(CommandError::NotFound(n.to_string()));
        }
        Ok(ActivateSetup {
            name: name.map(str::to_string),
            prev: setup::active(world),
        })
    }
}

impl Command for ActivateSetup {
    fn apply(&self, world: &World) {
        setup::set_active(world, self.name.clone());
    }

    fn invert(&self, world: &World) {
        setup::set_active(world, self.prev.clone());
    }
}

/// Move an operation into another setup (or into the implicit first
/// position with `None`). The stamp is KEPT: resolved heights and the
/// stock's identity re-digest on the next question, so the operation
/// reads Ok when the placements match and Stale when they do not - the
/// staleness law answering, not this command assuming.
pub struct SetOperationSetup {
    name: String,
    prev: Operation,
    next: Operation,
    prev_meta: OperationMeta,
    next_meta: OperationMeta,
}

impl SetOperationSetup {
    pub fn new(
        world: &World,
        op_name: &str,
        target: Option<&str>,
    ) -> Result<Self, CommandError> {
        let entity = model::find_by_name(world, op_name)
            .ok_or_else(|| CommandError::NotFound(op_name.to_string()))?;
        let prev = model::operation_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{op_name}' is not an operation")))?;
        if let Some(t) = target
            && !setup::exists(world, t)
        {
            return Err(CommandError::NotFound(t.to_string()));
        }
        if target == prev.setup.as_deref() {
            return Err(CommandError::Invalid(format!(
                "'{op_name}' already machines in {}",
                target.map_or_else(|| "the first setup".to_string(), str::to_string)
            )));
        }
        let next = Operation {
            setup: target.map(str::to_string),
            ..prev.clone()
        };
        let prev_meta = model::operation_meta_of(world, &entity);
        let mut next_meta = prev_meta.clone();
        // A rank belongs to its old setup. A moved operation joins the
        // target's unranked tail until the user places it there.
        next_meta.order = None;
        Ok(SetOperationSetup {
            name: op_name.to_string(),
            prev,
            next,
            prev_meta,
            next_meta,
        })
    }
}

impl Command for SetOperationSetup {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.next.clone());
            entity.entity_view(world).set(self.next_meta.clone());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.prev.clone());
            entity.entity_view(world).set(self.prev_meta.clone());
        }
    }
}
