//! SetPostOptions: the user's choices for the document's post (issue
//! #34) - today the lathe thread output, G32 passes or the G76 cycle.
//!
//! Setup furniture like `SelectPost`: no geometry changes, the command
//! writes only the `PostOptions` singleton and its inverse writes the
//! previous value back. The choice is the user's word (CAM choices are
//! user parameters); a post that cannot honour it refuses by name at
//! post time, never silently posts the other form.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::posting;
use crate::schema::PostOptions;

#[derive(Debug)]
pub struct SetPostOptions {
    prev: PostOptions,
    next: PostOptions,
}

impl SetPostOptions {
    /// Choose the post options; the current choice again is refused
    /// (a no-op would be an empty undo step).
    pub fn new(world: &World, next: PostOptions) -> Result<Self, CommandError> {
        let prev = crate::setup_post_options::document(world);
        if prev == next {
            return Err(CommandError::Invalid("those post options are already set".to_owned()));
        }
        Ok(Self { prev, next })
    }
}

impl Command for SetPostOptions {
    fn apply(&self, world: &World) {
        posting::post_options_set(world, self.next);
    }

    fn invert(&self, world: &World) {
        posting::post_options_set(world, self.prev);
    }
}
