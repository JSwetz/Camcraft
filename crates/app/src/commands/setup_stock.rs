//! `SetSetupStockSource` (#320): pick where a setup's stock comes from -
//! its own stock, or what the setup before it leaves behind.
//!
//! Provenance: our own design; "stock from the preceding setup" is a
//! plain CAM workflow (public documentation, behaviour only). No vendor
//! implementation inspected.
//!
//! ## The laws this command keeps
//!
//! - **Validated and snapshotted at construction.** A live setup, not the
//!   first (nothing before it), a carry the sim can verify when taking
//!   what the setup before leaves (refused by name otherwise), and a
//!   change from what the setup uses now. `apply`/`invert` are pure
//!   writes; undo restores the stored pick, "never picked" included.
//! - **Nothing else moves.** The stock entity, the part alignment and the
//!   work offset are untouched; the setup's operations read Stale
//!   (the pick rides their stamp).

use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::model;
use crate::schema::SetupDef;
use crate::setup_stock::{self, StockSource};

/// Pick a setup's stock source.
pub struct SetSetupStockSource {
    setup: String,
    prev: Option<StockSource>,
    next: StockSource,
}

impl SetSetupStockSource {
    /// Make `name` start from `source`.
    pub fn new(world: &World, name: &str, source: StockSource) -> Result<Self, CommandError> {
        let entity = model::find_by_name(world, name).ok_or_else(|| CommandError::NotFound(name.to_string()))?;
        let prev = entity
            .entity_view(world)
            .try_get::<&SetupDef>(|d| d.stock_source)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a setup")))?;
        let Some(now) = setup_stock::effective(world, name) else {
            return Err(CommandError::Invalid(format!(
                "'{name}' is the first setup - it always starts from its own stock"
            )));
        };
        if source == StockSource::FromPrevious
            && let Some(why) = setup_stock::carry_refusal(world, name)
        {
            return Err(CommandError::Invalid(why));
        }
        if now == source {
            return Err(CommandError::Invalid(format!(
                "setup '{name}' already uses that stock ({})",
                setup_stock::describe(world, name).unwrap_or_default()
            )));
        }
        Ok(SetSetupStockSource { setup: name.to_string(), prev, next: source })
    }

    fn write(&self, world: &World, source: Option<StockSource>) {
        if let Some(entity) = model::find_by_name(world, &self.setup) {
            let view = entity.entity_view(world);
            if let Some(mut def) = view.try_get::<&SetupDef>(|d| d.clone()) {
                def.stock_source = source;
                view.set(def);
            }
        }
    }
}

impl Command for SetSetupStockSource {
    fn apply(&self, world: &World) {
        self.write(world, Some(self.next));
    }

    fn invert(&self, world: &World) {
        self.write(world, self.prev);
    }
}
