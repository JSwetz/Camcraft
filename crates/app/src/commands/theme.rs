//! SetTheme: choose the document's theme (M6.1 C).
//!
//! Setup furniture, like `SelectPost`: no geometry changes, the command
//! writes only the world singleton and its inverse writes the previous
//! value back. What the choice MEANS on screen is the ui's resolution -
//! the theme's token tables and the JSON parser live there (layering:
//! app cannot name an egui type) - so this seam owns the reference and
//! its existence law, and the shell owns the rendering of it.
//! Provenance: our own command, mirroring our own `post.rs`; specced
//! from ROADMAP M6.1 (command-law switcher).

use flecs_ecs::core::WorldGet; // not in prelude (0.2.2)
use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::schema::ThemeRef;

/// The built-in themes, addressed by name. The ui seam maps each name
/// to its token table; this list is what a `Named` reference and a
/// project file's `{"named": ...}` are checked against.
pub const BUILTINS: [&str; 2] = ["dark", "light"];

/// The theme this document wears. Absent singleton = the default (a
/// fresh world has never been themed), exactly like `post_of`.
#[must_use]
pub fn theme_of(world: &World) -> ThemeRef {
    world.try_get::<&ThemeRef>(|t| t.clone()).unwrap_or_default()
}

/// Write the singleton. For the `SetTheme` command and the project
/// loader; every other reader goes through `theme_of`.
pub fn theme_set(world: &World, next: &ThemeRef) {
    world.set(next.clone());
}

pub struct SetTheme {
    prev: ThemeRef,
    next: ThemeRef,
}

impl SetTheme {
    /// Choose a theme. A `File` reference must exist as a file right
    /// now - the full parse is the surface's pre-step (it holds the
    /// parser), so a corrupt theme is refused before this constructor
    /// is even reached; a path that went missing after landing from a
    /// project file still refuses by name at apply time (the
    /// `script_source` law, worn in theme clothes). A `Named`
    /// reference must be a real built-in: the picker only offers
    /// `dark`/`light`, so a stranger is a corrupt document, named as
    /// such when it reads.
    pub fn new(world: &World, next: ThemeRef) -> Result<Self, CommandError> {
        match &next {
            ThemeRef::File(path) => {
                let meta = std::fs::metadata(path)
                    .map_err(|e| CommandError::Invalid(format!("theme file {path}: {e}")))?;
                if !meta.is_file() {
                    return Err(CommandError::Invalid(format!("theme file {path}: not a file")));
                }
            }
            ThemeRef::Named(name) => {
                if !BUILTINS.contains(&name.as_str()) {
                    return Err(CommandError::Invalid(format!(
                        "built-in theme {name} is not one of {}",
                        BUILTINS.join(", ")
                    )));
                }
            }
            ThemeRef::Dark => {}
        }
        let prev = theme_of(world);
        if prev == next {
            return Err(CommandError::Invalid("that theme is already selected".to_owned()));
        }
        Ok(Self { prev, next })
    }
}

impl Command for SetTheme {
    fn apply(&self, world: &World) {
        theme_set(world, &self.next);
    }

    fn invert(&self, world: &World) {
        theme_set(world, &self.prev);
    }
}
