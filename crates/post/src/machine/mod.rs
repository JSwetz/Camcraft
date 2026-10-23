//! The machine-definition seam: `.mch` grammar as data (`def`) and the
//! file's reader (`parse`) - the same facts `post::machine::mirror`
//! restates for scripts. Pure serde; no world, no units glue (that is
//! the app's `machine` module, which re-exports all of this).

pub mod def;
mod parse;

pub use def::*;
pub use parse::parse_mch;
