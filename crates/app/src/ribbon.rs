//! The ribbon's document setting (M6.2): the user's pins.
//!
//! The accessor pair rides the `theme_of`/`theme_set` seam's shape
//! (setup furniture on a flecs singleton, saved with the project).
//! Because the project file carries the list, a pin is a document
//! change: the UI changes it only through the `SetRibbonPins` command
//! (issue #143), so it can be undone and marks the document unsaved.
//! Provenance: our own seam, specced from ROADMAP M6.2 (pin to ribbon).

use flecs_ecs::core::WorldGet; // not in prelude (0.2.2)
use flecs_ecs::prelude::World;

use crate::operation::KIND_COUNT;
use crate::schema::RibbonPins;

/// The pinned kinds, in pin order, filtered to the registry that
/// exists: a document pinned on a bigger build's table reads back
/// without the kinds this one cannot name (never a dark cell for a
/// stranger - the theme's stranger law, worn in the pin's clothes).
#[must_use]
pub fn pinned_of(world: &World) -> Vec<usize> {
    world
        .try_get::<&RibbonPins>(|p| {
            p.0.iter().copied().filter(|i| *i < KIND_COUNT).collect()
        })
        .unwrap_or_default()
}

/// Write the list (order kept, duplicates and out-of-range indices
/// dropped). For the `SetRibbonPins` command and the project loader;
/// every other reader goes through `pinned_of`.
pub fn pinned_set(world: &World, next: &[usize]) {
    let mut clean: Vec<usize> = Vec::with_capacity(next.len());
    for i in next {
        if *i < KIND_COUNT && !clean.contains(i) {
            clean.push(*i);
        }
    }
    world.set(RibbonPins(clean));
}
