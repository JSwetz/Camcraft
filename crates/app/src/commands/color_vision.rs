//! SetColorVision: choose the viewer's colour-vision remap (M13.4).
//!
//! `SetTheme`'s twin under the same command law: the command writes
//! only the world singleton and its inverse writes the previous value
//! back, so Ctrl+Z puts the previous palette back exactly. What a mode
//! MEANS on screen (the remapped verdict, toolpath and selection
//! colours) is the ui's resolution - app cannot name a colour type -
//! so this seam owns only the reference. Not saved with the project:
//! see `schema::ColorVision` for why a deficiency is a view preference.
//! Provenance: our own command, mirroring our own `theme.rs`; specced
//! from ROADMAP M13.4 / DONE M6.1 (the ColorVision follow-on).

use flecs_ecs::core::WorldGet; // not in prelude (0.2.2)
use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::schema::ColorVision;

/// The remap in force. Absent singleton = `Off` (a fresh world has
/// never been remapped), exactly like `theme_of`.
#[must_use]
pub fn color_vision_of(world: &World) -> ColorVision {
    world.try_get::<&ColorVision>(|c| *c).unwrap_or_default()
}

/// Write the singleton. For the `SetColorVision` command; every other
/// reader goes through `color_vision_of`.
pub fn color_vision_set(world: &World, next: ColorVision) {
    world.set(next);
}

pub struct SetColorVision {
    prev: ColorVision,
    next: ColorVision,
}

impl SetColorVision {
    /// Choose a mode. Every variant is a legal mode (the enum is the
    /// whole registry); re-picking the mode in force is refused so the
    /// history never fills with no-ops (the theme's law).
    pub fn new(world: &World, next: ColorVision) -> Result<Self, CommandError> {
        let prev = color_vision_of(world);
        if prev == next {
            return Err(CommandError::Invalid(
                "that colour-vision mode is already selected".to_owned(),
            ));
        }
        Ok(Self { prev, next })
    }
}

impl Command for SetColorVision {
    fn apply(&self, world: &World) {
        color_vision_set(world, self.next);
    }

    fn invert(&self, world: &World) {
        color_vision_set(world, self.prev);
    }
}
